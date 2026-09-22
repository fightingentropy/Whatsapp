import XCTest
@testable import Whatsapp

@MainActor
final class MessagePresentationTests: XCTestCase {
    private var calendar: Calendar {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(secondsFromGMT: 0)!
        return calendar
    }
    private func message(at timestamp: TimeInterval, sender: String = "maya@lid", fromMe: Bool = false) -> Message {
        Message(id: "fixture", chat: "preview@g.us", sender: sender, senderName: nil, fromMe: fromMe, timestamp: timestamp, kind: "text", text: "Hello", status: "read", edited: false, mediaPath: nil, hasMedia: false, mediaState: "idle", mediaError: nil, quote: nil, reactions: [])
    }

    func testConsecutiveMessagesGroupOnlyWithinFiveMinutes() {
        let first = message(at: 1_800_000_000)
        XCTAssertTrue(MessageGrouping.joins(first, message(at: first.timestamp + 300), calendar: calendar))
        XCTAssertFalse(MessageGrouping.joins(first, message(at: first.timestamp + 301), calendar: calendar))
        XCTAssertFalse(MessageGrouping.joins(first, message(at: first.timestamp - 1), calendar: calendar))
    }

    func testSenderChatAndRevocationBreakGroups() {
        let first = message(at: 1_800_000_000)
        var next = message(at: first.timestamp + 30, sender: "alex@lid")
        XCTAssertFalse(MessageGrouping.joins(first, next, calendar: calendar))
        next = message(at: first.timestamp + 30); next.chat = "other@g.us"
        XCTAssertFalse(MessageGrouping.joins(first, next, calendar: calendar))
        next = message(at: first.timestamp + 30); next.kind = "revoked"
        XCTAssertFalse(MessageGrouping.joins(first, next, calendar: calendar))
        next = message(at: first.timestamp + 30, fromMe: true)
        XCTAssertFalse(MessageGrouping.joins(first, next, calendar: calendar))
    }

    func testMidnightSeparatesMessagesAndUsesRelativeDates() {
        let midnight = calendar.date(from: DateComponents(year: 2026, month: 9, day: 22))!
        XCTAssertFalse(MessageGrouping.joins(message(at: midnight.timeIntervalSince1970 - 1), message(at: midnight.timeIntervalSince1970), calendar: calendar))
        XCTAssertEqual(ChatDate.day(midnight.timeIntervalSince1970, now: midnight, calendar: calendar), "Today")
        XCTAssertEqual(ChatDate.day(midnight.timeIntervalSince1970 - 1, now: midnight, calendar: calendar), "Yesterday")
        XCTAssertEqual(ChatDate.preview(midnight.timeIntervalSince1970 - 1, now: midnight, calendar: calendar), "Yesterday")
    }

    func testChatsWithoutHistoryDoNotShowAnInventedDate() {
        XCTAssertEqual(ChatDate.preview(0), "")
        XCTAssertEqual(ChatDate.preview(.nan), "")
    }
}
