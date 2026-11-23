import BackgroundTasks
import UIKit

@MainActor
protocol BackgroundActivityManaging: AnyObject {
    var timeRemaining: TimeInterval { get }
    func begin(expiration: @escaping @Sendable () -> Void) -> UIBackgroundTaskIdentifier
    func end(_ task: UIBackgroundTaskIdentifier)
}

@MainActor
final class BackgroundActivity: BackgroundActivityManaging {
    var timeRemaining: TimeInterval { UIApplication.shared.backgroundTimeRemaining }

    func begin(expiration: @escaping @Sendable () -> Void) -> UIBackgroundTaskIdentifier {
        UIApplication.shared.beginBackgroundTask(withName: "Finish WhatsApp activity", expirationHandler: expiration)
    }

    func end(_ task: UIBackgroundTaskIdentifier) { UIApplication.shared.endBackgroundTask(task) }

    static func nextCheck(remaining: TimeInterval) -> TimeInterval? {
        // Rust can need five seconds to stop the bot and three to stop its
        // runtime. Recheck the changing UIKit allowance, reserving one more
        // second for the completion callback. Infinity is not a fixed budget.
        guard !remaining.isNaN, remaining > 9 else { return nil }
        return min(1, remaining - 9)
    }
}

@MainActor
final class BackgroundActivityCompletion {
    private let activity: BackgroundActivityManaging
    private var task: UIBackgroundTaskIdentifier

    init(activity: BackgroundActivityManaging, task: UIBackgroundTaskIdentifier) {
        self.activity = activity
        self.task = task
    }

    func end() {
        guard task != .invalid else { return }
        let ending = task
        task = .invalid
        activity.end(ending)
    }
}

/// Opportunistic catch-up while the app is closed. iOS decides when (and
/// whether) the task runs, from usage, battery and the Background App Refresh
/// setting; there is no push, so this narrows the gap rather than closing it.
enum BackgroundRefresh {
    static let identifier = "org.erlin.whatsapp.ios.refresh"
    /// The earliest next attempt. iOS usually waits longer.
    static let interval: TimeInterval = 15 * 60
    /// Time to wait for the offline backlog. iOS allows about 30 seconds in
    /// all, and stopping the worker can take eight of them.
    static let catchUpLimit: Duration = .seconds(18)
    /// Lets receipts for the drained backlog leave before disconnecting.
    static let settle: Duration = .milliseconds(1500)

    static func schedule(after interval: TimeInterval = interval) {
        let request = BGAppRefreshTaskRequest(identifier: identifier)
        request.earliestBeginDate = Date(timeIntervalSinceNow: interval)
        // Replaces an earlier request with the same identifier. Fails in the
        // simulator and when Background App Refresh is off; neither is an error.
        try? BGTaskScheduler.shared.submit(request)
    }

    static func cancel() { BGTaskScheduler.shared.cancel(taskRequestWithIdentifier: identifier) }
}
