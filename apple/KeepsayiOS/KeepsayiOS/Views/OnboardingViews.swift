// mobile.html ob-key / ob-model.

import SwiftUI

/// Enables the save button only; the core does the real format check and
/// the online verification (FR-1).
enum KeyInput {
    static func isPlausible(_ text: String) -> Bool {
        let t = text.trimmingCharacters(in: .whitespacesAndNewlines)
        return t.hasPrefix("sk-ant-") && t.count >= 20
    }
}

/// Masked key entry. INV-2: the text goes to the core once and the field is
/// cleared before the call returns. There is no reveal or copy affordance.
struct KeyEntryField: View {
    @Binding var text: String
    var onSubmit: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "key").foregroundStyle(.tertiary)
            SecureField("sk-ant-…", text: $text)
                // No .password content type: it would invite the system to
                // offer saving the key to (synced) iCloud Keychain (T3).
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .privacySensitive()
                .submitLabel(.done)
                .onSubmit(onSubmit)
        }
        .padding(.horizontal, 14)
        .frame(minHeight: 52)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 14))
    }
}

/// PROPOSAL §12 key-safety guidance, collapsible.
struct KeySafetyGuide: View {
    @State private var open = false

    var body: some View {
        DisclosureGroup(isExpanded: $open) {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(Self.items, id: \.title) { item in
                    guideItem(item.n, item.title, item.body)
                }
            }
            .padding(.top, 8)
        } label: {
            Label("安全建議（1 分鐘）", systemImage: "shield")
        }
        .padding(14)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 14))
    }

    struct Item {
        var n: Int
        var title: String
        var body: String
    }

    /// PROPOSAL §12 (v0.4, docs/changes/0006): the Default Workspace cannot
    /// have a spend limit, so step 1 is a dedicated non-default workspace.
    /// The Console locations for revoking a key and turning off auto-reload
    /// are not confirmed yet (§16 item 4), so the text names no menu path
    /// for them.
    static let items: [Item] = [
        Item(n: 1, title: "開 keepsay 專用的 workspace",
             body: "不要用 Default Workspace：它不能設花費上限。專用 workspace 也方便追蹤與撤銷。"),
        Item(n: 2, title: "不綁付款方式、關閉自動儲值",
             body: "預付少量額度即可。外洩時的損失上限，靠「沒有付款路徑」保證。"),
        Item(n: 3, title: "設定花費上限",
             body: "在 Console 的 Billing → Spend limits 為這個 workspace 設月上限（不能高於方案的每月上限）。"),
        Item(n: 4, title: "如何撤銷",
             body: "在 Console 停用或刪除這把金鑰，立即失效。keepsay 刪除金鑰只會清掉這支手機上的副本。"),
    ]

    private func guideItem(_ n: Int, _ title: String, _ body: String) -> some View {
        Text("\(n). \(Text(title).bold())：\(body)")
            .font(.subheadline)
            .foregroundStyle(.secondary)
            .fixedSize(horizontal: false, vertical: true)
    }
}

struct KeyOnboardingView: View {
    @Environment(AppModel.self) private var model
    @State private var keyText = ""

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                OnboardingDots(current: 0)
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text("連接 Claude").font(.largeTitle.bold())
                    Text("選用").font(.caption.bold()).foregroundStyle(.secondary)
                        .padding(.horizontal, 8).padding(.vertical, 3)
                        .background(Color(.tertiarySystemFill), in: RoundedRectangle(cornerRadius: 6))
                }
                .padding(.bottom, 8)
                Text("語音一律在手機上轉寫。要潤稿時，才用你自己的 API 金鑰送出文字。")
                    .foregroundStyle(.secondary)
                    .padding(.bottom, 24)

                Text("Claude API 金鑰").font(.footnote).foregroundStyle(.secondary).padding(.bottom, 6)
                KeyEntryField(text: $keyText, onSubmit: save)
                Text(model.keyError ?? "存進 iOS 鑰匙圈。之後只會顯示末四碼。")
                    .font(.footnote)
                    .foregroundStyle(model.keyError == nil ? Color.secondary : Color.red)
                    .padding(.top, 8)

                KeySafetyGuide().padding(.top, 18)
            }
            .padding(.horizontal, 28)
            .padding(.top, 20)
        }
        .scrollDismissesKeyboard(.interactively)
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: 4) {
                Button(action: save) {
                    if model.keyBusy { ProgressView() } else { Text("儲存金鑰").frame(maxWidth: .infinity) }
                }
                .buttonStyle(PrimaryButtonStyle())
                .disabled(!KeyInput.isPlausible(keyText) || model.keyBusy)

                Button("略過，只用原文模式") { keyText = ""; model.skipKey() }
                    .frame(minHeight: 44)
                Text("不需金鑰・可離線・之後可在設定加入")
                    .font(.caption).foregroundStyle(.tertiary)
            }
            .padding(.horizontal, 28)
            .padding(.bottom, 12)
            .background(.bar)
        }
    }

    private func save() {
        guard KeyInput.isPlausible(keyText), !model.keyBusy else { return }
        let key = keyText
        keyText = ""  // clear the field immediately (US-M4a-01 ①)
        Task {
            if await model.submitKey(key) { model.onboarding = .model }
        }
    }
}

