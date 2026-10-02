import Foundation

struct DraftSnapshot: Codable {
    var version = 1
    var texts: [String: String] = [:]
    var attachments: [String: [PendingAttachment]] = [:]
    var outgoing: [OutgoingAttachment] = []
    var replies: [String: Message] = [:]
}

/// Private, atomic snapshots. Encoding and writes never run on the UI/engine queue.
final class DraftStorage: @unchecked Sendable {
    let root: URL
    private let queue = DispatchQueue(label: "org.erlin.whatsapp.drafts", qos: .utility)
    private var file: URL { root.appendingPathComponent("state/native-drafts.json") }
    init(root: URL) { self.root = root }

    func load() -> DraftSnapshot {
        guard let size = try? file.resourceValues(forKeys: [.fileSizeKey]).fileSize, size <= 4 * 1024 * 1024,
              let data = try? Data(contentsOf: file), let value = try? JSONDecoder().decode(DraftSnapshot.self, from: data),
              value.version == 1 else { return DraftSnapshot() }
        return value
    }

    func save(_ snapshot: DraftSnapshot, completion: @escaping (Bool) -> Void) {
        queue.async {
            let saved: Bool
            do {
                let data = try JSONEncoder().encode(snapshot)
                guard data.count <= 4 * 1024 * 1024 else { throw CocoaError(.fileWriteOutOfSpace) }
                try FileManager.default.createDirectory(at: self.file.deletingLastPathComponent(), withIntermediateDirectories: true)
                try data.write(to: self.file, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
                saved = true
            } catch { saved = false }
            DispatchQueue.main.async { completion(saved) }
        }
    }

    func pruneTemporaryFiles(retaining urls: Set<URL>) {
        queue.async { AttachmentImport.prune(root: self.root, retaining: urls) }
    }
}

extension ChatStore {
    func restoreDrafts() {
        guard let draftStorage else { return }
        let saved = draftStorage.load()
        drafts = saved.texts
        attachments = saved.attachments.mapValues { files in
            files.compactMap { file in
                AttachmentImport.restoredURL(file.url, root: draftStorage.root).map { PendingAttachment(url: $0, id: file.id) }
            }
        }
        outgoingAttachments = saved.outgoing.compactMap { item in
            guard let url = AttachmentImport.restoredURL(item.file.url, root: draftStorage.root,
                maximumSize: item.voice == true ? AttachmentImport.maximumVoiceBytes : AttachmentImport.maximumBytes) else { return nil }
            return OutgoingAttachment(id: item.id, chat: item.chat, file: PendingAttachment(url: url, id: item.file.id),
                caption: item.caption, mentions: item.mentions, state: .failed,
                error: "Interrupted. Check this chat before retrying.", voice: item.voice, quoting: item.quoting)
        }
        draftReplies = saved.replies
        let retained = Set(attachments.values.flatMap { $0.map(\.url) } + outgoingAttachments.map { $0.file.url })
        draftStorage.pruneTemporaryFiles(retaining: retained)
    }

    func scheduleDraftSave() {
        guard draftStorage != nil else { return }
        draftSaveWork?.cancel()
        let work = DispatchWorkItem { [weak self] in self?.flushDrafts() }
        draftSaveWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.4, execute: work)
    }

    func saveDraft(_ text: String, chat: String) {
        let id = canonical(chat)
        let value = selectedChat == id ? (draftBeforeEditing ?? text) : text
        if drafts[id] != value { drafts[id] = value }
        if selectedChat == id, draftReplies[id] != reply { draftReplies[id] = reply }
    }

    func flushDrafts(completion: @escaping (Bool) -> Void = { _ in }) {
        draftSaveWork?.cancel(); draftSaveWork = nil
        guard let draftStorage else { completion(true); return }
        let value = DraftSnapshot(texts: drafts.filter { !$0.value.isEmpty },
            attachments: attachments.filter { !$0.value.isEmpty }, outgoing: outgoingAttachments, replies: draftReplies)
        draftStorage.save(value) { [weak self] saved in
            if !saved { self?.error = "Could not save your draft on this iPhone. Free some storage and try again." }
            completion(saved)
        }
    }

