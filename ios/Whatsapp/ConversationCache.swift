import Foundation

/// Reusable local pages, never a second archive. Invalidation wins over stale data.
struct ConversationCache {
    struct Page {
        var messages: [Message]
        var archiveComplete: Bool
        var phoneComplete: Bool
        var newerComplete: Bool
        var phoneRetryAfter: Date
        var anchor: String?
        var cost: Int { messages.reduce(0) { $0 + $1.memoryCost } }
    }
    private var pages: [String: Page] = [:]
    private var order: [String] = []
    let countLimit: Int
    let byteLimit: Int
    init(countLimit: Int = 4, byteLimit: Int = 16 * 1024 * 1024) {
        self.countLimit = countLimit; self.byteLimit = byteLimit
    }
    var count: Int { pages.count }
    var cost: Int { pages.values.reduce(0) { $0 + $1.cost } }
    mutating func take(_ id: String) -> Page? {
        order.removeAll { $0 == id }; return pages.removeValue(forKey: id)
    }
    mutating func remove(_ id: String) { _ = take(id) }
    mutating func removeAll() { pages.removeAll(); order.removeAll() }
    mutating func insert(_ page: Page, for id: String) {
        remove(id)
        guard !page.messages.isEmpty, page.cost <= byteLimit else { return }
        pages[id] = page; order.append(id)
        while pages.count > countLimit || cost > byteLimit {
            guard let oldest = order.first else { break }; remove(oldest)
        }
    }
}

extension Message {
    /// Conservative owned-payload estimate; decoded image/audio buffers have separate budgets.
    var memoryCost: Int {
        let strings: [String?] = [id, chat, sender, senderName, kind, text, status, mediaPath, mediaError,
            quote?.id, quote?.sender, quote?.text, content?.kind, content?.file_name, content?.name,
            content?.address, content?.display_name, content?.vcard, content?.question,
            content?.preview?.url, content?.preview?.title, content?.preview?.description, content?.media?.mime]
        let payload = strings.reduce(0) { $0 + ($1?.utf8.count ?? 0) }
            + reactions.reduce(0) { $0 + $1.utf8.count }
            + (content?.options ?? []).reduce(0) { $0 + $1.utf8.count }
            + (mentions ?? []).reduce(0) { $0 + $1.id.utf8.count + $1.user.utf8.count + 64 }
            + (reactionDetails ?? []).reduce(0) { $0 + $1.sender.utf8.count + $1.emoji.utf8.count + 64 }
            + (content?.waveform?.count ?? 0)
        return 1024 + payload * 2
    }
}

/// Keep enough context to scroll smoothly; never discard the visible rows or an active selection.
enum ConversationWindow {
    static let countLimit = 600
    static let byteLimit = 8 * 1024 * 1024
    static func bounds(_ messages: [Message], anchor: String?, towardOlder: Bool) -> Range<Int> {
        guard !messages.isEmpty else { return 0..<0 }
        if messages.count <= countLimit && messages.reduce(0, { $0 + $1.memoryCost }) <= byteLimit { return 0..<messages.count }
        let pivot = anchor.flatMap { id in messages.firstIndex { $0.id == id } }
            ?? (towardOlder ? 0 : messages.count - 1)
        var lower = pivot, upper = pivot + 1, cost = messages[pivot].memoryCost
        // Prefer the direction of travel, with a generous buffer on the other side.
        let preferredLower = max(0, pivot - (towardOlder ? 400 : 200))
        let preferredUpper = min(messages.count, pivot + (towardOlder ? 200 : 400))
        while upper - lower < countLimit {
            let left = lower > preferredLower
            let right = upper < preferredUpper
            guard left || right else { break }
            let takeLeft = left && (!right || (towardOlder ? pivot - lower <= 2 * (upper - pivot) : 2 * (pivot - lower) <= upper - pivot))
            let index = takeLeft ? lower - 1 : upper
            let next = messages[index].memoryCost
            guard cost + next <= byteLimit else { break }
            cost += next
            if takeLeft { lower -= 1 } else { upper += 1 }
        }
        // Small pages are retained whole even if the anchor is at either edge.
        return lower..<upper
    }
}
