import XCTest

@testable import KeepsayKit

final class DictationStateMachineTests: XCTestCase {
    func testToggleStartsAndStopsWithStartComboDecidingTranslation() {
        var m = DictationStateMachine()
        XCTAssertEqual(m.handle(.trigger(.polishAndTranslate)), [.startRecording, .enableCancelKey(true)])
        XCTAssertEqual(m.phase, .recording(.polishAndTranslate))
        // FR-2: either combination stops; translation follows the start.
        XCTAssertEqual(m.handle(.trigger(.polish)), [.enableCancelKey(false), .stopAndTranscribe(.polishAndTranslate)])
        XCTAssertEqual(m.phase, .transcribing(.polishAndTranslate))
    }

    func testEscCancelsOnlyWhileRecording() {
        var m = DictationStateMachine()
        XCTAssertEqual(m.handle(.cancel), [])
        _ = m.handle(.trigger(.polish))
        XCTAssertEqual(m.handle(.cancel), [.enableCancelKey(false), .discardRecording])
        XCTAssertEqual(m.phase, .idle)

        _ = m.handle(.trigger(.polish))
        _ = m.handle(.trigger(.polish))
        XCTAssertEqual(m.handle(.cancel), [])
        XCTAssertEqual(m.phase, .transcribing(.polish))
    }

    func testTriggersAreIgnoredWhileBusy() {
        var m = DictationStateMachine()
        _ = m.handle(.trigger(.polish))
        _ = m.handle(.trigger(.polish))
        XCTAssertEqual(m.handle(.trigger(.polish)), [])
        _ = m.handle(.transcribed(isEmpty: false, needsAPI: true))
        XCTAssertEqual(m.phase, .polishing(.polish))
        XCTAssertEqual(m.handle(.trigger(.polishAndTranslate)), [])
    }

    func testPolishedSuccessEndsInDone() {
        var m = DictationStateMachine(defaultMode: .formal)
        _ = m.handle(.trigger(.polish))
        _ = m.handle(.trigger(.polish))
        XCTAssertEqual(m.handle(.transcribed(isEmpty: false, needsAPI: true)), [.polish(.polish)])
        let ok = PolishOutcome(text: "x", fellBackToRaw: false, fallbackCause: nil, usage: nil)
        XCTAssertEqual(m.handle(.polished(ok)), [.insert(ok)])
        XCTAssertEqual(m.handle(.inserted), [.scheduleDismiss])
        XCTAssertEqual(m.phase, .done(mode: .formal, translated: false))
        XCTAssertEqual(m.handle(.dismiss), [])
        XCTAssertEqual(m.phase, .idle)
    }

    func testFallbackShowsCause() {
        var m = DictationStateMachine()
        _ = m.handle(.trigger(.polishAndTranslate))
        _ = m.handle(.trigger(.polishAndTranslate))
        _ = m.handle(.transcribed(isEmpty: false, needsAPI: true))
        let raw = PolishOutcome.raw("x", cause: .translationNeedsAPI)
        _ = m.handle(.polished(raw))
        _ = m.handle(.inserted)
        XCTAssertEqual(m.phase, .fellBack(.translationNeedsAPI))
        XCTAssertEqual(FallbackCause.translationNeedsAPI.message, "翻譯需要 API，已輸出原文")
    }

    func testRawModeSkipsPolish() {
        var m = DictationStateMachine(defaultMode: .raw)
        _ = m.handle(.trigger(.polish))
        _ = m.handle(.trigger(.polish))
        XCTAssertEqual(m.handle(.transcribed(isEmpty: false, needsAPI: false)), [.insertRaw(.polish)])
        _ = m.handle(.inserted)
        XCTAssertEqual(m.phase, .done(mode: .raw, translated: false))
    }

    func testEmptyTranscriptAndFailuresInsertNothing() {
        var m = DictationStateMachine()
        _ = m.handle(.trigger(.polish))
        _ = m.handle(.trigger(.polish))
        XCTAssertEqual(m.handle(.transcribed(isEmpty: true, needsAPI: true)), [.scheduleDismiss])
        XCTAssertEqual(m.phase, .failed(nil))

        XCTAssertEqual(m.handle(.trigger(.polish)), [.startRecording, .enableCancelKey(true)])
        _ = m.handle(.trigger(.polish))
        XCTAssertEqual(m.handle(.failed(.stt)), [.scheduleDismiss])
        XCTAssertEqual(m.phase, .failed(.stt))
    }

