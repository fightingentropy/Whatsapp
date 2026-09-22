import XCTest
@testable import Whatsapp

@MainActor
final class PerformanceTests: XCTestCase {
    override func setUpWithError() throws {
        #if !BENCHMARK
        throw XCTSkip("Run the optimized, offline Benchmark configuration explicitly")
        #endif
    }

    private var options: XCTMeasureOptions {
        let options = XCTMeasureOptions(); options.iterationCount = 5; return options
    }

    func testRepeatedRichTextRendering() {
        let texts = (0..<100).map { "*Plan \($0)*\n- Camera 📷\n> Meet @maya at https://example.com/\($0). _See you soon!_ `literal`" }
        var count = 0
        measure(metrics: [XCTClockMetric()], options: options) {
            for _ in 0..<10 {
                for text in texts { count += MessageText.render(text, mentions: ["maya": "Maya"]).characters.count }
            }
        }
        XCTAssertGreaterThan(count, 0)
    }

    func testFreshRichTextRendering() {
        var iteration = 0
        measure(metrics: [XCTClockMetric()], options: options) {
            iteration += 1
            for index in 0..<200 {
                let text = "*Plan \(iteration)-\(index)*\n- Camera 📷\n> Meet @maya at https://example.com/\(index). _See you soon!_ `literal`"
                XCTAssertFalse(MessageText.render(text, mentions: ["maya": "Maya"]).characters.isEmpty)
            }
        }
    }

    func testLiveUpdatesInLongConversation() {
        let existing = (0..<8_000).map { ChatStore.sampleMessage(id: "\($0)", chat: "fixture@lid", text: "Fixture", fromMe: false, time: Double($0)) }
        let incoming = (8_000..<8_100).map { ChatStore.sampleMessage(id: "\($0)", chat: "fixture@lid", text: "Fixture", fromMe: false, time: Double($0)) }
        measure(metrics: [XCTClockMetric()], options: options) {
            var messages = existing
            for message in incoming { messages = ConversationMessages.merge(messages, [message]) }
            XCTAssertEqual(messages.count, 8_100)
        }
    }

    func testChatEventBurst() {
        let defaults = UserDefaults(suiteName: "PerformanceTests")!
        let store = ChatStore(demo: true, defaults: defaults)
        let chats = (0..<2_000).map { Chat(id: "fixture-\($0)@lid", name: "Fixture", kind: "direct", timestamp: Double($0), unread: 0, archived: false, pinned: false, readOnly: false, preview: "Fixture") }
        let events = (0..<100).map { index in
            var chat = chats[index]; chat.timestamp = Double(2_000 + index)
            var event = CoreEvent(type: "chats"); event.chats = [chat]; return event
        }
        measure(metrics: [XCTClockMetric()], options: options) {
            store.chats = chats
            store.apply(events)
            XCTAssertEqual(store.chats.first?.id, "fixture-99@lid")
        }
    }
}
