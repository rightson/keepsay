// mobile.html home / recording / processing / done / fallback.

import SwiftUI

struct HomeView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        Group {
            switch model.phase {
            case .idle:
                IdleView()
            case .recording(let plan, let startedAt):
                RecordingView(plan: plan, startedAt: startedAt)
            case .transcribing(let plan):
                ProcessingView(plan: plan, polishing: false)
            case .polishing(let plan, _):
                ProcessingView(plan: plan, polishing: true)
            case .done(let output):
                ResultView(output: output)
            case .failed(let failure):
                FailureView(failure: failure)
            }
        }
        .animation(.default, value: model.phase.description)
    }
}

struct IdleView: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        @Bindable var model = model
        VStack(spacing: 26) {
            Spacer()
            Button { model.trigger(.polish) } label: {
                Image(systemName: "mic.fill")
                    .font(.system(size: 60))
                    .frame(width: 188, height: 188)
                    .foregroundStyle(.white)
                    .background(Color.accentColor, in: Circle())
                    .shadow(color: .accentColor.opacity(0.4), radius: 20, y: 12)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("開始錄音（潤飾）")
            Text("點一下，開始說話").foregroundStyle(.secondary)

            ModePicker(selection: $model.selectedMode, isAvailable: model.isModeAvailable)
            Text((model.hasKey ? "" : "未設定金鑰，只能用原文・") + model.selectedMode.summary)
                .font(.footnote).foregroundStyle(.tertiary)

            Button { model.trigger(.polishAndTranslate) } label: {
                Label("潤飾並翻譯成 \(model.settings.targetLanguage.label)", systemImage: "globe")
            }
            .buttonStyle(SoftButtonStyle())
            .frame(maxWidth: 320)
            Spacer()
            BudgetPill()
        }
        .padding(.horizontal, 24)
        .navigationTitle("keepsay")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                NavigationLink { SettingsView() } label: { Image(systemName: "gearshape") }
                    .accessibilityLabel("設定")
            }
        }
    }
}

struct ModePicker: View {
    @Binding var selection: PolishMode
    var isAvailable: (PolishMode) -> Bool

    var body: some View {
        HStack(spacing: 0) {
            ForEach(PolishMode.allCases) { mode in
                Button { selection = mode } label: {
                    Text(mode.label)
                        .font(.subheadline.weight(selection == mode ? .semibold : .regular))
                        .frame(maxWidth: .infinity, minHeight: 40)
                        .foregroundStyle(selection == mode ? Color.primary : Color.secondary)
                        .background {
                            if selection == mode {
                                RoundedRectangle(cornerRadius: 9)
                                    .fill(Color(.systemBackground))
                                    .shadow(color: .black.opacity(0.12), radius: 1.5, y: 1)
                            }
                        }
                }
                .buttonStyle(.plain)
                .disabled(!isAvailable(mode))
                .opacity(isAvailable(mode) ? 1 : 0.35)
                .accessibilityAddTraits(selection == mode ? .isSelected : [])
            }
        }
        .padding(3)
        .background(Color(.tertiarySystemFill), in: RoundedRectangle(cornerRadius: 12))
        .frame(maxWidth: 320)
    }
}

struct BudgetPill: View {
    @Environment(AppModel.self) private var model

    var body: some View {
        HStack(spacing: 6) {
            if model.hasKey {
                Text("本月 \(Usd.format(model.budget.estimatedUSD))")
                if let cap = model.budget.hardLimitUSD ?? model.budget.softLimitUSD {
                    Text("／ \(Usd.format(cap))")
                }
                ProgressView(value: model.budget.fraction)
                    .frame(width: 36)
                    .tint(model.budget.softLimited ? .orange : .secondary)
            } else {
                Text("原文模式・完全離線")
            }
        }
        .font(.caption)
        .foregroundStyle(.tertiary)
        .padding(.bottom, 14)
    }
}

enum Usd {
    static func format(_ v: Double) -> String { String(format: "US$%.2f", v) }
    static func formatSmall(_ v: Double) -> String { v < 0.01 ? String(format: "US$%.3f", v) : format(v) }
}

struct RecordingView: View {
    @Environment(AppModel.self) private var model
    let plan: DictationPlan
    let startedAt: Date

    var body: some View {
        VStack(spacing: 22) {
            Spacer()
            HStack(spacing: 6) {
                Circle().fill(Color.accentColor).frame(width: 8, height: 8)
                Text("錄音中・\(planLabel)")
            }
            .font(.footnote).foregroundStyle(.secondary)
            .padding(.horizontal, 12).padding(.vertical, 5)
            .background(Color(.tertiarySystemFill), in: Capsule())

            TimelineView(.periodic(from: startedAt, by: 0.5)) { ctx in
                Text(Self.clock(ctx.date.timeIntervalSince(startedAt)))
                    .font(.system(size: 54, weight: .light).monospacedDigit())
            }
            LevelMeter(level: model.level)

            Button { model.stop() } label: {
                Image(systemName: "stop.fill")
                    .font(.system(size: 36))
                    .frame(width: 112, height: 112)
                    .foregroundStyle(Color.accentColor)
                    .background(Circle().stroke(Color.accentColor, lineWidth: 2))
            }
            .buttonStyle(.plain)
            .accessibilityLabel("停止並處理")
            Text("點一下停止").foregroundStyle(.secondary)
            Spacer()
            Button("取消", role: .cancel) { model.cancel() }
            Text("音訊只在記憶體中，處理完即丟棄").font(.caption).foregroundStyle(.tertiary)
        }
        .padding(.horizontal, 28)
        .padding(.bottom, 12)
        .navigationBarBackButtonHidden()
    }

