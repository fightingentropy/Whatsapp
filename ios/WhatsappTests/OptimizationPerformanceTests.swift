import XCTest
import Observation
import UIKit
@testable import Whatsapp

// Reproducible synthetic scaling probes; never starts a messaging engine.
@MainActor
final class OptimizationPerformanceTests: XCTestCase {
    private func samples(_ name: String, rounds: Int = 7, _ work: () -> Void) {
        work()
        var values: [Double] = []
        for _ in 0..<rounds {
            let start = CFAbsoluteTimeGetCurrent()
            work()
            values.append((CFAbsoluteTimeGetCurrent() - start) * 1000)
        }
        let sorted = values.sorted()
        print("AUDIT " + name + " " + String(data: try! JSONEncoder().encode([
            "median_ms": sorted[sorted.count / 2], "min_ms": sorted.first!,
            "max_ms": sorted.last!, "rounds": Double(rounds)
        ]), encoding: .utf8)!)
    }

    private func store() -> ChatStore {
        ChatStore(demo: true, defaults: UserDefaults(suiteName: "Audit-\(UUID())")!)
    }

    private func chats(_ count: Int) -> [Chat] {
        (0..<count).map {
            Chat(id: "\(555000000000 + $0)@s.whatsapp.net", name: "Fixture \($0)", kind: "direct",
                 timestamp: Double(count - $0), unread: 0, archived: false, pinned: false,
                 readOnly: false, preview: "Synthetic preview")
        }
    }

    func testContactLookupsAndSearchScale() {
        let store = store()
        store.contacts = (0..<3000).map {
            .init(id: "\(555000000000 + $0)@s.whatsapp.net", name: "Fixture \($0)", fullName: "Fixture \($0)")
        }
        store.chats = (0..<500).map { index in
            Chat(id: store.contacts[(index * 17) % 3000].id, name: "Fixture \(index)", kind: "direct",
                 timestamp: Double(500 - index), unread: 0, archived: false, pinned: false,
                 readOnly: false, preview: "Synthetic preview")
        }
        let index = Dictionary(uniqueKeysWithValues: store.contacts.map { ($0.id, $0.fullName!) })
        var current: [String] = [], indexed: [String] = []
        samples("titles_500_chats_3000_contacts_current") {
            current = store.chats.map(store.chatTitle)
        }
        samples("titles_500_chats_3000_contacts_indexed_prototype") {
            indexed = store.chats.map { index[$0.id]! }
        }
        XCTAssertEqual(current, indexed)
        var matches = 0
        samples("search_500_chats_3000_contacts_current") {
            matches += store.chats.filter {
                store.chatTitle($0).localizedStandardContains("unmatched") || $0.preview.localizedStandardContains("unmatched")
            }.count
        }
        XCTAssertEqual(matches, 0)
    }

    func testActualInterleavedEventShape() {
        let store = store()
        store.selectedChat = nil
        let initial = chats(2000)
        var chatOnly: [CoreEvent] = []
        var interleaved: [CoreEvent] = []
        for n in 0..<100 {
            var chat = initial[n]; chat.timestamp = Double(3000 + n)
            var update = CoreEvent(type: "chats"); update.chats = [chat]
            chatOnly.append(update)
            let message = ChatStore.sampleMessage(id: "message-\(n)", chat: chat.id,
                text: "Synthetic message", fromMe: false, time: Double(3000 + n))
            var page = CoreEvent(type: "messages"); page.chat = chat.id; page.messages = [message]
            var notification = CoreEvent(type: "incoming"); notification.chat = chat.id; notification.message = message
            interleaved.append(contentsOf: [page, update, notification])
        }
        samples("100_adjacent_chat_updates_2000_chats") {
            store.chats = initial; store.apply(chatOnly)
        }
        let reference = store.chats.map(\.id)
        samples("100_interleaved_live_updates_2000_chats") {
            store.chats = initial; store.apply(interleaved)
        }
        XCTAssertEqual(store.chats.map(\.id), reference)
        print("AUDIT event_coalescing " + String(data: try! JSONEncoder().encode([
            "adjacent_output_events": CoreEvent.coalescing(chatOnly).count,
            "interleaved_output_events": CoreEvent.coalescing(interleaved).count
        ]), encoding: .utf8)!)
    }

    func testAvatarUpdateInvalidatesOnlyItsSubscriber() {
        let store = store()
        let counter = AuditCounter()
        for n in 0..<12 {
            withObservationTracking {
                _ = store.avatarURL("\(555000000000 + n)@s.whatsapp.net")
            } onChange: { counter.increment() }
        }
        store.avatars["555000000000@s.whatsapp.net"] = "/synthetic-avatar.jpg"
        print("AUDIT avatar_invalidation {\"observed_rows\":12,\"changed_avatar_ids\":1,\"invalidated_rows\":\(counter.value)}")
        XCTAssertEqual(counter.value, 1)
    }

    func testCachedImageFileChecksAndStartup() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("Audit-\(UUID())", isDirectory: true)
        let avatars = root.appendingPathComponent("cache/avatars", isDirectory: true)
        try FileManager.default.createDirectory(at: avatars, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root); Thumbnails.clear() }
        let format = UIGraphicsImageRendererFormat(); format.scale = 1
        let data = UIGraphicsImageRenderer(size: CGSize(width: 512, height: 512), format: format).jpegData(withCompressionQuality: 0.8) { context in
            UIColor.systemGreen.setFill(); context.fill(CGRect(x: 0, y: 0, width: 512, height: 512))
        }
        for n in 0..<16 {
            try data.write(to: avatars.appendingPathComponent("\(555000000000 + n)_s_whatsapp_net.jpg"))
        }
        let url = avatars.appendingPathComponent("555000000000_s_whatsapp_net.jpg")
        XCTAssertNotNil(Thumbnails.prepareStill(url, maximumSize: 156))
        let request = Thumbnails.Request(url, maximumSize: 156)
        var hits = 0
        samples("1000_warm_thumbnail_lookups_cached_file_revision") {
            for _ in 0..<1000 {
                if Thumbnails.cached(Thumbnails.Request(url, maximumSize: 156, revision: MediaFiles.revision(url))) != nil { hits += 1 }
            }
        }
        samples("1000_warm_thumbnail_lookups_precomputed_request") {
            for _ in 0..<1000 { if Thumbnails.cached(request) != nil { hits += 1 } }
        }
        XCTAssertEqual(hits, 16000)
        var event = CoreEvent(type: "chats"); event.chats = chats(2000)
        var prepared: [CoreEvent] = []
        samples("restore_2000_chats_16_warm_portraits") {
            prepared = CachedMedia.prepare([event], root: root)
        }
        XCTAssertEqual(prepared.filter { $0.type == "avatar" }.count, 16)
    }
}

private final class AuditCounter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0
    var value: Int { lock.lock(); defer { lock.unlock() }; return count }
    func increment() { lock.lock(); count += 1; lock.unlock() }
}
