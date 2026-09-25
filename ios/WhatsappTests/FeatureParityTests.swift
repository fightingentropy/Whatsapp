import XCTest
@testable import Whatsapp

@MainActor
final class FeatureParityTests: XCTestCase {
    private func fixture() -> (ChatStore, RecordingEngine, UserDefaults) {
        let defaults = UserDefaults(suiteName: "FeatureParity-\(UUID().uuidString)")!
        let engine = RecordingEngine()
        let store = ChatStore(defaults: defaults, engine: engine)
        store.activate()
        store.status = "connected"
        store.hasSession = true
        store.chats = [Chat(id: "fixture@g.us", name: "Fixture", kind: "group", timestamp: 1, unread: 3, archived: false, pinned: false, readOnly: false, preview: "", participants: ["123@lid", "1234@lid"])]
        store.open("fixture@g.us")
        store.loading = false
        engine.commands = []
        return (store, engine, defaults)
    }

    private func message(_ id: String = "message", age: Double = 30) -> Message {
        ChatStore.sampleMessage(id: id, chat: "fixture@g.us", text: "Hello", fromMe: true, time: Date().timeIntervalSince1970 - age)
    }

    func testMessageActionsKeepSourceDestinationAndMentions() {
        let (store, engine, _) = fixture()
        let message = message()
        store.messages = [message]
        store.editing = message
        store.sendText("Hello @123") { XCTAssertTrue($0) }
        XCTAssertEqual(engine.commands.last?["type"] as? String, "edit")
        XCTAssertEqual(engine.commands.last?["id"] as? String, message.id)
        XCTAssertEqual(engine.commands.last?["mentions"] as? [String], ["123@lid"])
        XCTAssertNil(store.editing)
        store.react(message, emoji: "👍")
        XCTAssertEqual(engine.commands.last?["emoji"] as? String, "👍")
        let before = engine.commands.count
        store.react(message, emoji: "not an emoji")
        XCTAssertEqual(engine.commands.count, before)
        var target = store.chats[0]; target = Chat(id: "target@lid", name: "Target", kind: "direct", timestamp: 1, unread: 0, archived: false, pinned: false, readOnly: false, preview: "")
        store.forward(message, to: target) { XCTAssertTrue($0) }
        XCTAssertEqual(engine.commands.last?["chat"] as? String, "fixture@g.us")
        XCTAssertEqual(engine.commands.last?["to"] as? String, "target@lid")
        store.delete(message, everyone: false)
        XCTAssertEqual(engine.commands.last?["everyone"] as? Bool, false)
    }

    func testExpiredEditsAndRevokesAndOfflineSendsAreNotQueued() {
        let (store, engine, _) = fixture()
        store.submitEdit(message(age: 901), text: "Changed") { XCTAssertFalse($0) }
        store.delete(message(age: 172_801), everyone: true)
        XCTAssertTrue(engine.commands.isEmpty)
        store.status = "disconnected"
        store.drafts["fixture@g.us"] = "Keep this draft"
        store.sendText("Keep this draft") { XCTAssertFalse($0) }
        XCTAssertTrue(engine.commands.isEmpty)
        XCTAssertEqual(store.drafts["fixture@g.us"], "Keep this draft")
    }

    func testRejectedEditKeepsOriginalDraftAndEditingState() {
        let (store, engine, _) = fixture()
        let message = message()
        store.editing = message
        store.draftBeforeEditing = "Original unsent draft"
        engine.accept = false
        store.sendText("Edited content") { XCTAssertFalse($0) }
        XCTAssertEqual(store.editing?.id, message.id)
        XCTAssertEqual(store.draftBeforeEditing, "Original unsent draft")
    }

