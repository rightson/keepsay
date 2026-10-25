// Dictation session state machine behind the HUD / overlay pill
// (FR-2 toggle semantics, FR-4/FR-8 fallback, US-M2-13). Pure logic: the
// app performs the returned effects. Shared by macOS and iOS.
//
// - Idle + trigger(a)        → recording(a)            [startRecording]
// - Recording(a) + trigger(_) → transcribing(a)        [stopAndTranscribe(a)]
//   (either combination stops; the START combination decides translation)
// - Recording + cancel (Esc) → idle                    [discardRecording]
// - Transcribing + transcribed → polishing (when the request needs the API)
//   or straight to inserting
// - Polishing + polished(outcome) → inserting
// - Inserting + inserted     → done(outcome) or fellBack(cause)
// - Any processing state + failed → failed
// - Terminal states + trigger → recording (new session); + dismiss → idle

import Foundation

public enum DictationPhase: Equatable, Sendable {
    case idle
    case recording(HotkeyAction)
    case transcribing(HotkeyAction)
    case polishing(HotkeyAction)
    case inserting(HotkeyAction, PolishOutcome)
    /// Inserted model output. `mode` is what the user sees on the pill.
    case done(mode: DictationMode, translated: Bool)
    /// Inserted the raw transcript; `cause` is shown on the pill.
    case fellBack(FallbackCause)
    /// Nothing inserted (STT failed, empty transcript, recorder error).
    case failed(CoreFailure?)

    public var isRecording: Bool {
        if case .recording = self { return true }
        return false
    }

    public var isBusy: Bool {
        switch self {
        case .transcribing, .polishing, .inserting: true
        default: false
        }
    }

    public var isTerminal: Bool {
        switch self {
        case .done, .fellBack, .failed: true
        default: false
        }
    }
}

public enum DictationEvent: Equatable, Sendable {
    case trigger(HotkeyAction)
    case cancel
    case recordingFailed
    /// `isEmpty`: nothing was recognised.
    case transcribed(isEmpty: Bool, needsAPI: Bool)
    case polished(PolishOutcome)
    case inserted
    case failed(CoreFailure?)
    case dismiss
}

public enum DictationEffect: Equatable, Sendable {
    case startRecording
    case stopAndTranscribe(HotkeyAction)
    case discardRecording
    case polish(HotkeyAction)
    /// Insert as-is (raw mode, or no API needed).
    case insertRaw(HotkeyAction)
    case insert(PolishOutcome)
    /// Hide the pill after a short delay.
    case scheduleDismiss
    /// Register / unregister the Esc hotkey (only active while recording).
    case enableCancelKey(Bool)
}

public struct DictationStateMachine: Sendable {
    public private(set) var phase: DictationPhase = .idle
    /// The mode of the current session, for the "已插入・潤稿" label.
    public var defaultMode: DictationMode

    public init(defaultMode: DictationMode = .polish) {
        self.defaultMode = defaultMode
    }

    public mutating func handle(_ event: DictationEvent) -> [DictationEffect] {
        switch (phase, event) {
        case (.idle, .trigger(let a)), (.done, .trigger(let a)), (.fellBack, .trigger(let a)),
            (.failed, .trigger(let a)):
            phase = .recording(a)
            return [.startRecording, .enableCancelKey(true)]

        case (.recording(let a), .trigger):
            phase = .transcribing(a)
            return [.enableCancelKey(false), .stopAndTranscribe(a)]

        case (.recording, .cancel):
            phase = .idle
            return [.enableCancelKey(false), .discardRecording]

        case (.recording, .recordingFailed):
            phase = .failed(nil)
            return [.enableCancelKey(false), .discardRecording, .scheduleDismiss]

        case (.transcribing(let a), .transcribed(let isEmpty, let needsAPI)):
            if isEmpty {
                phase = .failed(nil)
                return [.scheduleDismiss]
            }
            if needsAPI {
                phase = .polishing(a)
                return [.polish(a)]
            }
            phase = .inserting(a, .raw("", cause: nil))
            return [.insertRaw(a)]

        case (.polishing(let a), .polished(let outcome)):
            phase = .inserting(a, outcome)
            return [.insert(outcome)]

        case (.inserting(let a, let outcome), .inserted):
            if let cause = outcome.fallbackCause {
                phase = .fellBack(cause)
            } else {
                let mode: DictationMode = (a == .polishAndTranslate) ? .polish : defaultMode
                phase = .done(mode: mode, translated: a == .polishAndTranslate && !outcome.fellBackToRaw)
            }
            return [.scheduleDismiss]

        case (.transcribing, .failed(let e)), (.polishing, .failed(let e)), (.inserting, .failed(let e)):
            phase = .failed(e)
            return [.scheduleDismiss]

        case (.done, .dismiss), (.fellBack, .dismiss), (.failed, .dismiss):
            phase = .idle
            return []

        default:
            // Triggers while busy, Esc outside recording, stale events.
            return []
        }
    }
}