    func pumpAttachments() {
        guard connected, !isDemo, uploadingAttachment == nil,
              let index = outgoingAttachments.firstIndex(where: { $0.state == .queued }) else { return }
        let item = outgoingAttachments[index]
        uploadingAttachment = item.id
        outgoingAttachments[index].state = .uploading
        flushDrafts { [weak self] saved in
            guard let self, self.uploadingAttachment == item.id else { return }
            guard saved else { self.failAttachment(item.id, error: "Could not save this upload. Try again."); return }
            self.prepareForBackground()
            var command: [String: Any] = ["type": item.voice == true ? "voice" : "attachment",
                "chat": self.canonical(item.chat), "request": item.id, "path": item.file.url.path]
            if item.voice == true { if let quote = item.quoting { command["quoting"] = quote } }
            else { command["caption"] = item.caption; command["mentions"] = item.mentions }
            self.engine.send(command) { [weak self] accepted in
                if !accepted { self?.failAttachment(item.id, error: "The upload was not accepted. Try again after reconnecting.") }
            }
        }
    }

    /// Transfer the prepared recording only after its recoverable job is durable.
    func queueVoice(_ path: URL, chat: String, quoting: String?, completion: @escaping (Bool) -> Void) {
        let chat = canonical(chat)
        guard outgoingAttachments.filter({ canonical($0.chat) == chat }).count + attachments[chat, default: []].count < 30 else {
            error = "Send or remove some pending attachments before sending this recording."
            completion(false); return
        }
        let item = OutgoingAttachment(id: UUID().uuidString, chat: chat, file: PendingAttachment(url: path),
            caption: "", mentions: [], voice: true, quoting: quoting)
        outgoingAttachments.append(item)
        flushDrafts { [weak self] saved in
            guard let self else { completion(false); return }
            if !saved { self.outgoingAttachments.removeAll { $0.id == item.id } }
            completion(saved)
            if saved { self.pumpAttachments() }
        }
    }

    func failAttachment(_ id: String, error: String) {
        if let index = outgoingAttachments.firstIndex(where: { $0.id == id }) {
            outgoingAttachments[index].state = .failed; outgoingAttachments[index].error = error
        }
        if uploadingAttachment == id { uploadingAttachment = nil }
        flushDrafts()
        pumpAttachments()
    }

    func finishAttachment(_ event: CoreEvent) {
        guard let id = event.id, let item = outgoingAttachments.first(where: { $0.id == id }),
              event.chat.map(canonical) == canonical(item.chat) else { return }
        guard event.messageID != nil else { failAttachment(id, error: event.detail ?? "Upload failed. Tap Retry."); return }
        outgoingAttachments.removeAll { $0.id == id }
        if uploadingAttachment == id { uploadingAttachment = nil }
        // The worker has stored both the media and its protobuf before this ack.
        flushDrafts { saved in if saved { DispatchQueue.global(qos: .utility).async { AttachmentImport.discard(item.file.url) } } }
        pumpAttachments()
    }

    func retryAttachment(_ item: OutgoingAttachment) {
        guard connected, let index = outgoingAttachments.firstIndex(where: { $0.id == item.id && $0.state == .failed }) else { return }
        outgoingAttachments[index].state = .queued; outgoingAttachments[index].error = nil
        pumpAttachments()
    }

    func discardAttachment(_ item: OutgoingAttachment) {
        guard item.state != .uploading, uploadingAttachment != item.id else { return }
        outgoingAttachments.removeAll { $0.id == item.id }
        flushDrafts { saved in if saved { DispatchQueue.global(qos: .utility).async { AttachmentImport.discard(item.file.url) } } }
    }
}
