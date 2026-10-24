// App state and the dictation pipeline: hotkey → record → transcribe →
// polish → insert, driven by DictationStateMachine.
//
// Rules: never log or print transcripts, model output, glossary or the key.
// The key passes through `submitKey` exactly once (INV-2).

import AVFoundation
import AppKit
import KeepsayKit
import KeepsayMacSupport
import SwiftUI

@MainActor
final class AppModel: ObservableObject {
    let core: KeepsayCore

    @Published private(set) var phase: DictationPhase = .idle
    @Published private(set) var level: Float = 0
    @Published private(set) var recordingStartedAt: Date?
    @Published var settings: AppSettings {
        didSet { settingsChanged(from: oldValue) }
    }

    @Published private(set) var fingerprint: String?
    @Published private(set) var budget: BudgetSnapshot = .empty
    @Published private(set) var models: [ModelInfo] = []
    @Published private(set) var downloadProgress: [String: Double] = [:]
    @Published private(set) var hotkeyStatus: HotkeyController.Status = .none
    @Published private(set) var hotkeyRegistrationFailed = false
    @Published private(set) var accessibilityTrusted = false
    @Published private(set) var microphoneStatus: AVAuthorizationStatus = .notDetermined
    /// Last error from a settings action, for the settings window.
    @Published var settingsError: String?

    private var machine: DictationStateMachine
    private let recorder = PCMRecorder()
    private let inserter = TextInserter()
    private lazy var hotkeys = HotkeyController(
        onAction: { [weak self] action in self?.send(.trigger(action)) },
        onCancel: { [weak self] in self?.send(.cancel) })
    private var pendingTranscript: String?
    private var dismissTask: Task<Void, Never>?
    let overlay = OverlayController()

    init(core: KeepsayCore = CoreBridge.make()) {
        self.core = core
        let s = core.loadSettings()
        self.settings = s
        self.machine = DictationStateMachine(defaultMode: s.mode)
        recorder.onLevel = { [weak self] value in
            Task { @MainActor in self?.level = value }
        }
    }

    func start() {
        overlay.attach(model: self)
        refreshAll()
        hotkeys.install(settings: settings)
        hotkeyStatus = hotkeys.status
        hotkeyRegistrationFailed = hotkeys.registrationFailed
    }

    func refreshAll() {
        fingerprint = core.keyFingerprint()
        budget = core.budgetStatus()
        models = core.models()
        accessibilityTrusted = Permissions.accessibilityTrusted
        microphoneStatus = AVCaptureDevice.authorizationStatus(for: .audio)
    }

    func reinstallHotkeys() {
        accessibilityTrusted = Permissions.accessibilityTrusted
        hotkeys.install(settings: settings)
        hotkeyStatus = hotkeys.status
        hotkeyRegistrationFailed = hotkeys.registrationFailed
    }

    var hasKey: Bool { fingerprint != nil }

    // MARK: - Dictation

    func send(_ event: DictationEvent) {
        let effects = machine.handle(event)
        phase = machine.phase
        overlay.update(phase: phase)
        for effect in effects { perform(effect) }
    }

    private func perform(_ effect: DictationEffect) {
        switch effect {
        case .startRecording:
            dismissTask?.cancel()
            pendingTranscript = nil
            startRecording()
        case .enableCancelKey(let on):
            hotkeys.setCancelKeyEnabled(on)
        case .discardRecording:
            recorder.cancel()
            recordingStartedAt = nil
            level = 0
        case .stopAndTranscribe:
            let pcm = recorder.stop()
            recordingStartedAt = nil
            level = 0
            transcribe(pcm)
        case .polish(let action):
            polish(action)
        case .insertRaw:
            insert(pendingTranscript ?? "")
        case .insert(let outcome):
            insert(outcome.text)
        case .scheduleDismiss:
            pendingTranscript = nil
            budget = core.budgetStatus()
            dismissTask?.cancel()
            dismissTask = Task { [weak self] in
                try? await Task.sleep(nanoseconds: 2_500_000_000)
                guard !Task.isCancelled else { return }
                self?.send(.dismiss)
            }
        }
    }