    private var planLabel: String {
        if let t = plan.translateTo { return "潤稿並翻譯成 \(t.label)" }
        return plan.mode.label
    }

    static func clock(_ seconds: TimeInterval) -> String {
        let s = max(0, Int(seconds))
        return String(format: "%02d:%02d", s / 60, s % 60)
    }
}

struct LevelMeter: View {
    let level: Float

    var body: some View {
        HStack(spacing: 5) {
            ForEach(0..<21, id: \.self) { i in
                let shape = 1 - abs(Double(i) - 10) / 11
                Capsule()
                    .fill(Color.accentColor)
                    .frame(width: 5, height: max(6, CGFloat(Double(min(1, level * 4)) * 60 * shape)))
            }
        }
        .frame(height: 64)
        .animation(.linear(duration: 0.09), value: level)
        .accessibilityHidden(true)
    }
}

struct ProcessingView: View {
    let plan: DictationPlan
    let polishing: Bool

    var body: some View {
        VStack(spacing: 22) {
            Spacer()
            VStack(alignment: .leading, spacing: 14) {
                step("轉寫中", active: !polishing, done: polishing)
                if plan.needsAPI {
                    step(plan.translateTo == nil ? "潤稿中" : "潤稿並翻譯中", active: polishing, done: false)
                }
            }
            .frame(width: 220, alignment: .leading)
            Text(polishing ? "只送出文字到 api.anthropic.com" : "在手機上轉寫・不上傳音訊")
                .font(.footnote).foregroundStyle(.tertiary)
            Spacer()
        }
        .navigationBarBackButtonHidden()
    }

    private func step(_ title: String, active: Bool, done: Bool) -> some View {
        HStack(spacing: 12) {
            ZStack {
                if done {
                    Image(systemName: "checkmark.circle.fill").foregroundStyle(Color.accentColor)
                } else if active {
                    ProgressView()
                } else {
                    Image(systemName: "circle").foregroundStyle(.tertiary)
                }
            }
            .frame(width: 26, height: 26)
            Text(title).font(.title3).foregroundStyle(active ? Color.primary : Color.secondary)
        }
    }
}

struct ResultView: View {
    @Environment(AppModel.self) private var model
    let output: DictationOutput

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            Banner(output: output, copiedUntil: model.copiedUntil)
            ScrollView {
                Text(output.text)
                    .font(.title3)
                    .lineSpacing(6)
                    .textSelection(.enabled)
                    .privacySensitive()
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(18)
            }
            .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 16))
            .padding(.top, 16)
            HStack {
                Text(metaLeft)
                Spacer()
                Text("離開後不保留")
            }
            .font(.caption).foregroundStyle(.tertiary)
            .padding(.horizontal, 4).padding(.top, 10)
            Spacer(minLength: 16)
            VStack(spacing: 6) {
                Button("再錄一段") { model.trigger(output.plan.action) }
                    .buttonStyle(PrimaryButtonStyle())
                Button("再複製一次") { model.copyAgain() }
                    .buttonStyle(SoftButtonStyle())
                Button("完成") { model.dismiss() }
                    .frame(maxWidth: .infinity, minHeight: 44)
            }
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 12)
        .navigationBarBackButtonHidden()
    }

    private var metaLeft: String {
        if output.kind == .raw { return "原文（本機轉寫）" }
        let what = output.plan.translateTo.map { "潤稿並翻譯（\($0.label)）" } ?? output.plan.mode.label
        guard let u = output.usage else { return what }
        let s = model.settings
        let usd = Double(u.inputTokens) * s.inputPricePerMTok / 1e6 + Double(u.outputTokens) * s.outputPricePerMTok / 1e6
        return "\(what)・約 \(Usd.formatSmall(usd))"
    }
}

struct Banner: View {
    let output: DictationOutput
    let copiedUntil: Date?

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: output.fallback == nil ? "checkmark" : "exclamationmark")
                .font(.system(size: 16, weight: .bold))
                .frame(width: 32, height: 32)
                .foregroundStyle(.white)
                .background(output.fallback == nil ? Color.accentColor : Color.secondary, in: Circle())
            VStack(alignment: .leading, spacing: 2) {
                Text(output.fallback?.message ?? "完成").font(.headline)
                if let until = copiedUntil {
                    TimelineView(.periodic(from: .now, by: 1)) { ctx in
                        let left = Int(until.timeIntervalSince(ctx.date).rounded(.up))
                        Text(left > 0 ? "已複製到剪貼簿・\(left) 秒後清除" : "剪貼簿已清除")
                            .font(.footnote.monospacedDigit()).foregroundStyle(.secondary)
                    }
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 16).padding(.vertical, 14)
        .background(
            output.fallback == nil ? Color.accentColor.opacity(0.12) : Color(.tertiarySystemFill),
            in: RoundedRectangle(cornerRadius: 16))
        .accessibilityElement(children: .combine)
    }
}

struct FailureView: View {
    @Environment(AppModel.self) private var model
    let failure: DictationFailure

    var body: some View {
        VStack(spacing: 18) {
            Spacer()
            Image(systemName: "exclamationmark.triangle").font(.system(size: 44)).foregroundStyle(.secondary)
            Text(failure.message).multilineTextAlignment(.center)
            if failure == .microphoneDenied, let url = URL(string: UIApplication.openSettingsURLString) {
                Button("開啟設定") { UIApplication.shared.open(url) }
            }
            Spacer()
            Button("返回") { model.dismiss() }.buttonStyle(PrimaryButtonStyle())
        }
        .padding(24)
        .navigationBarBackButtonHidden()
    }
}
