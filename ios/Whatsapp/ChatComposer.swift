import PhotosUI
import SwiftUI

struct ChatComposer: View {
    let chatID: String
    @Binding var draft: String
    @ObservedObject var audio: NativeAudio
    @EnvironmentObject private var store: ChatStore
    @State private var sending = false
    @State private var importing = false
    @State private var filesPresented = false
    @State private var pickerPresented = false
    @State private var photos: [PhotosPickerItem] = []
    private var pending: [PendingAttachment] { store.attachments[store.canonical(chatID)] ?? [] }
    private var hasText: Bool { !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        VStack(spacing: 8) {
            if !store.connected && !store.isDemo { Text("Reconnecting… Your draft is saved.").font(.caption).foregroundStyle(.secondary) }
            if let editing = store.editing {
                HStack {
                    Label("Editing message", systemImage: "pencil").font(.caption.bold())
                    Spacer()
                    Button("Cancel") { store.editing = nil; draft = store.draftBeforeEditing ?? ""; store.draftBeforeEditing = nil }
                }.padding(8).accessibilityIdentifier("editing-message")
                Text(editing.text).font(.caption).lineLimit(2).foregroundStyle(.secondary)
            } else if let reply = store.reply {
                HStack {
                    VStack(alignment: .leading, spacing: 3) {
                        Text("Replying to \(store.displayName(reply.sender, fallback: reply.senderName))").font(.caption.bold()).foregroundStyle(Color.accentColor)
                        Text(reply.text).font(.caption).lineLimit(2).foregroundStyle(.secondary)
                    }.frame(maxWidth: .infinity, alignment: .leading)
                    Button { store.reply = nil } label: { Image(systemName: "xmark.circle.fill") }.accessibilityLabel("Cancel reply")
                }.padding(10).background(Color(.tertiarySystemBackground), in: RoundedRectangle(cornerRadius: 10))
            }
            if !pending.isEmpty {
                ScrollView(.horizontal) {
                    HStack {
                        ForEach(pending) { item in
                            HStack {
                                Image(systemName: "doc")
                                Text(item.name).lineLimit(1).frame(maxWidth: 150)
                                Button { store.removeAttachment(item, from: chatID) } label: { Image(systemName: "xmark.circle.fill") }.accessibilityLabel("Remove \(item.name)")
                            }.font(.caption).padding(10).background(.quaternary, in: Capsule())
                        }
                    }
                }
            }
            if store.currentChat?.readOnly == true {
                Text("Only admins can send messages here").font(.footnote).foregroundStyle(.secondary).padding(10)
            } else if audio.hasRecording && audio.recordingChat.map(store.canonical) == store.canonical(chatID) {
                recordingControls
            } else {
                completions
                HStack(alignment: .bottom, spacing: 8) {
                    attachmentMenu.disabled(importing || store.editing != nil)
                    TextField(pending.isEmpty ? "Message" : "Add a caption", text: $draft, axis: .vertical)
                        .font(.system(size: store.preferences.textSize)).lineLimit(1...5)
                        .padding(.horizontal, 14).padding(.vertical, 11)
                        .background(Color(.tertiarySystemBackground), in: RoundedRectangle(cornerRadius: 23))
                        .accessibilityIdentifier("message-composer")
                        .onChange(of: draft) { _, value in
                            let expanded = EmojiCatalog.expandCompletedShortcode(value)
                            if expanded != value { draft = expanded }
                            store.composing(expanded)
                        }
                    if hasText || !pending.isEmpty || store.editing != nil {
                        Button(action: send) {
                            Image(systemName: store.editing == nil ? "arrow.up" : "checkmark").font(.title3.bold()).foregroundStyle(.black)
                                .frame(width: 44, height: 44).background(Color.accentColor, in: Circle())
                        }.disabled(sending || importing || !store.canPost || (!hasText && pending.isEmpty))
                            .accessibilityLabel("Send message").accessibilityIdentifier("send-message")
                            .keyboardShortcut(.return, modifiers: .command)
                    } else {
                        Button { Task { await store.startVoiceRecording() } } label: {
                            Image(systemName: "mic.fill").font(.title3).frame(width: 44, height: 44)
                        }.disabled(!store.canPost).accessibilityLabel("Record voice message")
                    }
                }
            }
            if importing { ProgressView("Preparing attachments…").font(.caption) }
        }
        .padding(.horizontal, 12).padding(.vertical, 8).background(.bar)
        .onChange(of: store.editing?.id) { old, _ in
            if let message = store.editing { if old == nil { store.draftBeforeEditing = draft }; draft = message.text; store.reply = nil }
        }
        .onChange(of: photos) { _, items in
            guard !items.isEmpty else { return }
            let chat = store.canonical(chatID)
            importing = true
            Task {
                do {
                    var urls: [URL] = []
                    for item in items {
                        guard let photo = try await item.loadTransferable(type: ImportedPhoto.self) else { throw CocoaError(.fileReadUnknown) }
                        urls.append(photo.url)
                    }
                    store.addAttachments(urls, to: chat)
                } catch { store.error = "Could not import these photos or videos. Files must be under 100 MB." }
                photos = []; importing = false
            }
        }
        .fileImporter(isPresented: $filesPresented, allowedContentTypes: [.item], allowsMultipleSelection: true) { result in
            guard case .success(let urls) = result else { return }
            importFiles(urls)
        }
        .sheet(isPresented: $pickerPresented) {
            MediaPicker { emoji in draft += emoji; pickerPresented = false }
        }
        .onChange(of: audio.error) { _, error in if let error { store.error = error; audio.error = nil } }
    }