    private func startRecording() {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            break
        case .notDetermined:
            AVCaptureDevice.requestAccess(for: .audio) { granted in
                Task { @MainActor [weak self] in
                    self?.microphoneStatus = granted ? .authorized : .denied
                    if !granted { self?.send(.recordingFailed) } else { self?.beginCapture() }
                }
            }
            return
        default:
            send(.recordingFailed)
            return
        }
        beginCapture()
    }

    private func beginCapture() {
        guard phase.isRecording else { return }
        do {
            try recorder.start()
            recordingStartedAt = Date()
        } catch {
            send(.recordingFailed)
        }
    }

    private func transcribe(_ pcm: [Float]) {
        let glossary = settings.glossary
        let core = self.core
        Task { [weak self] in
            do {
                let text = try await core.transcribe(
                    pcm: pcm, sampleRate: UInt32(PCMRecorder.sampleRate), glossary: glossary)
                guard let self else { return }
                let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
                self.pendingTranscript = trimmed
                let needsAPI: Bool
                if case .transcribing(let action) = self.phase {
                    needsAPI = self.settings.polishRequest(for: action).needsAPI
                } else {
                    needsAPI = false
                }
                self.send(.transcribed(isEmpty: trimmed.isEmpty, needsAPI: needsAPI))
            } catch {
                self?.send(.failed(error as? CoreFailure))
            }
        }
    }

    private func polish(_ action: HotkeyAction) {
        let transcript = pendingTranscript ?? ""
        let request = settings.polishRequest(for: action)
        let core = self.core
        Task { [weak self] in
            let outcome: PolishOutcome
            do {
                outcome = try await core.polish(transcript, request: request)
            } catch {
                // Core already falls back to raw; this is the last resort.
                outcome = .raw(transcript, cause: .apiError)
            }
            self?.send(.polished(outcome))
        }
    }

    private func insert(_ text: String) {
        Task { [weak self] in
            guard let self else { return }
            // Let the user release the hotkey modifiers before we paste.
            try? await Task.sleep(nanoseconds: 80_000_000)
            let path = await self.inserter.insert(text)
            if path == .failed {
                self.send(.failed(nil))
            } else {
                self.send(.inserted)
            }
        }
    }

    // MARK: - Key (FR-1, INV-2)

    /// Takes the field's binding, clears it before anything else, then
    /// hands the value to core exactly once.
    func submitKey(from field: Binding<String>) async -> Bool {
        let value = field.wrappedValue
        field.wrappedValue = ""
        settingsError = nil
        do {
            fingerprint = try await core.storeKey(value)
            return true
        } catch {
            settingsError = (error as? CoreFailure)?.message ?? CoreFailure.vaultUnavailable.message
            return false
        }
    }

    func deleteKey() {
        do {
            try core.deleteKey()
            fingerprint = core.keyFingerprint()
        } catch {
            settingsError = (error as? CoreFailure)?.message
        }
    }

    // MARK: - Models (INV-7: only core downloads)

    func download(_ id: String) {
        downloadProgress[id] = 0
        let core = self.core
        Task { [weak self] in
            do {
                try await core.downloadModel(id: id) { p in
                    Task { @MainActor in self?.downloadProgress[id] = p }
                }
            } catch {
                self?.settingsError = (error as? CoreFailure)?.message
            }
            self?.downloadProgress[id] = nil
            self?.models = core.models()
        }
    }

    func importModel(_ id: String) {
        let panel = NSOpenPanel()
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        guard panel.runModal() == .OK, let url = panel.url else { return }
        let core = self.core
        Task { [weak self] in
            do {
                try await core.importModel(id: id, from: url)
            } catch {
                self?.settingsError = (error as? CoreFailure)?.message
            }
            self?.models = core.models()
        }
    }

    // MARK: - Settings

    private func settingsChanged(from old: AppSettings) {
        guard settings != old else { return }
        machine.defaultMode = settings.mode
        do {
            try core.saveSettings(settings)
        } catch {
            settingsError = (error as? CoreFailure)?.message
        }
        if settings.fallbackPolish != old.fallbackPolish || settings.fallbackTranslate != old.fallbackTranslate
            || settings.alwaysEnableFallbackHotkeys != old.alwaysEnableFallbackHotkeys
        {
            reinstallHotkeys()
        }
        budget = core.budgetStatus()
    }
}
