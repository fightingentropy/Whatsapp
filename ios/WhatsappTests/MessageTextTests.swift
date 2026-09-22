import XCTest
@testable import Whatsapp

@MainActor
final class MessageTextTests: XCTestCase {
    func testCachedTextChangesWhenBodyMentionNameOrSizeChanges() {
        MessageText.clearCache()
        let first = MessageText.render("Hello @maya", mentions: ["maya": "Maya"], size: 17)
        XCTAssertEqual(first, MessageText.render("Hello @maya", mentions: ["maya": "Maya"], size: 17))
        XCTAssertEqual(String(MessageText.render("Edited @maya", mentions: ["maya": "Maya"]).characters), "Edited @Maya")
        XCTAssertEqual(String(MessageText.render("Hello @maya", mentions: ["maya": "Renamed"]).characters), "Hello @Renamed")
        XCTAssertNotEqual(first, MessageText.render("Hello @maya", mentions: ["maya": "Maya"], size: 24))
        MessageText.clearCache()
        XCTAssertEqual(first, MessageText.render("Hello @maya", mentions: ["maya": "Maya"], size: 17))
    }

    func testCodeKeepsLiteralMarkersMentionsAndLinks() {
        let source = "`*literal* @maya https://example.com` and *bold* _italic_ ~removed~"
        let rendered = MessageText.render(source, mentions: ["maya": "Maya"])
        XCTAssertEqual(String(rendered.characters), "*literal* @maya https://example.com and bold italic removed")
        XCTAssertFalse(rendered.runs.contains { $0.link != nil })
    }

    func testLinksListsAndOverlappingMentionTokensRemainCorrect() {
        let rendered = MessageText.render("- @1234 and @123\n> https://example.com", mentions: ["123": "Alex", "1234": "Maya"])
        XCTAssertEqual(String(rendered.characters), "• @Maya and @Alex\n▎ https://example.com")
        XCTAssertTrue(rendered.runs.contains { $0.link?.absoluteString == "https://example.com" })
    }
}
