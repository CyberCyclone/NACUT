import Foundation
import UserNotifications
import UsageCore

/// Posts budget and plan-limit notifications, each at most once per cycle/window.
@MainActor
final class AlertService {
    private let defaults = UserDefaults.standard
    private var authorized: Bool?

    func requestAuthorization() async {
        let granted = (try? await UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound])) ?? false
        authorized = granted
    }

    func evaluate(forecast f: Forecast, limits: [PlanLimit], settings: AppSettings) {
        guard settings.alertsEnabled else { return }
        let cycleKey = ISO8601DateFormatter().string(from: f.cycle.start)
        let noun = settings.accountMode == .subscription ? "API-equivalent usage" : "spend"

        if f.budget > 0 {
            for t in settings.alertThresholds.sorted() where f.budgetUsedFraction * 100 >= Double(t) {
                fireOnce("budget-\(cycleKey)-\(t)",
                         title: t >= 100 ? "Claude budget reached" : "Claude budget \(t)% used",
                         body: "\(CurrencyFormat.string(f.spent)) of \(CurrencyFormat.string(f.budget)) \(noun) this cycle.")
            }
            if settings.forecastAlert, f.status == .willExceed, let d = f.exhaustionDate {
                fireOnce("forecast-\(cycleKey)",
                         title: "On pace to exceed your Claude budget",
                         body: "At the current rate you'll hit \(CurrencyFormat.string(f.budget)) around \(d.formatted(.dateTime.weekday(.wide).month().day())). Projected: \(CurrencyFormat.string(f.projected)).")
            }
        }

        if settings.limitAlerts {
            for l in limits where l.utilization >= 0.9 {
                let window = l.resetsAt.map { ISO8601DateFormatter().string(from: $0) } ?? "none"
                fireOnce("limit-\(l.id)-\(window)",
                         title: "\(l.label) limit at \(Int(l.utilization * 100))%",
                         body: l.resetText().isEmpty ? "You're close to your plan limit." : "Plan limit \(l.resetText()).")
            }
        }
    }

    func sendTest() {
        post(id: "test-\(UUID())", title: "NACUT alerts are on", body: "You'll be notified at your budget thresholds.")
    }

    private func fireOnce(_ key: String, title: String, body: String) {
        let k = "alert.fired.\(key)"
        guard !defaults.bool(forKey: k) else { return }
        defaults.set(true, forKey: k)
        post(id: key, title: title, body: body)
    }

    private func post(id: String, title: String, body: String) {
        let content = UNMutableNotificationContent()
        content.title = title
        content.body = body
        content.sound = .default
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: id, content: content, trigger: nil))
    }
}
