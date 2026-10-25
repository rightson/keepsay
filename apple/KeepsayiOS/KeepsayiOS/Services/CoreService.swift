// The app's view of keepsay-core (PROPOSAL §6.1).
//
// INV-2: there is deliberately no method that returns the key. `storeKey`
// takes it once and returns the fingerprint ("…a1B2"); everything else works
// on the key inside the core. INV-1: the core is the only code that touches
// the Keychain; this app has no Keychain code and no shared access group.
//
// The real implementation adapts KeepsayKit's core bridge (UniFFI). Until the
// XCFramework is linked, `UnlinkedCore` keeps the app runnable in raw-only
// form and every API path falls back to raw with `.coreUnavailable`.

import Foundation

enum CoreServiceError: Error, Equatable {
    /// Not an `sk-ant-` key (FR-1 format check). Carries no content.
    case keyFormat
    /// Online verification (GET /v1/models) rejected the key; not stored.
    case keyInvalid
    case network
    case modelMissing
    case downloadFailed
    case hashMismatch
    case transcriptionFailed
    case coreUnavailable

    var message: String {
        switch self {
        case .keyFormat: return "格式不符：金鑰應以 sk-ant- 開頭。"
        case .keyInvalid: return "驗證失敗，金鑰沒有儲存。"
        case .network: return "無法連線，請稍後再試。"
        case .modelMissing: return "尚未下載語音模型。"
        case .downloadFailed: return "下載失敗，請再試一次。"
        case .hashMismatch: return "SHA-256 不符，已刪除檔案。"
        case .transcriptionFailed: return "轉寫失敗。"
        case .coreUnavailable: return "核心元件尚未連結（開發版）。"
        }
    }
}

protocol CoreService: AnyObject, Sendable {
    func hasKey() -> Bool
    /// "…a1B2" or nil. The only key-derived value that leaves the core.
    func keyFingerprint() -> String?
    /// Format check → online verification → Keychain. Returns the fingerprint.
    /// The caller must drop its copy of `key` right after calling.
    func storeKey(_ key: String) async throws -> String
    func deleteKey() throws

    /// 16 kHz mono f32 in memory (INV-5). The transcript is user content.
    func transcribe(pcm: [Float], sampleRate: UInt32, modelID: String, glossary: [String]) async throws -> String
    /// Never throws: every failure is a raw fallback (FR-4).
    func polish(transcript: String, mode: PolishMode, translateTo: TargetLanguage?, glossary: [String]) async -> PolishOutcome

    func budgetStatus() -> BudgetSnapshot
    func isModelInstalled(_ id: String) -> Bool
    /// User-initiated only (INV-7). The core verifies SHA-256.
    func downloadModel(_ id: String, progress: @escaping @Sendable (Double) -> Void) async throws
}

/// Placeholder until the core XCFramework is linked through KeepsayKit.
/// It never stores, keeps, or sends anything.
final class UnlinkedCore: CoreService, @unchecked Sendable {
    func hasKey() -> Bool { false }
    func keyFingerprint() -> String? { nil }
    func storeKey(_ key: String) async throws -> String { throw CoreServiceError.coreUnavailable }
    func deleteKey() throws {}
    func transcribe(pcm: [Float], sampleRate: UInt32, modelID: String, glossary: [String]) async throws -> String {
        throw CoreServiceError.coreUnavailable
    }
    func polish(transcript: String, mode: PolishMode, translateTo: TargetLanguage?, glossary: [String]) async -> PolishOutcome {
        .raw(transcript, reason: .coreUnavailable)
    }
    func budgetStatus() -> BudgetSnapshot { .empty }
    func isModelInstalled(_ id: String) -> Bool { false }
    func downloadModel(_ id: String, progress: @escaping @Sendable (Double) -> Void) async throws {
        throw CoreServiceError.coreUnavailable
    }
}
