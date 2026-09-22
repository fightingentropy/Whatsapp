import XCTest

final class WhatsappUITests: XCTestCase {
    private func demo() -> XCUIApplication {
        let app = XCUIApplication(); app.launchArguments = ["--demo"]; app.launch()
        XCTAssertTrue(app.navigationBars["Chats"].waitForExistence(timeout: 10))
        return app
    }

    private func composer(_ app: XCUIApplication) -> XCUIElement {
        let field = app.textFields["message-composer"]
        return field.waitForExistence(timeout: 2) ? field : app.textViews["message-composer"]
    }

    private func capture(_ app: XCUIApplication, _ name: String) {
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = name; attachment.lifetime = .keepAlways; add(attachment)
    }

    func testEditForwardAndMessageInformationUseNativeControls() {
        let app = demo()
        app.buttons["chat-weekend@g.us"].tap()
        let input = composer(app); input.tap(); input.typeText("Parity fixture")
        app.buttons["send-message"].tap()
        let sent = app.staticTexts["Parity fixture"]
        XCTAssertTrue(sent.waitForExistence(timeout: 5))
        sent.press(forDuration: 1.2)
        XCTAssertTrue(app.buttons["Edit"].waitForExistence(timeout: 3)); app.buttons["Edit"].tap()
        XCTAssertTrue(app.otherElements["editing-message"].exists || app.staticTexts["Editing message"].exists)
        input.tap(); input.typeText(" updated")
        app.buttons["send-message"].tap()
        let edited = app.staticTexts["Parity fixture updated"]
        XCTAssertTrue(edited.waitForExistence(timeout: 5))
        edited.press(forDuration: 1.2)
        app.buttons["Forward"].tap()
        XCTAssertTrue(app.navigationBars["Forward message"].waitForExistence(timeout: 3))
        XCTAssertFalse(app.navigationBars.buttons["Forward"].isEnabled)
        app.buttons["Alex Morgan"].tap()
        XCTAssertTrue(app.navigationBars.buttons["Forward"].isEnabled)
        capture(app, "Choose a forwarding recipient — offline fixture")
        app.buttons["Cancel"].tap()
        edited.press(forDuration: 1.2); app.buttons["Message info"].tap()
        XCTAssertTrue(app.navigationBars["Message info"].waitForExistence(timeout: 3))
        XCTAssertTrue(app.staticTexts["Edited"].exists)
        capture(app, "Message information — offline fixture")
        app.buttons["Done"].tap()
    }

    func testEmojiCompletionAndSearchablePicker() {
        let app = demo()
        app.buttons["chat-weekend@g.us"].tap()
        let input = composer(app); input.tap(); input.typeText(":coffee")
        XCTAssertTrue(app.buttons["hot beverage"].waitForExistence(timeout: 3))
        app.buttons["hot beverage"].tap()
        XCTAssertTrue((input.value as? String)?.contains("☕") == true)
        app.buttons["composer-attachments"].tap()
        app.buttons["Emoji, GIFs and stickers"].tap()
        XCTAssertTrue(app.navigationBars["Add to your message"].waitForExistence(timeout: 3))
        let search = app.searchFields.firstMatch
        search.tap(); search.typeText("rocket")
        XCTAssertTrue(app.buttons["rocket"].waitForExistence(timeout: 3))
        capture(app, "Searchable desktop emoji catalog — offline fixture")
        app.buttons["rocket"].tap()
        XCTAssertTrue((input.value as? String)?.contains("🚀") == true)
    }

    func testRichMessageCardsAndGroupControls() {
        let app = demo()
        app.buttons["chat-showcase@g.us"].tap()
        XCTAssertTrue(app.staticTexts["Offline fixture · tap to retry"].waitForExistence(timeout: 5))
        capture(app, "Sticker and media messages — offline fixture")
        app.swipeDown(); app.swipeDown()
        XCTAssertTrue(app.staticTexts["Where should we go?"].exists)
        capture(app, "Formatted messages, link preview and poll — offline fixture")
        app.buttons["Chat information"].tap()
        XCTAssertTrue(app.navigationBars["Group info"].waitForExistence(timeout: 3))
        XCTAssertTrue(app.buttons["Pin chat"].exists)
        XCTAssertTrue(app.staticTexts["Members"].exists)
        capture(app, "Native group information — offline fixture")
    }

    func testInterruptedPairingExplainsRecoveryWithoutAnExpiredCode() {
        let app = XCUIApplication()
        app.launchArguments = ["--demo", "--demo-pairing-interrupted"]
        app.launch()
        XCTAssertTrue(app.staticTexts["pairing-interrupted"].waitForExistence(timeout: 10))
        XCTAssertFalse(app.staticTexts["pairing-code"].exists)
        XCTAssertTrue(app.textFields["pairing-phone"].exists)
        XCTAssertTrue(app.buttons["pairing-submit"].isEnabled)
        let attachment = XCTAttachment(screenshot: app.screenshot())
        attachment.name = "Interrupted pairing recovery — offline fixture"
        attachment.lifetime = .keepAlways
        add(attachment)
    }

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
