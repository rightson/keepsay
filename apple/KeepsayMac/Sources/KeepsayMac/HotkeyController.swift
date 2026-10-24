// Installs the hotkeys (docs/changes/0004):
// - fn+Z / fn+Shift+Z through FnKeyTap when Accessibility is granted;
// - Carbon ⌥Space / ⌥⇧Space when the tap is unavailable, or always when
//   the user enabled it;
// - Esc (Carbon) only while recording.

import Foundation
import KeepsayKit
import KeepsayMacSupport

@MainActor
final class HotkeyController {
    enum Status: Equatable {
        case fnTap
        case carbonOnly
        case none
    }

    private(set) var status: Status = .none
    /// Set when a Carbon combination could not be registered (taken).
    private(set) var registrationFailed = false

    private var tap: FnKeyTap?
    private lazy var carbon = CarbonHotkeys { [weak self] id in
        // Carbon delivers hotkey events on the main thread.
        MainActor.assumeIsolated { self?.carbonFired(id) }
    }
    private var suppressors: [HotkeyAction: RepeatSuppressor] = [:]
    private let onAction: (HotkeyAction) -> Void
    private let onCancel: () -> Void

    init(onAction: @escaping (HotkeyAction) -> Void, onCancel: @escaping () -> Void) {
        self.onAction = onAction
        self.onCancel = onCancel
    }

    func install(settings: AppSettings) {
        tap?.stop()
        tap = nil
        carbon.unregister(.polish)
        carbon.unregister(.translate)
        registrationFailed = false

        var tapOK = false
        if Permissions.accessibilityTrusted {
            // The keycode for "Z" is resolved here, for the current layout.
            let t = FnKeyTap(zKeyCode: Int64(KeyboardLayout.zKeyCode)) { [weak self] translate in
                // The tap's run loop source is on the main run loop.
                MainActor.assumeIsolated { self?.fire(translate ? .polishAndTranslate : .polish) }
            }
            if t.start() {
                tap = t
                tapOK = true
            }
        }
        if !tapOK || settings.alwaysEnableFallbackHotkeys {
            let a = carbon.register(.polish, combo: settings.fallbackPolish)
            let b = carbon.register(.translate, combo: settings.fallbackTranslate)
            registrationFailed = !(a && b)
        }
        status = tapOK ? .fnTap : (carbon.isRegistered(.polish) || carbon.isRegistered(.translate) ? .carbonOnly : .none)
    }

    func setCancelKeyEnabled(_ enabled: Bool) {
        if enabled {
            carbon.register(.cancel, combo: CarbonHotkeys.escape)
        } else {
            carbon.unregister(.cancel)
        }
    }

    private func carbonFired(_ id: CarbonHotkeyID) {
        switch id {
        case .polish: fire(.polish)
        case .translate: fire(.polishAndTranslate)
        case .cancel: onCancel()
        }
    }

    private func fire(_ action: HotkeyAction) {
        var s = suppressors[action] ?? RepeatSuppressor()
        let ok = s.shouldFire(at: ProcessInfo.processInfo.systemUptime)
        suppressors[action] = s
        if ok { onAction(action) }
    }
}
