import Foundation

extension ChatStore {
    func displayName(_ id: String, fallback: String? = nil) -> String {
        let id = canonical(id)
        if id == accountID { return "You" }
        if let contact = contacts.first(where: { canonical($0.id) == id }) {
            if preferences.contactNames, let name = contact.fullName, !name.isEmpty { return name }
            if let name = contact.pushName ?? fallback, !name.isEmpty { return preferences.contactNames ? "~" + name : name }
            if let name = contact.fullName, !name.isEmpty { return name }
            if let name = contact.name, !name.isEmpty { return name }
        }
        if let fallback, !fallback.isEmpty { return fallback }
        if let name = contactNames[id] { return name }
        if let chat = chats.first(where: { $0.id == id }) { return chat.name }
        return id.hasSuffix("@s.whatsapp.net") ? "+" + id.components(separatedBy: "@")[0] : "Participant"
    }

    func chatTitle(_ chat: Chat) -> String {
        if chat.kind == "group" || chat.id == accountID { return chat.name }
        if preferences.contactNames {
            if let name = contacts.first(where: { canonical($0.id) == chat.id })?.fullName, !name.isEmpty { return name }
            if chat.id.hasSuffix("@s.whatsapp.net") { return "+" + chat.id.components(separatedBy: "@")[0] }
        }
        return displayName(chat.id, fallback: chat.name)
    }

    func mentionNames(_ message: Message) -> [String: String] {
        Dictionary((message.mentions ?? []).map { ($0.user, $0.id == accountID ? accountName : displayName($0.id)) }, uniquingKeysWith: { _, last in last })
    }

    func rememberEmoji(_ emoji: String) {
        preferences.recentEmoji.removeAll { $0 == emoji }
        preferences.recentEmoji.insert(emoji, at: 0)
        preferences.recentEmoji = Array(preferences.recentEmoji.prefix(28))
    }

    func navigate(to id: String, message: String? = nil) {
        selectedTab = "chats"
        let id = canonical(id)
        pendingJump = message
        pendingJumpChat = message == nil ? nil : id
        jumpRequested = false
        navigation = [id]
        if selectedChat == id { resolveJump() }
    }

    func jump(to id: String) {
        pendingJump = id
        pendingJumpChat = selectedChat
        jumpRequested = false
        resolveJump()
    }

    func resolveJump() {
        guard let id = pendingJump, pendingJumpChat == selectedChat, !loading else { return }
        if messages.contains(where: { $0.id == id }) {
            scrollTarget = id
            pendingJump = nil
            pendingJumpChat = nil
        } else if !jumpRequested, let chat = selectedChat, !isDemo {
            jumpRequested = true
            loading = true
            engine.send(["type": "around", "chat": chat, "id": id])
        } else {
            pendingJump = nil
            pendingJumpChat = nil
            error = "That message is not in the downloaded history yet. Load earlier messages and try again."
        }
    }

    func search(_ query: String) {
        searchQuery = query
        guard !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { searchHits = []; searching = false; return }
        if isDemo {
            searchHits = chats.flatMap { $0.id == selectedChat ? messages : demoConversations[$0.id] ?? Self.demoMessages(chat: $0.id) }.filter { $0.text.localizedStandardContains(query) }
            searching = false
        } else {
            searching = true
            engine.send(["type": "search", "query": query]) { [weak self] accepted in
                if !accepted { self?.searching = false }
            }
        }
    }

    @discardableResult
    func perform(_ command: [String: Any], online: Bool = true, completion: ((Bool) -> Void)? = nil) -> Bool {
        guard isDemo || !online || connected else {
            error = "Wait for WhatsApp to reconnect and try again."
            completion?(false)
            return false
        }
        if isDemo { completion?(true); return true }
        prepareForBackground()
        engine.send(command) { [weak self] accepted in
            if !accepted { self?.error = "The action was not queued. Please try again." }
            completion?(accepted)
        }
        return true
    }

    func setArchived(_ chat: Chat) {
        if isDemo, let i = chats.firstIndex(where: { $0.id == chat.id }) { chats[i].archived.toggle(); return }
        perform(["type": "archive", "chat": chat.id, "value": !chat.archived])
    }

    func setPinned(_ chat: Chat) {
        if isDemo, let i = chats.firstIndex(where: { $0.id == chat.id }) { chats[i].pinned.toggle(); return }
        perform(["type": "pin", "chat": chat.id, "value": !chat.pinned])
    }

