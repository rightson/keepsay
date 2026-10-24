// Maps characters to virtual keycodes for the current keyboard layout, so
// fn+Z and Cmd+V hit the key labelled Z / V on AZERTY, QWERTZ, Dvorak, ...
// Called when hotkeys are (re)installed and before a paste, never inside
// the event tap callback (§15 V9).

import Carbon

public enum KeyboardLayout {
    /// ANSI positions, used when the layout data is unavailable.
    public static let ansiZ: CGKeyCode = CGKeyCode(kVK_ANSI_Z)
    public static let ansiV: CGKeyCode = CGKeyCode(kVK_ANSI_V)

    /// The keycode whose unmodified output is `character` (case-insensitive).
    public static func keyCode(for character: Character) -> CGKeyCode? {
        let target = String(character).lowercased()
        return withLayout { layout in
            for code in 0..<CGKeyCode(128) where translate(layout, code) == target {
                return code
            }
            return nil
        } ?? nil
    }

    public static func character(for keyCode: CGKeyCode) -> String? {
        withLayout { translate($0, keyCode) } ?? nil
    }

    public static var zKeyCode: CGKeyCode { keyCode(for: "z") ?? ansiZ }
    public static var vKeyCode: CGKeyCode { keyCode(for: "v") ?? ansiV }

    private static func withLayout<T>(_ body: (UnsafePointer<UCKeyboardLayout>) -> T) -> T? {
        guard let source = TISCopyCurrentKeyboardLayoutInputSource()?.takeRetainedValue(),
            let raw = TISGetInputSourceProperty(source, kTISPropertyUnicodeKeyLayoutData)
        else { return nil }
        let data = Unmanaged<CFData>.fromOpaque(raw).takeUnretainedValue() as Data
        return data.withUnsafeBytes { bytes -> T? in
            guard let base = bytes.baseAddress else { return nil }
            return body(base.assumingMemoryBound(to: UCKeyboardLayout.self))
        }
    }

    private static func translate(_ layout: UnsafePointer<UCKeyboardLayout>, _ code: CGKeyCode) -> String? {
        var deadKeys: UInt32 = 0
        var length = 0
        var chars = [UniChar](repeating: 0, count: 4)
        let status = UCKeyTranslate(
            layout, code, UInt16(kUCKeyActionDisplay), 0, UInt32(LMGetKbdType()),
            OptionBits(kUCKeyTranslateNoDeadKeysBit), &deadKeys, chars.count, &length, &chars)
        guard status == noErr, length > 0 else { return nil }
        return String(utf16CodeUnits: chars, count: length).lowercased()
    }
}
