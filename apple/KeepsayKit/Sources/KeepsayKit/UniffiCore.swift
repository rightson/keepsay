// Adapter from `KeepsayCore` to the UniFFI-generated bindings of
// keepsay-core (core/src/ffi.rs, docs/changes/0014). Compiled only when
// apple/scripts/build-core.sh produced the XCFramework and
// `Sources/KeepsayCoreBindings/keepsay_core.swift`.
//
// NOT YET COMPILED IN CI: generating the bindings needs uniffi's `cli`
// feature, which awaits approval (0014 D-12, docs/changes/0015). The names
// below follow UniFFI's Swift conventions (lowerCamelCase functions, fields
// and enum cases; flat error variants in UpperCamelCase) and are checked the
// first time the bindings are generated.
//
// Every FFI function blocks (each builds its own current-thread runtime), so
// all calls run on a detached task, never on the main thread.
// INV-2: no function here returns the key.

#if canImport(KeepsayCoreBindings)
import Foundation
import KeepsayCoreBindings

final class UniffiCore: KeepsayCore, @unchecked Sendable {
    /// Settings are in memory until core exposes config read/write over the
    /// FFI (docs/changes/0015 "core needs").
    private let lock = NSLock()
    private var settings = AppSettings()
    private let initError: CoreFailure?

    init() {
        var failure: CoreFailure?
        do {
            let dir = try Self.dataDirectory()
            try `init`(dataDir: dir.path)
            try setUtcOffset(seconds: Int32(TimeZone.current.secondsFromGMT()))
        } catch {
            failure = Self.map(error)
        }
        initError = failure
    }

