import Foundation
import NacutCore
import UserNotifications

/// Shows the alerts the core decides on (budget thresholds, forecast, plan limits).
@MainActor
final class Notifier {
    func requestAuthorization() async {
        _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
    }

    func post(_ alert: Alert) {
        let content = UNMutableNotificationContent()
        content.title = alert.title
        content.body = alert.body
        content.sound = .default
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: alert.id, content: content, trigger: nil))
    }
}
