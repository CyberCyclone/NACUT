import Foundation
import UsageCore

/// Reads Max/Pro plan limit utilization (5-hour session, weekly) using the
/// Claude Code sign-in stored in the Keychain.
///
/// This uses the same unofficial endpoint as Claude Code's `/usage` screen, so it may change.
/// The token is never refreshed here; when it expires, `ClaudeCodeRefresher` asks Claude Code to do it.
enum LimitsService {
    enum Failure: Error, LocalizedError {
        case noCredential, expired, http(Int), badResponse

        var errorDescription: String? {
            switch self {
            case .noCredential: return "No Claude Code sign-in found. Run `claude` in Terminal and sign in."
            case .expired: return "Sign-in expired and Claude Code couldn't refresh it. Run `claude` once in Terminal."
            case .http(let c): return c == 401 ? "Sign-in rejected. Run `claude` once in Terminal." : "Limits request failed (HTTP \(c))."
            case .badResponse: return "Unexpected limits response."
            }
        }
    }

    private struct Credentials: Decodable {
        struct OAuth: Decodable {
            var accessToken: String
            var expiresAt: Double?
        }
        var claudeAiOauth: OAuth?
    }

    private struct Token: Sendable {
        var value: String
        var expired: Bool
    }

    static func fetch() async throws -> [PlanLimit] {
        var token = try await readToken()
        var refreshed = false
        if token.expired {
            refreshed = await ClaudeCodeRefresher.shared.refresh()
            token = try await readToken()
            if token.expired { throw Failure.expired }
        }
        do {
            return try await request(token.value)
        } catch Failure.http(401) where !refreshed {
            // Token revoked early or rotated elsewhere: let Claude Code refresh once and retry.
            guard await ClaudeCodeRefresher.shared.refresh() else { throw Failure.http(401) }
            return try await request(try await readToken().value)
        }
    }

    private static func readToken() async throws -> Token {
        try await Task.detached(priority: .utility) { () throws -> Token in
            guard let raw = KeychainStore.read(service: "Claude Code-credentials", account: ""),
                  let creds = try? JSONDecoder().decode(Credentials.self, from: Data(raw.utf8)),
                  let oauth = creds.claudeAiOauth
            else { throw Failure.noCredential }
            // Treat tokens within a minute of expiry as expired.
            let expired = oauth.expiresAt.map { Date(timeIntervalSince1970: $0 / 1000) < .now.addingTimeInterval(60) } ?? false
            return Token(value: oauth.accessToken, expired: expired)
        }.value
    }

    private static func request(_ token: String) async throws -> [PlanLimit] {
        var req = URLRequest(url: URL(string: "https://api.anthropic.com/api/oauth/usage")!)
        req.timeoutInterval = 20
        req.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        req.setValue("oauth-2025-04-20", forHTTPHeaderField: "anthropic-beta")
        req.setValue("application/json", forHTTPHeaderField: "Accept")
        req.setValue("NACUT/1.0", forHTTPHeaderField: "User-Agent")
        let (data, resp) = try await URLSession.shared.data(for: req)
        guard let http = resp as? HTTPURLResponse else { throw Failure.badResponse }
        guard http.statusCode == 200 else { throw Failure.http(http.statusCode) }
        let limits = PlanLimit.parse(data)
        if limits.isEmpty { throw Failure.badResponse }
        return limits
    }
}

/// Gets Claude Code to refresh its own sign-in.
///
/// The Claude desktop app's Code tab signs in separately, so the CLI's Keychain token goes stale
/// (it lasts about 8 hours) unless the CLI runs. Rather than rotating the token ourselves, which
/// could race with Claude Code and sign it out, we run the CLI with a one-word Haiku prompt:
/// Claude Code refreshes and saves the token itself. `--no-session-persistence` keeps it out of
/// the transcripts this app counts. Runs at most once every 15 minutes.
actor ClaudeCodeRefresher {
    static let shared = ClaudeCodeRefresher()

    private var lastAttempt: Date?
    private var inFlight: Task<Bool, Never>?

    func refresh() async -> Bool {
        if let inFlight { return await inFlight.value }
        if let lastAttempt, Date.now.timeIntervalSince(lastAttempt) < 15 * 60 { return false }
        lastAttempt = .now
        let task = Task.detached(priority: .utility) { Self.run() }
        inFlight = task
        let ok = await task.value
        inFlight = nil
        return ok
    }

    static var executable: URL? {
        let home = FileManager.default.homeDirectoryForCurrentUser.path
        let candidates = [
            "\(home)/.local/bin/claude",
            "\(home)/.claude/local/claude",
            "/opt/homebrew/bin/claude",
            "/usr/local/bin/claude",
            "\(home)/.npm-global/bin/claude",
        ]
        return candidates.first { FileManager.default.isExecutableFile(atPath: $0) }.map(URL.init(fileURLWithPath:))
    }

    private nonisolated static func run() -> Bool {
        guard let exe = executable else { return false }
        let p = Process()
        p.executableURL = exe
        p.arguments = [
            "-p", "--model", "haiku", "--no-session-persistence", "--max-turns", "1",
            "--strict-mcp-config", "--mcp-config", #"{"mcpServers":{}}"#,
            "--disallowed-tools", "Bash,Edit,Write,Read,WebFetch,WebSearch",
        ]
        p.currentDirectoryURL = FileManager.default.temporaryDirectory
        var env = ProcessInfo.processInfo.environment
        env["PATH"] = "\(exe.deletingLastPathComponent().path):/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin"
        p.environment = env
        let input = Pipe()
        p.standardInput = input
        p.standardOutput = FileHandle.nullDevice
        p.standardError = FileHandle.nullDevice
        do { try p.run() } catch { return false }
        input.fileHandleForWriting.write(Data("Reply with OK".utf8))
        try? input.fileHandleForWriting.close()

        let deadline = Date.now.addingTimeInterval(60)
        while p.isRunning && Date.now < deadline { Thread.sleep(forTimeInterval: 0.25) }
        if p.isRunning { p.terminate(); return false }
        return p.terminationStatus == 0
    }
}

extension PlanLimit {
    /// Window length implied by the limit id.
    var window: TimeInterval { id == "five_hour" ? 5 * 3600 : 7 * 86_400 }

    /// Utilization projected to the reset time at the current pace, or nil if too early to tell.
    func projectedAtReset(now: Date = .now) -> Double? {
        guard let resetsAt else { return nil }
        let start = resetsAt.addingTimeInterval(-window)
        let elapsed = now.timeIntervalSince(start) / window
        guard elapsed > 0.1, elapsed < 1 else { return nil }
        return utilization / elapsed
    }

    /// When the limit would be hit at the current pace, if before reset.
    func projectedHit(now: Date = .now) -> Date? {
        guard let resetsAt, utilization > 0, utilization < 1 else { return nil }
        let start = resetsAt.addingTimeInterval(-window)
        let elapsed = now.timeIntervalSince(start)
        guard elapsed > window * 0.1 else { return nil }
        let rate = utilization / elapsed
        let hit = now.addingTimeInterval((1 - utilization) / rate)
        return hit < resetsAt ? hit : nil
    }

    func resetText(now: Date = .now) -> String {
        guard let resetsAt else { return "" }
        let s = max(resetsAt.timeIntervalSince(now), 0)
        let h = Int(s) / 3600, m = (Int(s) % 3600) / 60
        if h >= 24 { return "resets \(resetsAt.formatted(.dateTime.weekday(.abbreviated).hour()))" }
        return h > 0 ? "resets in \(h)h \(m)m" : "resets in \(m)m"
    }
}
