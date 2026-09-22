#if DEBUG || BENCHMARK
import Foundation

extension ChatStore {
    func loadPerformanceDemo() {
        guard isDemo else { return }
        let now = Date().timeIntervalSince1970
        chats = (0..<1_000).map { index in
            Chat(id: "performance-\(index)@g.us", name: "Fixture chat \(index)", kind: "group",
                 timestamp: now - Double(index), unread: 0, archived: false, pinned: false,
                 readOnly: false, preview: "Offline performance fixture", participants: ["maya@lid", "me@lid"])
        }
        demoConversations[chats[0].id] = (0..<3_000).map { index in
            var message = Self.sampleMessage(id: "performance-message-\(index)", chat: chats[0].id,
                text: "*Plan \(index)* — a walk and coffee ☕️\nAsk @maya or see https://example.com/\(index). _See you soon!_",
                fromMe: index % 3 == 0, time: now - Double(3_000 - index) * 40)
            message.mentions = [.init(user: "maya", id: "maya@lid")]
            return message
        }
    }
}
#endif
