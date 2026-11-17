import Foundation

extension ChatStore {
    func trackFrame(_ id: String, chat: String, frame: CGRect?) {
        guard canonical(chat) == selectedChat, visibleMessageFrames[id] != frame else { return }
        visibleMessageFrames[id] = frame
        updateVisibleRows()
    }

    func trackViewport(_ rect: CGRect, chat: String) {
        guard canonical(chat) == selectedChat, messageViewport != rect else { return }
        messageViewport = rect
        updateVisibleRows()
    }

    private func updateVisibleRows() {
        // Lazy rows report geometry only when laid out. Scrolling checks their
        // cached frames, without rewalking message history or tracking all targets.
        visibleMessageIDs = visibleMessageFrames.filter {
            $0.value.intersects(messageViewport) && $0.value.height > 0
        }.sorted {
            $0.value.minY == $1.value.minY ? $0.key < $1.key : $0.value.minY < $1.value.minY
        }.map(\.key)
    }

    func rememberConversation(_ chat: String) {
        phoneHistory[chat] = (phoneComplete, phoneRetryAfter)
        guard !loading, !loadingNewer, !fetchingPhone,
              !messages.contains(where: { $0.status == "pending" || $0.mediaState == "downloading" }) else {
            recentConversations.remove(chat); return
        }
        recentConversations.insert(.init(messages: messages, archiveComplete: archiveComplete,
            phoneComplete: phoneComplete, newerComplete: newerComplete, phoneRetryAfter: phoneRetryAfter,
            anchor: visibleMessageIDs.first), for: chat)
    }

    func clearMemoryCaches() {
        recentConversations.removeAll()
        MessageText.clearCache()
        Thumbnails.clear()
        MediaFiles.clear()
        AnimatedMediaPool.shared.clearIdle()
    }

    func trimConversation(towardOlder: Bool) {
        guard !isDemo, !selectionActive, pendingJump == nil else { return }
        let anchor = scrollTarget ?? visibleMessageIDs.first
        let bounds = ConversationWindow.bounds(messages, anchor: anchor, towardOlder: towardOlder)
        guard bounds.count < messages.count else { return }
        // Widen the window to keep visible rows (even during a fast gesture),
        // the message being edited, downloads and recent sends. Archived
        // history can stay "pending" forever, so only recent sends count.
        // The reply quote is held by value and needs no row.
        let now = Date().timeIntervalSince1970
        var protected = Set(visibleMessageIDs)
        if let id = editing?.id { protected.insert(id) }
        let kept = messages.indices.filter { index in
            let message = messages[index]
            return protected.contains(message.id) || message.mediaState == "downloading"
                || (message.fromMe && message.status == "pending" && now - message.timestamp < 600)
        }
        let lower = min(bounds.lowerBound, kept.first ?? bounds.lowerBound)
        let upper = max(bounds.upperBound, (kept.last ?? bounds.upperBound - 1) + 1)
        guard upper - lower < messages.count else { return }
        let retained = lower..<upper
        if retained.lowerBound > 0 { archiveComplete = false }
        if retained.upperBound < messages.count { newerComplete = false }
        if let anchor, messages[retained].contains(where: { $0.id == anchor }) { restoredAnchor = anchor }
        messages = Array(messages[retained])
    }

    func loadNewer() {
        guard let selectedChat, let last = messages.last, !newerComplete, !loading,
              !loadingNewer, !fetchingPhone, !isDemo else { return }
        loadingNewer = true
        requestPage(["type": "newer", "chat": selectedChat, "after": [Int64(last.timestamp), last.id]])
    }

    /// Sends a load/around/newer command. The worker answers each accepted one
    /// with exactly one `messages` (requested), `around`, `newer` or
    /// `load_failed` event, in order; the phone `older` fetch is not a page.
    func requestPage(_ command: [String: Any]) {
        let ticket = UUID()
        pageReplies.append((ticket, true))
        engine.send(command) { [weak self] accepted in
            if !accepted { self?.pageReplies.removeAll { $0.id == ticket } }
        }
    }

    /// Late replies to these requests belong to a window that no longer exists.
    func abandonPageReplies() {
        for index in pageReplies.indices { pageReplies[index].live = false }
    }

    /// Whether the next page reply belongs to the current window. Without an
    /// outstanding request (synthetic fixtures), a reply is accepted.
    func consumePageReply() -> Bool {
        pageReplies.isEmpty ? true : pageReplies.removeFirst().live
    }

    func loadLatest() {
        guard let selectedChat, !isDemo else { return }
        guard !loading, !loadingNewer else { loadLatestAfterPage = true; return }
        loadLatestAfterPage = false
        recentConversations.remove(selectedChat)
        messages = []; visibleMessageIDs = []; visibleMessageFrames = [:]; messageViewport = .zero; newerComplete = true; archiveComplete = false
        loading = true; restoredAnchor = nil
        requestPage(["type": "load", "chat": selectedChat])
    }
}
