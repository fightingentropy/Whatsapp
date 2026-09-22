import Foundation

struct Chat: Codable, Identifiable, Equatable {
    let id: String
    var name: String
    let kind: String
    var timestamp: TimeInterval
    var unread: Int
    var archived: Bool
    var pinned: Bool
    let readOnly: Bool
    var preview: String
    var mutedUntil: TimeInterval? = nil
    var participants: [String]? = nil

    var muted: Bool { mutedUntil.map { $0 == 0 || $0 > Date().timeIntervalSince1970 } ?? false }
}

struct Message: Codable, Identifiable, Equatable {
    let id: String
    var chat: String
    let sender: String
    let senderName: String?
    let fromMe: Bool
    let timestamp: TimeInterval
    var kind: String
    var text: String
    var status: String
    var edited: Bool
    var mediaPath: String?
    let hasMedia: Bool
    var mediaState: String
    var mediaError: String?
    let quote: Quote?
    var reactions: [String]
    var deliveredAt: TimeInterval? = nil
    var readAt: TimeInterval? = nil
    var forwarded: Bool? = nil
    var mentions: [Mention]? = nil
    var content: RichContent? = nil
    var reactionDetails: [Reaction]? = nil

    var canEdit: Bool { fromMe && kind == "text" && Date().timeIntervalSince1970 - timestamp <= 900 }
    var canRevoke: Bool { fromMe && kind != "revoked" && Date().timeIntervalSince1970 - timestamp <= 172_800 }

    struct Mention: Codable, Equatable { let user: String; let id: String }
    struct Reaction: Codable, Equatable {
        let sender: String
        let from_me: Bool
        let emoji: String
    }

    struct RichContent: Codable, Equatable {
        var kind: String
        var preview: LinkPreview? = nil
        var seconds: Double? = nil
        var gif: Bool? = nil
        var voice_note: Bool? = nil
        var waveform: [UInt8]? = nil
        var animated: Bool? = nil
        var media: Media? = nil
        var file_name: String? = nil
        var latitude: Double? = nil
        var longitude: Double? = nil
        var name: String? = nil
        var address: String? = nil
        var display_name: String? = nil
        var vcard: String? = nil
        var question: String? = nil
        var options: [String]? = nil
    }
    struct LinkPreview: Codable, Equatable { let url: String; let title: String?; let description: String? }
    struct Media: Codable, Equatable { let mime: String; let size: UInt64; let width: Int?; let height: Int? }

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
    var query: String? = nil
    var composing: Bool? = nil
    var online: Bool? = nil
    var lastSeen: TimeInterval? = nil
    var full: Bool? = nil
    var about: String? = nil
    var gifs: [GifItem]? = nil
    var saved: [String]? = nil
    var recent: [String]? = nil
    var packs: [StickerPack]? = nil

    struct Contact: Decodable {
        let id: String
        let name: String?
        var fullName: String? = nil
        var pushName: String? = nil
    }
}

struct GifItem: Codable, Identifiable, Equatable {
    let id: String
    let still: String?
    let mp4: String
    let width: Int
    let height: Int
}

struct StickerPack: Codable, Identifiable, Equatable {
    var id: String { dir }
    let name: String
    let dir: String
    let stickers: [String]
}

struct PendingAttachment: Identifiable, Equatable {
    let id = UUID()
    let url: URL
    var name: String { url.lastPathComponent }
}

struct Preferences: Codable, Equatable {
    var theme = "dark"
    var sendTyping = true
    var autoDownload = true
    var senderPictures = false
    var contactNames = true
    var saveContactsToPhone = false
    var notifications = false
    var giphyKey = ""
    var textSize = 17.0
    var recentEmoji: [String] = []
}

enum ConversationMessages {
    // History replays and live updates share IDs. Do not append duplicates or let a
    // receipt for a message outside this page grow the loaded conversation.
    static func merge(_ existing: [Message], _ incoming: [Message]) -> [Message] {
        OrderedUpdates.merge(existing, incoming) {
            $0.timestamp == $1.timestamp ? $0.id < $1.id : $0.timestamp < $1.timestamp
        }
    }
}
