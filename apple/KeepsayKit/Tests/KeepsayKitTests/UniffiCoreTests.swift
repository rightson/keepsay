// Runs only in builds that link keepsay-core (apple/scripts/build-core.sh;
// CI job "core-linked" in .github/workflows/macos-app.yml). Calls the real
// FFI against a temporary data directory. No network: nothing here has a
// valid key, and format checks fail before any request.
// Only flags, lengths and enum values are asserted (CLAUDE.md).

#if canImport(KeepsayCoreBindings)
import XCTest

@testable import KeepsayKit

final class UniffiCoreTests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        dir = FileManager.default.temporaryDirectory
            .appendingPathComponent("keepsay-tests-\(UUID().uuidString)", isDirectory: true)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: dir)
    }

    func testCoreIsLinked() {
        XCTAssertTrue(CoreBridge.isCoreLinked)
    }

    func testSettingsRoundTripThroughConfigJSON() throws {
        let core = UniffiCore(dataDirectory: dir)
        let defaults = core.loadSettings()
        XCTAssertEqual(defaults.mode, .polish)
        XCTAssertEqual(defaults.translateTo, .en)
        XCTAssertEqual(defaults.sttModel, "whisper-small")
        XCTAssertEqual(defaults.fallbackPolish, .fallbackPolish)

        var s = defaults
        s.mode = .formal
        s.translateTo = .ja
        s.addGlossaryTerm("keepsay")
        s.hardLimitUSD = 3
        s.fallbackPolish = KeyCombo(keyCode: KeyCombo.kVKSpace, carbonModifiers: KeyCombo.controlKey | KeyCombo.optionKey)
        try core.saveSettings(s)
        XCTAssertTrue(FileManager.default.fileExists(atPath: dir.appendingPathComponent("config.json").path))

        // A fresh instance reads core's file, not Swift memory.
        let again = UniffiCore(dataDirectory: dir).loadSettings()
        XCTAssertEqual(again.mode, .formal)
        XCTAssertEqual(again.translateTo, .ja)
        XCTAssertEqual(again.glossary.count, 1)
        XCTAssertEqual(again.hardLimitUSD, 3)
        XCTAssertEqual(again.fallbackPolish, s.fallbackPolish)
        XCTAssertEqual(again.fallbackTranslate, .fallbackTranslate)

        // Core validates: a negative price is refused and nothing changes.
        var bad = again
        bad.prices.inputUSDPerMTok = -1
        XCTAssertThrowsError(try core.saveSettings(bad)) { XCTAssertEqual($0 as? CoreFailure, .config) }
        XCTAssertEqual(UniffiCore(dataDirectory: dir).loadSettings().prices, again.prices)
    }

    func testModelsComeFromTheBundledManifest() throws {
        let core = UniffiCore(dataDirectory: dir)
        let models = core.models()
        XCTAssertEqual(Set(models.map(\.id)), ["whisper-base", "whisper-small", "whisper-large-v3-turbo"])
        XCTAssertTrue(models.allSatisfy { !$0.installed && $0.sizeBytes > 0 })
    }

    func testBudgetStartsEmptyWithDefaultLimits() {
        let b = UniffiCore(dataDirectory: dir).budgetStatus()
        XCTAssertEqual(b.usage.inputTokens, 0)
        XCTAssertEqual(b.estimatedUSD, 0)
        XCTAssertEqual(b.softLimitUSD, 5)
        XCTAssertEqual(b.hardLimitUSD, 10)
        XCTAssertFalse(b.hardLimited)
    }

    func testBadKeyFormatIsRefusedBeforeAnyRequest() async {
        let core = UniffiCore(dataDirectory: dir)
        do {
            _ = try await core.storeKey("not-a-key")
            XCTFail("accepted a malformed key")
        } catch {
            XCTAssertEqual(error as? CoreFailure, .keyFormat)
        }
    }

    func testRawModeNeverCallsTheAPI() async throws {
        let core = UniffiCore(dataDirectory: dir)
        let text = "測試 raw 模式"
        let r = try await core.polish(text, request: PolishRequest(mode: .raw, translateTo: nil, glossary: []))
        XCTAssertEqual(r.text, text)
        XCTAssertFalse(r.fellBackToRaw)
        XCTAssertNil(r.usage)
    }

    func testMissingModelIsReportedNotDownloaded() async {
        let core = UniffiCore(dataDirectory: dir)
        do {
            _ = try await core.transcribe(pcm: [Float](repeating: 0, count: 1600), sampleRate: 16_000, glossary: [])
            XCTFail("transcribed without a model")
        } catch {
            XCTAssertNotNil(error as? CoreFailure)
        }
    }
}
#endif
