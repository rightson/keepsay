// `CoreService` over KeepsayKit's core bridge (apple/KeepsayKit, M2).
//
// `CoreBridge.make()` returns the UniFFI-backed core when
// apple/scripts/build-core.sh has produced the XCFramework and bindings,
// and KeepsayKit's `UnavailableCore` otherwise (raw only, nothing stored).
// This file is the only one in the app that imports KeepsayKit, so the
// app's own mirror types (`AppSettings`, `PolishOutcome`, ...) and
// KeepsayKit's same-named types never meet elsewhere; here KeepsayKit's are
// always written module-qualified.
//
// Rules:
// - INV-2: the key is passed through once (`storeKey`); nothing returns it.
// - Every call into the core runs on a detached task, never the main
//   thread: the FFI blocks (network, Keychain, whisper).
// - Errors are mapped to content-free cases; no error text is kept.

import Foundation
import KeepsayKit

final class KitCoreService: CoreService, @unchecked Sendable {
    private let kit: any KeepsayKit.KeepsayCore
    private let linked: Bool
    private let lock = NSLock()
    /// Models downloaded and verified during this run. KeepsayKit does not
    /// report installed state from disk yet (docs/changes/0017).
    private var downloaded: Set<String> = []

    init(kit: any KeepsayKit.KeepsayCore, linked: Bool) {
        self.kit = kit
        self.linked = linked
    }

    /// The linked Rust core, or the raw-only stand-in in builds without it.
    static func live() -> KitCoreService {
        KitCoreService(kit: KeepsayKit.CoreBridge.make(), linked: KeepsayKit.CoreBridge.isCoreLinked)
    }

    /// The raw-only stand-in, for tests and previews.
    static func unlinked() -> KitCoreService {
        KitCoreService(kit: KeepsayKit.UnavailableCore(), linked: false)
    }

    private func offMain<T: Sendable>(_ body: @escaping @Sendable () -> T) async -> T {
        await Task.detached(priority: .userInitiated, operation: body).value
    }

    private func offMainThrowing<T: Sendable>(_ body: @escaping @Sendable () async throws -> T) async throws -> T {
        try await Task.detached(priority: .userInitiated, operation: body).value
    }

    // MARK: status

    func status() async -> CoreStatus {
        let kit = kit
        let linked = linked
        let extra = lock.withLock { downloaded }
        return await offMain {
            let hasKey = kit.hasKey()
            let b = kit.budgetStatus()
            let installed = Set(kit.models().filter(\.installed).map(\.id)).union(extra)
            return CoreStatus(
                coreLinked: linked,
                hasKey: hasKey,
                fingerprint: hasKey ? kit.keyFingerprint() : nil,
                budget: BudgetSnapshot(
                    estimatedUSD: b.estimatedUSD, softLimitUSD: b.softLimitUSD, hardLimitUSD: b.hardLimitUSD),
                installedModels: installed)
        }
    }

    // MARK: key (FR-1)

    func storeKey(_ key: String) async throws -> String {
        let kit = kit
        do {
            return try await offMainThrowing { try await kit.storeKey(key) }
        } catch {
            throw Self.keyError(error)
        }
    }

    func deleteKey() async throws {
        let kit = kit
        do {
            try await offMainThrowing { try kit.deleteKey() }
        } catch {
            throw CoreServiceError.vaultUnavailable
        }
    }

    // MARK: dictation

    func transcribe(pcm: [Float], sampleRate: UInt32, glossary: [String]) async throws -> String {
        let kit = kit
        do {
            return try await offMainThrowing {
                try await kit.transcribe(pcm: pcm, sampleRate: sampleRate, glossary: glossary)
            }
        } catch {
            throw Self.sttError(error)
        }
    }

    func polish(transcript: String, mode: PolishMode, translateTo: TargetLanguage?, glossary: [String]) async -> PolishOutcome {
        let kit = kit
        let request = KeepsayKit.PolishRequest(
            mode: Self.kitMode(mode), translateTo: translateTo.map(Self.kitTarget), glossary: glossary)
        do {
            let r = try await offMainThrowing { try await kit.polish(transcript, request: request) }
            return PolishOutcome(
                text: r.text,
                fellBackToRaw: r.fellBackToRaw,
                fallbackReason: r.fallbackCause.map(Self.reason) ?? (r.fellBackToRaw ? .apiError : nil),
                usage: r.usage.map { TokenUsage(inputTokens: $0.inputTokens, outputTokens: $0.outputTokens) })
        } catch {
            return .raw(transcript, reason: Self.polishReason(error))
        }
    }