struct ModelOnboardingView: View {
    @Environment(AppModel.self) private var model

    private var recommended: SttModelInfo {
        ModelCatalog.recommended(physicalMemoryBytes: ProcessInfo.processInfo.physicalMemory)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            OnboardingDots(current: 1)
            Text("下載語音模型").font(.largeTitle.bold()).padding(.bottom, 8)
            Text("轉寫在手機上完成，需要先下載一次模型。")
                .foregroundStyle(.secondary).padding(.bottom, 24)
            ModelCard(info: recommended)
            Text("想換其他大小，之後可在「設定 → 語音模型」更改。")
                .font(.footnote).foregroundStyle(.secondary).padding(.top, 12)
            Spacer()
            if model.installedModels.contains(recommended.id) {
                Button("開始使用") { model.finishOnboarding() }
                    .buttonStyle(PrimaryButtonStyle())
            } else {
                Button("下載（\(recommended.sizeLabel)）") { model.download(recommended) }
                    .buttonStyle(PrimaryButtonStyle())
                    .disabled(model.downloadProgress != nil)
                Button("稍後再說") { model.finishOnboarding() }
                    .frame(maxWidth: .infinity, minHeight: 44)
            }
        }
        .padding(.horizontal, 28)
        .padding(.vertical, 20)
    }
}

struct ModelCard: View {
    @Environment(AppModel.self) private var model
    let info: SttModelInfo

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(alignment: .firstTextBaseline) {
                Text(info.name).font(.headline)
                Spacer()
                Text(info.sizeLabel).foregroundStyle(.secondary)
            }
            Text(info.summary).font(.subheadline).foregroundStyle(.secondary)
            if let p = model.downloadProgress {
                ProgressView(value: p).padding(.top, 10)
                Text(p < 1 ? "下載中 \(Int(p * 100))%" : "驗證 SHA-256…")
                    .font(.footnote).foregroundStyle(.secondary)
            } else if let err = model.downloadError {
                Text(err).font(.footnote).foregroundStyle(.red).padding(.top, 8)
            } else if model.installedModels.contains(info.id) {
                Text("SHA-256 相符，已完成。").font(.footnote).foregroundStyle(.secondary).padding(.top, 8)
            } else {
                Text("下載完成後會核對 SHA-256，不符就刪除。")
                    .font(.footnote).foregroundStyle(.secondary).padding(.top, 8)
            }
        }
        .padding(18)
        .background(Color(.secondarySystemGroupedBackground), in: RoundedRectangle(cornerRadius: 16))
    }
}

/// mobile.html `.dots`: two onboarding steps.
struct OnboardingDots: View {
    let current: Int
    var count = 2

    var body: some View {
        HStack(spacing: 6) {
            ForEach(0..<count, id: \.self) { i in
                Capsule()
                    .fill(i == current ? Color.accentColor : Color(.separator))
                    .frame(width: i == current ? 18 : 6, height: 6)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.bottom, 24)
        .accessibilityElement()
        .accessibilityLabel("第 \(current + 1) 步，共 \(count) 步")
    }
}

struct PrimaryButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .frame(maxWidth: .infinity, minHeight: 52)
            .foregroundStyle(.white)
            .background(Color.accentColor, in: RoundedRectangle(cornerRadius: 14))
            .opacity(isEnabled ? (configuration.isPressed ? 0.8 : 1) : 0.4)
    }
}

struct SoftButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.headline)
            .frame(maxWidth: .infinity, minHeight: 52)
            .foregroundStyle(Color.accentColor)
            .background(Color.accentColor.opacity(0.12), in: RoundedRectangle(cornerRadius: 14))
            .opacity(configuration.isPressed ? 0.7 : 1)
    }
}
