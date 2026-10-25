// The app's view of keepsay-core (PROPOSAL §6.1).
//
// INV-2: there is deliberately no method that returns the key. `storeKey`
// takes it once and returns the fingerprint ("…a1B2"); everything else works
// on the key inside the core. INV-1: the core is the only code that touches
// the Keychain; this app has no Keychain code and no shared access group.
//
// Every method is async: the core's FFI calls block (each builds its own
// runtime, and even `has_key` reads Keychain attributes), so implementations
// run them off the main thread. `KitCoreService` (KitCoreService.swift) is
// the implementation over KeepsayKit's core bridge.

import Foundation

enum CoreServiceError: Error, Equatable {
    /// Not an `sk-ant-` key (FR-1 format check). Carries no content.
    case keyFormat
    /// Online verification (GET /v1/models) rejected the key; not stored.
    case keyInvalid
    /// The Keychain refused the item (e.g. unsigned build, -34018).
    case vaultUnavailable
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
        case .vaultUnavailable: return "無法寫入鑰匙圈（未簽章的開發版），金鑰沒有儲存。"
        case .network: return "無法連線，金鑰沒有儲存。請稍後再試。"
        case .modelMissing: return "尚未下載語音模型。"
        case .downloadFailed: return "下載失敗，請再試一次。"
        case .hashMismatch: return "SHA-256 不符，已刪除檔案。"
        case .transcriptionFailed: return "轉寫失敗。"
        case .coreUnavailable: return "核心元件尚未連結（開發版）。"
        }
    }
}

/// Everything the UI shows about the core, read in one off-main hop.
struct CoreStatus: Equatable, Sendable {
    /// False in development builds without the Rust core (raw only).
    var coreLinked: Bool
    var hasKey: Bool
    /// "…a1B2" or nil. The only key-derived value that leaves the core.
    var fingerprint: String?
    var budget: BudgetSnapshot
    var installedModels: Set<String>

    static let unlinked = CoreStatus(
        coreLinked: false, hasKey: false, fingerprint: nil, budget: .empty, installedModels: [])
}

protocol CoreService: AnyObject, Sendable {
    func status() async -> CoreStatus

    /// Format check → online verification → Keychain. Returns the fingerprint.
    /// The caller must drop its copy of `key` right after calling.
    func storeKey(_ key: String) async throws -> String
    func deleteKey() async throws

    /// 16 kHz mono f32 in memory (INV-5). The transcript is user content.
    /// The model is the one selected in the core's settings.
    func transcribe(pcm: [Float], sampleRate: UInt32, glossary: [String]) async throws -> String
    /// Never throws: every failure is a raw fallback (FR-4).
    func polish(transcript: String, mode: PolishMode, translateTo: TargetLanguage?, glossary: [String]) async -> PolishOutcome

    /// User-initiated only (INV-7). The core verifies size and SHA-256.
    func downloadModel(_ id: String, progress: @escaping @Sendable (Double) -> Void) async throws

    /// Hands the non-secret settings to the core (mode, glossary, limits,
    /// prices, STT model). Best effort.
    func apply(_ settings: AppSettings) async
}
