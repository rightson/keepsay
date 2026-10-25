// Non-secret settings (PROPOSAL §10): JSON with a schema version in
// Application Support, file protection "complete". Never holds the key
// (INV-1). The glossary is private: never log it.
//
// The canonical settings live in the core's config (core/src/config.rs),
// reached through KeepsayKit. Until it is linked this store keeps the
// iOS-side subset in the same shape.

import Foundation

struct AppSettings: Codable, Equatable {
    static let schemaVersion = 1

    var schemaVersion: Int = AppSettings.schemaVersion
    var onboarded: Bool = false
    var defaultMode: PolishMode = .polish
    var targetLanguage: TargetLanguage = .en
    var glossary: [String] = []
    var sttModel: String = "whisper-small"
    var softLimitUSD: Double? = 5
    var hardLimitUSD: Double? = 10
    /// USD per million tokens. UI labels these "請核對官方定價".
    var inputPricePerMTok: Double = 0.10
    var outputPricePerMTok: Double = 0.50
    /// "YYYY-MM" of the last soft-limit notice ("到了提醒一次").
    var softLimitWarnedMonth: String?
}

extension AppSettings: CustomStringConvertible, CustomDebugStringConvertible {
    var description: String {
        "AppSettings(v\(schemaVersion), mode: \(defaultMode.rawValue), glossary_len: \(glossary.count))"
    }
    var debugDescription: String { description }
}

/// Glossary editing rules, kept pure for tests.
enum Glossary {
    static let maxTerms = 200
    static let maxTermLength = 64

    /// Adds a trimmed, non-empty, non-duplicate term. Returns the new list.
    static func adding(_ term: String, to list: [String]) -> [String] {
        let t = term.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !t.isEmpty, t.count <= maxTermLength, list.count < maxTerms else { return list }
        guard !list.contains(where: { $0.caseInsensitiveCompare(t) == .orderedSame }) else { return list }
        return list + [t]
    }
}

final class SettingsStore {
    private let url: URL
    /// Set when the file has a newer schema: never overwrite it (downgrade).
    private var frozen = false

    init(directory: URL? = nil) {
        let base = directory
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        url = base.appendingPathComponent("settings.json")
    }

    func load() -> AppSettings {
        guard let data = try? Data(contentsOf: url),
              let s = try? JSONDecoder().decode(AppSettings.self, from: data)
        else { return AppSettings() }
        // A newer schema is not ours to overwrite; use defaults in memory.
        if s.schemaVersion > AppSettings.schemaVersion {
            frozen = true
            return AppSettings()
        }
        return s
    }

    func save(_ settings: AppSettings) {
        guard !frozen else { return }
        do {
            try FileManager.default.createDirectory(
                at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
            let data = try JSONEncoder().encode(settings)
            try data.write(to: url, options: [.atomic, .completeFileProtection])
        } catch {
            // Settings are best effort; nothing user-identifying to report.
        }
    }
}
