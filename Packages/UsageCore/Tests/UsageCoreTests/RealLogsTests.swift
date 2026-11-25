import Foundation
import Testing
@testable import UsageCore

/// Opt-in: `REAL_LOGS=1 swift test --filter RealLogs` prints totals from this Mac's transcripts.
@Suite struct RealLogs {
    @Test(.enabled(if: ProcessInfo.processInfo.environment["REAL_LOGS"] == "1"))
    func printTotals() async {
        let clock = ContinuousClock()
        var cache = LogCache()
        let scanner = LogScanner(roots: LogScanner.defaultRoots())
        let elapsed = await clock.measure {
            _ = cache.merge(await scanner.scan(checkpoints: [:]))
        }
        let store = UsageStore(records: Array(cache.records.values), priceBook: .bundled)
        let cal = Calendar.current
        let today = cal.startOfDay(for: .now)
        let month = cal.date(from: cal.dateComponents([.year, .month], from: .now))!
        let tomorrow = cal.date(byAdding: .day, value: 1, to: today)!
        print(String(format: "SCAN records=%d time=%@ today=%.4f month=%.4f all=%.4f",
                     cache.records.count, "\(elapsed)",
                     store.cost(in: DateInterval(start: today, end: tomorrow)),
                     store.cost(in: DateInterval(start: month, end: tomorrow)),
                     store.records.reduce(0) { $0 + $1.cost }))
        for m in store.modelStats(in: DateInterval(start: month, end: tomorrow)) {
            print(String(format: "  %@ %.4f", m.id, m.cost))
        }
        let t2 = await clock.measure { _ = await scanner.scan(checkpoints: cache.checkpoints) }
        print("INCREMENTAL rescan time=\(t2)")
    }
}
