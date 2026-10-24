// Carbon RegisterEventHotKey: the fallback combinations ⌥Space / ⌥⇧Space
// (docs/changes/0004) and Esc while recording. Needs no permission; the app
// receives only the combinations it registered.

import Carbon
import KeepsayKit

public enum CarbonHotkeyID: UInt32 {
    case polish = 1
    case translate = 2
    case cancel = 3
}

public final class CarbonHotkeys {
    public typealias Handler = (CarbonHotkeyID) -> Void

    private static let signature: OSType = 0x4B_53_41_59  // "KSAY"
    private var refs: [CarbonHotkeyID: EventHotKeyRef] = [:]
    private var handlerRef: EventHandlerRef?
    private let handler: Handler

    public init(handler: @escaping Handler) {
        self.handler = handler
    }

    deinit {
        unregisterAll()
        if let h = handlerRef { RemoveEventHandler(h) }
    }

    /// Registers `combo` under `id`, replacing any previous registration.
    /// Returns false when the combination is taken by another app.
    @discardableResult
    public func register(_ id: CarbonHotkeyID, combo: KeyCombo) -> Bool {
        installHandlerIfNeeded()
        unregister(id)
        var ref: EventHotKeyRef?
        let hotKeyID = EventHotKeyID(signature: Self.signature, id: id.rawValue)
        let status = RegisterEventHotKey(
            combo.keyCode, combo.carbonModifiers, hotKeyID, GetApplicationEventTarget(), 0, &ref)
        guard status == noErr, let ref else { return false }
        refs[id] = ref
        return true
    }

    public func unregister(_ id: CarbonHotkeyID) {
        if let ref = refs.removeValue(forKey: id) { UnregisterEventHotKey(ref) }
    }

    public func unregisterAll() {
        for id in Array(refs.keys) { unregister(id) }
    }

    public func isRegistered(_ id: CarbonHotkeyID) -> Bool { refs[id] != nil }

    private func installHandlerIfNeeded() {
        guard handlerRef == nil else { return }
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        InstallEventHandler(GetApplicationEventTarget(), carbonHotkeyCallback, 1, &spec, refcon, &handlerRef)
    }

    fileprivate func fire(_ rawID: UInt32) {
        if let id = CarbonHotkeyID(rawValue: rawID) { handler(id) }
    }

    /// Esc with no modifiers, registered only while recording.
    public static let escape = KeyCombo(keyCode: UInt32(kVK_Escape), carbonModifiers: 0)
}

private func carbonHotkeyCallback(
    _ next: EventHandlerCallRef?, _ event: EventRef?, _ refcon: UnsafeMutableRawPointer?
) -> OSStatus {
    guard let event, let refcon else { return OSStatus(eventNotHandledErr) }
    var hotKeyID = EventHotKeyID()
    let status = GetEventParameter(
        event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID), nil,
        MemoryLayout<EventHotKeyID>.size, nil, &hotKeyID)
    guard status == noErr else { return status }
    Unmanaged<CarbonHotkeys>.fromOpaque(refcon).takeUnretainedValue().fire(hotKeyID.id)
    return noErr
}

/// Display strings for KeyCombo (settings UI).
public enum KeyComboFormatter {
    public static func string(for combo: KeyCombo) -> String {
        var s = ""
        if combo.carbonModifiers & KeyCombo.controlKey != 0 { s += "⌃" }
        if combo.carbonModifiers & KeyCombo.optionKey != 0 { s += "⌥" }
        if combo.carbonModifiers & KeyCombo.shiftKey != 0 { s += "⇧" }
        if combo.carbonModifiers & KeyCombo.cmdKey != 0 { s += "⌘" }
        return s + " " + keyName(combo.keyCode)
    }

    public static func keyName(_ code: UInt32) -> String {
        switch Int(code) {
        case kVK_Space: return "Space"
        case kVK_Return: return "Return"
        case kVK_Tab: return "Tab"
        case kVK_Escape: return "Esc"
        default:
            return KeyboardLayout.character(for: CGKeyCode(code))?.uppercased() ?? "#\(code)"
        }
    }

    /// Carbon modifier mask from NSEvent-style flags (recorder UI).
    public static func carbonModifiers(command: Bool, option: Bool, control: Bool, shift: Bool) -> UInt32 {
        var m: UInt32 = 0
        if command { m |= KeyCombo.cmdKey }
        if option { m |= KeyCombo.optionKey }
        if control { m |= KeyCombo.controlKey }
        if shift { m |= KeyCombo.shiftKey }
        return m
    }

    /// A recorded combination must have at least one of ⌘⌥⌃ so plain typing
    /// (and Shift+letter) is never captured.
    public static func isAcceptable(_ combo: KeyCombo) -> Bool {
        combo.carbonModifiers & (KeyCombo.cmdKey | KeyCombo.optionKey | KeyCombo.controlKey) != 0
            && Int(combo.keyCode) != kVK_Escape
    }
}
