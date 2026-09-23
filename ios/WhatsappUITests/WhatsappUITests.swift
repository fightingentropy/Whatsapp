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

    private func assertLatestMessageIsAboveComposer(_ app: XCUIApplication, file: StaticString = #filePath, line: UInt = #line) {
        let message = app.staticTexts["Sounds good"]
        XCTAssertTrue(message.waitForExistence(timeout: 3), file: file, line: line)
        XCTAssertLessThan(message.frame.maxY, composer(app).frame.minY, "The latest message must be above the composer, not covered by its panel", file: file, line: line)
        XCTAssertGreaterThan(message.frame.minY, app.navigationBars.firstMatch.frame.maxY, file: file, line: line)
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
        app.buttons["attachment-emoji"].tap()
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
        capture(app, "Refined chat list — dark appearance")
        app.buttons["chat-weekend@g.us"].tap()
        XCTAssertTrue(app.staticTexts["Sounds good"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.staticTexts["Sounds good"].isHittable, "The newest message should be visible above the composer when a chat opens")
        capture(app, "Grouped messages — dark appearance")
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

    func testAttachmentPanelAndKeyboardKeepTheDraft() {
        let app = demo()
        app.buttons["chat-weekend@g.us"].tap()
        XCTAssertTrue(app.buttons["composer-camera"].isHittable)
        XCTAssertTrue(app.buttons["composer-stickers"].isHittable)
        XCTAssertTrue(app.buttons["composer-microphone"].isHittable)
        capture(app, "Reference composer — dark and collapsed")
        let input = composer(app); input.tap(); input.typeText("Keep this draft")
        app.buttons["composer-attachments"].tap()
        for action in ["photos", "camera", "document", "paste", "emoji", "gifs", "stickers", "mention"] {
            XCTAssertTrue(app.buttons["attachment-" + action].waitForExistence(timeout: 3))
            XCTAssertTrue(app.buttons["attachment-" + action].isHittable)
        }
        XCTAssertEqual(app.buttons["composer-attachments"].label, "Show keyboard")
        XCTAssertLessThan(input.frame.maxY, app.buttons["attachment-photos"].frame.minY)
        assertLatestMessageIsAboveComposer(app)
        capture(app, "Reference attachment panel — dark")
        app.buttons["composer-attachments"].tap()
        XCTAssertTrue(app.keyboards.firstMatch.waitForExistence(timeout: 3))
        XCTAssertFalse(app.buttons["attachment-photos"].exists)
        XCTAssertEqual(composer(app).value as? String, "Keep this draft")
    }

    func testStickerControlAndGifTileOpenTheirOwnSections() {
        let app = demo()
        app.buttons["chat-weekend@g.us"].tap()
        app.buttons["composer-stickers"].tap()
        XCTAssertTrue(app.buttons["Import a .wastickers or ZIP file"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.segmentedControls.buttons["Stickers"].isSelected)
        app.buttons["Done"].tap()
        app.buttons["composer-attachments"].tap()
        app.buttons["attachment-gifs"].tap()
        XCTAssertTrue(app.staticTexts["Connect GIPHY"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.segmentedControls.buttons["GIFs"].isSelected)
    }

    func testOfflineCameraKeepsPreviewPrivateAndDraftIntact() {
        let app = demo()
        app.buttons["chat-weekend@g.us"].tap()
        let input = composer(app); input.tap(); input.typeText("Camera draft")
        app.buttons["composer-camera"].tap()
        XCTAssertTrue(app.alerts.staticTexts["The offline preview does not open your camera."].waitForExistence(timeout: 3))
        app.alerts.buttons["OK"].tap()
        XCTAssertEqual(composer(app).value as? String, "Camera draft")
    }

    func testLightAppearanceAndLargeMessageText() {
        let app = demo()
        app.tabBars.buttons["Settings"].tap()
        capture(app, "Settings — dark appearance")
        app.buttons["Appearance"].tap()
        XCTAssertTrue(app.navigationBars["Appearance"].waitForExistence(timeout: 3))
        app.buttons["appearance-theme"].tap()
        app.buttons["Light"].tap()
        app.sliders["Message text size"].adjust(toNormalizedSliderPosition: 1)
        capture(app, "Appearance — live message preview")
        app.tabBars.buttons["Chats"].tap()
        capture(app, "Refined chat list — light appearance")
        app.buttons["chat-weekend@g.us"].tap()
        XCTAssertTrue(app.staticTexts["Sounds good"].waitForExistence(timeout: 5))
        assertLatestMessageIsAboveComposer(app)
        capture(app, "Grouped messages — light appearance and large text")
        app.buttons["composer-attachments"].tap()
        XCTAssertTrue(app.buttons["attachment-photos"].isHittable)
        assertLatestMessageIsAboveComposer(app)
        capture(app, "Reference attachment panel — light appearance")
        app.buttons["composer-attachments"].tap()
        let input = composer(app)
        input.tap(); input.typeText("A longer draft that should grow naturally across several lines without hiding either the attachment or send controls.")
        XCTAssertTrue(app.buttons["send-message"].isHittable)
        XCTAssertTrue(app.buttons["composer-attachments"].isHittable)
        capture(app, "Multiline composer — light appearance and large text")
    }
}