    func setMuted(_ chat: Chat, seconds: TimeInterval?) {
        let until = seconds.map { $0 == 0 ? 0 : Int64(Date().timeIntervalSince1970 + $0) }
        if isDemo, let i = chats.firstIndex(where: { $0.id == chat.id }) { chats[i].mutedUntil = until.map(Double.init); return }
        perform(["type": "mute", "chat": chat.id, "until": until.map { $0 as Any } ?? NSNull()])
    }

    func markChatRead(_ chat: Chat) {
        if isDemo, let i = chats.firstIndex(where: { $0.id == chat.id }) { chats[i].unread = 0; return }
        perform(["type": "read", "chat": chat.id, "receipts": receiptsAllowed(in: chat.id)])
    }

    func react(_ message: Message, emoji: String) {
        guard canPost, canonical(message.chat) == selectedChat, emoji.isEmpty || EmojiCatalog.isEmoji(emoji) else { return }
        let chosen = message.reactionDetails?.contains { $0.from_me && $0.emoji == emoji } == true
        let value = chosen ? "" : emoji
        if isDemo, let index = messages.firstIndex(where: { $0.id == message.id }) {
            var details = messages[index].reactionDetails ?? []
            details.removeAll(where: \.from_me)
            if !value.isEmpty { details.append(.init(sender: "me@lid", from_me: true, emoji: value)) }
            messages[index].reactionDetails = details
            messages[index].reactions = details.map(\.emoji)
            return
        }
        perform(["type": "react", "chat": message.chat, "message": message.id, "emoji": value])
    }

    func submitEdit(_ message: Message, text: String, completion: @escaping (Bool) -> Void) {
        guard message.canEdit, canPost, canonical(message.chat) == selectedChat else { completion(false); return }
        if isDemo, let i = messages.firstIndex(where: { $0.id == message.id }) {
            messages[i].text = text; messages[i].edited = true; editing = nil; completion(true); return
        }
        perform(["type": "edit", "chat": message.chat, "id": message.id, "text": text, "mentions": mentionedIDs(in: text)]) { [weak self] accepted in
            if accepted { self?.editing = nil }
            completion(accepted)
        }
    }

    func delete(_ message: Message, everyone: Bool) {
        guard !everyone || message.canRevoke else { return }
        if isDemo {
            if everyone, let i = messages.firstIndex(where: { $0.id == message.id }) {
                messages[i].kind = "revoked"; messages[i].text = "This message was deleted."
            } else { messages.removeAll { $0.id == message.id } }
            return
        }
        perform(["type": "delete", "chat": message.chat, "id": message.id, "everyone": everyone], online: everyone)
    }

    func forward(_ message: Message, to chat: Chat, completion: @escaping (Bool) -> Void) {
        guard !chat.readOnly else { completion(false); return }
        perform(["type": "forward", "chat": message.chat, "message": message.id, "to": chat.id], completion: completion)
    }

    func composing(_ text: String) {
        guard let chat = selectedChat, preferences.sendTyping, canPost, !isDemo else { return }
        composingWork?.cancel()
        if text.isEmpty { stopComposing(); return }
        if Date().timeIntervalSince(lastComposing) > 4 {
            engine.send(["type": "compose", "chat": chat, "composing": true]); lastComposing = Date()
        }
        let work = DispatchWorkItem { [weak self] in self?.stopComposing(chat: chat) }
        composingWork = work
        DispatchQueue.main.asyncAfter(deadline: .now() + 5, execute: work)
    }

    func stopComposing(chat: String? = nil) {
        composingWork?.cancel(); composingWork = nil
        if !isDemo, lastComposing != .distantPast, let chat = chat ?? selectedChat {
            engine.send(["type": "compose", "chat": chat, "composing": false])
        }
        lastComposing = .distantPast
    }

    func presenceLabel(_ chat: Chat) -> String? {
        let people = (typing[chat.id] ?? [:]).filter { $0.value > Date() }.keys
        if !people.isEmpty { return chat.kind == "group" ? people.map { displayName($0) }.joined(separator: ", ") + " typing…" : "typing…" }
        if let presence = presence[chat.id] {
            if presence.online { return "online" }
            if let seen = presence.lastSeen { return "last seen " + Date(timeIntervalSince1970: seen).formatted(date: .abbreviated, time: .shortened) }
        }
        return chat.kind == "group" ? "\(chat.participants?.count ?? 0) members" : nil
    }

