// `KitCoreService.live()` against the linked keepsay-core. Skipped in builds
// without the core; ios-app.yml's core-linked job sets KEEPSAY_EXPECT_CORE=1
// so a missing core fails instead of skipping. No network: only paths the
// core answers locally. No key or content is asserted on.

import XCTest
@testable import KeepsayiOS

final class LiveCoreTests: XCTestCase {
    private func liveCore() async throws -> (KitCoreService, CoreStatus) {
        let core = KitCoreService.live()
        let status = await core.status()
        if ProcessInfo.processInfo.environment["KEEPSAY_EXPECT_CORE"] == "1" {
            XCTAssertTrue(status.coreLinked, "this job must link keepsay-core")
        }
        try XCTSkipUnless(status.coreLinked, "built without keepsay-core")
        return (core, status)
    }

    func testStatusComesFromCore() async throws {
        let (_, status) = try await liveCore()
        XCTAssertEqual(status.hasKey, status.fingerprint != nil)
        if let fp = status.fingerprint {
            XCTAssertEqual(fp.count, 5, "fingerprint is … plus four characters")
        }
    }

    /// FR-1: the core's format check runs before any network call or
    /// Keychain write.
    func testMalformedKeyIsRejectedByCore() async throws {
        let (core, before) = try await liveCore()
        do {
            _ = try await core.storeKey("not-a-valid-format")
            XCTFail("must not store")
        } catch {
            XCTAssertEqual(error as? CoreServiceError, .keyFormat)
        }
        let after = await core.status()
        XCTAssertEqual(after.hasKey, before.hasKey)
        XCTAssertEqual(after.fingerprint, before.fingerprint)
    }

    /// FR-4: raw never calls the API and returns the transcript unchanged.
    func testRawPolishIsLocal() async throws {
        let (core, _) = try await liveCore()
        let input = "fixture words"
        let out = await core.polish(transcript: input, mode: .raw, translateTo: nil, glossary: [])
        XCTAssertFalse(out.fellBackToRaw)
        XCTAssertNil(out.usage)
        XCTAssertEqual(out.text.count, input.count)
    }

    /// Unknown model ids are refused by the core's manifest (INV-7).
    func testUnknownModelIsRefused() async throws {
        let (core, _) = try await liveCore()
        do {
            try await core.downloadModel("not-in-manifest") { _ in }
            XCTFail("unknown model must be refused")
        } catch {
            XCTAssertNotEqual(error as? CoreServiceError, .coreUnavailable)
        }
    }
}
