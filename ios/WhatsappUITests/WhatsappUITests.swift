import XCTest

final class WhatsappUITests: XCTestCase {
    func testNativeChatAndOfflineComposer() {
        let app = XCUIApplication()
        app.launchArguments = ["--demo"]
        app.launch()
        XCTAssertTrue(app.navigationBars["Chats"].waitForExistence(timeout: 10))
        app.buttons["chat-weekend@g.us"].tap()
        let composer = app.textFields["message-composer"]
        let multiline = app.textViews["message-composer"]
        let input = composer.waitForExistence(timeout: 3) ? composer : multiline
        XCTAssertTrue(input.waitForExistence(timeout: 3))
        input.tap()
        input.typeText("Offline test reply")
        XCTAssertEqual(input.value as? String, "Offline test reply")
        app.buttons["send-message"].tap()
        XCTAssertTrue(app.staticTexts["Offline test reply"].waitForExistence(timeout: 5))
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "Native iPhone conversation — offline fixture"
        attachment.lifetime = .keepAlways
        add(attachment)
    }
}
