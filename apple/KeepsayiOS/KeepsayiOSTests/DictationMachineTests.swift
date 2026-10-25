// State machine and plan tests. Fixtures are synthetic placeholders; the
// assertions check phases, kinds, reasons and lengths, never real content.

import XCTest
@testable import KeepsayiOS

final class DictationPlanTests: XCTestCase {
    func testPolishUsesSelectedModeWithKey() {
        let p = DictationPlan.make(action: .polish, selectedMode: .formal, target: .en, hasKey: true, hardLimited: false)
        XCTAssertEqual(p.mode, .formal)
        XCTAssertNil(p.translateTo)
        XCTAssertNil(p.preFallback)
        XCTAssertTrue(p.needsAPI)
    }

    func testPolishRawModeNeedsNoAPI() {
        let p = DictationPlan.make(action: .polish, selectedMode: .raw, target: .en, hasKey: true, hardLimited: false)
        XCTAssertEqual(p.mode, .raw)
        XCTAssertFalse(p.needsAPI)
        XCTAssertNil(p.preFallback)
    }

    func testPolishWithoutKeyIsRaw() {
        let p = DictationPlan.make(action: .polish, selectedMode: .polish, target: .en, hasKey: false, hardLimited: false)
        XCTAssertEqual(p.mode, .raw)
        XCTAssertFalse(p.needsAPI)
        XCTAssertEqual(p.preFallback, .noKey)
    }

    func testHardLimitSwitchesToRaw() {
        let p = DictationPlan.make(action: .polish, selectedMode: .polish, target: .en, hasKey: true, hardLimited: true)
        XCTAssertEqual(p.mode, .raw)
        XCTAssertEqual(p.preFallback, .hardLimit)
    }

    func testTranslateIsAlwaysPolishPlusTarget() {
        for mode in [PolishMode.light, .polish, .formal] {
            for lang in TargetLanguage.allCases {
                let p = DictationPlan.make(
                    action: .polishAndTranslate, selectedMode: mode, target: lang, hasKey: true, hardLimited: false)
                XCTAssertEqual(p.mode, .polish)
                XCTAssertEqual(p.translateTo, lang)
                XCTAssertNil(p.preFallback)
            }
        }
    }

    func testTranslateWithoutAPIOutputsRawWithNotice() {
        let cases: [(PolishMode, Bool, Bool)] = [
            (.raw, true, false),    // raw mode
            (.polish, false, false), // no key
            (.polish, true, true),  // hard limit
        ]
        for (mode, hasKey, hard) in cases {
            let p = DictationPlan.make(
                action: .polishAndTranslate, selectedMode: mode, target: .ja, hasKey: hasKey, hardLimited: hard)
            XCTAssertEqual(p.mode, .raw)
            XCTAssertNil(p.translateTo)
            XCTAssertFalse(p.needsAPI)
            XCTAssertEqual(p.preFallback, .translationNeedsApi)
        }
        XCTAssertEqual(FallbackReason.translationNeedsApi.message, "翻譯需要 API，已輸出原文")
    }
}

final class DictationMachineTests: XCTestCase {
    private let t0 = Date(timeIntervalSince1970: 0)
    private let polishPlan = DictationPlan(action: .polish, mode: .polish, translateTo: nil, preFallback: nil)
    private let rawPlan = DictationPlan(action: .polish, mode: .raw, translateTo: nil, preFallback: nil)

    private func run(_ events: [DictationEvent], from start: DictationPhase = .idle) -> DictationPhase {
        var phase = start
        for e in events {
            guard let next = DictationMachine.reduce(phase, e) else {
                XCTFail("event not accepted in \(phase)")
                return phase
            }
            phase = next
        }
        return phase
    }

    func testHappyPathPolish() {
        let phase = run([
            .start(polishPlan, at: t0), .stop, .transcribed("  fixture-a  "),
            .polished(PolishOutcome(text: "fixture-b", fellBackToRaw: false, fallbackReason: nil,
                                    usage: TokenUsage(inputTokens: 10, outputTokens: 5))),
        ])
        guard case .done(let out) = phase else { return XCTFail("expected done, got \(phase)") }
        XCTAssertEqual(out.kind, .processed)
        XCTAssertNil(out.fallback)
        XCTAssertEqual(out.text.count, "fixture-b".count)
        XCTAssertEqual(out.usage?.inputTokens, 10)
    }

    func testTranscribingThenPolishingCarriesTrimmedTranscript() {
        let phase = run([.start(polishPlan, at: t0), .stop, .transcribed("  fixture-a \n")])
        guard case .polishing(let plan, let transcript) = phase else { return XCTFail("expected polishing") }
        XCTAssertEqual(plan, polishPlan)
        XCTAssertEqual(transcript.count, "fixture-a".count)
    }

    func testRawModeSkipsPolishing() {
        let phase = run([.start(rawPlan, at: t0), .stop, .transcribed("fixture-a")])
        guard case .done(let out) = phase else { return XCTFail("expected done") }
        XCTAssertEqual(out.kind, .raw)
        XCTAssertNil(out.fallback)
    }

