import XCTest
@testable import Whatsapp

@MainActor
final class ChatStoreTests: XCTestCase {
    private func store() -> ChatStore {
        let defaults = UserDefaults(suiteName: "WhatsappTests-\(UUID().uuidString)")!
        return ChatStore(demo: true, defaults: defaults)
    }

    func testHistoryReplayDeduplicatesAndPreservesOrder() {
        let first = ChatStore.sampleMessage(id: "1", chat: "test@lid", text: "First", fromMe: false, time: 1)
        var second = ChatStore.sampleMessage(id: "2", chat: "test@lid", text: "Second\n👋", fromMe: true, time: 2, status: "sent")
        let old = second
        second.status = "read"
        let result = ConversationMessages.merge([old], [second, first])
        XCTAssertEqual(result.map(\.id), ["1", "2"])
        XCTAssertEqual(result[1].status, "read")
        XCTAssertEqual(result[1].text, "Second\n👋")
    }

    func testLiveEventDoesNotCompleteArchiveRequestOrLoadInactiveChat() {
        let store = store()
        store.open("test@lid")
        store.messages = []
        store.loading = true
        store.archiveComplete = false
        let message = ChatStore.sampleMessage(id: "m", chat: "test@lid", text: "Live", fromMe: false, time: 1)
        var event = CoreEvent(type: "messages")
        event.chat = "test@lid"; event.messages = [message]; event.requested = false; event.complete = true
        store.apply([event])
        XCTAssertTrue(store.loading)
        XCTAssertFalse(store.archiveComplete)
        XCTAssertEqual(store.messages.count, 1)
        event.chat = "elsewhere@lid"
        store.apply([event])
        XCTAssertEqual(store.messages.count, 1)
        event.chat = "test@lid"; event.requested = true
        store.apply([event])
        XCTAssertFalse(store.loading)
        XCTAssertTrue(store.archiveComplete)
    }

    func testPrivacyIDMergeMovesSelectionAndDraft() {
        let store = store()
        store.open("old@lid")
        store.drafts["old@lid"] = "Unsent text"
        var event = CoreEvent(type: "merged")
        event.from = "old@lid"; event.into = "123@s.whatsapp.net"
        store.apply([event])
        XCTAssertEqual(store.selectedChat, "123@s.whatsapp.net")
        XCTAssertEqual(store.canonical("old@lid"), "123@s.whatsapp.net")
        XCTAssertEqual(store.drafts["123@s.whatsapp.net"], "Unsent text")
        XCTAssertTrue(store.messages.allSatisfy { $0.chat == "123@s.whatsapp.net" })
    }

    func testLoadFailureAllowsRetryAndReceiptDoesNotInsertUnloadedMessages() {
        let store = store()
        store.open("test@lid")
        store.messages = []
        store.loading = true
        var receipt = CoreEvent(type: "message")
        receipt.message = ChatStore.sampleMessage(id: "unloaded", chat: "test@lid", text: "Old", fromMe: true, time: 1)
        var failed = CoreEvent(type: "load_failed")
        failed.chat = "test@lid"; failed.detail = "Fixture failure"
        store.apply([receipt, failed])
        XCTAssertTrue(store.messages.isEmpty)
        XCTAssertFalse(store.loading)
        XCTAssertEqual(store.error, "Fixture failure")
    }

    func testBridgeBatchDecodesPagingAndNullFields() throws {
        let data = Data(#"{"version":1,"events":[{"type":"messages","chat":"test@lid","messages":[],"requested":true,"older":false,"complete":true},{"type":"link","status":"unlinked","qr":null,"code":null}]}"#.utf8)
        let batch = try JSONDecoder().decode(CoreBatch.self, from: data)
        XCTAssertEqual(batch.version, 1)
        XCTAssertEqual(batch.events[0].requested, true)
        XCTAssertNil(batch.events[1].qr)
    }

    func testLatePhoneHistoryReopensLocalPagingEvenWhenPhoneIsExhausted() {
        let store = store()
        store.open("test@lid")
        store.archiveComplete = true
        var event = CoreEvent(type: "older")
        event.chat = "test@lid"; event.more = false
        store.apply([event])
        XCTAssertTrue(store.phoneComplete)
        XCTAssertFalse(store.archiveComplete)
    }

    func testRejectedSendCompletesWithoutLosingDraft() {
        let store = store()
        store.open("test@lid")
        store.drafts["test@lid"] = "Saved draft"
        let originalCount = store.messages.count
        var accepted: Bool?
        store.sendText(String(repeating: "x", count: 65_537)) { accepted = $0 }
        XCTAssertEqual(accepted, false)
        XCTAssertEqual(store.drafts["test@lid"], "Saved draft")
        XCTAssertEqual(store.messages.count, originalCount)
    }
}
