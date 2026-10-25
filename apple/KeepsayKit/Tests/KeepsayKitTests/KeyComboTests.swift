import XCTest

@testable import KeepsayKit

final class KeyComboTests: XCTestCase {
    func testDefaultsMatchCoreFallbackStrings() {
        // core/src/config.rs MACOS_FALLBACK_HOTKEYS.
        XCTAssertEqual(KeyCombo.fallbackPolish.configString, "Option+Space")
        XCTAssertEqual(KeyCombo.fallbackTranslate.configString, "Option+Shift+Space")
    }

    func testRoundTrip() {
        let combos = [
            KeyCombo.fallbackPolish,
            KeyCombo.fallbackTranslate,
            KeyCombo(keyCode: 6, carbonModifiers: KeyCombo.controlKey | KeyCombo.optionKey),
            KeyCombo(keyCode: 0, carbonModifiers: KeyCombo.cmdKey | KeyCombo.shiftKey),
        ]
        for c in combos {
            XCTAssertEqual(KeyCombo(configString: c.configString), c)
        }
        XCTAssertEqual(
            KeyCombo(keyCode: 6, carbonModifiers: KeyCombo.controlKey | KeyCombo.optionKey).configString,
            "Control+Option+VK6")
    }

    func testRejectsOtherStrings() {
        for s in ["", "Space", "Fn+Z", "Option+", "Option+Option+Space", "Hyper+Space", "Option+VK999", "Option+VKx"] {
            XCTAssertNil(KeyCombo(configString: s), s)
        }
    }
}