    func scheduleTypingExpiry(now: Date = Date()) {
        typingExpiry?.cancel()
        typing = typing.mapValues { $0.filter { $0.value > now } }.filter { !$0.value.isEmpty }
        guard let next = typing.values.flatMap({ $0.values }).min() else { typingExpiry = nil; return }
        let work = DispatchWorkItem { [weak self] in self?.scheduleTypingExpiry() }
        typingExpiry = work
        DispatchQueue.main.asyncAfter(deadline: .now() + max(0.01, next.timeIntervalSince(now)), execute: work)
    }

    func mentionedIDs(in text: String) -> [String] {
        (currentChat?.participants ?? []).filter {
            let token = NSRegularExpression.escapedPattern(for: $0.components(separatedBy: "@")[0])
            return text.range(of: "(?<![\\p{L}\\p{N}])@" + token + "(?![\\p{L}\\p{N}])", options: .regularExpression) != nil
        }
    }

    func newContact(phone: String, name: String) {
        let number = phone.filter(\.isASCII).filter(\.isNumber)
        guard (7...15).contains(number.count) else { error = "Enter a phone number with its country code."; return }
        newContactBusy = true
        if isDemo {
            let id = number + "@s.whatsapp.net"
            chats.append(Chat(id: id, name: name.isEmpty ? "+" + number : name, kind: "direct", timestamp: Date().timeIntervalSince1970, unread: 0, archived: false, pinned: false, readOnly: false, preview: ""))
            newContactBusy = false; navigate(to: id); return
        }
        perform(["type": "new_contact", "phone": number, "name": name.isEmpty ? NSNull() : name as Any, "to_phone": preferences.saveContactsToPhone]) { [weak self] accepted in
            if !accepted { self?.newContactBusy = false }
        }
    }

    func ensureChat(_ id: String) {
        if isDemo, !chats.contains(where: { $0.id == canonical(id) }) {
            chats.append(Chat(id: canonical(id), name: displayName(id), kind: "direct", timestamp: Date().timeIntervalSince1970, unread: 0, archived: false, pinned: false, readOnly: false, preview: ""))
            demoConversations[canonical(id)] = []
        }
        perform(["type": "ensure", "chat": id, "name": displayName(id)], online: false)
        navigate(to: id)
    }

    func saveContact(id: String, name: String) {
        guard !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        if isDemo { contactNames[id] = name; return }
        perform(["type": "save_contact", "id": id, "name": name, "to_phone": preferences.saveContactsToPhone])
    }

    func fetchFullAvatar(_ id: String) {
        guard !isDemo else { return }
        engine.send(["type": "avatar", "id": id, "full": true])
    }

    func addAttachments(_ urls: [URL], to chat: String) {
        let chat = canonical(chat)
        guard hasSession else { urls.forEach(AttachmentImport.discard); return }
        guard attachments[chat, default: []].count + urls.count <= 30 else {
            urls.forEach(AttachmentImport.discard)
            error = "Send up to 30 files at a time."
            return
        }
        attachments[chat, default: []].append(contentsOf: urls.map { PendingAttachment(url: $0) })
        // The shared file sender does not support a quoted attachment.
        if selectedChat == chat { reply = nil }
    }

    func removeAttachment(_ item: PendingAttachment, from chat: String) {
        attachments[canonical(chat)]?.removeAll { $0.id == item.id }
        AttachmentImport.discard(item.url)
    }

    func sendAttachments(caption: String, completion: @escaping (Bool) -> Void) {
        guard canPost, let chat = selectedChat, let files = attachments[chat], !files.isEmpty else { completion(false); return }
        guard caption.unicodeScalars.count <= 65_536 else { error = "This caption is too long."; completion(false); return }
        perform(["type": "files", "chat": chat, "paths": files.map { $0.url.path }, "caption": caption.isEmpty ? NSNull() : caption as Any, "mentions": mentionedIDs(in: caption)]) { [weak self] accepted in
            if accepted { self?.attachments[chat]?.removeAll { files.contains($0) }; self?.drafts[chat] = "" }
            completion(accepted)
        }
    }

