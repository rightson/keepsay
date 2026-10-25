// AppModel flows against fake core and recorder. Fixtures are synthetic
// placeholders; assertions check phases, reasons, counts and lengths, never
// key, transcript, output or glossary content.

import XCTest
@testable import KeepsayiOS

final class FakeCore: CoreService, @unchecked Sendable {
    private let lock = NSLock()
    private var _status = CoreStatus(
        coreLinked: true, hasKey: false, fingerprint: nil, budget: .empty, installedModels: [])
    private var _storeError: CoreServiceError?
    private var _transcribeError: CoreServiceError?
    private var _polishOutcome: PolishOutcome?
    private var _storeCalls = 0
    private var _polishCalls: [(PolishMode, TargetLanguage?)] = []
    private var _applied: [AppSettings] = []

    static let fingerprint = "…AbCd"
    static let transcript = "fixture words"


    var current: CoreStatus {
        get { lock.withLock { _status } }
        set { lock.withLock { _status = newValue } }
    }
    var storeError: CoreServiceError? {
        get { lock.withLock { _storeError } }
        set { lock.withLock { _storeError = newValue } }
    }
    var transcribeError: CoreServiceError? {
        get { lock.withLock { _transcribeError } }
        set { lock.withLock { _transcribeError = newValue } }
    }
    var polishOutcome: PolishOutcome? {
        get { lock.withLock { _polishOutcome } }
        set { lock.withLock { _polishOutcome = newValue } }
    }
    var storeCalls: Int { lock.withLock { _storeCalls } }
    var polishCalls: [(PolishMode, TargetLanguage?)] { lock.withLock { _polishCalls } }
    var applied: [AppSettings] { lock.withLock { _applied } }

    func status() async -> CoreStatus { current }

    func storeKey(_ key: String) async throws -> String {
        lock.withLock { _storeCalls += 1 }
        if let e = storeError { throw e }
        lock.withLock {
            _status.hasKey = true
            _status.fingerprint = Self.fingerprint
        }
        return Self.fingerprint
    }

    func deleteKey() async throws {
        lock.withLock {
            _status.hasKey = false
            _status.fingerprint = nil
        }
    }

    func transcribe(pcm: [Float], sampleRate: UInt32, glossary: [String]) async throws -> String {
        if let e = transcribeError { throw e }
        return Self.transcript
    }

    func polish(transcript: String, mode: PolishMode, translateTo: TargetLanguage?, glossary: [String]) async -> PolishOutcome {
        lock.withLock { _polishCalls.append((mode, translateTo)) }
        return polishOutcome
            ?? PolishOutcome(text: "fixture output", fellBackToRaw: false, fallbackReason: nil,
                             usage: TokenUsage(inputTokens: 10, outputTokens: 12))
    }

    func downloadModel(_ id: String, progress: @escaping @Sendable (Double) -> Void) async throws {
        progress(1)
        lock.withLock { _ = _status.installedModels.insert(id) }
    }

    func apply(_ settings: AppSettings) async {
        lock.withLock { _applied.append(settings) }
    }
}

final class FakeRecorder: AudioCapturing, @unchecked Sendable {
    let sampleRate: UInt32 = 16_000
    var permission = true
    private(set) var started = false
    private(set) var cancelled = false

    func requestPermission() async -> Bool { permission }
    func start(level: @escaping @Sendable (Float) -> Void) throws { started = true }
    func stop() -> [Float] { [Float](repeating: 0, count: 1_600) }
    func cancel() { cancelled = true }
}

@MainActor
final class AppModelTests: XCTestCase {
    private var dir: URL!
    private var core: FakeCore!
    private var recorder: FakeRecorder!

