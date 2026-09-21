import Foundation

/// Local, bounded lifecycle records only. Never accept event payloads or errors:
/// they can contain phone numbers, account identifiers, codes or message text.
@MainActor
final class ConnectionDiagnostics {
    enum Stage: String {
        case appStarted = "app_started"
        case foreground, background, starting, connecting, connected, unlinked, disconnected, failed, loggedOut = "logged_out"
        case backgroundTaskStarted = "background_task_started"
        case backgroundTaskUnavailable = "background_task_unavailable"
        case backgroundBudgetLow = "background_budget_low"
        case backgroundExpired = "background_expired"
        case engineStopped = "engine_stopped"
        case engineError = "engine_error"
        case pairingRequested = "pairing_requested"
        case codeReady = "code_ready"
        case updateError = "update_error"
        case unknownLinkState = "unknown_link_state"
        case syncStarted = "sync_started"
        case syncFinished = "sync_finished"
    }

    static let key = "connectionDiagnostics"
    private let defaults: UserDefaults

    init(defaults: UserDefaults) { self.defaults = defaults }

    func record(_ stage: Stage, remaining: TimeInterval? = nil) {
        var entry: [String: Any] = ["stage": stage.rawValue, "time": Date()]
        if let remaining, remaining.isFinite, (0...3600).contains(remaining) {
            entry["backgroundSecondsRemaining"] = Int(remaining)
        }
        var entries = defaults.array(forKey: Self.key) as? [[String: Any]] ?? []
        entries.append(entry)
        defaults.set(Array(entries.suffix(32)), forKey: Self.key)
    }
}
