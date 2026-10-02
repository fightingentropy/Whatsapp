import XCTest
import Observation
import UIKit
@testable import Whatsapp

@MainActor final class RecoveryAndLoadingTests: XCTestCase {
    func testChatObservationIgnoresOtherChatsButTracksChangesRemovalAndAliases() {
        let store = ChatStore(demo: true)
        var chat = store.chats[0], other = store.chats[1]
        let unrelated = RecoveryFlag()
        withObservationTracking { _ = store.chatByID(chat.id) } onChange: { unrelated.set() }
        other.unread += 1
        var event = CoreEvent(type: "chats"); event.chats = [other]; store.apply([event])
        XCTAssertFalse(unrelated.value)
        chat.unread += 1; event.chats = [chat]; store.apply([event])
        XCTAssertTrue(unrelated.value)
        let removal = RecoveryFlag()
        withObservationTracking { _ = store.chatByID(chat.id) } onChange: { removal.set() }
        store.chats.removeAll { $0.id == chat.id }
        XCTAssertTrue(removal.value); XCTAssertNil(store.chatByID(chat.id))
        store.aliases["alias@lid"] = other.id
        XCTAssertEqual(store.chatByID("alias@lid"), other)
    }

    func testVoiceUploadKeepsSourceAndQuoteThroughFailureRelaunchRetryAndAck() async throws {
        // Keep the fixture within the sandbox's outgoing boundary so the real
        // acknowledgement cleanup runs without relaxing its path checks.
        let root = try AttachmentImport.directory().appendingPathComponent("VoiceRecovery-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        let path = root.appendingPathComponent("cache/outgoing/voice.f32")
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
        try Data(repeating: 0, count: 192_000).write(to: path)
        let engine = RecoveryEngine()
        let disk = DraftStorage(root: root)
        let store = ChatStore(defaults: UserDefaults(suiteName: "Voice-\(UUID())")!, engine: engine, draftStorage: disk)
        store.activate(); store.status = "connected"; store.hasSession = true
        let sent = expectation(description: "Voice command queued")
        engine.onSend = { command in if command["type"] as? String == "voice" { sent.fulfill() } }
        let queued = await withCheckedContinuation { continuation in
            store.queueVoice(path, chat: "fixture@g.us", quoting: "quote-id") { continuation.resume(returning: $0) }
        }
        XCTAssertTrue(queued)
        await fulfillment(of: [sent], timeout: 3)
        engine.onSend = nil
        let job = try XCTUnwrap(store.outgoingAttachments.first)
        XCTAssertEqual(engine.commands.last?["request"] as? String, job.id)
        XCTAssertEqual(engine.commands.last?["quoting"] as? String, "quote-id")
        XCTAssertTrue(FileManager.default.fileExists(atPath: path.path), "Queue acceptance cannot discard the source")
        var failed = CoreEvent(type: "attachment"); failed.chat = job.chat; failed.id = job.id; failed.detail = "Synthetic failure"
        store.apply([failed])
        let saved = await withCheckedContinuation { continuation in store.flushDrafts { continuation.resume(returning: $0) } }
        XCTAssertTrue(saved)
        let restartedEngine = RecoveryEngine()
        let restarted = ChatStore(defaults: UserDefaults(suiteName: "Voice-\(UUID())")!, engine: restartedEngine, draftStorage: disk)
        defer { store.draftSaveWork?.cancel(); restarted.draftSaveWork?.cancel() }
        let restored = try XCTUnwrap(restarted.outgoingAttachments.first)
        XCTAssertEqual(restored.state, .failed); XCTAssertEqual(restored.voice, true)
        XCTAssertEqual(restored.quoting, "quote-id"); XCTAssertTrue(restartedEngine.commands.isEmpty)
        restarted.status = "connected"
        let retried = expectation(description: "Explicit retry")
        restartedEngine.onSend = { command in if command["type"] as? String == "voice" { retried.fulfill() } }
        restarted.retryAttachment(restored)
        await fulfillment(of: [retried], timeout: 3)
        XCTAssertEqual(restartedEngine.commands.last?["request"] as? String, job.id)
        XCTAssertTrue(FileManager.default.fileExists(atPath: path.path))
        var ack = failed; ack.messageID = "archived-message"; ack.detail = nil
        restarted.apply([ack])
        XCTAssertTrue(restarted.outgoingAttachments.isEmpty)
        try await eventually { !FileManager.default.fileExists(atPath: path.path) }
    }

    func testRejectedVoiceCommandAndFailedSnapshotKeepTheRecording() async throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("VoiceRejected-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        // A file occupying state/ makes an atomic snapshot impossible.
        try Data([1]).write(to: root.appendingPathComponent("state"))
        let engine = RecoveryEngine()
        let store = ChatStore(defaults: UserDefaults(suiteName: "Voice-\(UUID())")!, engine: engine, draftStorage: DraftStorage(root: root))
        store.status = "connected"
        let path = root.appendingPathComponent("voice.f32"); try Data([0, 0, 0, 0]).write(to: path)
        let saved = await withCheckedContinuation { continuation in store.queueVoice(path, chat: "fixture@g.us", quoting: nil) { continuation.resume(returning: $0) } }
        XCTAssertFalse(saved); XCTAssertTrue(store.outgoingAttachments.isEmpty)
        XCTAssertTrue(engine.commands.isEmpty); XCTAssertTrue(FileManager.default.fileExists(atPath: path.path))
        store.draftSaveWork?.cancel()
        let rejected = ChatStore(defaults: UserDefaults(suiteName: "Voice-\(UUID())")!, engine: engine)
        rejected.status = "connected"; engine.accept = false
        rejected.queueVoice(path, chat: "fixture@g.us", quoting: nil) { XCTAssertTrue($0) }
        XCTAssertEqual(rejected.outgoingAttachments.first?.state, .failed)
        XCTAssertTrue(FileManager.default.fileExists(atPath: path.path))
    }

    func testVoiceSizeLimitRestoresTenMinutePCMAndOlderSnapshotFields() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("VoiceSize-\(UUID())")
        defer { try? FileManager.default.removeItem(at: root) }
        let path = root.appendingPathComponent("cache/outgoing/voice.f32")
        try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
        FileManager.default.createFile(atPath: path.path, contents: nil)
        let file = try FileHandle(forWritingTo: path); defer { try? file.close() }
        try file.truncate(atOffset: UInt64(AttachmentImport.maximumVoiceBytes))
        XCTAssertNotNil(AttachmentImport.restoredURL(path, root: root, maximumSize: AttachmentImport.maximumVoiceBytes))
        XCTAssertNil(AttachmentImport.restoredURL(path, root: root))
        let item = OutgoingAttachment(id: "old", chat: "fixture@g.us", file: .init(url: path), caption: "", mentions: [])
        let decoded = try JSONDecoder().decode(OutgoingAttachment.self, from: JSONEncoder().encode(item))
        XCTAssertNil(decoded.voice); XCTAssertNil(decoded.quoting)
    }