    /// ~/Library/Application Support/keepsay (macOS) or the app container's
    /// Application Support (iOS). Holds config.json, budget.json, models/.
    static func dataDirectory() throws -> URL {
        let base = try FileManager.default.url(
            for: .applicationSupportDirectory, in: .userDomainMask, appropriateFor: nil, create: true)
        let dir = base.appendingPathComponent("keepsay", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        return dir
    }

    private static func offMain<T: Sendable>(_ body: @escaping @Sendable () throws -> T) async throws -> T {
        try await Task.detached(priority: .userInitiated) {
            do { return try body() } catch { throw UniffiCore.map(error) }
        }.value
    }

    // MARK: key

    func storeKey(_ key: String) async throws -> String {
        try await Self.offMain { try KeepsayCoreBindings.storeKey(key: key) }
    }

    func hasKey() -> Bool { KeepsayCoreBindings.hasKey() }
    func keyFingerprint() -> String? { KeepsayCoreBindings.keyFingerprint() }

    func deleteKey() throws {
        do { try KeepsayCoreBindings.deleteKey() } catch { throw Self.map(error) }
    }

    func verifyKey() async throws {
        try await Self.offMain { try KeepsayCoreBindings.verifyKey() }
    }

    // MARK: dictation

    func polish(_ transcript: String, request: PolishRequest) async throws -> PolishOutcome {
        let opts = PolishOptions(
            mode: Self.mode(request.mode), translateTo: request.translateTo.map(Self.target),
            glossary: request.glossary)
        let r = try await Self.offMain { try KeepsayCoreBindings.polish(transcript: transcript, opts: opts) }
        return PolishOutcome(
            text: r.text, fellBackToRaw: r.fellBackToRaw, fallbackCause: r.fallbackReason.map(Self.cause),
            usage: r.usage.map { TokenUsage(inputTokens: $0.inputTokens, outputTokens: $0.outputTokens) })
    }

    func transcribe(pcm: [Float], sampleRate: UInt32, glossary: [String]) async throws -> String {
        let opts = SttOptions(language: "zh-TW", glossary: glossary)
        return try await Self.offMain {
            try KeepsayCoreBindings.transcribe(pcm: pcm, sampleRate: sampleRate, opts: opts)
        }
    }

    // MARK: budget, models

    func budgetStatus() -> BudgetSnapshot {
        guard let b = try? KeepsayCoreBindings.budgetStatus() else { return .empty }
        return BudgetSnapshot(
            usage: TokenUsage(inputTokens: b.monthUsage.inputTokens, outputTokens: b.monthUsage.outputTokens),
            estimatedUSD: b.estimatedUsd, softLimitUSD: b.softLimitUsd, hardLimitUSD: b.hardLimitUsd)
    }

    func models() -> [ModelInfo] {
        // Installed state is not exposed by the FFI yet (0015 "core needs").
        UnavailableCore().models()
    }

    func downloadModel(id: String, progress: @escaping @Sendable (Double) -> Void) async throws {
        let sink = ProgressSink(progress)
        try await Self.offMain { try KeepsayCoreBindings.downloadModel(id: id, progress: sink) }
    }

    func importModel(id: String, from url: URL) async throws {
        try await Self.offMain { try KeepsayCoreBindings.importModel(id: id, path: url.path) }
    }

    // MARK: settings

    func loadSettings() -> AppSettings {
        lock.lock()
        defer { lock.unlock() }
        return settings
    }

    func saveSettings(_ settings: AppSettings) throws {
        lock.lock()
        defer { lock.unlock() }
        self.settings = settings
    }

    // MARK: conversions

    static func mode(_ m: DictationMode) -> Mode {
        switch m {
        case .raw: .raw
        case .light: .light
        case .polish: .polish
        case .formal: .formal
        }
    }

    static func target(_ t: TranslationTarget) -> TargetLang {
        switch t {
        case .en: .en
        case .zhTW: .zhTw
        case .ja: .ja
        }
    }

    static func cause(_ r: FallbackReason) -> FallbackCause {
        switch r {
        case .translationNeedsApi: .translationNeedsAPI
        case .noKey: .noKey
        case .keyInvalid: .keyInvalid
        case .hardLimit: .hardLimit
        case .budgetExhausted: .budgetExhausted
        case .network: .network
        case .timeout: .timeout
        case .rateLimited: .rateLimited
        case .apiError: .apiError
        case .guardRejected: .guardRejected
        @unknown default: .apiError
        }
    }

    static func map(_ error: Error) -> CoreFailure {
        if let f = error as? CoreFailure { return f }
        guard let e = error as? CoreError else { return .io }
        switch e {
        case .KeyInvalid: return .keyInvalid
        case .KeyFormat: return .keyFormat
        case .KeyNotFound: return .keyNotFound
        case .VaultUnavailable: return .vaultUnavailable
        case .VaultNotEntitled: return .unsupported
        case .DisallowedHost: return .disallowedHost
        case .Network: return .network
        case .Timeout: return .timeout
        case .RateLimited: return .rateLimited
        case .BudgetExhausted: return .budgetExhausted
        case .InvalidResponse: return .invalidResponse
        case .ModelNotAllowed: return .modelNotAllowed
        case .GuardRejected: return .guardRejected
        case .HardLimitReached: return .hardLimitReached
        case .AudioFormat: return .audioFormat
        case .Stt: return .stt
        case .ModelUnknown: return .modelUnknown
        case .ModelHash: return .modelHash
        case .ModelTooLarge: return .modelTooLarge
        case .Config: return .config
        case .Unsupported: return .unsupported
        case .NotInitialized: return .coreNotLinked
        default: return .io
        }
    }
}

/// Swift side of core's foreign `DownloadProgress` trait (two numbers only).
private final class ProgressSink: DownloadProgress, @unchecked Sendable {
    private let report: @Sendable (Double) -> Void

    init(_ report: @escaping @Sendable (Double) -> Void) {
        self.report = report
    }

    func onProgress(received: UInt64, total: UInt64) {
        report(total == 0 ? 0 : min(1, Double(received) / Double(total)))
    }
}
#endif
