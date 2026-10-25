// fn+Z / fn+Shift+Z via ONE active CGEventTap (docs/changes/0004, V9 (b)).
//
// THREAT_MODEL T7 — this file is the only code in keepsay that sees global
// key events. Review rules, enforced by the "fn tap scan" step in
// .github/workflows/macos-app.yml:
// - The only import is CoreGraphics.
// - The tap subscribes to keyDown and keyUp only.
// - The callback reads exactly three things from an event: its keycode,
//   the fn flag (maskSecondaryFn) and the Shift flag (maskShift).
// - Nothing is stored, logged, written or forwarded. Non-matching events are
//   returned unchanged. A matching event is swallowed (nil) and only the
//   decided action is passed on.
// - No file, network, pasteboard or logging API is referenced here.
//
// The Z keycode for the current layout is computed by the caller when the
// tap is installed (AZERTY moves Z), never inside the callback.

import CoreGraphics

/// Which hotkey a key event is, decided from keycode + fn + Shift only.
public enum FnTapDecision: Equatable {
    /// Not ours: deliver unchanged.
    case passThrough
    /// fn+Z key-up (or a key-down we swallow): swallow, no action.
    case swallow
    /// fn+Z → polish; fn+Shift+Z → polish and translate.
    case fire(translate: Bool)
}

public enum FnKeyTapRules {
    /// Pure matcher; unit-tested. `isKeyDown` is false for keyUp.
    public static func decide(isKeyDown: Bool, keyCode: Int64, fn: Bool, shift: Bool, zKeyCode: Int64)
        -> FnTapDecision
    {
        guard fn, keyCode == zKeyCode else { return .passThrough }
        return isKeyDown ? .fire(translate: shift) : .swallow
    }
}

/// Owns the tap. Create on the main thread; the callback runs on the main
/// run loop.
public final class FnKeyTap {
    public typealias Handler = (_ translate: Bool) -> Void

    private let zKeyCode: Int64
    private let handler: Handler
    private var port: CFMachPort?
    private var source: CFRunLoopSource?

    public init(zKeyCode: Int64, handler: @escaping Handler) {
        self.zKeyCode = zKeyCode
        self.handler = handler
    }

    deinit { stop() }

    /// Returns false when the tap cannot be created (no Accessibility
    /// permission); the caller then falls back to the Carbon combinations.
    public func start() -> Bool {
        guard port == nil else { return true }
        let mask = (CGEventMask(1) << CGEventType.keyDown.rawValue) | (CGEventMask(1) << CGEventType.keyUp.rawValue)
        let refcon = Unmanaged.passUnretained(self).toOpaque()
        guard
            let tap = CGEvent.tapCreate(
                tap: .cgSessionEventTap, place: .headInsertEventTap, options: .defaultTap,
                eventsOfInterest: mask, callback: fnKeyTapCallback, userInfo: refcon)
        else { return false }
        port = tap
        source = CFMachPortCreateRunLoopSource(kCFAllocatorDefault, tap, 0)
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
        CGEvent.tapEnable(tap: tap, enable: true)
        return true
    }

    public func stop() {
        if let tap = port { CGEvent.tapEnable(tap: tap, enable: false) }
        if let src = source { CFRunLoopRemoveSource(CFRunLoopGetMain(), src, .commonModes) }
        port = nil
        source = nil
    }

    fileprivate func handle(type: CGEventType, event: CGEvent) -> Unmanaged<CGEvent>? {
        switch type {
        case .tapDisabledByTimeout, .tapDisabledByUserInput:
            if let tap = port { CGEvent.tapEnable(tap: tap, enable: true) }
            return Unmanaged.passUnretained(event)
        case .keyDown, .keyUp:
            let decision = FnKeyTapRules.decide(
                isKeyDown: type == .keyDown,
                keyCode: event.getIntegerValueField(.keyboardEventKeycode),
                fn: event.flags.contains(.maskSecondaryFn),
                shift: event.flags.contains(.maskShift),
                zKeyCode: zKeyCode)
            switch decision {
            case .passThrough:
                return Unmanaged.passUnretained(event)
            case .swallow:
                return nil
            case .fire(let translate):
                handler(translate)
                return nil
            }
        default:
            return Unmanaged.passUnretained(event)
        }
    }
}

private func fnKeyTapCallback(
    proxy: CGEventTapProxy, type: CGEventType, event: CGEvent, refcon: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    guard let refcon else { return Unmanaged.passUnretained(event) }
    let tap = Unmanaged<FnKeyTap>.fromOpaque(refcon).takeUnretainedValue()
    return tap.handle(type: type, event: event)
}
