// Before a Cmd+V paste, switch away from a CJK input method to an
// ASCII-capable keyboard layout and switch back afterwards, so the IME does
// not swallow the V (US-M2-12; learned from shibing624/VoiceInput). Used only
// on the clipboard path; AX insertion does not send keystrokes.

import Carbon

public enum InputSourceRules {
    /// Pure decision, unit-tested. `languages` is the source's
    /// kTISPropertyInputSourceLanguages; `isInputMethod` is true for
    /// kTISTypeKeyboardInputMethod* / InputMode sources (Zhuyin, Cangjie,
    /// Kotoeri, ...), false for plain keyboard layouts (ABC, AZERTY).
    public static func needsSwitch(isASCIICapable: Bool, isInputMethod: Bool, languages: [String]) -> Bool {
        if !isASCIICapable { return true }
        guard isInputMethod else { return false }
        let cjk = ["zh", "ja", "ko", "yue"]
        return languages.contains { lang in cjk.contains { lang == $0 || lang.hasPrefix($0 + "-") } }
    }
}

public final class InputSourceSwitcher {
    private var previous: TISInputSource?

    public init() {}

    /// Switches when needed; returns whether it did.
    @discardableResult
    public func switchToASCIIIfNeeded() -> Bool {
        guard previous == nil, let current = TISCopyCurrentKeyboardInputSource()?.takeRetainedValue() else {
            return false
        }
        let ascii = boolProperty(current, kTISPropertyInputSourceIsASCIICapable)
        let type = stringProperty(current, kTISPropertyInputSourceType) ?? ""
        let isIM = type != (kTISTypeKeyboardLayout as String)
        let langs = arrayProperty(current, kTISPropertyInputSourceLanguages)
        guard InputSourceRules.needsSwitch(isASCIICapable: ascii, isInputMethod: isIM, languages: langs),
            let layout = TISCopyCurrentASCIICapableKeyboardLayoutInputSource()?.takeRetainedValue(),
            TISSelectInputSource(layout) == noErr
        else { return false }
        previous = current
        return true
    }

    public func restore() {
        guard let prev = previous else { return }
        previous = nil
        TISSelectInputSource(prev)
    }

    private func boolProperty(_ src: TISInputSource, _ key: CFString) -> Bool {
        guard let raw = TISGetInputSourceProperty(src, key) else { return false }
        return CFBooleanGetValue(Unmanaged<CFBoolean>.fromOpaque(raw).takeUnretainedValue())
    }

    private func stringProperty(_ src: TISInputSource, _ key: CFString) -> String? {
        guard let raw = TISGetInputSourceProperty(src, key) else { return nil }
        return Unmanaged<CFString>.fromOpaque(raw).takeUnretainedValue() as String
    }

    private func arrayProperty(_ src: TISInputSource, _ key: CFString) -> [String] {
        guard let raw = TISGetInputSourceProperty(src, key) else { return [] }
        return (Unmanaged<CFArray>.fromOpaque(raw).takeUnretainedValue() as? [String]) ?? []
    }
}
