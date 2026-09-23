import PhotosUI
import SwiftUI

struct ChatComposer: View {
    let chatID: String
    @Binding var draft: String
    @ObservedObject var audio: NativeAudio
    @EnvironmentObject private var store: ChatStore
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @FocusState private var focused: Bool
    @State private var sending = false
    @State private var importing = false
    @State private var filesPresented = false
    @State private var picker: MediaPicker.Tab?
    @State private var attachmentsExpanded = false
    @State private var photosPresented = false
    @State private var cameraPresented = false
    @State private var cameraChat: String?
    @State private var cameraAccount: String?
    @State private var photos: [PhotosPickerItem] = []
    private var pending: [PendingAttachment] { store.attachments[store.canonical(chatID)] ?? [] }
    private var hasText: Bool { !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    var body: some View {
        VStack(spacing: 0) {
            VStack(spacing: 8) {
                if !store.connected && !store.isDemo { Text("Reconnecting… Your draft is saved.").font(.caption).foregroundStyle(.secondary) }
                if let editing = store.editing {
                    composerContext(title: "Editing message", text: editing.text, icon: "pencil") {
                        store.editing = nil; draft = store.draftBeforeEditing ?? ""; store.draftBeforeEditing = nil
                    }.accessibilityIdentifier("editing-message")
                } else if let reply = store.reply {
                    composerContext(title: store.displayName(reply.sender, fallback: reply.senderName), text: reply.text, icon: "arrowshape.turn.up.left.fill") { store.reply = nil }
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
                    composerRow
                }
                if importing { ProgressView("Preparing attachments…").font(.caption) }
            }.padding(.horizontal, 2).padding(.vertical, 2)
            if attachmentsExpanded {
                ComposerAttachments(group: store.currentChat?.kind == "group", select: selectAttachment)
                    .padding(.top, 2).transition(.move(edge: .bottom).combined(with: .opacity))
            }
        }
        .background(ChatAppearance.composerBar)
        .onChange(of: focused) { _, value in if value { setAttachmentsExpanded(false) } }
        .onChange(of: store.editing?.id) { old, _ in
            if let message = store.editing { if old == nil { store.draftBeforeEditing = draft }; draft = message.text; store.reply = nil; setAttachmentsExpanded(false); focused = true }
        }
        .onChange(of: store.reply?.id) { _, id in if id != nil { focused = true } }
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
        .photosPicker(isPresented: $photosPresented, selection: $photos, maxSelectionCount: max(1, 30 - pending.count), matching: .any(of: [.images, .videos]), preferredItemEncoding: .compatible)
        .sheet(item: $picker) { tab in
            MediaPicker(initialTab: tab) { emoji in draft += emoji; picker = nil; focused = true }
        }
        .fullScreenCover(isPresented: $cameraPresented) {
            CameraCapture(completion: capturePhoto).ignoresSafeArea()
        }
        .onChange(of: audio.error) { _, error in if let error { store.error = error; audio.error = nil } }
    }

    private var composerRow: some View {
        HStack(alignment: .bottom, spacing: 2) {
            Button {
                let expanded = !attachmentsExpanded
                focused = !expanded
                setAttachmentsExpanded(expanded)
            } label: {
                Image(systemName: attachmentsExpanded ? "keyboard" : "plus")
                    .font(.system(size: attachmentsExpanded ? 20 : 25, weight: .regular))
                    .foregroundStyle(.primary).frame(width: 44, height: 44).contentShape(Rectangle())
            }.buttonStyle(.plain).disabled(importing || sending || store.editing != nil)
                .accessibilityLabel(attachmentsExpanded ? "Show keyboard" : "Add attachment")
                .accessibilityIdentifier("composer-attachments")
            TextField(pending.isEmpty ? "" : "Add a caption", text: $draft, axis: .vertical)
                .font(.system(size: store.preferences.textSize)).lineLimit(1...5)
                .focused($focused).accessibilityLabel(pending.isEmpty ? "Message" : "Add a caption")
                .accessibilityIdentifier("message-composer")
                .padding(.leading, 11).padding(.trailing, 40).padding(.vertical, 5).frame(minHeight: 32)
                .background(ChatAppearance.composerField, in: RoundedRectangle(cornerRadius: 19))
                .overlay(RoundedRectangle(cornerRadius: 19).stroke(.primary.opacity(0.08), lineWidth: 0.5))
                .overlay(alignment: .bottomTrailing) {
                    Button { openPicker(.stickers) } label: {
                        ComposerStickerIcon().stroke(.primary, style: StrokeStyle(lineWidth: 1.4, lineCap: .round, lineJoin: .round))
                            .frame(width: 21, height: 21).frame(width: 44, height: 44).contentShape(Rectangle())
                    }.buttonStyle(.plain).offset(y: 6).disabled(importing || sending || store.editing != nil)
                        .accessibilityLabel("Stickers").accessibilityIdentifier("composer-stickers")
                }
                .padding(.vertical, 6).tint(ChatAppearance.composerAction)
                .onChange(of: draft) { _, value in
                    let expanded = EmojiCatalog.expandCompletedShortcode(value)
                    if expanded != value { draft = expanded }
                    store.composing(expanded)
                }
            if store.editing == nil {
                Button(action: openCamera) {
                    Image(systemName: "camera").font(.system(size: 23, weight: .regular)).foregroundStyle(.primary)
                        .frame(width: 44, height: 44).contentShape(Rectangle())
                }.buttonStyle(.plain).disabled(importing || sending || pending.count >= 30)
                    .accessibilityLabel("Camera").accessibilityIdentifier("composer-camera")
            }
            if hasText || !pending.isEmpty || store.editing != nil {
                Button(action: send) { actionIcon(store.editing == nil ? "arrow.up" : "checkmark") }
                    .buttonStyle(.plain).disabled(sending || importing || !store.canPost || (!hasText && pending.isEmpty))
                    .accessibilityLabel("Send message").accessibilityIdentifier("send-message")
                    .keyboardShortcut(.return, modifiers: .command)
            } else {
                Button {
                    focused = false; setAttachmentsExpanded(false)
                    Task { await store.startVoiceRecording() }
                } label: { actionIcon("mic.fill") }
                    .buttonStyle(.plain).disabled(importing || !store.canPost)
                    .accessibilityLabel("Record voice message").accessibilityIdentifier("composer-microphone")
            }
        }
    }

    private func actionIcon(_ name: String) -> some View {
        Image(systemName: name).font(.system(size: 18, weight: .medium)).foregroundStyle(.black)
            .frame(width: 32, height: 32).background(ChatAppearance.composerAction, in: Circle())
            .frame(width: 44, height: 44).contentShape(Rectangle())
    }

    private func setAttachmentsExpanded(_ expanded: Bool) {
        guard attachmentsExpanded != expanded else { return }
        withAnimation(reduceMotion ? nil : .easeOut(duration: 0.2)) { attachmentsExpanded = expanded }
    }

    private func composerContext(title: String, text: String, icon: String, cancel: @escaping () -> Void) -> some View {
        HStack(spacing: 9) {
            Image(systemName: icon).font(.system(size: 14)).foregroundStyle(Color.accentColor)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.caption.weight(.semibold)).foregroundStyle(Color.accentColor)
                Text(text).font(.caption).lineLimit(1).foregroundStyle(.secondary)
            }.frame(maxWidth: .infinity, alignment: .leading)
            Button(action: cancel) { Image(systemName: "xmark").font(.system(size: 12, weight: .semibold)).foregroundStyle(.secondary).frame(width: 44, height: 44) }
                .accessibilityLabel(store.editing == nil ? "Cancel reply" : "Cancel edit")
        }.padding(.leading, 12)
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

