import Foundation

/// Persisted, deduplicated message history plus per-file read checkpoints.
///
/// The cache keeps history even after Claude Code deletes old transcripts,
/// so totals never shrink when transcripts are cleaned up.
public struct LogCache: Codable, Sendable {
    public static let currentVersion = 2

    public var version: Int = LogCache.currentVersion
    public var checkpoints: [String: FileCheckpoint] = [:]
    public var records: [String: UsageRecord] = [:]

    public init() {}

    /// Merges scan results. A message ID can appear on several lines (streaming updates,
    /// resumed sessions); the line with the most output tokens is the final one.
    public mutating func merge(_ results: [LogScanner.FileResult]) -> Bool {
        var changed = false
        for r in results {
            checkpoints[r.path] = r.checkpoint
            for rec in r.records {
                if let existing = records[rec.id] {
                    if rec.output > existing.output
                        || (rec.output == existing.output && rec.totalTokens > existing.totalTokens) {
                        records[rec.id] = rec
                        changed = true
                    }
                } else {
                    records[rec.id] = rec
                    changed = true
                }
            }
        }
        return changed
    }

    public static func load(from url: URL) -> LogCache {
        guard let data = try? Data(contentsOf: url),
              let cache = try? JSONDecoder.cache.decode(LogCache.self, from: data),
              cache.version == currentVersion
        else { return LogCache() }
        return cache
    }

    public func save(to url: URL) throws {
        try FileManager.default.createDirectory(
            at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
        let data = try JSONEncoder.cache.encode(self)
        try data.write(to: url, options: .atomic)
    }
}

extension JSONDecoder {
    static var cache: JSONDecoder {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .secondsSince1970
        return d
    }
}

extension JSONEncoder {
    static var cache: JSONEncoder {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .secondsSince1970
        return e
    }
}
