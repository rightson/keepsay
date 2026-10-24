// keepsay menu bar app (M2). LSUIElement app: no Dock icon, one status
// item, a settings window and the floating HUD pill.

import AppKit
import KeepsayKit
import KeepsayMacSupport
import SwiftUI

@main
struct KeepsayMacApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var delegate

    var body: some Scene {
        MenuBarExtra {
            MenuContent().environmentObject(delegate.model)
        } label: {
            MenuBarIcon().environmentObject(delegate.model)
        }
        .menuBarExtraStyle(.menu)

        Settings {
            SettingsView().environmentObject(delegate.model)
        }
    }
}

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate {
    let model = AppModel()

    func applicationDidFinishLaunching(_ notification: Notification) {
        model.start()
        if !Permissions.accessibilityTrusted {
            // fn+Z and AX insertion need Accessibility; the Carbon fallback
            // works meanwhile.
            Permissions.requestAccessibility()
        }
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        // Permissions may have changed in System Settings.
        let wasTrusted = model.accessibilityTrusted
        model.refreshAll()
        if model.accessibilityTrusted != wasTrusted { model.reinstallHotkeys() }
    }
}

struct MenuBarIcon: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        switch model.phase {
        case .recording: Image(systemName: "waveform.circle.fill")
        case .transcribing, .polishing, .inserting: Image(systemName: "ellipsis.circle")
        default: Image(systemName: "waveform")
        }
    }
}

struct MenuContent: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Text("keepsay・\(statusText)")
        Text(hotkeyHint)
        Divider()
        Text("模式")
        ForEach(Array(DictationMode.allCases.enumerated()), id: \.element) { index, mode in
            Button {
                model.settings.mode = mode
            } label: {
                if model.settings.mode == mode {
                    Label(mode.label, systemImage: "checkmark")
                } else {
                    Text(mode.label)
                }
            }
            .keyboardShortcut(KeyEquivalent(Character("\(index + 1)")), modifiers: .command)
            .disabled(!model.hasKey && mode != .raw)
        }
        Divider()
        Text(budgetText)
        Divider()
        SettingsLink {
            Text("設定…")
        }
        .keyboardShortcut(",", modifiers: .command)
        Button("結束 keepsay") { NSApp.terminate(nil) }
            .keyboardShortcut("q", modifiers: .command)
    }

    private var statusText: String {
        switch model.phase {
        case .recording: "錄音中"
        case .transcribing, .polishing, .inserting: "處理中"
        default: "就緒"
        }
    }

    private var hotkeyHint: String {
        switch model.hotkeyStatus {
        case .fnTap: "按 fn Z 開始／停止說話"
        case .carbonOnly: "按 \(KeyComboFormatter.string(for: model.settings.fallbackPolish)) 開始／停止說話"
        case .none: "快捷鍵未註冊，請到設定修改"
        }
    }

    private var budgetText: String {
        guard model.hasKey else { return "未設定金鑰・只用原文" }
        let b = model.budget
        let limit = b.hardLimitUSD.map { String(format: " ／ %.2f", $0) } ?? ""
        return String(format: "本月 US$%.2f", b.estimatedUSD) + limit
    }
}