    private func selectAttachment(_ action: ComposerAttachment) {
        setAttachmentsExpanded(false)
        switch action {
        case .photos: photosPresented = true
        case .camera: openCamera()
        case .document: filesPresented = true
        case .paste:
            guard let data = UIPasteboard.general.image?.jpegData(compressionQuality: 0.9) else { store.error = "Copy an image first, then choose Paste photo."; return }
            do { store.addAttachments([try AttachmentImport.pasteImage(data)], to: chatID) }
            catch { store.error = "Could not paste this photo." }
        case .emoji: openPicker(.emoji)
        case .gifs: openPicker(.gifs)
        case .stickers: openPicker(.stickers)
        case .mention: draft += "@"; focused = true
        }
    }

    private func openPicker(_ tab: MediaPicker.Tab) {
        focused = false; setAttachmentsExpanded(false); picker = tab
    }

    private func openCamera() {
        focused = false; setAttachmentsExpanded(false)
        guard !store.isDemo else { store.error = "The offline preview does not open your camera."; return }
        guard UIImagePickerController.isSourceTypeAvailable(.camera) else { store.error = "A camera is not available on this device."; return }
        let chat = store.canonical(chatID)
        let account = store.accountID
        Task {
            guard await CameraCapture.requestAccess() else { store.error = "Allow camera access in iPhone Settings to take a photo."; return }
            guard store.hasSession, store.accountID == account, store.selectedChat == chat else { return }
            cameraChat = chat; cameraAccount = account; cameraPresented = true
        }
    }

    private func capturePhoto(_ image: UIImage?) {
        let chat = cameraChat ?? store.canonical(chatID)
        let account = cameraAccount
        cameraPresented = false; cameraChat = nil; cameraAccount = nil
        guard let image else { return }
        importing = true
        Task {
            do {
                let url = try await Task.detached(priority: .userInitiated) { try AttachmentImport.cameraPhoto(image) }.value
                if store.accountID == account { store.addAttachments([url], to: chat) }
                else { AttachmentImport.discard(url) }
            } catch { store.error = "Could not prepare this photo. Try taking it again." }
            importing = false
        }
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
