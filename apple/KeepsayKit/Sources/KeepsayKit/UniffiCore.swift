// Adapter from `KeepsayCore` to the UniFFI-generated bindings of
// keepsay-core. Compiled only when apple/scripts/build-core.sh produced the
// XCFramework and `Sources/KeepsayCoreBindings/keepsay_core.swift`.
//
// Wiring to the generated functions lands once core/src/ffi.rs exports them
// (US-M1-22, docs/changes "m1-integration"). Until then this adapter
// forwards to `UnavailableCore` so a core-linked build still compiles.

#if canImport(KeepsayCoreBindings)
import Foundation
import KeepsayCoreBindings

final class UniffiCore: KeepsayCore, @unchecked Sendable {
    private let fallback = UnavailableCore()

    func storeKey(_ key: String) async throws -> String { try await fallback.storeKey(key) }
    func hasKey() -> Bool { fallback.hasKey() }
    func keyFingerprint() -> String? { fallback.keyFingerprint() }
    func deleteKey() throws { try fallback.deleteKey() }
    func verifyKey() async throws { try await fallback.verifyKey() }

    func polish(_ transcript: String, request: PolishRequest) async throws -> PolishOutcome {
        try await fallback.polish(transcript, request: request)
    }

    func transcribe(pcm: [Float], sampleRate: UInt32, glossary: [String]) async throws -> String {
        try await fallback.transcribe(pcm: pcm, sampleRate: sampleRate, glossary: glossary)
    }

    func budgetStatus() -> BudgetSnapshot { fallback.budgetStatus() }
    func models() -> [ModelInfo] { fallback.models() }

    func downloadModel(id: String, progress: @escaping @Sendable (Double) -> Void) async throws {
        try await fallback.downloadModel(id: id, progress: progress)
    }

    func importModel(id: String, from url: URL) async throws {
        try await fallback.importModel(id: id, from: url)
    }

    func loadSettings() -> AppSettings { fallback.loadSettings() }
    func saveSettings(_ settings: AppSettings) throws { try fallback.saveSettings(settings) }
}
#endif