    override func setUp() async throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        core = FakeCore()
        recorder = FakeRecorder()
    }

    override func tearDown() async throws {
        try? FileManager.default.removeItem(at: dir)
    }

    private func makeModel(onboarded: Bool = true) async -> AppModel {
        let store = SettingsStore(directory: dir)
        var s = AppSettings()
        s.onboarded = onboarded
        store.save(s)
        let m = AppModel(core: core, recorder: recorder, store: store)
        await m.refresh()
        return m
    }

    private func waitUntil(
        _ what: String, timeout: TimeInterval = 5, _ cond: () -> Bool
    ) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while !cond() {
            if Date() > deadline {
                XCTFail("timed out waiting for \(what)")
                return
            }
            try await Task.sleep(for: .milliseconds(10))
        }
    }

    private func record(_ m: AppModel, _ action: DictationAction) async throws {
        m.trigger(action)
        guard case .recording = m.phase else { return XCTFail("not recording: \(m.phase)") }
        try await waitUntil("recorder start") { recorder.started }
        m.trigger(action)
    }

    // MARK: key (FR-1, INV-2)

    func testSubmitKeyShowsFingerprintOnly() async {
        let m = await makeModel()
        XCTAssertFalse(m.hasKey)
        XCTAssertEqual(m.selectedMode, .raw)
        let ok = await m.submitKey("sk-ant-test-0000000000000000")
        XCTAssertTrue(ok)
        XCTAssertEqual(core.storeCalls, 1)
        XCTAssertTrue(m.hasKey)
        XCTAssertEqual(m.fingerprint, FakeCore.fingerprint)
        XCTAssertNil(m.keyError)
        XCTAssertEqual(m.selectedMode, m.settings.defaultMode)
    }

    func testFailedKeyIsNotShownAsStored() async {
        core.storeError = .keyInvalid
        let m = await makeModel()
        let ok = await m.submitKey("sk-ant-test-0000000000000000")
        XCTAssertFalse(ok)
        XCTAssertFalse(m.hasKey)
        XCTAssertNil(m.fingerprint)
        XCTAssertEqual(m.keyError, CoreServiceError.keyInvalid.message)
    }

    func testDeleteKeyFallsBackToRaw() async {
        core.current = CoreStatus(
            coreLinked: true, hasKey: true, fingerprint: FakeCore.fingerprint, budget: .empty, installedModels: [])
        let m = await makeModel()
        m.selectedMode = .polish
        await m.deleteKey()
        XCTAssertFalse(m.hasKey)
        XCTAssertNil(m.fingerprint)
        XCTAssertEqual(m.selectedMode, .raw)
    }

    func testHardLimitAllowsRawOnly() async {
        core.current = CoreStatus(
            coreLinked: true, hasKey: true, fingerprint: FakeCore.fingerprint,
            budget: BudgetSnapshot(estimatedUSD: 10, softLimitUSD: 5, hardLimitUSD: 10), installedModels: [])
        let m = await makeModel()
        XCTAssertEqual(m.selectedMode, .raw)
        XCTAssertTrue(m.isModeAvailable(.raw))
        for mode in [PolishMode.light, .polish, .formal] {
            XCTAssertFalse(m.isModeAvailable(mode))
        }
    }

    // MARK: dictation

    func testPolishDictationReachesDone() async throws {
        core.current = CoreStatus(
            coreLinked: true, hasKey: true, fingerprint: FakeCore.fingerprint, budget: .empty, installedModels: [])
        let m = await makeModel()
        m.selectedMode = .formal
        try await record(m, .polish)
        try await waitUntil("done") { if case .done = m.phase { return true } else { return false } }
        guard case .done(let out) = m.phase else { return XCTFail() }
        XCTAssertEqual(out.kind, .processed)
        XCTAssertNil(out.fallback)
        XCTAssertEqual(core.polishCalls.count, 1)
        XCTAssertEqual(core.polishCalls.first?.0, .formal)
        XCTAssertNil(core.polishCalls.first?.1)
        XCTAssertNotNil(m.copiedUntil)
    }

    func testTranslateUsesPolishAndTarget() async throws {
        core.current = CoreStatus(
            coreLinked: true, hasKey: true, fingerprint: FakeCore.fingerprint, budget: .empty, installedModels: [])
        let m = await makeModel()
        m.settings.targetLanguage = .ja
        m.selectedMode = .light
        try await record(m, .polishAndTranslate)
        try await waitUntil("done") { if case .done = m.phase { return true } else { return false } }
        XCTAssertEqual(core.polishCalls.first?.0, .polish)
        XCTAssertEqual(core.polishCalls.first?.1, .ja)
    }

    func testTranslateWithoutKeyOutputsRawWithNotice() async throws {
        let m = await makeModel()
        try await record(m, .polishAndTranslate)
        try await waitUntil("done") { if case .done = m.phase { return true } else { return false } }
        guard case .done(let out) = m.phase else { return XCTFail() }
        XCTAssertEqual(out.kind, .raw)
        XCTAssertEqual(out.fallback, .translationNeedsApi)
        XCTAssertEqual(out.text.count, FakeCore.transcript.count)
        XCTAssertTrue(core.polishCalls.isEmpty, "no API call without a key")
    }

    func testCoreFallbackOutputsRawTranscript() async throws {
        core.current = CoreStatus(
            coreLinked: true, hasKey: true, fingerprint: FakeCore.fingerprint, budget: .empty, installedModels: [])
        core.polishOutcome = .raw(FakeCore.transcript, reason: .timeout)
        let m = await makeModel()
        m.selectedMode = .polish
        try await record(m, .polish)
        try await waitUntil("done") { if case .done = m.phase { return true } else { return false } }
        guard case .done(let out) = m.phase else { return XCTFail() }
        XCTAssertEqual(out.kind, .raw)
        XCTAssertEqual(out.fallback, .timeout)
    }

    func testMissingModelFails() async throws {
        core.transcribeError = .modelMissing
        let m = await makeModel()
        try await record(m, .polish)
        try await waitUntil("failed") { if case .failed = m.phase { return true } else { return false } }
        XCTAssertEqual(m.phase, .failed(.modelMissing))
    }

    func testUnlinkedCoreFailsTranscription() async throws {
        core.transcribeError = .coreUnavailable
        let m = await makeModel()
        try await record(m, .polish)
        try await waitUntil("failed") { if case .failed = m.phase { return true } else { return false } }
        XCTAssertEqual(m.phase, .failed(.coreUnavailable))
    }

    func testMicrophoneDenied() async throws {
        recorder.permission = false
        let m = await makeModel()
        m.trigger(.polish)
        try await waitUntil("failed") { if case .failed = m.phase { return true } else { return false } }
        XCTAssertEqual(m.phase, .failed(.microphoneDenied))
    }

    func testCancelDropsRecording() async throws {
        let m = await makeModel()
        m.trigger(.polish)
        try await waitUntil("recorder start") { recorder.started }
        m.cancel()
        XCTAssertEqual(m.phase, .idle)
        XCTAssertTrue(recorder.cancelled)
    }

    // MARK: intents, settings, budget

    func testIntentBeforeOnboardingIsIgnored() async {
        let m = await makeModel(onboarded: false)
        XCTAssertEqual(m.onboarding, .key)
        m.handleIntent(.polish)
        XCTAssertEqual(m.phase, .idle)
    }

    func testIntentTogglesRecording() async throws {
        let m = await makeModel()
        m.handleIntent(.polishAndTranslate)
        guard case .recording(let plan, _) = m.phase else { return XCTFail() }
        XCTAssertEqual(plan.action, .polishAndTranslate)
        try await waitUntil("recorder start") { recorder.started }
        m.handleIntent(.polish)
        XCTAssertFalse(m.phase == .idle)
        if case .recording = m.phase { XCTFail("second run must stop") }
    }

    func testSettingsChangesReachCore() async throws {
        let m = await makeModel()
        m.settings.defaultMode = .formal
        try await waitUntil("apply") { core.applied.last?.defaultMode == .formal }
    }

    func testSoftLimitNoticeOncePerMonth() async {
        core.current = CoreStatus(
            coreLinked: true, hasKey: true, fingerprint: FakeCore.fingerprint,
            budget: BudgetSnapshot(estimatedUSD: 6, softLimitUSD: 5, hardLimitUSD: 10), installedModels: [])
        let m = await makeModel()
        let now = Date()
        m.warnSoftLimitIfNeeded(now: now)
        XCTAssertNotNil(m.toast)
        XCTAssertEqual(m.settings.softLimitWarnedMonth, SoftLimitNotice.monthKey(now))
        m.toast = nil
        m.warnSoftLimitIfNeeded(now: now)
        XCTAssertNil(m.toast)
    }

    func testDownloadMarksModelInstalled() async throws {
        let m = await makeModel()
        m.download(ModelCatalog.base)
        try await waitUntil("download") { m.downloadProgress == nil && m.installedModels.contains("whisper-base") }
        XCTAssertEqual(m.settings.sttModel, "whisper-base")
        XCTAssertNil(m.downloadError)
    }
}

