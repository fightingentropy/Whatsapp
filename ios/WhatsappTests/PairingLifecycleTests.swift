import XCTest
@testable import Whatsapp

@MainActor
final class PairingLifecycleTests: XCTestCase {
    private func fixture() -> (ChatStore, FakeMessagingEngine, FakeBackgroundActivity, UserDefaults) {
        let defaults = UserDefaults(suiteName: "PairingLifecycleTests-\(UUID().uuidString)")!
        let engine = FakeMessagingEngine()
        let activity = FakeBackgroundActivity()
        let store = ChatStore(defaults: defaults, engine: engine, backgroundActivity: activity)
        store.activate()
        return (store, engine, activity, defaults)
    }

    private func codeEvent() -> CoreEvent {
        var event = CoreEvent(type: "link")
        event.status = "unlinked"
        event.code = "fixture-private-code"
        event.qr = "fixture-private-qr"
        return event
    }

    func testPairingAcquiresForegroundAllowanceBeforeRequestingCode() {
        let (store, engine, activity, _) = fixture()
        engine.onSend = { XCTAssertEqual(activity.begun.count, 1) }
        store.pair(phone: "+44 7700 900123")
        XCTAssertEqual(engine.commands, ["pair"])
        store.apply([codeEvent()])
        store.background()
        XCTAssertEqual(activity.begun.count, 1, "Reuse the assertion started in the foreground")
        XCTAssertEqual(engine.stops.count, 0, "Use the available budget, without a 20-second cutoff")
        XCTAssertNotNil(store.pairingCode)
        store.activate()
    }

    func testInterruptedCodeIsRemovedAndLateCodeCannotRestoreIt() {
        let (store, engine, activity, _) = fixture()
        store.apply([codeEvent()])
        store.prepareForBackground()
        activity.timeRemaining = 8
        store.background()
        XCTAssertTrue(store.pairingInterrupted)
        XCTAssertNil(store.pairingCode)
        XCTAssertNil(store.qr)
        XCTAssertFalse(store.pairingBusy)
        XCTAssertEqual(engine.stops.count, 1)
        XCTAssertTrue(activity.ended.isEmpty, "Keep the reserved time until shutdown completes")
        store.apply([codeEvent()])
        XCTAssertNil(store.pairingCode)
        engine.finishStop()
        XCTAssertEqual(store.status, "suspended")
        XCTAssertEqual(activity.ended, activity.begun)
        store.activate()
        var ready = CoreEvent(type: "link")
        ready.status = "unlinked"
        store.apply([ready])
        XCTAssertTrue(store.pairingInterrupted, "Explain why a fresh code is needed after reconnecting")
        store.apply([codeEvent()])
        XCTAssertFalse(store.pairingInterrupted)
        XCTAssertNotNil(store.pairingCode)
    }

    func testOldShutdownCannotSuspendNewForegroundOrEndItsBackgroundAssertion() {
        let (store, engine, activity, _) = fixture()
        store.prepareForBackground()
        activity.timeRemaining = 8
        store.background()
        let old = activity.begun[0]
        store.activate()
        store.prepareForBackground()
        let current = activity.begun[1]
        engine.finishStop()
        XCTAssertEqual(store.status, "connecting")
        XCTAssertEqual(activity.ended, [old])
        XCTAssertFalse(activity.ended.contains(current))
        store.activate()
    }

    func testUnexpectedExpirationEndsAssertionWithoutWaitingForRust() async {
        let (store, engine, activity, _) = fixture()
        store.apply([codeEvent()])
        store.prepareForBackground()
        store.background()
        let stopped = expectation(description: "Shutdown requested")
        engine.onStop = { stopped.fulfill() }
        activity.expirations[0]()
        await fulfillment(of: [stopped], timeout: 2)
        XCTAssertEqual(activity.ended, activity.begun)
        XCTAssertNil(store.pairingCode)
        engine.finishStop()
        XCTAssertEqual(activity.ended.count, 1, "An expired assertion is ended exactly once")
    }

    func testUnavailableAllowanceStopsImmediately() {
        let (store, engine, activity, _) = fixture()
        activity.available = false
        store.prepareForBackground()
        store.background()
        XCTAssertEqual(engine.stops.count, 1)
        engine.finishStop()
        XCTAssertTrue(activity.ended.isEmpty)
    }

    func testExpirationDuringOldShutdownDoesNotStopNewConnection() async {
        let (store, engine, activity, _) = fixture()
        store.prepareForBackground()
        activity.timeRemaining = 8
        store.background()
        store.activate()
        store.prepareForBackground()
        let expired = expectation(description: "Old assertion ended")
        activity.onEnd = { expired.fulfill() }
        activity.expirations[0]()
        await fulfillment(of: [expired], timeout: 2)
        activity.onEnd = nil
        XCTAssertEqual(engine.stops.count, 1)
        XCTAssertEqual(activity.ended, [activity.begun[0]])
        engine.finishStop()
        XCTAssertEqual(activity.ended.count, 1)
        XCTAssertEqual(store.status, "connecting")
        store.activate()
    }

