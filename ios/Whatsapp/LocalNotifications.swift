import UserNotifications

@MainActor
final class LocalNotifications: NSObject, UNUserNotificationCenterDelegate {
    var onOpen: ((String) -> Void)?
    override init() {
        super.init()
        UNUserNotificationCenter.current().delegate = self
    }
    func request() async -> Bool {
        (try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge])) ?? false
    }
    func clear() {
        UNUserNotificationCenter.current().removeAllDeliveredNotifications()
        UNUserNotificationCenter.current().removeAllPendingNotificationRequests()
    }
    func show(_ message: Message, name: String) {
        let content = UNMutableNotificationContent()
        content.title = name
        content.body = message.text.isEmpty ? "New attachment" : message.text
        content.sound = .default
        content.userInfo = ["chat": message.chat]
        let request = UNNotificationRequest(identifier: message.chat + ":" + message.id, content: content, trigger: nil)
        UNUserNotificationCenter.current().add(request)
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, willPresent notification: UNNotification, withCompletionHandler completionHandler: @escaping (UNNotificationPresentationOptions) -> Void) {
        completionHandler([.banner, .sound])
    }
    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse, withCompletionHandler completionHandler: @escaping () -> Void) {
        if let chat = response.notification.request.content.userInfo["chat"] as? String {
            Task { @MainActor in self.onOpen?(chat) }
        }
        completionHandler()
    }
}