    func testPreFallbackIsShownOnRawResult() {
        let plan = DictationPlan.make(
            action: .polishAndTranslate, selectedMode: .polish, target: .en, hasKey: false, hardLimited: false)
        let phase = run([.start(plan, at: t0), .stop, .transcribed("fixture-a")])
        guard case .done(let out) = phase else { return XCTFail("expected done") }
        XCTAssertEqual(out.kind, .raw)
        XCTAssertEqual(out.fallback, .translationNeedsApi)
    }

    func testCoreFallbackOutputsTranscriptNotPartialResult() {
        let transcript = "fixture-transcript"
        let phase = run([
            .start(polishPlan, at: t0), .stop, .transcribed(transcript),
            .polished(PolishOutcome(text: "partial", fellBackToRaw: true, fallbackReason: .timeout, usage: nil)),
        ])
        guard case .done(let out) = phase else { return XCTFail("expected done") }
        XCTAssertEqual(out.kind, .raw)
        XCTAssertEqual(out.fallback, .timeout)
        XCTAssertEqual(out.text.count, transcript.count)
    }

    func testFallbackWithoutReasonIsApiError() {
        let phase = run([
            .start(polishPlan, at: t0), .stop, .transcribed("fixture-a"),
            .polished(PolishOutcome(text: "fixture-a", fellBackToRaw: true, fallbackReason: nil, usage: nil)),
        ])
        guard case .done(let out) = phase else { return XCTFail("expected done") }
        XCTAssertEqual(out.fallback, .apiError)
    }

    func testEmptyModelOutputFallsBack() {
        let phase = run([
            .start(polishPlan, at: t0), .stop, .transcribed("fixture-a"),
            .polished(PolishOutcome(text: "  \n", fellBackToRaw: false, fallbackReason: nil, usage: nil)),
        ])
        guard case .done(let out) = phase else { return XCTFail("expected done") }
        XCTAssertEqual(out.kind, .raw)
        XCTAssertEqual(out.fallback, .guardRejected)
    }

    func testEmptyTranscriptFails() {
        XCTAssertEqual(run([.start(polishPlan, at: t0), .stop, .transcribed(" \n ")]), .failed(.nothingHeard))
    }

    func testCancelReturnsToIdle() {
        XCTAssertEqual(run([.start(polishPlan, at: t0), .cancel]), .idle)
    }

    func testFailuresAndDismiss() {
        XCTAssertEqual(run([.start(polishPlan, at: t0), .recordingFailed(.microphoneDenied)]), .failed(.microphoneDenied))
        XCTAssertEqual(run([.start(polishPlan, at: t0), .stop, .transcriptionFailed(.modelMissing)]), .failed(.modelMissing))
        XCTAssertEqual(run([.dismiss], from: .failed(.nothingHeard)), .idle)
    }

    func testRecordAgainFromDone() {
        let done = run([.start(rawPlan, at: t0), .stop, .transcribed("fixture-a")])
        let again = run([.start(polishPlan, at: t0)], from: done)
        guard case .recording(let plan, _) = again else { return XCTFail("expected recording") }
        XCTAssertEqual(plan, polishPlan)
    }

    func testIgnoredEvents() {
        XCTAssertNil(DictationMachine.reduce(.idle, .stop))
        XCTAssertNil(DictationMachine.reduce(.idle, .transcribed("x")))
        XCTAssertNil(DictationMachine.reduce(.recording(polishPlan, startedAt: t0), .start(polishPlan, at: t0)))
        XCTAssertNil(DictationMachine.reduce(.transcribing(polishPlan), .cancel))
        XCTAssertNil(DictationMachine.reduce(.transcribing(polishPlan), .polished(.raw("x", reason: nil))))
        XCTAssertNil(DictationMachine.reduce(.polishing(polishPlan, transcript: "x"), .stop))
    }

    func testBusyPhases() {
        XCTAssertTrue(DictationPhase.recording(polishPlan, startedAt: t0).isBusy)
        XCTAssertTrue(DictationPhase.transcribing(polishPlan).isBusy)
        XCTAssertTrue(DictationPhase.polishing(polishPlan, transcript: "x").isBusy)
        XCTAssertFalse(DictationPhase.idle.isBusy)
        XCTAssertFalse(DictationPhase.failed(.nothingHeard).isBusy)
    }

    /// User content must not reach logs through string interpolation.
    func testDescriptionsAreRedacted() {
        let secretish = "fixture-should-not-appear"
        let polishing = DictationPhase.polishing(polishPlan, transcript: secretish)
        XCTAssertFalse(String(describing: polishing).contains(secretish))
        XCTAssertFalse(String(reflecting: polishing).contains(secretish))
        let out = DictationOutput(text: secretish, kind: .raw, plan: rawPlan, fallback: nil, usage: nil)
        XCTAssertFalse(String(describing: DictationPhase.done(out)).contains(secretish))
        XCTAssertFalse("\(out)".contains(secretish))
        var settings = AppSettings()
        settings.glossary = [secretish]
        XCTAssertFalse(String(describing: settings).contains(secretish))
        XCTAssertFalse(String(reflecting: settings).contains(secretish))
    }
}