    func testReadPrivacyIsResolvedForTheTargetChat() {
        let (store, engine, _) = fixture()
        var privacy = CoreEvent(type: "privacy"); privacy.disabled = true
        store.apply([privacy])
        let direct = Chat(id: "person@lid", name: "Person", kind: "direct", timestamp: 1, unread: 1, archived: false, pinned: false, readOnly: false, preview: "")
        store.markChatRead(direct)
        XCTAssertEqual(engine.commands.last?["receipts"] as? Bool, false)
        store.markChatRead(store.chats[0])
        XCTAssertEqual(engine.commands.last?["receipts"] as? Bool, true)
        store.sendReadReceipts = false
        store.markChatRead(store.chats[0])
        XCTAssertEqual(engine.commands.last?["receipts"] as? Bool, false)
    }

    func testMentionTokensDoNotMatchTheStartOfAnotherNumber() {
        let (store, _, _) = fixture()
        XCTAssertEqual(store.mentionedIDs(in: "Hello @1234, and @123."), ["123@lid", "1234@lid"])
        XCTAssertEqual(store.mentionedIDs(in: "Hello @12345"), [])
        XCTAssertEqual(store.mentionedIDs(in: "email@123"), [])
    }

    func testContactNamesMatchDesktopPreferenceAndUnsavedPhoneTitles() {
        let (store, _, _) = fixture()
        store.contacts = [.init(id: "15550000000@s.whatsapp.net", name: "Saved", fullName: "Saved", pushName: "Public")]
        XCTAssertEqual(store.displayName("15550000000@s.whatsapp.net"), "Saved")
        store.preferences.contactNames = false
        XCTAssertEqual(store.displayName("15550000000@s.whatsapp.net"), "Public")
        store.preferences.contactNames = true
        var direct = store.chats[0]
        direct = Chat(id: "15550000001@s.whatsapp.net", name: "Public", kind: "direct", timestamp: 1, unread: 0, archived: false, pinned: false, readOnly: false, preview: "")
        XCTAssertEqual(store.chatTitle(direct), "+15550000001")
        XCTAssertEqual(store.chatTitle(store.chats[0]), "Fixture")
    }

    func testSearchIgnoresStaleResultsAndJumpsAfterLoadingHistory() {
        let (store, engine, _) = fixture()
        store.search("new query")
        var event = CoreEvent(type: "search"); event.query = "old query"; event.messages = [message("wrong")]
        store.apply([event])
        XCTAssertTrue(store.searchHits.isEmpty)
        event.query = "new query"; event.messages = [message("wanted")]
        store.apply([event])
        XCTAssertEqual(store.searchHits.map(\.id), ["wanted"])
        store.messages = [message("latest")]
        store.navigate(to: "fixture@g.us", message: "wanted")
        XCTAssertEqual(engine.commands.last?["type"] as? String, "around")
        XCTAssertTrue(store.loading)
        var loaded = CoreEvent(type: "around"); loaded.chat = "fixture@g.us"; loaded.messages = [message("wanted")]; loaded.complete = false; loaded.more = true
        store.apply([loaded])
        XCTAssertEqual(store.scrollTarget, "wanted")
        XCTAssertNil(store.pendingJump)
    }

    func testJumpAndDelayedAudioAreCancelledWhenLeavingTheChat() throws {
        let (store, engine, _) = fixture()
        store.messages = [message("latest")]
        store.jump(to: "missing")
        let root = try CoreEngine.storageDirectory()
        var audio = message("voice"); audio.mediaPath = root.appendingPathComponent("cache/media/fixture.ogg").path
        store.play(audio)
        XCTAssertEqual(engine.audioCallbacks.count, 1)
        store.close("fixture@g.us")
        store.open("another@lid")
        engine.audioCallbacks[0](nil)
        XCTAssertNil(store.error, "A cancelled decode must not play or show an error in another chat")
        XCTAssertNil(store.pendingJump)
        XCTAssertNil(store.audio.playingID)
    }

