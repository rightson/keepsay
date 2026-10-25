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
    static let shared = AppModel(core: KitCoreService.live(), recorder: MemoryRecorder(), store: SettingsStore())

    enum OnboardingStep { case key, model, finished }

    // Dependencies
    @ObservationIgnored let core: CoreService
    @ObservationIgnored private let recorder: AudioCapturing
    @ObservationIgnored private let store: SettingsStore
    @ObservationIgnored private var work: Task<Void, Never>?

    // State
    var settings: AppSettings {
        didSet {
            guard settings != oldValue else { return }
            store.save(settings)
            let s = settings
            Task { await core.apply(s) }
        }
    }
    var onboarding: OnboardingStep
    /// False in development builds without the Rust core (raw only).
    var coreLinked = false
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
        Task {
            await core.apply(s)
            await refresh()
        }
    }

    /// Re-reads key, budget and model state from the core (off the main
    /// thread inside the core service).
    func refresh() async {
        apply(await core.status())
    }

    func apply(_ status: CoreStatus) {
        coreLinked = status.coreLinked
        hasKey = status.hasKey
        fingerprint = status.fingerprint
        budget = status.budget
        installedModels = status.installedModels
        if !hasKey || budget.hardLimited { selectedMode = .raw }
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
            await refresh()
            if selectedMode == .raw && isModeAvailable(settings.defaultMode) { selectedMode = settings.defaultMode }
            return true
        } catch let e as CoreServiceError {
            keyError = e.message
        } catch {
            keyError = CoreServiceError.keyInvalid.message
        }
        await refresh()
        return false
    }

    func deleteKey() async {
        do {
            try await core.deleteKey()
            showToast("已刪除金鑰，改用原文模式")
        } catch {
            showToast("無法刪除金鑰，請再試一次")
        }
        await refresh()
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
                    Task { @MainActor in
                        // Late progress after completion must not resurrect the bar.
                        guard let self, self.downloadProgress != nil else { return }
                        self.downloadProgress = min(1, max(0, p))
                    }
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

    /// Plans from the last core status (refreshed on foreground, after key
    /// changes and after each dictation). The core enforces the key and the
    /// hard limit again on every call, so a stale status cannot bypass them.
    func start(_ action: DictationAction) {
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
        let glossary = settings.glossary
        work = Task {
            let transcript: String
            do {
                transcript = try await core.transcribe(pcm: pcm, sampleRate: rate, glossary: glossary)
            } catch {
                send(.transcriptionFailed(DictationFailure.from(error)))
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
            UINotificationFeedbackGenerator().notificationOccurred(out.fallback == nil ? .success : .warning)
            if out.plan.needsAPI {
                Task {
                    await refresh()
                    warnSoftLimitIfNeeded()
                }
            }
        }
        return true
    }

    /// FR-6 / mobile.html "到了提醒一次": one notice per calendar month.
    func warnSoftLimitIfNeeded(now: Date = Date()) {
        guard SoftLimitNotice.shouldWarn(budget: budget, lastWarnedMonth: settings.softLimitWarnedMonth, now: now)
        else { return }
        settings.softLimitWarnedMonth = SoftLimitNotice.monthKey(now)
        showToast(SoftLimitNotice.message(budget))
    }

    func showToast(_ message: String) {
        toast = message
        Task {
            try? await Task.sleep(for: .seconds(1.6))
            if toast == message { toast = nil }
        }
    }
}
