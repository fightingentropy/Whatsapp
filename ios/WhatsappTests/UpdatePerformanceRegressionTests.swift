import XCTest
@testable import Whatsapp

@MainActor
final class UpdatePerformanceRegressionTests: XCTestCase {
    private func message(_ id: String, time: Double) -> Message {
        ChatStore.sampleMessage(id: id, chat: "fixture@lid", text: "Fixture \(id)", fromMe: false, time: time)
    }

    func testOrderedMergeMatchesReferenceForReplayPrependEditsAndTimestampChanges() {
        var current: [Message] = []
        for turn in 0..<40 {
            let incoming = (0..<30).map { offset -> Message in
                let index = (turn * 13 + offset * 7) % 120
                var value = message("\(index)", time: Double((index * 17 + turn) % 80))
                value.text = "Revision \(turn)"; value.status = turn % 2 == 0 ? "read" : "sent"
                return value
            }
            var reference = Dictionary(current.map { ($0.id, $0) }, uniquingKeysWith: { _, last in last })
            for value in incoming { reference[value.id] = value }
            let expected = reference.values.sorted { $0.timestamp == $1.timestamp ? $0.id < $1.id : $0.timestamp < $1.timestamp }
            current = ConversationMessages.merge(current, incoming)
            XCTAssertEqual(current, expected)
        }
    }

    func testMergeAcceptsUnsortedInitialSnapshotAndDuplicateIDs() {
        let old = [message("later", time: 3), message("old", time: 1), message("later", time: 4)]
        let new = [message("new", time: 2), message("new", time: 3)]
        let result = ConversationMessages.merge(old, new)
        XCTAssertEqual(result.map(\.id), ["old", "new", "later"])
        XCTAssertEqual(result.map(\.timestamp), [1, 3, 4])
    }

    func testBatchingPreservesPagingDeletionAndIdentityBarriers() {
        func page(_ id: String, requested: Bool = false) -> CoreEvent {
            var event = CoreEvent(type: "messages"); event.chat = "fixture@lid"
            event.messages = [message(id, time: 1)]; event.requested = requested
            return event
        }
        var deleted = CoreEvent(type: "deleted"); deleted.chat = "fixture@lid"; deleted.id = "a"
        var merged = CoreEvent(type: "merged"); merged.from = "fixture@lid"; merged.into = "fixture@s.whatsapp.net"
        let input = [page("a"), page("b"), page("c", requested: true), page("d"), deleted, page("a"), merged, page("e")]
        let output = CoreEvent.coalescing(input)
        XCTAssertEqual(output.count, 7)
        XCTAssertEqual(output[0].messages?.map(\.id), ["a", "b"])
        XCTAssertEqual(output[1].requested, true)
        let batched = ChatStore(demo: true); batched.open("fixture@lid"); batched.messages = []
        let sequential = ChatStore(demo: true); sequential.open("fixture@lid"); sequential.messages = []
        batched.apply(input)
        for event in input { sequential.apply([event]) }
        XCTAssertEqual(batched.messages, sequential.messages)
        XCTAssertEqual(batched.selectedChat, sequential.selectedChat)
        XCTAssertEqual(batched.loading, sequential.loading)
        XCTAssertEqual(batched.archiveComplete, sequential.archiveComplete)
    }

    func testChatsStayPinnedAndOrderedAfterBurstAndUnpin() {
        let store = ChatStore(demo: true); store.chats = []
        var pinned = Chat(id: "p", name: "Pinned", kind: "direct", timestamp: 1, unread: 0, archived: false, pinned: true, readOnly: false, preview: "")
        let recent = Chat(id: "r", name: "Recent", kind: "direct", timestamp: 10, unread: 0, archived: false, pinned: false, readOnly: false, preview: "")
        var a = CoreEvent(type: "chats"); a.chats = [recent]
        var b = CoreEvent(type: "chats"); b.chats = [pinned]
        store.apply([a, b]); XCTAssertEqual(store.chats.map(\.id), ["p", "r"])
        pinned.pinned = false; b.chats = [pinned]
        store.apply([b]); XCTAssertEqual(store.chats.map(\.id), ["r", "p"])
    }

    func testReceiptCanCorrectTimestampWithoutInsertingAnUnloadedMessage() {
        let store = ChatStore(demo: true); store.open("fixture@lid")
        store.messages = [message("a", time: 1), message("b", time: 2)]
        var event = CoreEvent(type: "message"); event.message = message("a", time: 3)
        store.apply([event]); XCTAssertEqual(store.messages.map(\.id), ["b", "a"])
        event.message = message("unloaded", time: 4)
        store.apply([event]); XCTAssertEqual(store.messages.count, 2)
    }
}