    func testTypingExpiresWithoutAPermanentPollingTimer() {
        let (store, _, _) = fixture()
        var typing = CoreEvent(type: "typing"); typing.chat = "fixture@g.us"; typing.id = "123@lid"; typing.composing = true
        store.apply([typing])
        XCTAssertTrue(store.presenceLabel(store.chats[0])?.contains("typing") == true)
        store.scheduleTypingExpiry(now: Date().addingTimeInterval(13))
        XCTAssertTrue(store.typing.isEmpty)
        XCTAssertNil(store.typingExpiry)
    }

    func testAttachmentLimitsAndFailedQueuePreservePendingFiles() {
        let (store, engine, _) = fixture()
        let urls = (0..<30).map { URL(fileURLWithPath: "/fictional/\($0).jpg") }
        store.reply = message()
        store.addAttachments(urls, to: "fixture@g.us")
        XCTAssertNil(store.reply)
        store.addAttachments([URL(fileURLWithPath: "/fictional/extra.jpg")], to: "fixture@g.us")
        XCTAssertEqual(store.attachments["fixture@g.us"]?.count, 30)
        engine.accept = false
        store.sendAttachments(caption: "My caption") { XCTAssertFalse($0) }
        XCTAssertEqual(store.attachments["fixture@g.us"]?.count, 30)
        engine.accept = true
        store.sendAttachments(caption: "My caption") { XCTAssertTrue($0) }
        XCTAssertEqual(engine.commands.last?["caption"] as? String, "My caption")
        XCTAssertTrue(store.attachments["fixture@g.us"]?.isEmpty == true)
    }

