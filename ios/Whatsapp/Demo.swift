import UIKit
import ImageIO
import UniformTypeIdentifiers

extension ChatStore {
    static let demoRoot = FileManager.default.temporaryDirectory.appendingPathComponent("WhatsappOfflinePreview", isDirectory: true)
    func loadInterruptedPairingDemo() {
        guard isDemo else { return }
        status = "unlinked"
        hasSession = false
        chats = []
        pairingInterrupted = true
    }

    // Opt-in offline fixture data. No real contacts, session or network is used.
    func loadDemo() {
        status = "connected"
        accountName = "Preview account"
        accountID = "me@lid"
        contacts = [CoreEvent.Contact(id: "maya@lid", name: "Maya", fullName: "Maya", pushName: "Maya"), CoreEvent.Contact(id: "alex@lid", name: "Alex Morgan", fullName: "Alex Morgan", pushName: "Alex")]
        let now = Date().timeIntervalSince1970
        chats = [
            Chat(id: "weekend@g.us", name: "Weekend plans", kind: "group", timestamp: now - 60, unread: 3, archived: false, pinned: true, readOnly: false, preview: "Maya: See you there! ☀️", participants: ["maya@lid", "alex@lid", "me@lid"]),
            Chat(id: "showcase@g.us", name: "Media preview", kind: "group", timestamp: now - 300, unread: 0, archived: false, pinned: false, readOnly: false, preview: "Photos, voice, stickers and message cards", participants: ["maya@lid", "me@lid"]),
            Chat(id: "alex@lid", name: "Alex Morgan", kind: "direct", timestamp: now - 900, unread: 0, archived: false, pinned: false, readOnly: false, preview: "That sounds good. Thanks!"),
            Chat(id: "studio@g.us", name: "Studio", kind: "group", timestamp: now - 3600, unread: 1, archived: false, pinned: false, readOnly: false, preview: "The new sketches are ready to look at."),
            Chat(id: "family@g.us", name: "Family", kind: "group", timestamp: now - 7200, unread: 0, archived: false, pinned: false, readOnly: false, preview: "Dinner on Sunday? 🍝"),
            Chat(id: "summer@g.us", name: "Summer trip", kind: "group", timestamp: now - 86400, unread: 0, archived: true, pinned: false, readOnly: false, preview: "A weekend to remember.")
        ]
        try? FileManager.default.createDirectory(at: Self.demoRoot, withIntermediateDirectories: true)
        Self.makeDemoAnimation()
        savedStickers = [Self.demoRoot.appendingPathComponent("sticker.gif").path]
    }

    static func sampleMessage(id: String, chat: String, text: String, fromMe: Bool, time: TimeInterval, status: String = "read") -> Message {
        Message(id: id, chat: chat, sender: fromMe ? "me@lid" : "maya@lid", senderName: fromMe ? "You" : "Maya", fromMe: fromMe, timestamp: time, kind: "text", text: text, status: status, edited: false, mediaPath: nil, hasMedia: false, mediaState: "idle", mediaError: nil, quote: nil, reactions: [])
    }

    static func demoMessages(chat: String) -> [Message] {
        let now = Date().timeIntervalSince1970
        if chat == "showcase@g.us" { return richDemoMessages(now: now) }
        let texts = ["Anyone up for a walk this weekend?", "Absolutely. Saturday morning?", "Perfect! We could grab a coffee first ☕️", "Let's meet at 10 by the park entrance.", "I'll bring my camera 📷", "Great — see you there! ☀️"]
        return texts.enumerated().map { index, text in
            sampleMessage(id: "fixture-\(index)", chat: chat, text: text, fromMe: index == 1 || index == 3, time: now - Double(3600 - index * 420))
        }
    }

    private static func richDemoMessages(now: TimeInterval) -> [Message] {
        let chat = "showcase@g.us"
        var rich = sampleMessage(id: "rich-text", chat: chat, text: "*Weekend checklist*\n- Camera 📷\n- Coffee ☕️\n> Meet at the park\nAsk @maya for the details. _See you soon!_", fromMe: false, time: now - 240)
        rich.mentions = [.init(user: "maya", id: "maya@lid")]
        rich.reactionDetails = [.init(sender: "me@lid", from_me: true, emoji: "❤️")]; rich.reactions = ["❤️"]
        var link = sampleMessage(id: "rich-link", chat: chat, text: "https://example.com", fromMe: true, time: now - 210)
        link.content = .init(kind: "text", preview: .init(url: "https://example.com", title: "A weekend outdoors", description: "An offline link-preview fixture."))
        link.forwarded = true; link.deliveredAt = now - 200; link.readAt = now - 190
        var poll = sampleMessage(id: "rich-poll", chat: chat, text: "Where should we go?", fromMe: false, time: now - 180)
        poll.content = .init(kind: "poll", question: "Where should we go?", options: ["The park", "The beach", "The museum"])
        var location = sampleMessage(id: "rich-location", chat: chat, text: "Meeting point", fromMe: false, time: now - 150)
        location.content = .init(kind: "location", latitude: 51.5, longitude: -0.1, name: "Meeting point", address: "An offline example")
        var contact = sampleMessage(id: "rich-contact", chat: chat, text: "Maya", fromMe: false, time: now - 120)
        contact.content = .init(kind: "contact", display_name: "Maya", vcard: "BEGIN:VCARD\nVERSION:3.0\nFN:Maya Example\nTEL:+447700900123\nEND:VCARD")
        let sticker = Message(id: "rich-sticker", chat: chat, sender: "maya@lid", senderName: "Maya", fromMe: false, timestamp: now - 90, kind: "sticker", text: "", status: "read", edited: false, mediaPath: demoRoot.appendingPathComponent("sticker.gif").path, hasMedia: true, mediaState: "idle", mediaError: nil, quote: nil, reactions: [], content: .init(kind: "sticker", animated: true, media: .init(mime: "image/gif", size: 512, width: 100, height: 100)))
        let voice = Message(id: "rich-voice", chat: chat, sender: "maya@lid", senderName: "Maya", fromMe: false, timestamp: now - 60, kind: "audio", text: "Voice message", status: "read", edited: false, mediaPath: nil, hasMedia: true, mediaState: "failed", mediaError: "Offline fixture · tap to retry", quote: nil, reactions: [], content: .init(kind: "audio", seconds: 12, voice_note: true, waveform: Array(repeating: 64, count: 64), media: .init(mime: "audio/ogg", size: 512, width: nil, height: nil)))
        return [rich, link, poll, location, contact, sticker, voice]
    }

    private static func makeDemoAnimation() {
        let url = demoRoot.appendingPathComponent("sticker.gif")
        guard !FileManager.default.fileExists(atPath: url.path),
              let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.gif.identifier as CFString, 3, nil) else { return }
        CGImageDestinationSetProperties(destination, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFLoopCount: 0]] as CFDictionary)
        for emoji in ["👋", "😊", "☀️"] {
            let image = UIGraphicsImageRenderer(size: CGSize(width: 100, height: 100)).image { _ in
                (emoji as NSString).draw(at: CGPoint(x: 10, y: 10), withAttributes: [.font: UIFont.systemFont(ofSize: 70)])
            }
            if let image = image.cgImage { CGImageDestinationAddImage(destination, image, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: 0.5]] as CFDictionary) }
        }
        CGImageDestinationFinalize(destination)
    }
}
