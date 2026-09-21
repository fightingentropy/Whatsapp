import Foundation

extension ChatStore {
    // Opt-in offline fixture data. No real contacts, session or network is used.
    func loadDemo() {
        status = "connected"
        accountName = "Preview account"
        let now = Date().timeIntervalSince1970
        chats = [
            Chat(id: "weekend@g.us", name: "Weekend plans", kind: "group", timestamp: now - 60, unread: 3, archived: false, pinned: true, readOnly: false, preview: "Maya: See you there! ☀️"),
            Chat(id: "alex@lid", name: "Alex Morgan", kind: "direct", timestamp: now - 900, unread: 0, archived: false, pinned: false, readOnly: false, preview: "That sounds good. Thanks!"),
            Chat(id: "studio@g.us", name: "Studio", kind: "group", timestamp: now - 3600, unread: 1, archived: false, pinned: false, readOnly: false, preview: "The new sketches are ready to look at."),
            Chat(id: "family@g.us", name: "Family", kind: "group", timestamp: now - 7200, unread: 0, archived: false, pinned: false, readOnly: false, preview: "Dinner on Sunday? 🍝"),
            Chat(id: "summer@g.us", name: "Summer trip", kind: "group", timestamp: now - 86400, unread: 0, archived: true, pinned: false, readOnly: false, preview: "A weekend to remember.")
        ]
    }

    static func sampleMessage(id: String, chat: String, text: String, fromMe: Bool, time: TimeInterval, status: String = "read") -> Message {
        Message(id: id, chat: chat, sender: fromMe ? "me@lid" : "maya@lid", senderName: fromMe ? "You" : "Maya", fromMe: fromMe, timestamp: time, kind: "text", text: text, status: status, edited: false, mediaPath: nil, hasMedia: false, mediaState: "idle", mediaError: nil, quote: nil, reactions: [])
    }

    static func demoMessages(chat: String) -> [Message] {
        let now = Date().timeIntervalSince1970
        let texts = ["Anyone up for a walk this weekend?", "Absolutely. Saturday morning?", "Perfect! We could grab a coffee first ☕️", "Let's meet at 10 by the park entrance.", "I'll bring my camera 📷", "Great — see you there! ☀️"]
        return texts.enumerated().map { index, text in
            sampleMessage(id: "fixture-\(index)", chat: chat, text: text, fromMe: index == 1 || index == 3, time: now - Double(3600 - index * 420))
        }
    }
}