    func testUnlinkCleanupRemovesOnlyNativeTemporaryMedia() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        for name in ["cache/outgoing", "cache/audio-preview", "state/stickers"] {
            let folder = root.appendingPathComponent(name)
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try Data("fixture".utf8).write(to: folder.appendingPathComponent("fixture"))
        }
        AttachmentImport.clearTransientMedia(root: root)
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("cache/outgoing").path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: root.appendingPathComponent("cache/audio-preview").path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.appendingPathComponent("state/stickers/fixture").path))
    }

    func testPrivacyIDMergeKeepsAttachmentsNavigationAndEditing() {
        let (store, _, _) = fixture()
        store.navigation = ["fixture@g.us"]
        store.editing = message()
        store.attachments["fixture@g.us"] = [PendingAttachment(url: URL(fileURLWithPath: "/fictional/photo.jpg"))]
        var merged = CoreEvent(type: "merged"); merged.from = "fixture@g.us"; merged.into = "canonical@g.us"
        store.apply([merged])
        XCTAssertEqual(store.navigation, ["canonical@g.us"])
        XCTAssertEqual(store.editing?.chat, "canonical@g.us")
        XCTAssertEqual(store.attachments["canonical@g.us"]?.count, 1)
    }

    func testLogoutClearsPrivateUIStateAndPreferencesSurviveRelaunch() throws {
        let (store, _, defaults) = fixture()
        store.preferences.theme = "light"; store.preferences.autoDownload = false
        let decoded = try JSONDecoder().decode(Preferences.self, from: XCTUnwrap(defaults.data(forKey: "preferences")))
        XCTAssertEqual(decoded.theme, "light"); XCTAssertFalse(decoded.autoDownload)
        store.accountID = "me@lid"; store.accountAbout = "Fixture"; store.contacts = [.init(id: "person@lid", name: "Fixture")]
        store.searchHits = [message()]; store.avatars = ["person@lid": "/fictional/avatar"]
        var event = CoreEvent(type: "link"); event.status = "logged_out"
        store.apply([event])
        XCTAssertFalse(store.hasSession); XCTAssertNil(store.accountID)
        XCTAssertTrue(store.contacts.isEmpty); XCTAssertTrue(store.searchHits.isEmpty); XCTAssertTrue(store.avatars.isEmpty)
        XCTAssertTrue(store.navigation.isEmpty); XCTAssertEqual(store.preferences.theme, "light")
    }

    func testFormattedTextKeepsCodeLiteralAndCopyResolvesMentions() {
        let plain = MessageText.plain("*Bold* _italic_ ~gone~ `*literal*`\n- Item\n> Quote\n@123 @1234", mentions: ["123": "Maya"])
        XCTAssertEqual(plain, "Bold italic gone *literal*\n• Item\n▎ Quote\n@Maya @1234")
        XCTAssertEqual(MessageText.plain("```- code\n> literal```"), "- code\n> literal")
        let rendered = NSAttributedString(MessageText.render("*_both_*"))
        let font = rendered.attribute(.font, at: 0, effectiveRange: nil) as? UIFont
        XCTAssertTrue(font?.fontDescriptor.symbolicTraits.contains([.traitBold, .traitItalic]) == true)
        var m = message(); m.text = "Hi @123"
        let transcript = MessageText.transcript([m], name: { _ in "You" }, mentions: { _ in ["123": "Maya"] })
        XCTAssertTrue(transcript.hasSuffix("You: Hi @Maya"))
    }

    func testCompleteEmojiCatalogSearchRecentsAndShortcodes() {
        XCTAssertGreaterThan(EmojiCatalog.entries.count, 1_900)
        XCTAssertTrue(EmojiCatalog.search("coffee").contains { $0.emoji == "☕" })
        XCTAssertEqual(EmojiCatalog.search("", recent: ["🎉"]).first?.emoji, "🎉")
        XCTAssertEqual(EmojiCatalog.expandCompletedShortcode("Hello :wave:"), "Hello 👋")
        XCTAssertEqual(EmojiCatalog.expandCompletedShortcode("https://example.com:"), "https://example.com:")
        XCTAssertNil(EmojiCatalog.trailingToken(in: "mail@example.com ", marker: "@"))
        XCTAssertTrue(EmojiCatalog.isEmoji("👨‍👩‍👧‍👦")); XCTAssertFalse(EmojiCatalog.isEmoji("1")); XCTAssertFalse(EmojiCatalog.isEmoji("hello"))
    }

    func testRichMessageBridgeFieldsDecodeTogether() throws {
        let raw = #"{"version":1,"events":[{"type":"messages","chat":"fixture@g.us","messages":[{"id":"voice","chat":"fixture@g.us","sender":"person@lid","senderName":null,"fromMe":false,"timestamp":1,"kind":"audio","text":"Voice message","status":"played","edited":false,"mediaPath":null,"hasMedia":true,"mediaState":"idle","mediaError":null,"quote":null,"reactions":["❤️"],"content":{"kind":"audio","seconds":12,"voice_note":true,"waveform":[0,64,100],"media":{"mime":"audio/ogg","size":512,"width":null,"height":null,"path":null}},"reactionDetails":[{"sender":"me@lid","from_me":true,"emoji":"❤️"}],"deliveredAt":2,"readAt":3,"forwarded":false,"mentions":[]}]}]}"#
        let batch = try JSONDecoder().decode(CoreBatch.self, from: Data(raw.utf8))
        let voice = try XCTUnwrap(batch.events.first?.messages?.first)
        XCTAssertEqual(voice.content?.waveform, [0, 64, 100]); XCTAssertEqual(voice.content?.seconds, 12)
        XCTAssertEqual(voice.reactionDetails?.first?.from_me, true); XCTAssertEqual(voice.readAt, 3)
    }
}

private final class RecordingEngine: MessagingEngine {
    var onEvents: (([CoreEvent]) -> Void)?
    var onError: ((String) -> Void)?
    var commands: [[String: Any]] = []
    var accept = true
    var audioCallbacks: [(URL?) -> Void] = []
    func start(root: URL) {}
    func drain() {}
    func stop(completion: @escaping () -> Void) { completion() }
    func send(_ command: [String: Any], completion: ((Bool) -> Void)?) { commands.append(command); completion?(accept) }
    func prepareAudio(_ url: URL, completion: @escaping (URL?) -> Void) { audioCallbacks.append(completion) }
}
