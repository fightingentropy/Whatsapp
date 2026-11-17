import XCTest
import Observation
import ImageIO
import UIKit
import UniformTypeIdentifiers
@testable import Whatsapp

@MainActor
final class EfficiencyTests: XCTestCase {
    private func message(_ n: Int, chat: String = "a@lid") -> Message {
        ChatStore.sampleMessage(id: String(format: "%05d", n), chat: chat, text: "Fixture \(n)", fromMe: false, time: Double(n))
    }
    private func fixture() -> (ChatStore, CacheEngine) {
        let engine = CacheEngine()
        let store = ChatStore(defaults: UserDefaults(suiteName: "Efficiency-\(UUID())")!, engine: engine)
        store.activate(); store.open("a@lid")
        var event = CoreEvent(type: "messages"); event.chat = "a@lid"; event.requested = true
        event.messages = (0..<60).map { message($0) }; event.complete = false
        store.apply([event]); return (store, engine)
    }

    func testRecentChatReopensWithoutQueryAndRestoresPosition() {
        let (store, engine) = fixture()
        store.visibleMessageIDs = ["00020", "00021"]
        store.close("a@lid"); let loads = engine.loads
        store.open("a@lid")
        XCTAssertEqual(engine.loads, loads)
        XCTAssertEqual(store.messages.count, 60)
        XCTAssertEqual(store.restoredAnchor, "00020")
        XCTAssertFalse(store.loading)
    }

    func testInactiveMutationInvalidatesSnapshotInsteadOfRevivingDeletedMessage() {
        for type in ["messages", "message", "deleted", "media", "older"] {
            let (store, engine) = fixture()
            store.close("a@lid")
            var event = CoreEvent(type: type); event.chat = "a@lid"; event.id = "00020"
            store.apply([event]); let loads = engine.loads
            store.open("a@lid")
            XCTAssertEqual(engine.loads, loads + 1, type)
            XCTAssertTrue(store.messages.isEmpty, type)
        }
    }

    func testCacheEvictsLeastRecentByCountAndPayloadAndClearsOnMemoryPressure() {
        var cache = ConversationCache(countLimit: 2, byteLimit: 100_000)
        func page(_ n: Int) -> ConversationCache.Page {
            .init(messages: [message(n)], archiveComplete: false, phoneComplete: false,
                  newerComplete: true, phoneRetryAfter: .distantPast, anchor: nil)
        }
        cache.insert(page(1), for: "a"); cache.insert(page(2), for: "b")
        let a = cache.take("a")!; cache.insert(a, for: "a"); cache.insert(page(3), for: "c")
        XCTAssertNil(cache.take("b")); XCTAssertNotNil(cache.take("a"))
        var huge = page(4); huge.messages[0].text = String(repeating: "x", count: 100_000)
        cache.insert(huge, for: "huge"); XCTAssertNil(cache.take("huge"))
        let (store, _) = fixture(); store.close("a@lid"); store.clearMemoryCaches()
        XCTAssertEqual(store.recentConversations.count, 0)
    }

    func testUnfinishedLoadAndInFlightMediaAreNeverCached() {
        let (store, engine) = fixture(); store.messages[0].mediaState = "downloading"
        store.close("a@lid"); let loads = engine.loads; store.open("a@lid")
        XCTAssertEqual(engine.loads, loads + 1)
        store.close("a@lid"); XCTAssertEqual(store.recentConversations.count, 0)
    }

    func testOlderWindowIsBoundedAndDoesNotAppendLiveMessagesAcrossGap() {
        let (store, engine) = fixture()
        store.messages = (100..<700).map { message($0) }
        store.visibleMessageIDs = ["00105", "00106"]
        var older = CoreEvent(type: "messages"); older.chat = "a@lid"; older.older = true; older.requested = true
        older.messages = (40..<100).map { message($0) }; older.complete = false
        store.apply([older])
        XCTAssertLessThanOrEqual(store.messages.count, ConversationWindow.countLimit)
        XCTAssertTrue(store.messages.contains { $0.id == "00105" })
        XCTAssertFalse(store.newerComplete)
        var live = CoreEvent(type: "messages"); live.chat = "a@lid"; live.messages = [message(999)]
        store.apply([live]); XCTAssertFalse(store.messages.contains { $0.id == "00999" })
        let last = store.messages.last!
        store.loadNewer()
        XCTAssertEqual(engine.commands.last?["type"] as? String, "newer")
        var next = CoreEvent(type: "newer"); next.chat = "a@lid"; next.complete = false
        next.messages = (Int(last.timestamp)+1...Int(last.timestamp)+60).map { message($0) }
        store.apply([next]); XCTAssertFalse(store.loadingNewer)
        XCTAssertLessThanOrEqual(store.messages.count, ConversationWindow.countLimit)
    }

