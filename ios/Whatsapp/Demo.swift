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

    func loadCachedPicturesDemo() {
        guard isDemo else { return }
        status = "connecting"
        let directory = Self.demoRoot.appendingPathComponent("cache/avatars")
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        for (index, chat) in chats.enumerated() {
            let stem = chat.id.map { $0.isASCII && ($0.isLetter || $0.isNumber) ? $0 : "_" }
            let file = directory.appendingPathComponent(String(stem) + ".jpg")
            guard !FileManager.default.fileExists(atPath: file.path) else { continue }
            // Fictional landscape portraits, retained on disk across launches.
            let image = UIGraphicsImageRenderer(size: CGSize(width: 240, height: 240)).image { context in
                UIColor(hue: CGFloat(index) / 8 + 0.45, saturation: 0.7, brightness: 0.85, alpha: 1).setFill()
                context.fill(CGRect(x: 0, y: 0, width: 240, height: 240))
                UIColor.systemYellow.setFill()
                context.cgContext.fillEllipse(in: CGRect(x: 140, y: 35, width: 56, height: 56))
                UIColor(red: 0.04, green: 0.35, blue: 0.3, alpha: 1).setFill()
                let hill = UIBezierPath(); hill.move(to: CGPoint(x: 0, y: 190))
                hill.addLine(to: CGPoint(x: 80, y: 90)); hill.addLine(to: CGPoint(x: 240, y: 210))
                hill.addLine(to: CGPoint(x: 240, y: 240)); hill.addLine(to: CGPoint(x: 0, y: 240)); hill.close(); hill.fill()
            }
            try? image.jpegData(compressionQuality: 0.8)?.write(to: file, options: .atomic)
        }
        let photo = Message(id: "cached-photo", chat: "weekend@g.us", sender: "maya@lid", senderName: "Maya",
            fromMe: false, timestamp: Date().timeIntervalSince1970, kind: "image", text: "Saved photo — available offline",
            status: "read", edited: false, mediaPath: CachedMedia.avatarURL(root: Self.demoRoot, id: "weekend@g.us")?.path,
            hasMedia: true, mediaState: "idle", mediaError: nil, quote: nil, reactions: [])
        demoConversations["weekend@g.us"] = [photo]
        Thumbnails.clear()
        var chatEvent = CoreEvent(type: "chats"); chatEvent.chats = chats
        var history = CoreEvent(type: "messages"); history.chat = photo.chat; history.messages = [photo]
        apply(CachedMedia.prepare([chatEvent, history], root: Self.demoRoot))
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
            Chat(id: "family@g.us", name: "Family", kind: "group", timestamp: now - 86400, unread: 0, archived: false, pinned: false, readOnly: false, preview: "Dinner on Sunday? 🍝"),
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
        let texts = ["Anyone up for a walk this weekend?", "The forecast looks lovely ☀️", "Absolutely. Saturday morning?", "Perfect! We could grab a coffee first ☕️", "Let's meet at 10 by the park entrance.", "I'll bring my camera 📷", "See you there!", "Sounds good"]
        return texts.enumerated().map { index, text in
            var message = sampleMessage(id: "fixture-\(index)", chat: chat, text: text, fromMe: index == 2 || index == 4 || index == 7, time: now - Double(600 - index * 40))
            if index == 4 { message.reactions = ["👍"]; message.reactionDetails = [.init(sender: "maya@lid", from_me: false, emoji: "👍")] }
            return message
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