    func play(_ message: Message) {
        guard let url = localURL(message.mediaPath) else { download(message); return }
        guard foreground || isDemo, !audio.hasRecording else { return }
        let request = UUID()
        audioRequest = request
        let receipts = receiptsAllowed(in: message.chat)
        let ready: (URL?) -> Void = { [weak self] playable in
            guard let self, self.audioRequest == request, self.foreground || self.isDemo,
                  self.selectedChat == self.canonical(message.chat) else { return }
            guard let playable else { self.error = "Could not decode this audio. Voice playback supports clips up to 10 minutes."; return }
            Task {
                do {
                    let started = try await self.audio.play(url: playable, id: message.id)
                    if started && !message.fromMe && message.content?.voice_note == true && receipts && !self.isDemo && self.audioRequest == request && self.playedMessages.insert(message.searchIdentity).inserted {
                        self.engine.send(["type": "played", "chat": message.chat, "message": message.id, "sender": message.sender, "receipts": receipts])
                    }
                } catch {
                    guard self.audioRequest == request else { return }
                    self.audio.stopPlayback(); self.error = "Could not play this audio file."
                }
            }
        }
        if url.pathExtension.lowercased() == "ogg" || message.content?.media?.mime.contains("opus") == true {
            engine.prepareAudio(url, completion: ready)
        } else { ready(url) }
    }

    func startVoiceRecording() async {
        guard let chat = selectedChat, canPost, !isDemo else {
            if isDemo { error = "Voice recording is disabled in the offline preview." }
            return
        }
        await audio.startRecording(chat: chat, quote: reply?.id) { [weak self] in
            self?.selectedChat == chat && self?.foreground == true && self?.canPost == true
        }
    }

    func sendVoice(completion: @escaping (Bool) -> Void) {
        guard !voiceSending, canPost, let chat = selectedChat, audio.recordingChat.map(canonical) == chat else { completion(false); return }
        voiceSending = true
        let recording = audio.recordingID
        let quote = audio.quotedMessageID
        Task {
            do {
                guard let path = try await audio.finishRecording(), canPost, selectedChat == canonical(chat),
                      recording == audio.recordingID, audio.recordingChat.map(canonical) == canonical(chat) else { voiceSending = false; completion(false); return }
                var command: [String: Any] = ["type": "voice", "chat": chat, "path": path.path]
                if let quote { command["quoting"] = quote }
                perform(command) { accepted in
                    self.voiceSending = false
                    if accepted, self.audio.recordingID == recording { self.audio.discardRecording() }
                    if accepted, self.reply?.id == quote { self.reply = nil }
                    completion(accepted)
                }
            } catch { voiceSending = false; self.error = "Could not prepare the recording. Your recording has not been sent."; completion(false) }
        }
    }

    func autoDownload(_ message: Message) {
        guard preferences.autoDownload, message.mediaPath == nil, message.mediaState == "idle",
              let size = message.content?.media?.size, size <= 64 * 1024 * 1024 else { return }
        download(message)
    }

    func loadStickers() { perform(["type": "stickers"], online: false) }
    func sendSticker(_ path: String) {
        guard canPost, let chat = selectedChat else { return }
        perform(["type": "sticker", "chat": chat, "path": path])
    }
    func saveSticker(_ path: String, remove: Bool = false) {
        perform(["type": remove ? "forget_sticker" : "save_sticker", "path": path], online: false)
    }
    func importStickers(url: String) { perform(["type": "import_sticker_url", "url": url], online: false) }
    func importStickers(file: URL) { perform(["type": "import_sticker_archive", "path": file.path], online: false) }
    func deletePack(_ pack: StickerPack) { perform(["type": "delete_sticker_pack", "path": pack.dir], online: false) }
    func searchGifs(_ query: String) {
        gifQuery = query; gifError = nil; gifsLoading = true
        guard !preferences.giphyKey.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { gifs = []; gifsLoading = false; return }
        perform(["type": "gifs", "query": query, "key": preferences.giphyKey], online: false) { [weak self] accepted in
            if !accepted || self?.isDemo == true { self?.gifsLoading = false }
        }
    }
    func sendGif(_ gif: GifItem) {
        guard canPost, let chat = selectedChat else { return }
        perform(["type": "gif", "chat": chat, "id": gif.id, "mp4": gif.mp4, "width": gif.width, "height": gif.height])
    }
    func unlink() { perform(["type": "unlink"]) }
}
