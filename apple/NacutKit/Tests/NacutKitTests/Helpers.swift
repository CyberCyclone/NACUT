import Foundation

/// A Gregorian calendar with a fixed time zone, locale and first weekday (1 = Sunday, 2 = Monday),
/// so tests don't depend on the machine's settings.
func fixedCalendar(firstWeekday: Int = 1, timeZone: String = "UTC") -> Calendar {
    var c = Calendar(identifier: .gregorian)
    c.timeZone = TimeZone(identifier: timeZone)!
    c.locale = Locale(identifier: "en_US_POSIX")
    c.firstWeekday = firstWeekday
    return c
}

func date(_ y: Int, _ m: Int, _ d: Int, _ h: Int = 0, _ min: Int = 0, _ s: Int = 0,
          in cal: Calendar = fixedCalendar()) -> Date {
    cal.date(from: DateComponents(year: y, month: m, day: d, hour: h, minute: min, second: s))!
}

/// ICU puts a narrow no-break space before AM/PM; compare with plain spaces.
func plainSpaces(_ s: String) -> String {
    s.replacingOccurrences(of: "\u{202F}", with: " ").replacingOccurrences(of: "\u{00A0}", with: " ")
}

/// A fresh, empty folder under the system temp dir; removed by the caller.
func makeTempDir() throws -> URL {
    let url = FileManager.default.temporaryDirectory.appending(path: "nacutkit-tests-\(UUID().uuidString)", directoryHint: .isDirectory)
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    return url
}