    func testRecorderFailureDiscards() {
        var m = DictationStateMachine()
        _ = m.handle(.trigger(.polish))
        XCTAssertEqual(m.handle(.recordingFailed), [.enableCancelKey(false), .discardRecording, .scheduleDismiss])
    }
}

final class RepeatSuppressorTests: XCTestCase {
    func testHoldingFiresOnceAndSecondPressFiresAgain() {
        var r = RepeatSuppressor(quietInterval: 0.25)
        XCTAssertTrue(r.shouldFire(at: 10.0))
        // Auto-repeat after the initial delay, then every 50 ms.
        for t in stride(from: 10.2, through: 11.0, by: 0.05) {
            XCTAssertFalse(r.shouldFire(at: t))
        }
        XCTAssertTrue(r.shouldFire(at: 11.5))
    }
}

final class SettingsAndTypesTests: XCTestCase {
    func testTranslateHotkeyAlwaysPolishesAndTranslates() {
        let s = AppSettings(mode: .raw, translateTo: .ja, glossary: ["keepsay"])
        let p = s.polishRequest(for: .polish)
        XCTAssertEqual(p.mode, .raw)
        XCTAssertNil(p.translateTo)
        XCTAssertFalse(p.needsAPI)
        let t = s.polishRequest(for: .polishAndTranslate)
        XCTAssertEqual(t.mode, .polish)
        XCTAssertEqual(t.translateTo, .ja)
        XCTAssertTrue(t.needsAPI)
    }

    func testGlossaryAddTrimsAndDedupes() {
        var s = AppSettings()
        XCTAssertTrue(s.addGlossaryTerm("  TestFlight "))
        XCTAssertFalse(s.addGlossaryTerm("TestFlight"))
        XCTAssertFalse(s.addGlossaryTerm("   "))
        XCTAssertEqual(s.glossary, ["TestFlight"])
    }

    func testDescriptionsNeverContainUserContent() {
        let secret = "CANARY-content-7f3a"
        let s = AppSettings(glossary: [secret])
        XCTAssertFalse(String(describing: s).contains(secret))
        XCTAssertFalse(String(reflecting: s).contains(secret))
        let o = PolishOutcome.raw(secret, cause: nil)
        XCTAssertFalse(String(describing: o).contains(secret))
        XCTAssertFalse(String(reflecting: o).contains(secret))
        let r = PolishRequest(mode: .polish, translateTo: nil, glossary: [secret])
        XCTAssertFalse(String(describing: r).contains(secret))
    }

    func testTargetRawValuesMatchCore() {
        XCTAssertEqual(TranslationTarget.allCases.map(\.rawValue), ["en", "zh-TW", "ja"])
        XCTAssertEqual(DictationMode.allCases.map(\.rawValue), ["raw", "light", "polish", "formal"])
    }

    func testUnavailableCoreFallsBackToRaw() async throws {
        let core = UnavailableCore()
        XCTAssertFalse(core.hasKey())
        XCTAssertNil(core.keyFingerprint())
        let t = try await core.polish("abc", request: PolishRequest(mode: .polish, translateTo: .en, glossary: []))
        XCTAssertEqual(t, .raw("abc", cause: .translationNeedsAPI))
        let r = try await core.polish("abc", request: PolishRequest(mode: .raw, translateTo: nil, glossary: []))
        XCTAssertEqual(r, .raw("abc", cause: nil))
    }
}

final class SampleAccumulatorTests: XCTestCase {
    func testCapacityIsEnforcedAndTakeEmpties() {
        var acc = SampleAccumulator(capacity: 5)
        let chunk: [Float] = [1, 2, 3, 4]
        chunk.withUnsafeBufferPointer { XCTAssertEqual(acc.append(contentsOf: $0), 4) }
        chunk.withUnsafeBufferPointer { XCTAssertEqual(acc.append(contentsOf: $0), 1) }
        XCTAssertTrue(acc.isFull)
        XCTAssertEqual(acc.take(), [1, 2, 3, 4, 1])
        XCTAssertEqual(acc.samples, [])
    }

    func testWipeDropsSamples() {
        var acc = SampleAccumulator(capacity: 10)
        [Float](repeating: 0.5, count: 3).withUnsafeBufferPointer { _ = acc.append(contentsOf: $0) }
        acc.wipe()
        XCTAssertEqual(acc.samples, [])
    }

    func testRMS() {
        [Float](repeating: 0.5, count: 8).withUnsafeBufferPointer {
            XCTAssertEqual(SampleAccumulator.rms($0), 0.5, accuracy: 1e-6)
        }
    }
}
