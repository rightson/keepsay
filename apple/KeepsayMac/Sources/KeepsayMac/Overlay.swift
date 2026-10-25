// Floating HUD pill (docs/mockups/desktop.html "錄音浮條"): recording,
// processing, inserted, fallback. A non-activating panel that ignores the
// mouse, so it never takes focus from the app we insert into.

import AppKit
import KeepsayKit
import KeepsayMacSupport
import SwiftUI

@MainActor
final class OverlayController {
    private var panel: NSPanel?
    private weak var model: AppModel?

    func attach(model: AppModel) {
        self.model = model
    }

    func update(phase: DictationPhase) {
        if phase == .idle {
            panel?.orderOut(nil)
            return
        }
        let p = panel ?? makePanel()
        panel = p
        position(p)
        p.orderFrontRegardless()
    }

    private func makePanel() -> NSPanel {
        let p = NSPanel(
            contentRect: NSRect(x: 0, y: 0, width: 320, height: 48),
            styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: true)
        p.level = .statusBar
        p.isOpaque = false
        p.backgroundColor = .clear
        p.hasShadow = true
        p.ignoresMouseEvents = true
        p.hidesOnDeactivate = false
        p.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary]
        if let model {
            let host = NSHostingView(rootView: OverlayPill().environmentObject(model))
            host.frame = p.contentLayoutRect
            host.autoresizingMask = [.width, .height]
            p.contentView = host
        }
        return p
    }

    private func position(_ p: NSPanel) {
        guard let screen = NSScreen.main else { return }
        let f = screen.visibleFrame
        let size = p.frame.size
        p.setFrameOrigin(NSPoint(x: f.midX - size.width / 2, y: f.minY + 72))
    }
}

struct OverlayPill: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        HStack(spacing: 10) {
            content
        }
        .font(.system(size: 13, weight: .medium))
        .padding(.horizontal, 16)
        .frame(height: 40)
        .background(.regularMaterial, in: Capsule())
        .overlay(Capsule().strokeBorder(.quaternary))
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    @ViewBuilder private var content: some View {
        switch model.phase {
        case .idle:
            EmptyView()
        case .recording(let action):
            LevelBars(level: model.level)
            TimelineView(.periodic(from: .now, by: 1)) { ctx in
                Text(elapsed(at: ctx.date)).monospacedDigit().foregroundStyle(.secondary)
            }
            if action == .polishAndTranslate {
                Text("翻譯→\(model.settings.translateTo.label)").foregroundStyle(.secondary)
            }
            Text(stopHint).font(.system(size: 11)).foregroundStyle(.tertiary)
        case .transcribing:
            ProgressView().controlSize(.small)
            Text("轉寫中…")
        case .polishing(let action):
            ProgressView().controlSize(.small)
            Text(action == .polishAndTranslate ? "潤稿並翻譯中…" : "潤稿中…")
        case .inserting:
            ProgressView().controlSize(.small)
            Text("插入中…")
        case .done(let mode, let translated):
            Image(systemName: "checkmark.circle.fill").foregroundStyle(.green)
            Text("已插入・\(mode.label)\(translated ? "＋翻譯" : "")")
        case .fellBack(let cause):
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.orange)
            Text(cause.message)
        case .failed(let failure):
            Image(systemName: "xmark.circle.fill").foregroundStyle(.red)
            Text(failure.map { "未插入：\($0.message)" } ?? "未插入任何文字")
        }
    }

    private var stopHint: String {
        model.hotkeyStatus == .fnTap ? "fn Z 停止・Esc 取消" : "\(KeyComboFormatter.string(for: model.settings.fallbackPolish)) 停止・Esc 取消"
    }

    private func elapsed(at date: Date) -> String {
        let s = Int(date.timeIntervalSince(model.recordingStartedAt ?? date))
        return String(format: "%d:%02d", s / 60, s % 60)
    }
}

struct LevelBars: View {
    var level: Float

    var body: some View {
        HStack(spacing: 2) {
            ForEach(0..<10, id: \.self) { i in
                let center = 1 - abs(Double(i) - 4.5) / 6
                Capsule()
                    .fill(Color.red)
                    .frame(width: 3, height: max(4, 20 * CGFloat(level) * CGFloat(center)))
            }
        }
        .frame(height: 22)
        .animation(.easeOut(duration: 0.1), value: level)
    }
}
