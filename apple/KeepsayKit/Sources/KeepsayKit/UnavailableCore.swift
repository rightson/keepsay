// Stand-in used when no Rust core is linked: development builds before
// apple/scripts/build-core.sh ran, SwiftUI previews and unit tests.
//
// Behaves like core with no key and no network: every polish returns the
// raw transcript (FR-4), translation reports "翻譯需要 API" (FR-8), and STT
// is unavailable. Settings live in memory only; nothing is written to disk.

import Foundation

public final class UnavailableCore: KeepsayCore, @unchecked Sendable {
    private let lock = NSLock()
    private var settings: AppSettings

    public init(settings: AppSettings = AppSettings()) {
        self.settings = settings
    }

    public func storeKey(_ key: String) async throws -> String {
        throw CoreFailure.coreNotLinked
    }

    public func hasKey() -> Bool { false }
    public func keyFingerprint() -> String? { nil }
    public func deleteKey() throws {}
    public func verifyKey() async throws { throw CoreFailure.coreNotLinked }

    public func polish(_ transcript: String, request: PolishRequest) async throws -> PolishOutcome {
        guard request.needsAPI else { return .raw(transcript, cause: nil) }
        return .raw(transcript, cause: request.translateTo != nil ? .translationNeedsAPI : .noKey)
    }

    public func transcribe(pcm: [Float], sampleRate: UInt32, glossary: [String]) async throws -> String {
        throw CoreFailure.coreNotLinked
    }

    public func budgetStatus() -> BudgetSnapshot {
        let s = loadSettings()
        return BudgetSnapshot(
            usage: TokenUsage(inputTokens: 0, outputTokens: 0),
            estimatedUSD: 0, softLimitUSD: s.softLimitUSD, hardLimitUSD: s.hardLimitUSD)
    }

    public func models() -> [ModelInfo] {
        // Mirrors models/manifest.json so the UI can be laid out.
        [
            ModelInfo(id: "whisper-base", name: "Whisper base", sizeBytes: 147_951_465, installed: false),
            ModelInfo(id: "whisper-small", name: "Whisper small", sizeBytes: 487_601_967, installed: false),
            ModelInfo(
                id: "whisper-large-v3-turbo", name: "Whisper large-v3-turbo", sizeBytes: 1_624_555_275,
                installed: false),
        ]
    }

    public func downloadModel(id: String, progress: @escaping @Sendable (Double) -> Void) async throws {
        throw CoreFailure.coreNotLinked
    }

    public func importModel(id: String, from url: URL) async throws {
        throw CoreFailure.coreNotLinked
    }

    public func loadSettings() -> AppSettings {
        lock.lock()
        defer { lock.unlock() }
        return settings
    }

    public func saveSettings(_ settings: AppSettings) throws {
        lock.lock()
        defer { lock.unlock() }
        self.settings = settings
    }
}