/// `KitCoreService` over KeepsayKit's raw-only stand-in (no Rust core).
final class KitCoreServiceUnlinkedTests: XCTestCase {
    private let core = KitCoreService.unlinked()

    func testStatusReportsUnlinkedAndNoKey() async {
        let s = await core.status()
        XCTAssertFalse(s.coreLinked)
        XCTAssertFalse(s.hasKey)
        XCTAssertNil(s.fingerprint)
        XCTAssertTrue(s.installedModels.isEmpty)
    }

    func testStoreKeyIsRefused() async {
        do {
            _ = try await core.storeKey("sk-ant-test-0000000000000000")
            XCTFail("must not store")
        } catch {
            XCTAssertEqual(error as? CoreServiceError, .coreUnavailable)
        }
        let s = await core.status()
        XCTAssertFalse(s.hasKey)
    }

    func testTranscribeAndDownloadAreUnavailable() async {
        do {
            _ = try await core.transcribe(pcm: [0, 0], sampleRate: 16_000, glossary: [])
            XCTFail()
        } catch {
            XCTAssertEqual(error as? CoreServiceError, .coreUnavailable)
        }
        do {
            try await core.downloadModel("whisper-base") { _ in }
            XCTFail()
        } catch {
            XCTAssertEqual(error as? CoreServiceError, .coreUnavailable)
        }
    }

