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
        guard !isDemo, !selectionActive, pendingJump == nil, reply == nil, editing == nil,
              !messages.contains(where: { $0.status == "pending" || $0.mediaState == "downloading" }) else { return }
        let anchor = scrollTarget ?? visibleMessageIDs.first
        let bounds = ConversationWindow.bounds(messages, anchor: anchor, towardOlder: towardOlder)
        guard bounds.count < messages.count else { return }
        // Never trim any visible row, even during a fast gesture.
        let retainedIDs = Set(messages[bounds].map(\.id))
        guard visibleMessageIDs.allSatisfy(retainedIDs.contains) else { return }
        if bounds.lowerBound > 0 { archiveComplete = false }
        if bounds.upperBound < messages.count { newerComplete = false }
        if let anchor, retainedIDs.contains(anchor) { restoredAnchor = anchor }
        messages = Array(messages[bounds])
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
