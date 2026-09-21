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
