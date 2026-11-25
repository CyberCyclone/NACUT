import Foundation

/// Where a transcript file was last read up to.
public struct FileCheckpoint: Codable, Sendable, Hashable {
    public var inode: UInt64
    public var offset: UInt64
}

/// Reads Claude Code JSONL transcripts incrementally and turns assistant messages into `UsageRecord`s.
public struct LogScanner: Sendable {
    public var roots: [URL]

    public init(roots: [URL]) {
        self.roots = roots
    }

    /// Default transcript folders for this Mac.
    public static func defaultRoots(environment: [String: String] = ProcessInfo.processInfo.environment) -> [URL] {
        let home = FileManager.default.homeDirectoryForCurrentUser
        var urls = [
            home.appending(path: ".claude/projects", directoryHint: .isDirectory),
            home.appending(path: ".config/claude/projects", directoryHint: .isDirectory),
        ]
        if let dir = environment["CLAUDE_CONFIG_DIR"], !dir.isEmpty {
            for part in dir.split(separator: ",") {
                let path = (String(part).trimmingCharacters(in: .whitespaces) as NSString).expandingTildeInPath
                urls.append(URL(filePath: path, directoryHint: .isDirectory).appending(path: "projects"))
            }
        }
        var seen = Set<String>()
        return urls.filter { seen.insert($0.standardizedFileURL.path).inserted }
    }

    /// All `.jsonl` files under the roots (recursive, so subagent transcripts are included).
    public func transcriptFiles() -> [URL] {
        var out: [URL] = []
        let fm = FileManager.default
        for root in roots {
            guard let e = fm.enumerator(
                at: root, includingPropertiesForKeys: [.isRegularFileKey],
                options: [.skipsHiddenFiles, .skipsPackageDescendants]
            ) else { continue }
            for case let url as URL in e where url.pathExtension == "jsonl" {
                out.append(url)
            }
        }
        return out
    }

    public struct FileResult: Sendable {
        public var path: String
        public var checkpoint: FileCheckpoint
        public var records: [UsageRecord]
    }

    /// Scans every transcript, reading only bytes appended since `checkpoints`.
    public func scan(checkpoints: [String: FileCheckpoint]) async -> [FileResult] {
        let files = transcriptFiles()
        return await withTaskGroup(of: FileResult?.self) { group in
            var it = files.makeIterator()
            let width = max(2, ProcessInfo.processInfo.activeProcessorCount)
            func addNext() -> Bool {
                guard let url = it.next() else { return false }
                let cp = checkpoints[url.path]
                group.addTask { Self.scanFile(url, from: cp) }
                return true
            }
            for _ in 0..<width where !addNext() { break }
            var results: [FileResult] = []
            while let r = await group.next() {
                if let r { results.append(r) }
                _ = addNext()
            }
            return results
        }
    }

    /// Reads one file from its checkpoint. Returns nil when nothing changed.
    public static func scanFile(_ url: URL, from checkpoint: FileCheckpoint?) -> FileResult? {
        guard let attrs = try? FileManager.default.attributesOfItem(atPath: url.path),
              let size = (attrs[.size] as? NSNumber)?.uint64Value,
              let inode = (attrs[.systemFileNumber] as? NSNumber)?.uint64Value
        else { return nil }

        var start: UInt64 = 0
        if let cp = checkpoint, cp.inode == inode, cp.offset <= size {
            if cp.offset == size { return nil }
            start = cp.offset
        }
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? handle.close() }
        do { try handle.seek(toOffset: start) } catch { return nil }
        guard let data = try? handle.readToEnd(), !data.isEmpty else {
            return FileResult(path: url.path, checkpoint: .init(inode: inode, offset: start), records: [])
        }