    @ViewBuilder private var completions: some View {
        if let range = EmojiCatalog.trailingToken(in: draft, marker: ":"), draft[range].count > 1 {
            ScrollView(.horizontal) {
                HStack {
                    ForEach(Array(EmojiCatalog.search(String(draft[range].dropFirst())).prefix(12))) { entry in
                        Button { draft.replaceSubrange(range, with: entry.emoji + " "); store.rememberEmoji(entry.emoji) } label: {
                            Text(entry.emoji + " " + (entry.shortcodes.first ?? entry.name)).font(.subheadline).padding(7)
                        }.accessibilityLabel(entry.name)
                    }
                }
            }.accessibilityIdentifier("emoji-completions")
        } else if let range = EmojiCatalog.trailingToken(in: draft, marker: "@"), store.currentChat?.kind == "group" {
            let query = String(draft[range].dropFirst())
            ScrollView(.horizontal) {
                HStack {
                    ForEach((store.currentChat?.participants ?? []).filter { query.isEmpty || store.displayName($0).localizedStandardContains(query) || $0.hasPrefix(query) }.prefix(12), id: \.self) { id in
                        Button(store.displayName(id)) { draft.replaceSubrange(range, with: "@" + id.components(separatedBy: "@")[0] + " ") }.padding(7)
                    }
                }
            }.accessibilityIdentifier("mention-completions")
        }
    }

    private var attachmentMenu: some View {
        Menu {
            PhotosPicker(selection: $photos, maxSelectionCount: max(1, 30 - pending.count), matching: .any(of: [.images, .videos]), preferredItemEncoding: .compatible) {
                Label("Photos and videos", systemImage: "photo.on.rectangle")
            }
            Button { filesPresented = true } label: { Label("Files", systemImage: "doc") }
            Button {
                guard let data = UIPasteboard.general.image?.jpegData(compressionQuality: 0.9) else { store.error = "Copy an image first, then choose Paste photo."; return }
                do { store.addAttachments([try AttachmentImport.pasteImage(data)], to: chatID) }
                catch { store.error = "Could not paste this photo." }
            } label: { Label("Paste photo", systemImage: "doc.on.clipboard") }
            Button { pickerPresented = true } label: { Label("Emoji, GIFs and stickers", systemImage: "face.smiling") }
            if store.currentChat?.kind == "group" {
                Menu("Mention someone") {
                    ForEach(store.currentChat?.participants ?? [], id: \.self) { id in
                        Button(store.displayName(id)) { draft += "@" + id.components(separatedBy: "@")[0] + " " }
                    }
                }
            }
        } label: { Image(systemName: "plus").font(.title2).frame(width: 30, height: 44) }
            .accessibilityLabel("Add attachment").accessibilityIdentifier("composer-attachments")
    }

    private var recordingControls: some View {
        HStack {
            Button(role: .destructive) { audio.discardRecording() } label: { Image(systemName: "trash").frame(width: 40, height: 44) }.accessibilityLabel("Discard recording").disabled(store.voiceSending)
            Circle().fill(audio.isRecording ? .red : .secondary).frame(width: 8, height: 8)
            Text(Duration.seconds(audio.recordingDuration).formatted(.time(pattern: .minuteSecond))).monospacedDigit()
            HStack(spacing: 2) {
                ForEach(Array(audio.levels.suffix(30).enumerated()), id: \.offset) { _, level in
                    Capsule().fill(Color.accentColor).frame(width: 3, height: max(3, level * 32))
                }
            }.frame(maxWidth: .infinity)
            Button {
                sending = true
                store.sendVoice { _ in sending = false }
            } label: { Image(systemName: "arrow.up.circle.fill").font(.largeTitle) }
                .disabled(store.voiceSending || sending || !store.canPost).accessibilityLabel("Send voice message")
        }
    }

    private func send() {
        sending = true
        let value = draft
        let wasEditing = store.editing != nil
        let originalDraft = store.draftBeforeEditing ?? ""
        let completion: (Bool) -> Void = { accepted in
            sending = false
            if accepted && draft == value { draft = wasEditing ? originalDraft : ""; store.draftBeforeEditing = nil; store.stopComposing() }
        }
        if !pending.isEmpty && !wasEditing { store.sendAttachments(caption: value, completion: completion) }
        else { store.sendText(value, completion: completion) }
    }

    private func importFiles(_ urls: [URL]) {
        guard urls.count + pending.count <= 30 else { store.error = "Send up to 30 files at a time."; return }
        let chat = store.canonical(chatID)
        importing = true
        Task {
            do {
                let copies = try await Task.detached(priority: .userInitiated) { try urls.map(AttachmentImport.copy) }.value
                store.addAttachments(copies, to: chat)
            } catch { store.error = "Could not import these files. Files must be under 100 MB." }
            importing = false
        }
    }
}
