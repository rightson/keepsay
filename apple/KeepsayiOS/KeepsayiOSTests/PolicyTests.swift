import UIKit
import UniformTypeIdentifiers
import XCTest
@testable import KeepsayiOS

final class ClipboardPolicyTests: XCTestCase {
    func testOptionsAreLocalOnlyAndExpireIn60s() throws {
        let now = Date(timeIntervalSince1970: 1_000)
        let opts = ClipboardPolicy.options(now: now)
        XCTAssertEqual(opts.count, 2)
        XCTAssertEqual(opts[.localOnly] as? Bool, true)
        let exp = try XCTUnwrap(opts[.expirationDate] as? Date)
        XCTAssertEqual(exp.timeIntervalSince(now), 60, accuracy: 0.001)
        XCTAssertEqual(ClipboardPolicy.lifetime, 60)
    }

    func testSingleUTF8PlainTextItem() {
        let items = ClipboardPolicy.items(for: "fixture")
        XCTAssertEqual(items.count, 1)
        XCTAssertEqual(Array(items[0].keys), [UTType.utf8PlainText.identifier])
    }

    func testCopyWritesOneItemToGivenPasteboard() throws {
        let pb = UIPasteboard.withUniqueName()
        defer { UIPasteboard.remove(withName: pb.name) }
        let now = Date()
        let until = ClipboardPolicy.copy("fixture", to: pb, now: now)
        XCTAssertEqual(until.timeIntervalSince(now), 60, accuracy: 0.001)
        XCTAssertEqual(pb.numberOfItems, 1)
        XCTAssertTrue(pb.hasStrings)
    }
}

final class KeyInputTests: XCTestCase {
    func testPlausibility() {
        XCTAssertFalse(KeyInput.isPlausible(""))
        XCTAssertFalse(KeyInput.isPlausible("sk-ant-"))
        XCTAssertFalse(KeyInput.isPlausible("sk-other-0000000000000000"))
        XCTAssertTrue(KeyInput.isPlausible("  sk-ant-0000000000000000  "))
    }
}

final class GlossaryTests: XCTestCase {
    func testAddingTrimsAndDeduplicates() {
        var g: [String] = []
        g = Glossary.adding("  Alpha ", to: g)
        XCTAssertEqual(g, ["Alpha"])
        g = Glossary.adding("alpha", to: g)
        XCTAssertEqual(g.count, 1)
        g = Glossary.adding("   ", to: g)
        XCTAssertEqual(g.count, 1)
        g = Glossary.adding(String(repeating: "x", count: Glossary.maxTermLength + 1), to: g)
        XCTAssertEqual(g.count, 1)
    }

    func testCapacity() {
        var g = (0..<Glossary.maxTerms).map { "t\($0)" }
        g = Glossary.adding("extra", to: g)
        XCTAssertEqual(g.count, Glossary.maxTerms)
    }
}

final class ModelCatalogTests: XCTestCase {
    private let gib: UInt64 = 1_073_741_824

    func testRecommendationByMemory() {
        XCTAssertEqual(ModelCatalog.recommended(physicalMemoryBytes: 3 * gib).id, "whisper-base")
        XCTAssertEqual(ModelCatalog.recommended(physicalMemoryBytes: 4 * gib).id, "whisper-small")
        XCTAssertEqual(ModelCatalog.recommended(physicalMemoryBytes: 6 * gib).id, "whisper-small")
        XCTAssertEqual(ModelCatalog.recommended(physicalMemoryBytes: 8 * gib).id, "whisper-large-v3-turbo")
    }

    func testDefaultSettingsModelIsInCatalog() {
        XCTAssertNotNil(ModelCatalog.info(id: AppSettings().sttModel))
    }
}

final class SettingsStoreTests: XCTestCase {
    private var dir: URL!

    override func setUpWithError() throws {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
    }

    override func tearDownWithError() throws {
        try? FileManager.default.removeItem(at: dir)
    }

    func testRoundTrip() {
        let store = SettingsStore(directory: dir)
        var s = store.load()
        XCTAssertEqual(s, AppSettings())
        s.defaultMode = .formal
        s.targetLanguage = .ja
        s.onboarded = true
        store.save(s)
        XCTAssertEqual(SettingsStore(directory: dir).load(), s)
    }

    func testNewerSchemaIsNotOverwritten() throws {
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        var future = AppSettings()
        future.schemaVersion = AppSettings.schemaVersion + 1
        let url = dir.appendingPathComponent("settings.json")
        let original = try JSONEncoder().encode(future)
        try original.write(to: url)

        let store = SettingsStore(directory: dir)
        XCTAssertEqual(store.load().schemaVersion, AppSettings.schemaVersion)
        store.save(AppSettings())
        XCTAssertEqual(try Data(contentsOf: url), original)
    }

    /// INV-1: the settings type has no field that could hold a key.
    func testSettingsHaveNoKeyField() throws {
        let json = try JSONEncoder().encode(AppSettings())
        let obj = try XCTUnwrap(try JSONSerialization.jsonObject(with: json) as? [String: Any])
        for k in obj.keys {
            XCTAssertFalse(k.lowercased().contains("key"), "unexpected field \(k)")
            XCTAssertFalse(k.lowercased().contains("secret"), "unexpected field \(k)")
        }
    }
}

final class BudgetSnapshotTests: XCTestCase {
    func testLimitsAndFraction() {
        let b = BudgetSnapshot(estimatedUSD: 5, softLimitUSD: 5, hardLimitUSD: 10)
        XCTAssertTrue(b.softLimited)
        XCTAssertFalse(b.hardLimited)
        XCTAssertEqual(b.fraction, 0.5, accuracy: 1e-9)
        XCTAssertEqual(BudgetSnapshot.empty.fraction, 0)
        XCTAssertTrue(BudgetSnapshot(estimatedUSD: 11, softLimitUSD: nil, hardLimitUSD: 10).hardLimited)
    }
}