        // Only consume complete lines; a partially written last line is re-read next time.
        guard let lastNewline = data.lastIndex(of: 0x0A) else {
            return FileResult(path: url.path, checkpoint: .init(inode: inode, offset: start), records: [])
        }
        let complete = data[data.startIndex...lastNewline]
        let consumed = UInt64(complete.count)
        let isSubagentFile = url.pathComponents.contains("subagents")
        let records = parseLines(complete, isSubagentFile: isSubagentFile)
        return FileResult(path: url.path, checkpoint: .init(inode: inode, offset: start + consumed), records: records)
    }

    private static let usageNeedle = Data("\"usage\"".utf8)
    private static let assistantNeedle = Data("\"assistant\"".utf8)

    static func parseLines(_ data: Data, isSubagentFile: Bool) -> [UsageRecord] {
        var out: [UsageRecord] = []
        let decoder = JSONDecoder()
        var lineStart = data.startIndex
        while lineStart < data.endIndex {
            let lineEnd = data[lineStart...].firstIndex(of: 0x0A) ?? data.endIndex
            let line = data[lineStart..<lineEnd]
            lineStart = lineEnd < data.endIndex ? data.index(after: lineEnd) : data.endIndex
            guard line.count > 20,
                  line.range(of: usageNeedle) != nil,
                  line.range(of: assistantNeedle) != nil
            else { continue }
            if let rec = parseLine(Data(line), decoder: decoder, isSubagentFile: isSubagentFile) {
                out.append(rec)
            }
        }
        return out
    }

    static func parseLine(_ line: Data, decoder: JSONDecoder, isSubagentFile: Bool) -> UsageRecord? {
        guard let raw = try? decoder.decode(RawLine.self, from: line),
              raw.type == "assistant",
              raw.isApiErrorMessage != true,
              let msg = raw.message,
              let usage = msg.usage,
              let model = msg.model, !model.isEmpty, model != "<synthetic>",
              let ts = raw.timestamp.flatMap(Self.parseDate)
        else { return nil }
        guard let id = msg.id ?? raw.requestId ?? raw.uuid else { return nil }

        var w5 = usage.cache_creation?.ephemeral_5m_input_tokens ?? 0
        let w1h = usage.cache_creation?.ephemeral_1h_input_tokens ?? 0
        let totalWrite = usage.cache_creation_input_tokens ?? 0
        if usage.cache_creation == nil || w5 + w1h < totalWrite {
            w5 = max(w5, totalWrite - w1h)
        }
        return UsageRecord(
            id: id,
            timestamp: ts,
            model: model,
            input: usage.input_tokens ?? 0,
            output: usage.output_tokens ?? 0,
            cacheWrite5m: w5,
            cacheWrite1h: w1h,
            cacheRead: usage.cache_read_input_tokens ?? 0,
            webSearches: usage.server_tool_use?.web_search_requests ?? 0,
            fast: usage.speed == "fast",
            cwd: raw.cwd ?? "",
            sessionId: raw.sessionId ?? "",
            slug: raw.slug,
            entrypoint: raw.entrypoint ?? "cli",
            isSubagent: isSubagentFile || raw.isSidechain == true
        )
    }

    nonisolated(unsafe) private static let isoFrac: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return f
    }()
    nonisolated(unsafe) private static let iso: ISO8601DateFormatter = {
        let f = ISO8601DateFormatter()
        f.formatOptions = [.withInternetDateTime]
        return f
    }()
    private static let dateLock = NSLock()

    public static func parseDate(_ s: String) -> Date? {
        dateLock.lock(); defer { dateLock.unlock() }
        return isoFrac.date(from: s) ?? iso.date(from: s)
    }
}

// MARK: - Raw JSON shapes (only the fields we need; message content is never decoded)

private struct RawLine: Decodable {
    var type: String?
    var timestamp: String?
    var uuid: String?
    var requestId: String?
    var sessionId: String?
    var cwd: String?
    var slug: String?
    var entrypoint: String?
    var isSidechain: Bool?
    var isApiErrorMessage: Bool?
    var message: RawMessage?
}

private struct RawMessage: Decodable {
    var id: String?
    var model: String?
    var usage: RawUsage?
}

private struct RawUsage: Decodable {
    var input_tokens: Int?
    var output_tokens: Int?
    var cache_creation_input_tokens: Int?
    var cache_read_input_tokens: Int?
    var cache_creation: RawCacheCreation?
    var server_tool_use: RawServerToolUse?
    var speed: String?
}

private struct RawCacheCreation: Decodable {
    var ephemeral_5m_input_tokens: Int?
    var ephemeral_1h_input_tokens: Int?
}

private struct RawServerToolUse: Decodable {
    var web_search_requests: Int?
}
