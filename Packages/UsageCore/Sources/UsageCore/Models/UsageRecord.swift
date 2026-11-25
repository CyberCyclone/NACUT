import Foundation

/// One deduplicated assistant message from a Claude Code transcript.
public struct UsageRecord: Codable, Sendable, Hashable, Identifiable {
    public var id: String
    public var timestamp: Date
    public var model: String
    public var input: Int
    public var output: Int
    public var cacheWrite5m: Int
    public var cacheWrite1h: Int
    public var cacheRead: Int
    public var webSearches: Int
    public var fast: Bool
    public var cwd: String
    public var sessionId: String
    public var slug: String?
    public var entrypoint: String
    public var isSubagent: Bool

    /// Computed by `PriceBook`; not persisted.
    public var cost: Double = 0
    /// True when the model's price was guessed from its family.
    public var estimated: Bool = false

    enum CodingKeys: String, CodingKey {
        case id, timestamp, model, input, output, cacheWrite5m, cacheWrite1h, cacheRead
        case webSearches, fast, cwd, sessionId, slug, entrypoint, isSubagent
    }

    public init(
        id: String, timestamp: Date, model: String, input: Int, output: Int,
        cacheWrite5m: Int, cacheWrite1h: Int, cacheRead: Int, webSearches: Int = 0,
        fast: Bool = false, cwd: String = "", sessionId: String = "", slug: String? = nil,
        entrypoint: String = "cli", isSubagent: Bool = false
    ) {
        self.id = id
        self.timestamp = timestamp
        self.model = model
        self.input = input
        self.output = output
        self.cacheWrite5m = cacheWrite5m
        self.cacheWrite1h = cacheWrite1h
        self.cacheRead = cacheRead
        self.webSearches = webSearches
        self.fast = fast
        self.cwd = cwd
        self.sessionId = sessionId
        self.slug = slug
        self.entrypoint = entrypoint
        self.isSubagent = isSubagent
    }

    public var cacheWrite: Int { cacheWrite5m + cacheWrite1h }
    public var totalTokens: Int { input + output + cacheWrite + cacheRead }
    /// Tokens the model read for this request (used for long-context pricing tiers).
    public var promptTokens: Int { input + cacheWrite + cacheRead }

    /// Repo-level project path: worktree paths fold into the repo they belong to.
    public var projectPath: String { Self.projectPath(for: cwd) }

    public var projectName: String {
        let p = projectPath
        if p.isEmpty { return "Unknown" }
        return (p as NSString).lastPathComponent
    }

    public static func projectPath(for cwd: String) -> String {
        for marker in ["/.claude/worktrees/", "/.worktrees/"] {
            if let r = cwd.range(of: marker) { return String(cwd[..<r.lowerBound]) }
        }
        return cwd
    }
}

public enum Entrypoint {
    public static func displayName(_ raw: String) -> String {
        switch raw {
        case "claude-desktop": return "Desktop app"
        case "claude-vscode": return "VS Code"
        case "cli", "": return "Terminal CLI"
        case "sdk-cli", "sdk-ts", "sdk-py": return "Agent SDK"
        default:
            return raw.replacingOccurrences(of: "claude-", with: "").capitalized
        }
    }
}