    private func page(_ range: Range<Int>, chat: String = "a@lid", older: Bool = false) -> CoreEvent {
        var event = CoreEvent(type: "messages"); event.chat = chat; event.requested = true; event.older = older
        event.messages = range.map { message($0, chat: chat) }; event.complete = false
        return event
    }

    func testStaleOlderPageFromEarlierVisitCannotJoinReopenedWindow() {
        let (store, engine) = fixture()
        store.messages = (100..<280).map { message($0) }; store.archiveComplete = false
        store.loadOlder()
        XCTAssertEqual(engine.commands.last?["type"] as? String, "load")
        store.close("a@lid"); store.open("a@lid")
        XCTAssertTrue(store.loading)
        store.apply([page(40..<100, older: true)])
        XCTAssertTrue(store.messages.isEmpty, "The earlier visit's page is discarded")
        XCTAssertTrue(store.loading)
        store.apply([page(220..<280)])
        XCTAssertEqual(store.messages.map(\.id), (220..<280).map { String(format: "%05d", $0) })
        XCTAssertFalse(store.loading)
    }

    func testStaleJumpWindowIsIgnoredAndJumpIsRetriedOnReopen() {
        let (store, engine) = fixture()
        store.jump(to: "09000")
        XCTAssertEqual(engine.commands.last?["type"] as? String, "around")
        store.close("a@lid"); store.open("a@lid")
        var stale = CoreEvent(type: "around"); stale.chat = "a@lid"; stale.messages = (8970..<9030).map { message($0) }
        stale.complete = false; stale.more = true
        store.apply([stale])
        XCTAssertTrue(store.messages.isEmpty); XCTAssertTrue(store.loading); XCTAssertNil(store.scrollTarget)
        let arounds = engine.commands.filter { $0["type"] as? String == "around" }.count
        store.apply([page(0..<60)])
        XCTAssertEqual(store.messages.count, 60)
        XCTAssertEqual(engine.commands.filter { $0["type"] as? String == "around" }.count, arounds + 1, "The jump is requested again")
        XCTAssertNil(store.error)
    }

    func testMergingOpenOlderWindowReloadsAroundVisibleRow() {
        let (store, engine) = fixture()
        store.newerComplete = false; store.visibleMessageIDs = ["00050"]
        store.loadOlder()
        var merged = CoreEvent(type: "merged"); merged.from = "a@lid"; merged.into = "555@s.whatsapp.net"
        store.apply([merged])
        XCTAssertEqual(store.selectedChat, "555@s.whatsapp.net")
        XCTAssertTrue(store.messages.isEmpty); XCTAssertTrue(store.loading)
        XCTAssertEqual(engine.commands.last?["type"] as? String, "around")
        XCTAssertEqual(engine.commands.last?["chat"] as? String, "555@s.whatsapp.net")
        XCTAssertEqual(engine.commands.last?["id"] as? String, "00050")
        XCTAssertEqual(store.restoredAnchor, "00050")
        store.apply([page(0..<60, chat: "555@s.whatsapp.net", older: true)])
        XCTAssertTrue(store.messages.isEmpty, "A page requested before the merge is discarded")
        var around = CoreEvent(type: "around"); around.chat = "555@s.whatsapp.net"; around.complete = false; around.more = true
        around.messages = (20..<80).map { message($0, chat: "555@s.whatsapp.net") }
        store.apply([around])
        XCTAssertEqual(store.messages.count, 60); XCTAssertFalse(store.loading); XCTAssertFalse(store.newerComplete)
    }

    func testRejectedPageRequestLeavesNoTicket() {
        let (store, engine) = fixture()
        engine.accept = false
        store.loadOlder()
        XCTAssertTrue(store.pageReplies.isEmpty)
        XCTAssertFalse(store.loading, "A refused request leaves no spinner behind")
        engine.accept = true
        store.apply([page(0..<60)])
        XCTAssertEqual(store.messages.count, 60)
    }

    func testSelectionAndQuotedMessagesProtectRowsFromEviction() {
        let (store, _) = fixture(); store.messages = (0..<900).map { message($0) }
        store.selectionActive = true; store.trimConversation(towardOlder: false)
        XCTAssertEqual(store.messages.count, 900)
        store.selectionActive = false; let quote = store.messages[0]; store.reply = quote
        store.trimConversation(towardOlder: false)
        XCTAssertLessThanOrEqual(store.messages.count, ConversationWindow.countLimit, "A quote is kept by value")
        XCTAssertEqual(store.reply, quote)
    }

