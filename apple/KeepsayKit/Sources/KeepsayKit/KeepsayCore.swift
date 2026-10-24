// The only way Swift talks to keepsay-core (PROPOSAL §6.1).
//
// INV-2: no method returns the key. `storeKey` takes it once, at the moment
// the user submits the SecureField, and returns the fingerprint (`…a1B2`).
// The caller must clear its field before awaiting anything else.

import Foundation

public protocol KeepsayCore: AnyObject, Sendable {
    /// Format check → online verification → store → fingerprint (FR-1).
    func storeKey(_ key: String) async throws -> String
    func hasKey() -> Bool
    func keyFingerprint() -> String?
    func deleteKey() throws
    func verifyKey() async throws

    /// Never throws for API failures: core returns the raw transcript with a
    /// fallback cause instead (FR-4). Throws only for programming errors.
    func polish(_ transcript: String, request: PolishRequest) async throws -> PolishOutcome
    /// 16 kHz mono f32 PCM, in memory only (INV-5).
    func transcribe(pcm: [Float], sampleRate: UInt32, glossary: [String]) async throws -> String

    func budgetStatus() -> BudgetSnapshot
    func models() -> [ModelInfo]
    func downloadModel(id: String, progress: @escaping @Sendable (Double) -> Void) async throws
    /// Manual import (§8); core verifies the SHA-256 before accepting it.
    func importModel(id: String, from url: URL) async throws

    func loadSettings() -> AppSettings
    func saveSettings(_ settings: AppSettings) throws
}

public enum CoreBridge {
    /// The linked Rust core, or `UnavailableCore` in builds without it.
    public static func make() -> KeepsayCore {
        #if canImport(KeepsayCoreBindings)
        return UniffiCore()
        #else
        return UnavailableCore()
        #endif
    }

    public static var isCoreLinked: Bool {
        #if canImport(KeepsayCoreBindings)
        return true
        #else
        return false
        #endif
    }
}