    func testThumbnailReadersShareOneDecodeAndOneCancellationDoesNotCancelOthers() async throws {
        let request = thumbnailRequest()
        let fixture = ThumbnailDecodeFixture()
        let loader = ThumbnailLoader(limit: 1, decode: { await fixture.decode($0) })
        let tasks = (0..<8).map { _ in Task { await loader.load(request) } }
        try await eventually { await loader.pendingReaderCount == 8 }
        tasks[0].cancel()
        let cancelled = await tasks[0].value; XCTAssertNil(cancelled)
        let calls = await fixture.count; XCTAssertEqual(calls, 1)
        await fixture.finish(request)
        var images: [UIImage] = []
        for task in tasks.dropFirst() { let image = await task.value; images.append(try XCTUnwrap(image)) }
        XCTAssertEqual(Set(images.map(ObjectIdentifier.init)).count, 1)
        Thumbnails.clear()
    }

    func testObsoleteThumbnailWorkIsCancelledAndDecodeSlotsStayBounded() async throws {
        let a = thumbnailRequest(), b = thumbnailRequest(), c = thumbnailRequest()
        let fixture = ThumbnailDecodeFixture()
        let loader = ThumbnailLoader(limit: 1, decode: { await fixture.decode($0) })
        let first = Task { await loader.load(a) }
        try await eventually { await fixture.count == 1 }
        let queued = Task { await loader.load(b) }
        try await eventually { await loader.pendingReaderCount == 2 }
        queued.cancel(); first.cancel()
        let cancelled = await first.value, cancelledQueued = await queued.value
        XCTAssertNil(cancelled); XCTAssertNil(cancelledQueued)
        let replacement = Task { await loader.load(c) }
        try await eventually { await loader.pendingReaderCount == 1 }
        let count = await fixture.count; XCTAssertEqual(count, 1, "An uncancellable ImageIO call still owns its slot until it returns")
        await fixture.finish(a)
        try await eventually { await fixture.count == 2 }
        XCTAssertNil(Thumbnails.cached(a)); XCTAssertNil(Thumbnails.cached(b))
        await fixture.finish(c)
        let image = await replacement.value; XCTAssertNotNil(image)
        Thumbnails.clear()
    }

    private func thumbnailRequest() -> Thumbnails.Request { .init(URL(fileURLWithPath: "/synthetic/\(UUID()).jpg"), maximumSize: 156, revision: "fixture") }
    private func eventually(_ condition: () async -> Bool) async throws {
        let end = Date().addingTimeInterval(3)
        while !(await condition()) {
            guard Date() < end else { XCTFail("Asynchronous condition did not complete"); throw CocoaError(.coderInvalidValue) }
            try await Task.sleep(for: .milliseconds(10))
        }
    }
}

private actor ThumbnailDecodeFixture {
    private var waiting: [Thumbnails.Request: CheckedContinuation<UIImage?, Never>] = [:]
    private(set) var count = 0
    func decode(_ request: Thumbnails.Request) async -> UIImage? {
        count += 1
        return await withCheckedContinuation { waiting[request] = $0 }
    }
    func finish(_ request: Thumbnails.Request) { waiting.removeValue(forKey: request)?.resume(returning: UIImage()) }
}
private final class RecoveryFlag: @unchecked Sendable {
    private let lock = NSLock(); private var changed = false
    var value: Bool { lock.withLock { changed } }
    func set() { lock.withLock { changed = true } }
}
private final class RecoveryEngine: MessagingEngine {
    var onEvents: (([CoreEvent]) -> Void)?
    var onError: ((String) -> Void)?
    var commands: [[String: Any]] = []
    var onSend: (([String: Any]) -> Void)?
    var accept = true
    func start(root: URL) {}
    func drain() {}
    func stop(completion: @escaping () -> Void) { completion() }
    func send(_ command: [String: Any], completion: ((Bool) -> Void)?) { commands.append(command); onSend?(command); completion?(accept) }
}
