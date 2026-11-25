import Foundation

/// One-time move from the app's old identity ("Claude Usage", `com.caseygibson.ClaudeUsage`) to NACUT:
/// settings (including the saved menu bar position), the history cache folder, and the API key.
/// Runs before anything reads settings. Safe to run again; it never overwrites newer values.
enum LegacyMigration {
    static let oldBundleID = "com.caseygibson.ClaudeUsage"
    private static let doneKey = "migratedFromClaudeUsage"

    static func run() {
        let d = UserDefaults.standard
        guard !d.bool(forKey: doneKey) else { return }

        // Settings. Not sandboxed, so another domain of the same user can be read.
        if let old = d.persistentDomain(forName: oldBundleID) {
            for (key, value) in old where d.object(forKey: key) == nil {
                d.set(value, forKey: key)
            }
        }

        // History cache and widget snapshot.
        let fm = FileManager.default
        let base = fm.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        let oldDir = base.appending(path: "ClaudeUsage", directoryHint: .isDirectory)
        let newDir = base.appending(path: "NACUT", directoryHint: .isDirectory)
        if fm.fileExists(atPath: oldDir.path), !fm.fileExists(atPath: newDir.path) {
            try? fm.moveItem(at: oldDir, to: newDir)
        }

        // Analytics API key.
        if KeychainStore.analyticsKey == nil,
           let key = KeychainStore.read(service: oldBundleID, account: KeychainStore.analyticsAccount),
           KeychainStore.save(key, account: KeychainStore.analyticsAccount) {
            KeychainStore.delete(service: oldBundleID, account: KeychainStore.analyticsAccount)
        }

        d.set(true, forKey: doneKey)
    }
}