    func testPolishFallsBackToRaw() async {
        let input = "fixture words"
        let polished = await core.polish(transcript: input, mode: .polish, translateTo: nil, glossary: [])
        XCTAssertTrue(polished.fellBackToRaw)
        XCTAssertEqual(polished.fallbackReason, .noKey)
        XCTAssertEqual(polished.text.count, input.count)

        let translated = await core.polish(transcript: input, mode: .polish, translateTo: .en, glossary: [])
        XCTAssertEqual(translated.fallbackReason, .translationNeedsApi)

        let raw = await core.polish(transcript: input, mode: .raw, translateTo: nil, glossary: [])
        XCTAssertFalse(raw.fellBackToRaw)
        XCTAssertNil(raw.fallbackReason)
    }

    func testApplyAndDeleteDoNotThrow() async throws {
        await core.apply(AppSettings())
        try await core.deleteKey()
    }
}

final class SoftLimitNoticeTests: XCTestCase {
    private var utc: Calendar {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "UTC")!
        return c
    }

    func testMonthKey() {
        let d = Date(timeIntervalSince1970: 1_761_350_400)  // 2025-10-25 UTC
        XCTAssertEqual(SoftLimitNotice.monthKey(d, calendar: utc), "2025-10")
    }

    func testWarnsOnlyBetweenSoftAndHard() {
        let now = Date(timeIntervalSince1970: 1_761_350_400)
        let soft = BudgetSnapshot(estimatedUSD: 6, softLimitUSD: 5, hardLimitUSD: 10)
        let under = BudgetSnapshot(estimatedUSD: 1, softLimitUSD: 5, hardLimitUSD: 10)
        let hard = BudgetSnapshot(estimatedUSD: 10, softLimitUSD: 5, hardLimitUSD: 10)
        XCTAssertTrue(SoftLimitNotice.shouldWarn(budget: soft, lastWarnedMonth: nil, now: now, calendar: utc))
        XCTAssertTrue(SoftLimitNotice.shouldWarn(budget: soft, lastWarnedMonth: "2025-09", now: now, calendar: utc))
        XCTAssertFalse(SoftLimitNotice.shouldWarn(budget: soft, lastWarnedMonth: "2025-10", now: now, calendar: utc))
        XCTAssertFalse(SoftLimitNotice.shouldWarn(budget: under, lastWarnedMonth: nil, now: now, calendar: utc))
        XCTAssertFalse(SoftLimitNotice.shouldWarn(budget: hard, lastWarnedMonth: nil, now: now, calendar: utc))
    }
}

final class OnboardingTextTests: XCTestCase {
    /// PROPOSAL §12 (v0.4): dedicated non-default workspace; no unconfirmed
    /// Console menu path for revoking.
    func testKeySafetyGuideFollowsSection12() {
        let items = KeySafetyGuide.items
        XCTAssertEqual(items.map(\.n), [1, 2, 3, 4])
        XCTAssertTrue(items[0].body.contains("Default Workspace"))
        XCTAssertTrue(items[1].title.contains("自動儲值"))
        XCTAssertTrue(items[2].body.contains("Spend limits"))
        XCTAssertFalse(items[3].body.contains("→"))
    }

    func testFailureMapping() {
        XCTAssertEqual(DictationFailure.from(CoreServiceError.modelMissing), .modelMissing)
        XCTAssertEqual(DictationFailure.from(CoreServiceError.coreUnavailable), .coreUnavailable)
        XCTAssertEqual(DictationFailure.from(CoreServiceError.network), .transcriptionFailed)
    }
}