    func testInFlightRowsAreRetainedWithoutDisablingTrim() {
        let (store, _) = fixture()
        func pending(_ n: Int, time: Double) -> Message {
            ChatStore.sampleMessage(id: String(format: "%05d", n), chat: "a@lid", text: "Fixture", fromMe: true, time: time, status: "pending")
        }
        store.messages = (0..<900).map { message($0) }; store.messages[200].mediaState = "downloading"
        store.trimConversation(towardOlder: false)
        XCTAssertEqual(store.messages.first?.id, "00200", "The window widens to the download")
        XCTAssertEqual(store.messages.last?.id, "00899"); XCTAssertFalse(store.archiveComplete)

        store.messages = (0..<900).map { message($0) }; store.messages[1] = pending(1, time: 1)
        store.trimConversation(towardOlder: false)
        XCTAssertLessThanOrEqual(store.messages.count, ConversationWindow.countLimit, "Archived pending history does not block trimming")

        store.messages = (0..<900).map { message($0) }
        store.messages[100] = pending(100, time: Date().timeIntervalSince1970 - 5)
        store.trimConversation(towardOlder: false)
        XCTAssertEqual(store.messages.first?.id, "00100", "A recent send stays in the window")

        store.messages = (0..<900).map { message($0) }; store.editing = store.messages[10]
        store.trimConversation(towardOlder: false)
        XCTAssertEqual(store.messages.first?.id, "00010", "The edited message stays in the window")
        store.editing = nil; store.trimConversation(towardOlder: false)
        XCTAssertLessThanOrEqual(store.messages.count, ConversationWindow.countLimit)
    }

    func testMessageObservationIgnoresTypingAndConnectionChanges() {
        let (store, _) = fixture(); let changed = ChangeFlag()
        withObservationTracking { _ = store.messages } onChange: { changed.set() }
        store.status = "connected"; store.typing["a@lid"] = ["someone": .distantFuture]
        XCTAssertFalse(changed.value)
        store.messages[0].status = "read"
        XCTAssertTrue(changed.value)
    }

    func testAudioTicksOnlyInvalidateReadersOfPositionAndLevels() {
        let audio = NativeAudio()
        let composer = ChangeFlag(), meter = ChangeFlag()
        withObservationTracking { _ = audio.hasRecording; _ = audio.recordingChat; _ = audio.playingID; _ = audio.isPlaying } onChange: { composer.set() }
        withObservationTracking { _ = audio.levels; _ = audio.recordingDuration } onChange: { meter.set() }
        audio.position = 1; audio.levels = [0.5]; audio.recordingDuration = 1
        XCTAssertFalse(composer.value, "Playback and recording ticks do not redraw the composer or other rows")
        XCTAssertTrue(meter.value)
        audio.playingID = "voice"
        XCTAssertTrue(composer.value)
    }

    func testCachedAvatarCanBeRequestedOffline() {
        let (store, engine) = fixture(); store.status = "connecting"
        store.avatar("a@lid")
        XCTAssertEqual(engine.commands.last?["type"] as? String, "avatar")
    }

    func testRepeatedAvatarRestorationDoesNotRedrawButSamePathRefreshDoes() throws {
        let (store, _) = fixture()
        let file = FileManager.default.temporaryDirectory.appendingPathComponent("Portrait-\(UUID()).jpg")
        defer { try? FileManager.default.removeItem(at: file) }
        try Data([1]).write(to: file)
        var event = CoreEvent(type: "avatar"); event.id = "a@lid"; event.path = file.path
        store.apply([event])
        let changed = ChangeFlag()
        withObservationTracking {
            _ = store.identity("a@lid").avatar; _ = store.identity("a@lid").revision
        } onChange: { changed.set() }
        store.apply([event]); XCTAssertFalse(changed.value)
        try Data([2, 3]).write(to: file)
        store.apply([event]); XCTAssertTrue(changed.value)
    }

    func testQuoteUsesBoundedWindowAndKeepsTargetWhenOlderHistoryIsLarge() {
        let (store, engine) = fixture()
        store.jump(to: "09000")
        XCTAssertEqual(engine.commands.last?["type"] as? String, "around")
        var event = CoreEvent(type: "around"); event.chat = "a@lid"; event.messages = (8970..<9030).map { message($0) }
        event.complete = false; event.more = true
        store.apply([event]); XCTAssertEqual(store.scrollTarget, "09000")
        XCTAssertFalse(store.newerComplete); XCTAssertFalse(store.loading)
    }

    func testPhoneExhaustionSurvivesSnapshotInvalidation() {
        let (store, _) = fixture(); store.phoneComplete = true; store.close("a@lid")
        var changed = CoreEvent(type: "deleted"); changed.chat = "a@lid"; changed.id = "00001"
        store.apply([changed]); store.open("a@lid")
        XCTAssertTrue(store.phoneComplete)
    }

