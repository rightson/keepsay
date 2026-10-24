import KeepsayKit
import XCTest

@testable import KeepsayMacSupport

final class FnKeyTapRulesTests: XCTestCase {
    let z: Int64 = 6  // kVK_ANSI_Z
    let azertyZ: Int64 = 13  // AZERTY puts Z on the ANSI W key

    func testFnZFiresPolishAndFnShiftZFiresTranslate() {
        XCTAssertEqual(FnKeyTapRules.decide(isKeyDown: true, keyCode: z, fn: true, shift: false, zKeyCode: z),
                       .fire(translate: false))
        XCTAssertEqual(FnKeyTapRules.decide(isKeyDown: true, keyCode: z, fn: true, shift: true, zKeyCode: z),
                       .fire(translate: true))
    }

    func testKeyUpOfMatchIsSwallowedWithoutAction() {
        XCTAssertEqual(FnKeyTapRules.decide(isKeyDown: false, keyCode: z, fn: true, shift: false, zKeyCode: z),
                       .swallow)
    }

    func testEverythingElsePassesThrough() {
        // Plain Z, Shift+Z, fn + another key, fn+Z on a layout where Z moved.
        XCTAssertEqual(FnKeyTapRules.decide(isKeyDown: true, keyCode: z, fn: false, shift: false, zKeyCode: z),
                       .passThrough)
        XCTAssertEqual(FnKeyTapRules.decide(isKeyDown: true, keyCode: z, fn: false, shift: true, zKeyCode: z),
                       .passThrough)
        XCTAssertEqual(FnKeyTapRules.decide(isKeyDown: false, keyCode: z, fn: false, shift: false, zKeyCode: z),
                       .passThrough)
        for code in Int64(0)..<128 where code != z {
            XCTAssertEqual(FnKeyTapRules.decide(isKeyDown: true, keyCode: code, fn: true, shift: false, zKeyCode: z),
                           .passThrough, "keycode \(code)")
        }
        XCTAssertEqual(FnKeyTapRules.decide(isKeyDown: true, keyCode: z, fn: true, shift: false, zKeyCode: azertyZ),
                       .passThrough)
        XCTAssertEqual(
            FnKeyTapRules.decide(isKeyDown: true, keyCode: azertyZ, fn: true, shift: false, zKeyCode: azertyZ),
            .fire(translate: false))
    }
}

final class FakePasteboard: PasteboardIO {
    var changeCount = 0
    var items: [[PasteboardEntry]] = []
    var restores = 0

    func snapshot() -> PasteboardSnapshot { PasteboardSnapshot(items: items) }

    func write(item: [PasteboardEntry]) -> Int {
        items = [item]
        changeCount += 1
        return changeCount
    }

    func restore(_ snapshot: PasteboardSnapshot) {
        items = snapshot.items
        changeCount += 1
        restores += 1
    }

    /// Another app copies something.
    func externalCopy(_ s: String) {
        items = [[PasteboardEntry(type: "public.utf8-plain-text", data: Data(s.utf8))]]
        changeCount += 1
    }
}

final class PasteboardTransactionTests: XCTestCase {
    func original() -> [[PasteboardEntry]] {
        [
            [PasteboardEntry(type: "public.utf8-plain-text", data: Data("a".utf8)),
             PasteboardEntry(type: "public.rtf", data: Data([1, 2, 3]))],
            [PasteboardEntry(type: "public.png", data: Data([9, 9]))],
        ]
    }

    func testWritesMarkedItemAndRestoresAllItems() {
        let pb = FakePasteboard()
        pb.items = original()
        let tx = PasteboardTransaction(io: pb)
        tx.begin(text: "hello")
        XCTAssertEqual(pb.items.count, 1)
        let types = pb.items[0].map(\.type)
        XCTAssertTrue(types.contains(PasteboardMarkers.concealed))
        XCTAssertTrue(types.contains(PasteboardMarkers.transient))
        XCTAssertTrue(types.contains(PasteboardMarkers.plainText))
        XCTAssertEqual(tx.finish(), .restored)
        XCTAssertEqual(pb.items, original())
    }

    func testRestoresEmptyPasteboardToEmpty() {
        let pb = FakePasteboard()
        let tx = PasteboardTransaction(io: pb)
        tx.begin(text: "x")
        XCTAssertEqual(tx.finish(), .restored)
        XCTAssertEqual(pb.items, [])
    }

    func testNewerContentFromOthersWins() {
        let pb = FakePasteboard()
        pb.items = original()
        let tx = PasteboardTransaction(io: pb)
        tx.begin(text: "x")
        pb.externalCopy("newer")
        XCTAssertEqual(tx.finish(), .skippedChangedByOthers)
        XCTAssertEqual(pb.restores, 0)
        XCTAssertEqual(pb.items[0][0].data, Data("newer".utf8))
    }

    func testFinishIsOneShot() {
        let pb = FakePasteboard()
        let tx = PasteboardTransaction(io: pb)
        XCTAssertEqual(tx.finish(), .notStarted)
        tx.begin(text: "x")
        XCTAssertEqual(tx.finish(), .restored)
        XCTAssertEqual(tx.finish(), .notStarted)
        XCTAssertEqual(pb.restores, 1)
    }

    func testSnapshotDescriptionHasNoContent() {
        let s = PasteboardSnapshot(items: [[PasteboardEntry(type: "t", data: Data("CANARY-7f3a".utf8))]])
        XCTAssertFalse(String(describing: s).contains("CANARY"))
        XCTAssertFalse(String(reflecting: s).contains("CANARY"))
    }
}

final class InputSourceRulesTests: XCTestCase {
    func testSwitchesForCJKInputMethodsOnly() {
        XCTAssertTrue(InputSourceRules.needsSwitch(isASCIICapable: true, isInputMethod: true, languages: ["zh-Hant"]))
        XCTAssertTrue(InputSourceRules.needsSwitch(isASCIICapable: true, isInputMethod: true, languages: ["ja"]))
        XCTAssertTrue(InputSourceRules.needsSwitch(isASCIICapable: false, isInputMethod: true, languages: ["ko"]))
        XCTAssertFalse(InputSourceRules.needsSwitch(isASCIICapable: true, isInputMethod: false, languages: ["en"]))
        XCTAssertFalse(InputSourceRules.needsSwitch(isASCIICapable: true, isInputMethod: false, languages: ["fr"]))
        XCTAssertFalse(InputSourceRules.needsSwitch(isASCIICapable: true, isInputMethod: true, languages: ["vi"]))
        XCTAssertFalse(InputSourceRules.needsSwitch(isASCIICapable: true, isInputMethod: true, languages: ["jv"]))
    }
}

final class KeyComboTests: XCTestCase {
    func testFormattingAndAcceptance() {
        XCTAssertEqual(KeyComboFormatter.string(for: .fallbackPolish), "⌥ Space")
        XCTAssertEqual(KeyComboFormatter.string(for: .fallbackTranslate), "⌥⇧ Space")
        XCTAssertTrue(KeyComboFormatter.isAcceptable(.fallbackPolish))
        XCTAssertFalse(KeyComboFormatter.isAcceptable(KeyCombo(keyCode: 6, carbonModifiers: KeyCombo.shiftKey)))
        XCTAssertFalse(KeyComboFormatter.isAcceptable(KeyCombo(keyCode: 53, carbonModifiers: KeyCombo.optionKey)))
        XCTAssertEqual(
            KeyComboFormatter.carbonModifiers(command: false, option: true, control: false, shift: true),
            KeyCombo.optionKey | KeyCombo.shiftKey)
    }
}
