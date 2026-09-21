import Foundation

struct Chat: Codable, Identifiable, Equatable {
    let id: String
    var name: String
    let kind: String
    var timestamp: TimeInterval
    var unread: Int
    var archived: Bool
    let pinned: Bool
    let readOnly: Bool
    var preview: String
}

struct Message: Codable, Identifiable, Equatable {
    let id: String
    var chat: String
    let sender: String
    let senderName: String?
    let fromMe: Bool
    let timestamp: TimeInterval
    let kind: String
    let text: String
    var status: String
    let edited: Bool
    var mediaPath: String?
    let hasMedia: Bool
    var mediaState: String
    var mediaError: String?
    let quote: Quote?
    let reactions: [String]

    struct Quote: Codable, Equatable {
        let id: String
        let sender: String
        let text: String
    }
}

struct CoreBatch: Decodable {
    let version: Int
    let events: [CoreEvent]
}

struct CoreEvent: Decodable {
    let type: String
    var status: String? = nil
    var qr: String? = nil
    var code: String? = nil
    var detail: String? = nil
    var id: String? = nil
    var name: String? = nil
    var chat: String? = nil
    var chats: [Chat]? = nil
    var messages: [Message]? = nil
    var message: Message? = nil
    var older: Bool? = nil
    var complete: Bool? = nil
    var requested: Bool? = nil
    var from: String? = nil
    var into: String? = nil
    var path: String? = nil
    var active: Bool? = nil
    var progress: Int? = nil
    var more: Bool? = nil
    var disabled: Bool? = nil
    var contacts: [Contact]? = nil

    struct Contact: Decodable {
        let id: String
        let name: String?
    }
}

enum ConversationMessages {
    // History replays and live updates share IDs. Do not append duplicates or let a
    // receipt for a message outside this page grow the loaded conversation.
    static func merge(_ existing: [Message], _ incoming: [Message]) -> [Message] {
        var byID = Dictionary(existing.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
        for message in incoming { byID[message.id] = message }
        return byID.values.sorted {
            $0.timestamp == $1.timestamp ? $0.id < $1.id : $0.timestamp < $1.timestamp
        }
    }
}