    func testSendingFromOldWindowLoadsLatestInsteadOfCreatingGap() {
        let (store, engine) = fixture(); store.newerComplete = false
        var pending = message(900); pending = ChatStore.sampleMessage(id: pending.id, chat: "a@lid", text: "Fixture", fromMe: true, time: 900, status: "pending")
        var event = CoreEvent(type: "messages"); event.chat = "a@lid"; event.messages = [pending]
        let loads = engine.loads; store.apply([event])
        XCTAssertEqual(engine.loads, loads + 1); XCTAssertTrue(store.newerComplete)
        XCTAssertTrue(store.messages.isEmpty)
    }

    func testSendDuringPagingWaitsForReplyThenLoadsLatest() {
        let (store, engine) = fixture(); store.newerComplete = false; store.loadNewer()
        let pending = ChatStore.sampleMessage(id: "own", chat: "a@lid", text: "Fixture", fromMe: true, time: 900, status: "pending")
        var live = CoreEvent(type: "messages"); live.chat = "a@lid"; live.messages = [pending]
        store.apply([live]); XCTAssertTrue(store.loadLatestAfterPage)
        let loads = engine.loads
        var page = CoreEvent(type: "newer"); page.chat = "a@lid"; page.messages = [message(60)]; page.complete = false
        store.apply([page]); XCTAssertEqual(engine.loads, loads + 1)
        XCTAssertTrue(store.newerComplete); XCTAssertFalse(store.loadLatestAfterPage)
    }

    func testVisibilityCallbacksFromClosedChatCannotMoveCurrentAnchor() {
        let (store, _) = fixture()
        store.trackViewport(CGRect(x: 0, y: 100, width: 300, height: 200), chat: "a@lid")
        store.trackFrame("00020", chat: "a@lid", frame: CGRect(x: 0, y: 150, width: 300, height: 60))
        store.trackFrame("00020", chat: "old@lid", frame: nil)
        XCTAssertEqual(store.visibleMessageIDs, ["00020"])
        store.trackFrame("00010", chat: "a@lid", frame: CGRect(x: 0, y: 80, width: 300, height: 60))
        XCTAssertEqual(store.visibleMessageIDs, ["00010", "00020"])
        store.trackViewport(CGRect(x: 0, y: 160, width: 300, height: 200), chat: "a@lid")
        XCTAssertEqual(store.visibleMessageIDs, ["00020"])
    }

    func testAnimationReservationsAndVideoPlayersAreBounded() async throws {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let frame = UIGraphicsImageRenderer(size: CGSize(width: 32, height: 32)).image { context in
            UIColor.green.setFill(); context.fill(CGRect(x: 0, y: 0, width: 32, height: 32))
        }.cgImage!
        let urls = (0..<5).map { directory.appendingPathComponent("fixture-\($0).gif") }
        for url in urls {
            let destination = try XCTUnwrap(CGImageDestinationCreateWithURL(url as CFURL, UTType.gif.identifier as CFString, 2, nil))
            for _ in 0..<2 { CGImageDestinationAddImage(destination, frame, [kCGImagePropertyGIFDictionary: [kCGImagePropertyGIFDelayTime: 0.1]] as CFDictionary) }
            XCTAssertTrue(CGImageDestinationFinalize(destination))
        }
        let pool = AnimatedMediaPool()
        let clients = (0..<5).map { _ in UUID() }
        var images: [UIImage] = []
        for index in 0..<4 {
            let image = await pool.image(urls[index], client: clients[index])
            images.append(try XCTUnwrap(image))
        }
        let unavailable = await pool.image(urls[4], client: clients[4])
        XCTAssertNil(unavailable); XCTAssertEqual(pool.reservedBytes, 32 * 1024 * 1024)
        let sharedClient = UUID()
        let shared = await pool.image(urls[0], client: sharedClient)
        XCTAssertTrue(shared === images[0], "Concurrent consumers reuse the same decoded frames")
        pool.release(urls[0], client: sharedClient)
        XCTAssertTrue(pool.video(clients[0])); XCTAssertTrue(pool.video(clients[1])); XCTAssertFalse(pool.video(clients[2]))
        pool.releaseVideo(clients[0]); XCTAssertTrue(pool.video(clients[2]))
        for index in 0..<5 { pool.release(urls[index], client: clients[index]) }
        pool.clearIdle(); XCTAssertEqual(pool.reservedBytes, 0)
    }
}

private final class ChangeFlag: @unchecked Sendable {
    private let lock = NSLock()
    private var changed = false
    var value: Bool { lock.withLock { changed } }
    func set() { lock.withLock { changed = true } }
}
private final class CacheEngine: MessagingEngine {
    var onEvents: (([CoreEvent]) -> Void)?
    var onError: ((String) -> Void)?
    var commands: [[String: Any]] = []
    var accept = true
    var loads: Int { commands.filter { $0["type"] as? String == "load" }.count }
    func start(root: URL) {}
    func drain() {}
    func stop(completion: @escaping () -> Void) { completion() }
    func send(_ command: [String: Any], completion: ((Bool) -> Void)?) { commands.append(command); completion?(accept) }
}
