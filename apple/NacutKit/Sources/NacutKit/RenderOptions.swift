import Foundation

/// Command-line options for `NACUT --render-to <dir> [--limits-json <file>]`, which renders the
/// panel to PNGs and quits.
public struct RenderOptions: Sendable, Equatable {
    public var outputDir: URL
    /// A saved plan-limits response to render with instead of reading the Keychain.
    public var limitsJSON: URL?

    public init(outputDir: URL, limitsJSON: URL? = nil) {
        self.outputDir = outputDir
        self.limitsJSON = limitsJSON
    }

    /// `nil` unless `--render-to` is followed by a path. A `--limits-json` without a path is ignored.
    public static func parse(arguments: [String]) -> RenderOptions? {
        guard let dir = value(after: "--render-to", in: arguments) else { return nil }
        return RenderOptions(
            outputDir: URL(filePath: dir, directoryHint: .isDirectory),
            limitsJSON: value(after: "--limits-json", in: arguments).map { URL(filePath: $0) })
    }

    private static func value(after flag: String, in args: [String]) -> String? {
        guard let i = args.firstIndex(of: flag), i + 1 < args.count else { return nil }
        return args[i + 1]
    }
}
