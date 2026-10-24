// App state and the effects of the dictation state machine.
//
// Rules this file keeps:
// - INV-2: the key passes through `submitKey` once and is not stored here.
// - INV-5: audio goes recorder → core in memory and is dropped after.
// - Nothing here logs; user content (transcript, output, glossary) is only
//   held in memory for the current screen and cleared on dismiss.

import Foundation
import Observation
import UIKit

@MainActor
@Observable
final class AppModel {
    static let shared = AppModel(core: UnlinkedCore(), recorder: MemoryRecorder(), store: SettingsStore())

    enum OnboardingStep { case key, model, finished }

    // Dependencies
    @ObservationIgnored let core: CoreService
    @ObservationIgnored private let recorder: AudioCapturing
    @ObservationIgnored private let store: SettingsStore
    @ObservationIgnored private var work: Task<Void, Never>?

    // State
    var settings: AppSettings { didSet { store.save(settings) } }
    var onboarding: OnboardingStep
    var hasKey = false
    var fingerprint: String?
    var budget = BudgetSnapshot.empty
    var selectedMode: PolishMode
    var phase: DictationPhase = .idle
    var level: Float = 0
    var copiedUntil: Date?
    var toast: String?

    // Key entry
    var keyBusy = false
    var keyError: String?

    // Model download
    var downloadProgress: Double?
    var downloadError: String?
    var installedModels: Set<String> = []

    init(core: CoreService, recorder: AudioCapturing, store: SettingsStore) {
        self.core = core
        self.recorder = recorder
        self.store = store
        let s = store.load()
        settings = s
        selectedMode = s.defaultMode
        onboarding = s.onboarded ? .finished : .key
        refresh()
    }

    func refresh() {
        hasKey = core.hasKey()
        fingerprint = core.keyFingerprint()
        budget = core.budgetStatus()
        installedModels = Set(ModelCatalog.all.map(\.id).filter { core.isModelInstalled($0) })
        if !hasKey { selectedMode = .raw }
    }

    /// Modes the home selector allows (FR-1: without a key, raw only).
    func isModeAvailable(_ mode: PolishMode) -> Bool { mode == .raw || (hasKey && !budget.hardLimited) }

    // MARK: Key (FR-1)

    /// `key` is the SecureField text; the caller has already cleared the
    /// field. This function keeps no copy.
    func submitKey(_ key: String) async -> Bool {
        keyBusy = true
        keyError = nil
        defer { keyBusy = false }
        do {
            fingerprint = try await core.storeKey(key)
            hasKey = true
            if selectedMode == .raw { selectedMode = settings.defaultMode }
            return true
        } catch let e as CoreServiceError {
            keyError = e.message
        } catch {
            keyError = CoreServiceError.keyInvalid.message
        }
        refresh()
        return false
    }

    func deleteKey() {
        try? core.deleteKey()
        refresh()
        showToast("已刪除金鑰，改用原文模式")
    }

    func skipKey() {
        selectedMode = .raw
        onboarding = .model
    }

    func finishOnboarding() {
        settings.onboarded = true
        onboarding = .finished
    }

    // MARK: Models (INV-7: user-initiated only)

    func download(_ model: SttModelInfo) {
        guard downloadProgress == nil else { return }
        downloadProgress = 0
        downloadError = nil
        Task {
            do {
                try await core.downloadModel(model.id) { [weak self] p in
                    Task { @MainActor in self?.downloadProgress = min(1, max(0, p)) }
                }
                installedModels.insert(model.id)
                settings.sttModel = model.id
            } catch let e as CoreServiceError {
                downloadError = e.message
            } catch {
                downloadError = CoreServiceError.downloadFailed.message
            }
            downloadProgress = nil
        }
    }

    // MARK: Dictation

    /// In-app buttons and App Intents both land here. Toggle semantics
    /// (FR-2): while recording, any action stops; the action at start decides
    /// translation.
    func trigger(_ action: DictationAction) {
        switch phase {
        case .recording:
            stop()
        case .idle, .done, .failed:
            start(action)
        case .transcribing, .polishing:
            break
        }
    }

    func start(_ action: DictationAction) {
        refresh()
        let plan = DictationPlan.make(
            action: action, selectedMode: selectedMode, target: settings.targetLanguage,
            hasKey: hasKey, hardLimited: budget.hardLimited)
        guard send(.start(plan, at: Date())) else { return }
        copiedUntil = nil
        work = Task {
            guard await recorder.requestPermission() else {
                send(.recordingFailed(.microphoneDenied))
                return
            }
            do {
                try recorder.start { [weak self] peak in
                    Task { @MainActor in self?.level = peak }
                }
            } catch {
                send(.recordingFailed(.audioUnavailable))
            }
        }
    }

    func stop() {
        guard case .recording(let plan, _) = phase else { return }
        let pcm = recorder.stop()
        level = 0
        guard send(.stop) else { return }
        let rate = recorder.sampleRate
        let modelID = settings.sttModel
        let glossary = settings.glossary
        work = Task {
            guard core.isModelInstalled(modelID) else {
                send(.transcriptionFailed(.modelMissing))
                return
            }
            let transcript: String
            do {
                transcript = try await core.transcribe(pcm: pcm, sampleRate: rate, modelID: modelID, glossary: glossary)
            } catch {
                send(.transcriptionFailed(.transcriptionFailed))
                return
            }
            guard send(.transcribed(transcript)) else { return }
            if case .polishing = phase {
                let outcome = await core.polish(
                    transcript: transcript, mode: plan.mode, translateTo: plan.translateTo, glossary: glossary)
                send(.polished(outcome))
            }
        }
    }

    func cancel() {
        recorder.cancel()
        level = 0
        work?.cancel()
        send(.cancel)
    }

    func dismiss() {
        send(.dismiss)
        copiedUntil = nil
    }

    func copyAgain() {
        guard case .done(let out) = phase else { return }
        copiedUntil = ClipboardPolicy.copy(out.text)
        showToast("已再次複製・60 秒後清除")
    }

    // MARK: Internals

    @discardableResult
    private func send(_ event: DictationEvent) -> Bool {
        guard let next = DictationMachine.reduce(phase, event) else { return false }
        phase = next
        if case .done(let out) = next {
            copiedUntil = ClipboardPolicy.copy(out.text)
            budget = core.budgetStatus()
            UINotificationFeedbackGenerator().notificationOccurred(out.fallback == nil ? .success : .warning)
        }
        return true
    }

    func showToast(_ message: String) {
        toast = message
        Task {
            try? await Task.sleep(for: .seconds(1.6))
            if toast == message { toast = nil }
        }
    }
}