    // MARK: models (INV-7)

    func downloadModel(_ id: String, progress: @escaping @Sendable (Double) -> Void) async throws {
        let kit = kit
        do {
            try await offMainThrowing { try await kit.downloadModel(id: id, progress: progress) }
        } catch {
            throw Self.downloadError(error)
        }
        _ = lock.withLock { downloaded.insert(id) }
    }

    // MARK: settings

    func apply(_ settings: AppSettings) async {
        let kit = kit
        let ok: Bool = await offMain {
            let s = Self.kitSettings(settings, base: kit.loadSettings())
            return (try? kit.saveSettings(s)) != nil
        }
        _ = ok
    }

    // MARK: conversions

    static func kitMode(_ m: PolishMode) -> KeepsayKit.DictationMode {
        switch m {
        case .raw: return .raw
        case .light: return .light
        case .polish: return .polish
        case .formal: return .formal
        }
    }

    static func kitTarget(_ t: TargetLanguage) -> KeepsayKit.TranslationTarget {
        switch t {
        case .en: return .en
        case .zhTW: return .zhTW
        case .ja: return .ja
        }
    }

    static func kitSettings(_ s: AppSettings, base: KeepsayKit.AppSettings) -> KeepsayKit.AppSettings {
        var k = base
        k.mode = kitMode(s.defaultMode)
        k.translateTo = kitTarget(s.targetLanguage)
        k.sttModel = s.sttModel
        k.glossary = s.glossary
        k.prices = KeepsayKit.PriceTable(inputUSDPerMTok: s.inputPricePerMTok, outputUSDPerMTok: s.outputPricePerMTok)
        k.softLimitUSD = s.softLimitUSD
        k.hardLimitUSD = s.hardLimitUSD
        return k
    }

    static func reason(_ c: KeepsayKit.FallbackCause) -> FallbackReason {
        switch c {
        case .translationNeedsAPI: return .translationNeedsApi
        case .noKey: return .noKey
        case .keyInvalid: return .keyInvalid
        case .hardLimit: return .hardLimit
        case .budgetExhausted: return .budgetExhausted
        case .network: return .network
        case .timeout: return .timeout
        case .rateLimited: return .rateLimited
        case .apiError: return .apiError
        case .guardRejected: return .guardRejected
        }
    }

    private static func failure(_ error: Error) -> KeepsayKit.CoreFailure? {
        error as? KeepsayKit.CoreFailure
    }

    static func keyError(_ error: Error) -> CoreServiceError {
        switch failure(error) {
        case .keyFormat: return .keyFormat
        case .keyInvalid, .keyNotFound: return .keyInvalid
        case .vaultUnavailable, .unsupported: return .vaultUnavailable
        case .network, .timeout, .rateLimited, .disallowedHost: return .network
        case .coreNotLinked: return .coreUnavailable
        default: return .keyInvalid
        }
    }

    static func sttError(_ error: Error) -> CoreServiceError {
        switch failure(error) {
        case .modelUnknown, .modelHash: return .modelMissing
        case .coreNotLinked: return .coreUnavailable
        default: return .transcriptionFailed
        }
    }

    static func downloadError(_ error: Error) -> CoreServiceError {
        switch failure(error) {
        case .modelHash, .modelTooLarge: return .hashMismatch
        case .network, .timeout, .disallowedHost: return .network
        case .coreNotLinked: return .coreUnavailable
        default: return .downloadFailed
        }
    }

    static func polishReason(_ error: Error) -> FallbackReason {
        switch failure(error) {
        case .coreNotLinked: return .coreUnavailable
        case .hardLimitReached: return .hardLimit
        case .keyNotFound: return .noKey
        case .keyInvalid: return .keyInvalid
        case .budgetExhausted: return .budgetExhausted
        case .network: return .network
        case .timeout: return .timeout
        case .rateLimited: return .rateLimited
        case .guardRejected: return .guardRejected
        default: return .apiError
        }
    }
}
