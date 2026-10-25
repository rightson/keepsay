// Pure dictation state machine: recording → 轉寫中 → 潤稿中 → done.
//
// No side effects here. `AppModel` feeds events in and performs the effects
// (start/stop audio, call the core, write the pasteboard) on transitions.
// Phases hold user content (transcript, output); none of the types print it:
// `CustomStringConvertible` / `CustomDebugStringConvertible` are redacted.

import Foundation

/// What one dictation will ask of the core, decided when recording starts
/// (FR-2: the action at start decides whether the output is translated).
struct DictationPlan: Equatable, Sendable {
    var action: DictationAction
    var mode: PolishMode
    var translateTo: TargetLanguage?
    /// Set when the plan was downgraded to raw before any call
    /// (e.g. translation without API). Shown with the result.
    var preFallback: FallbackReason?

    var needsAPI: Bool { mode != .raw || translateTo != nil }

    /// Builds the plan from the current app state.
    ///
    /// - 潤飾 uses the selected mode; without a usable API it is raw.
    /// - 潤飾並翻譯 is polish + translate. If the API is not usable (raw mode,
    ///   no key, hard limit) the output is the untranslated raw transcript
    ///   with "翻譯需要 API，已輸出原文" (FR-8).
    static func make(
        action: DictationAction,
        selectedMode: PolishMode,
        target: TargetLanguage,
        hasKey: Bool,
        hardLimited: Bool
    ) -> DictationPlan {
        let apiUsable = hasKey && !hardLimited
        switch action {
        case .polish:
            if selectedMode == .raw {
                return DictationPlan(action: action, mode: .raw, translateTo: nil, preFallback: nil)
            }
            if !apiUsable {
                return DictationPlan(
                    action: action, mode: .raw, translateTo: nil,
                    preFallback: hasKey ? .hardLimit : .noKey)
            }
            return DictationPlan(action: action, mode: selectedMode, translateTo: nil, preFallback: nil)
        case .polishAndTranslate:
            if selectedMode == .raw || !apiUsable {
                return DictationPlan(
                    action: action, mode: .raw, translateTo: nil, preFallback: .translationNeedsApi)
            }
            return DictationPlan(action: action, mode: .polish, translateTo: target, preFallback: nil)
        }
    }
}

/// Final output of one dictation. `text` is user content.
struct DictationOutput: Equatable, Sendable, CustomStringConvertible, CustomDebugStringConvertible {
    enum Kind: Equatable, Sendable {
        /// Model output that passed the guard.
        case processed
        /// The local transcript, either by choice (raw mode) or by fallback.
        case raw
    }

    var text: String
    var kind: Kind
    var plan: DictationPlan
    /// Non-nil when the user asked for processing but got raw.
    var fallback: FallbackReason?
    var usage: TokenUsage?

    var description: String { "DictationOutput(kind: \(kind), len: \(text.count))" }
    var debugDescription: String { description }
}

enum DictationFailure: Equatable, Sendable {
    case microphoneDenied
    case audioUnavailable
    case nothingHeard
    case transcriptionFailed
    case modelMissing
    /// Development build without the Rust core: no STT.
    case coreUnavailable

    /// Maps a core service error to a content-free failure.
    static func from(_ error: Error) -> DictationFailure {
        switch error as? CoreServiceError {
        case .modelMissing: return .modelMissing
        case .coreUnavailable: return .coreUnavailable
        default: return .transcriptionFailed
        }
    }

    var message: String {
        switch self {
        case .microphoneDenied: return "沒有麥克風權限。請到「設定 → keepsay」開啟。"
        case .audioUnavailable: return "無法開始錄音，請再試一次。"
        case .nothingHeard: return "沒有聽到內容。"
        case .transcriptionFailed: return "轉寫失敗，請再試一次。"
        case .modelMissing: return "尚未下載語音模型。請到設定下載。"
        case .coreUnavailable: return "此開發版未連結 keepsay-core，無法轉寫。"
        }
    }
}

enum DictationPhase: Equatable, Sendable, CustomStringConvertible, CustomDebugStringConvertible {
    case idle
    case recording(DictationPlan, startedAt: Date)
    case transcribing(DictationPlan)
    /// Holds the transcript so a failure can fall back to it.
    case polishing(DictationPlan, transcript: String)
    case done(DictationOutput)
    case failed(DictationFailure)

    var isBusy: Bool {
        switch self {
        case .recording, .transcribing, .polishing: return true
        case .idle, .done, .failed: return false
        }
    }

    /// Redacted: never includes transcript or output text.
    var description: String {
        switch self {
        case .idle: return "idle"
        case .recording: return "recording"
        case .transcribing: return "transcribing"
        case .polishing(_, let t): return "polishing(len: \(t.count))"
        case .done(let o): return "done(\(o.description))"
        case .failed(let f): return "failed(\(f))"
        }
    }
    var debugDescription: String { description }
}

enum DictationEvent: Equatable, Sendable {
    case start(DictationPlan, at: Date)
    case stop
    case cancel
    case recordingFailed(DictationFailure)
    case transcribed(String)
    case transcriptionFailed(DictationFailure)
    case polished(PolishOutcome)
    case dismiss
}

enum DictationMachine {
    /// Returns the next phase, or `nil` when the event does not apply in the
    /// current phase (the caller ignores it).
    static func reduce(_ phase: DictationPhase, _ event: DictationEvent) -> DictationPhase? {
        switch (phase, event) {
        // Start from any resting phase ("再錄一段" from done / failed).
        case (.idle, .start(let plan, let at)),
             (.done, .start(let plan, let at)),
             (.failed, .start(let plan, let at)):
            return .recording(plan, startedAt: at)

        case (.recording(let plan, _), .stop):
            return .transcribing(plan)
        case (.recording, .cancel):
            return .idle
        case (.recording, .recordingFailed(let f)):
            return .failed(f)

        case (.transcribing(let plan), .transcribed(let text)):
            let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
            if trimmed.isEmpty {
                return .failed(.nothingHeard)
            }
            if plan.needsAPI {
                return .polishing(plan, transcript: trimmed)
            }
            return .done(DictationOutput(
                text: trimmed, kind: .raw, plan: plan, fallback: plan.preFallback, usage: nil))
        case (.transcribing, .transcriptionFailed(let f)):
            return .failed(f)

        case (.polishing(let plan, let transcript), .polished(let outcome)):
            // FR-4 / FR-8: any failure outputs the untranslated raw transcript,
            // never a partial result.
            if outcome.fellBackToRaw {
                return .done(DictationOutput(
                    text: transcript, kind: .raw, plan: plan,
                    fallback: outcome.fallbackReason ?? .apiError, usage: outcome.usage))
            }
            let text = outcome.text.trimmingCharacters(in: .whitespacesAndNewlines)
            if text.isEmpty {
                return .done(DictationOutput(
                    text: transcript, kind: .raw, plan: plan, fallback: .guardRejected,
                    usage: outcome.usage))
            }
            return .done(DictationOutput(
                text: text, kind: .processed, plan: plan, fallback: nil, usage: outcome.usage))

        case (.done, .dismiss), (.failed, .dismiss):
            return .idle

        default:
            return nil
        }
    }
}
