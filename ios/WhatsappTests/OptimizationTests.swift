import XCTest
import Observation
@testable import Whatsapp

@MainActor
final class OptimizationTests: XCTestCase {
    func testIdentityMergePreservesPortraitAndQuoteMentionNames() throws {
        let store = ChatStore(demo: true)
        store.contactNames["old@lid"] = "Taylor"
        store.avatars["old@lid"] = "/fixture/photo.jpg"
        store.avatarRevisions["old@lid"] = "revision"
        var merge = CoreEvent(type: "merged"); merge.from = "old@lid"; merge.into = "555@s.whatsapp.net"
        store.apply([merge])
        XCTAssertEqual(store.identity("555@s.whatsapp.net").avatar, "/fixture/photo.jpg")
        XCTAssertEqual(store.identity("555@s.whatsapp.net").revision, "revision")
        let quote = try JSONDecoder().decode(Message.Quote.self, from: Data(#"{"id":"m","sender":"old@lid","senderName":"Taylor","text":"Ask @old","mentions":[{"user":"old","id":"old@lid"}]}"#.utf8))
        let names = store.mentionNames(quote.mentions)
        XCTAssertEqual(names["old"], "Taylor")
        XCTAssertEqual(String(MessageText.render(quote.text, mentions: names, size: 13).characters), "Ask @Taylor")
        let legacy = try JSONDecoder().decode(Message.Quote.self, from: Data(#"{"id":"m","sender":"Taylor","text":"Old quote"}"#.utf8))
        XCTAssertNil(legacy.mentions)
    }

    func testIdentityUpdatesOnlyInvalidateReadersOfThatIdentity() {
        let store = ChatStore(demo: true)
        let changed = OptimizationChange(), unrelated = OptimizationChange()
        withObservationTracking { _ = store.avatarURL("a@lid") } onChange: { changed.set() }
        withObservationTracking { _ = store.avatarURL("b@lid") } onChange: { unrelated.set() }
        store.avatars["a@lid"] = "/synthetic/portrait.jpg"
        XCTAssertTrue(changed.value)
        XCTAssertFalse(unrelated.value)
        let otherName = OptimizationChange()
        withObservationTracking { _ = store.displayName("b@lid") } onChange: { otherName.set() }
        store.contacts = [.init(id: "a@lid", name: "Alex", fullName: "Alex")]
        XCTAssertEqual(store.displayName("a@lid"), "Alex")
        XCTAssertFalse(otherName.value)
    }

    func testNameIndexHandlesUpdatesRemovalPreferencesAndAliasMerge() {
        let store = ChatStore(demo: true)
        store.contacts = [.init(id: "123@lid", name: "Old", fullName: "Old"),
                          .init(id: "555@s.whatsapp.net", name: "Saved", fullName: "Saved", pushName: "Public")]
        store.aliases["123@lid"] = "555@s.whatsapp.net"
        XCTAssertEqual(store.displayName("123@lid"), "Saved")
        store.preferences.contactNames = false
        XCTAssertEqual(store.displayName("123@lid"), "Public")
        store.contacts = []
        XCTAssertEqual(store.displayName("123@lid"), "+555")
    }

    func testInterleavedBatchMatchesSequentialUpdatesAcrossBarriers() {
        let batched = ChatStore(demo: true), sequential = ChatStore(demo: true)
        batched.chats = []; sequential.chats = []
        var events: [CoreEvent] = []
        for n in 0..<100 {
            let chat = Chat(id: "fixture\(n % 10)@g.us", name: "Revision \(n)", kind: "group", timestamp: Double(n), unread: n, archived: false, pinned: n == 95, readOnly: false, preview: "Fixture")
            var message = CoreEvent(type: "messages"); message.chat = chat.id; message.messages = []
            var update = CoreEvent(type: "chats"); update.chats = [chat]
            events += [message, update, CoreEvent(type: "incoming")]
            if n == 50 { var merged = CoreEvent(type: "merged"); merged.from = "fixture0@g.us"; merged.into = "merged@g.us"; events.append(merged) }
            if n == 75 { var page = message; page.requested = true; events.append(page) }
        }
        batched.apply(events)
        for event in events { sequential.apply([event]) }
        XCTAssertEqual(batched.chats, sequential.chats)
        XCTAssertEqual(batched.aliases, sequential.aliases)
        for chat in batched.chats { XCTAssertEqual(batched.chatByID(chat.id), sequential.chatByID(chat.id)) }
    }
}

private final class OptimizationChange: @unchecked Sendable {
    private let lock = NSLock()
    private var changed = false
    var value: Bool { lock.lock(); defer { lock.unlock() }; return changed }
    func set() { lock.lock(); changed = true; lock.unlock() }
}