    func testBudgetRechecksUnknownOrExtendedAllowancesAndReservesShutdownTime() {
        XCTAssertEqual(BackgroundActivity.nextCheck(remaining: .infinity), 1)
        XCTAssertEqual(BackgroundActivity.nextCheck(remaining: .greatestFiniteMagnitude), 1)
        XCTAssertEqual(BackgroundActivity.nextCheck(remaining: 180), 1)
        XCTAssertEqual(BackgroundActivity.nextCheck(remaining: 9.5), 0.5)
        XCTAssertNil(BackgroundActivity.nextCheck(remaining: 9))
        XCTAssertNil(BackgroundActivity.nextCheck(remaining: 0))
        XCTAssertNil(BackgroundActivity.nextCheck(remaining: .nan))
    }

    func testDiagnosticHistoryIsBoundedAndExcludesPrivatePayloads() throws {
        let (store, _, _, defaults) = fixture()
        let diagnostics = ConnectionDiagnostics(defaults: defaults)
        for _ in 0..<40 { diagnostics.record(.background, remaining: 42.5) }
        store.apply([codeEvent()])
        var failure = CoreEvent(type: "error")
        failure.detail = "fixture-private-error"
        store.apply([failure])
        let entries = try XCTUnwrap(defaults.array(forKey: ConnectionDiagnostics.key) as? [[String: Any]])
        XCTAssertEqual(entries.count, 32)
        let data = try PropertyListSerialization.data(fromPropertyList: entries, format: .xml, options: 0)
        let text = String(decoding: data, as: UTF8.self)
        XCTAssertFalse(text.contains("fixture-private"))
        XCTAssertEqual(entries.last?["stage"] as? String, "update_error")
        for entry in entries {
            XCTAssertTrue(Set(entry.keys).isSubset(of: ["stage", "time", "backgroundSecondsRemaining"]))
        }
    }

    func testSuccessfulLinkClearsInterruptionAndRecordsConnectedSession() {
        let (store, engine, activity, defaults) = fixture()
        store.apply([codeEvent()])
        store.prepareForBackground()
        activity.timeRemaining = 0
        store.background()
        engine.finishStop()
        store.activate()
        var connected = CoreEvent(type: "link")
        connected.status = "connected"
        store.apply([connected])
        XCTAssertTrue(store.hasSession)
        XCTAssertTrue(defaults.bool(forKey: "hasLinkedSession"))
        XCTAssertFalse(store.pairingInterrupted)
        XCTAssertNil(store.pairingCode)
    }

    func testBriefAppSwitchKeepsWorkerAndDoesNotReloadVisibleConversation() {
        let (store, engine, _, _) = fixture()
        store.open("fixture@lid"); store.loading = false
        engine.commands = []
        store.prepareForBackground(); store.background(); store.activate()
        XCTAssertEqual(engine.starts, 1)
        XCTAssertFalse(engine.commands.contains("load"))
        XCTAssertFalse(store.loading)
        store.activate()
        XCTAssertEqual(engine.starts, 1, "Duplicate foreground callbacks should be cheap")
    }

    func testActualSuspensionRestartsWorkerAndReloadsVisibleConversationOnce() {
        let (store, engine, activity, _) = fixture()
        store.open("fixture@lid"); engine.commands = []
        store.prepareForBackground(); activity.timeRemaining = 0; store.background()
        engine.finishStop(); store.activate(); store.activate()
        XCTAssertEqual(engine.starts, 2)
        XCTAssertEqual(engine.commands.filter { $0 == "load" }.count, 1)
        XCTAssertTrue(store.loading)
    }

    func testStorageFailureCanRetryOnNextActivation() {
        let (store, engine, _, _) = fixture()
        engine.onError?("Fixture storage failure")
        store.activate()
        XCTAssertEqual(engine.starts, 2)
    }
}

private final class FakeMessagingEngine: MessagingEngine {
    var onEvents: (([CoreEvent]) -> Void)?
    var onError: ((String) -> Void)?
    var onSend: (() -> Void)?
    var onStop: (() -> Void)?
    var commands: [String] = []
    var stops: [() -> Void] = []
    var starts = 0
    func start(root: URL) { starts += 1 }
    func drain() {}
    func send(_ command: [String: Any], completion: ((Bool) -> Void)?) {
        onSend?()
        commands.append(command["type"] as? String ?? "")
        completion?(true)
    }
    func stop(completion: @escaping () -> Void) { stops.append(completion); onStop?() }
    func finishStop() { stops.removeFirst()() }
}

@MainActor
private final class FakeBackgroundActivity: BackgroundActivityManaging {
    var timeRemaining: TimeInterval = 180
    var available = true
    var begun: [UIBackgroundTaskIdentifier] = []
    var ended: [UIBackgroundTaskIdentifier] = []
    var expirations: [@Sendable () -> Void] = []
    var onEnd: (() -> Void)?
    func begin(expiration: @escaping @Sendable () -> Void) -> UIBackgroundTaskIdentifier {
        guard available else { return .invalid }
        let task = UIBackgroundTaskIdentifier(rawValue: begun.count + 1)
        begun.append(task)
        expirations.append(expiration)
        return task
    }
    func end(_ task: UIBackgroundTaskIdentifier) { ended.append(task); onEnd?() }
}
