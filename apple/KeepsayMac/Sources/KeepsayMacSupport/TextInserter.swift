// Inserts text into the focused app (FR-5, PROPOSAL §7.1):
// 1. Accessibility: set kAXSelectedTextAttribute on the focused element.
// 2. Clipboard fallback: PasteboardTransaction + Cmd+V, with a temporary
//    switch to an ASCII layout when a CJK input method is active.
//
// Nothing here logs the text.

import AppKit
import ApplicationServices
import Carbon

public enum InsertionPath: Equatable {
    case accessibility
    case pasteboard
    case failed
}

public enum Permissions {
    /// Accessibility ("輔助使用"): AX insertion, Cmd+V events, the fn tap.
    public static var accessibilityTrusted: Bool { AXIsProcessTrusted() }

    /// Shows the system prompt once.
    public static func requestAccessibility() {
        let key = kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String
        _ = AXIsProcessTrustedWithOptions([key: true] as CFDictionary)
    }

    public static let accessibilitySettingsURL = URL(
        string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
    public static let microphoneSettingsURL = URL(
        string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")!
}

public enum AXInserter {
    public static func insert(_ text: String) -> Bool {
        guard Permissions.accessibilityTrusted else { return false }
        let system = AXUIElementCreateSystemWide()
        var focused: CFTypeRef?
        guard AXUIElementCopyAttributeValue(system, kAXFocusedUIElementAttribute as CFString, &focused) == .success,
            let focused, CFGetTypeID(focused) == AXUIElementGetTypeID()
        else { return false }
        let element = focused as! AXUIElement  // CF type, checked above
        var settable: DarwinBoolean = false
        guard AXUIElementIsAttributeSettable(element, kAXSelectedTextAttribute as CFString, &settable) == .success,
            settable.boolValue
        else { return false }
        return AXUIElementSetAttributeValue(element, kAXSelectedTextAttribute as CFString, text as CFString)
            == .success
    }
}

@MainActor
public final class TextInserter {
    private let pasteboard: PasteboardIO
    private let inputSources = InputSourceSwitcher()

    public init(pasteboard: PasteboardIO = SystemPasteboard()) {
        self.pasteboard = pasteboard
    }

    public func insert(_ text: String) async -> InsertionPath {
        if AXInserter.insert(text) { return .accessibility }
        guard Permissions.accessibilityTrusted else { return .failed }  // CGEvent posting needs it too
        let tx = PasteboardTransaction(io: pasteboard)
        tx.begin(text: text)
        let switched = inputSources.switchToASCIIIfNeeded()
        if switched { try? await Task.sleep(nanoseconds: 60_000_000) }
        Self.postCommandV()
        try? await Task.sleep(nanoseconds: UInt64(PasteboardTransaction.restoreDelay * 1_000_000_000))
        tx.finish()
        if switched { inputSources.restore() }
        return .pasteboard
    }

    private static func postCommandV() {
        let v = KeyboardLayout.vKeyCode
        let source = CGEventSource(stateID: .combinedSessionState)
        let down = CGEvent(keyboardEventSource: source, virtualKey: v, keyDown: true)
        let up = CGEvent(keyboardEventSource: source, virtualKey: v, keyDown: false)
        down?.flags = .maskCommand
        up?.flags = .maskCommand
        down?.post(tap: .cghidEventTap)
        up?.post(tap: .cghidEventTap)
    }
}
