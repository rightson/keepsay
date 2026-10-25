// Settings window, tabs per docs/mockups/desktop.html:
// 一般, 金鑰, 詞彙表, 預算, 模型, 隱私.

import AppKit
import KeepsayKit
import KeepsayMacSupport
import SwiftUI

struct SettingsView: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        TabView {
            GeneralPane().tabItem { Label("一般", systemImage: "gearshape") }
            KeyPane().tabItem { Label("金鑰", systemImage: "key") }
            GlossaryPane().tabItem { Label("詞彙表", systemImage: "character.book.closed") }
            BudgetPane().tabItem { Label("預算", systemImage: "dollarsign.circle") }
            ModelPane().tabItem { Label("模型", systemImage: "cpu") }
            PrivacyPane().tabItem { Label("隱私", systemImage: "hand.raised") }
        }
        .frame(width: 560)
        .padding(20)
        .onAppear { model.refreshAll() }
        .alert(
            "keepsay", isPresented: Binding(get: { model.settingsError != nil }, set: { if !$0 { model.settingsError = nil } })
        ) {
            Button("好") { model.settingsError = nil }
        } message: {
            Text(model.settingsError ?? "")
        }
    }
}

// MARK: - 一般

struct GeneralPane: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Form {
            Picker("預設模式", selection: $model.settings.mode) {
                ForEach(DictationMode.allCases, id: \.self) { m in
                    Text(m.label).tag(m)
                }
            }
            .pickerStyle(.segmented)
            Text(model.settings.mode.detail).font(.caption).foregroundStyle(.secondary)
            if !model.hasKey {
                Text("未設定金鑰時只會輸出原文。").font(.caption).foregroundStyle(.secondary)
            }

            Picker("翻譯目標語言", selection: $model.settings.translateTo) {
                ForEach(TranslationTarget.allCases, id: \.self) { t in
                    Text(t.label).tag(t)
                }
            }

            LabeledContent("快捷鍵") {
                VStack(alignment: .leading, spacing: 4) {
                    Text("fn Z 潤飾　fn ⇧ Z 潤飾並翻譯")
                    Text(fnStatus).font(.caption).foregroundStyle(.secondary)
                }
            }
            Toggle("同時啟用備援組合", isOn: $model.settings.alwaysEnableFallbackHotkeys)
            LabeledContent("備援：潤飾") {
                ComboRecorder(combo: $model.settings.fallbackPolish)
            }
            LabeledContent("備援：潤飾並翻譯") {
                ComboRecorder(combo: $model.settings.fallbackTranslate)
            }
            if model.hotkeyRegistrationFailed {
                Text("備援組合已被其他 App 使用，請錄製新的快捷鍵。").font(.caption).foregroundStyle(.orange)
            }

            LabeledContent("錄音方式") {
                VStack(alignment: .leading) {
                    Text("按一下開始，再按一下停止；錄音中按 Esc 取消")
                    Text("按住說話（v1.1）").foregroundStyle(.tertiary)
                }
            }
        }
        .formStyle(.grouped)
    }

    private var fnStatus: String {
        switch model.hotkeyStatus {
        case .fnTap: "已啟用（需要「輔助使用」權限）"
        case .carbonOnly: "未啟用：尚未允許「輔助使用」或鍵盤沒有 fn 鍵，目前使用備援組合"
        case .none: "未啟用"
        }
    }
}

/// Click, then press a combination with ⌘, ⌥ or ⌃. Uses a local NSEvent
/// monitor, which sees only keys sent to this window.
struct ComboRecorder: View {
    @Binding var combo: KeyCombo
    @State private var recording = false
    @State private var monitor: Any?

    var body: some View {
        Button(recording ? "請按下新組合…（Esc 取消）" : KeyComboFormatter.string(for: combo)) {
            recording ? stop() : start()
        }
        .onDisappear { stop() }
    }

    private func start() {
        recording = true
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            if event.keyCode == 53 {  // Esc
                stop()
                return nil
            }
            let f = event.modifierFlags
            let candidate = KeyCombo(
                keyCode: UInt32(event.keyCode),
                carbonModifiers: KeyComboFormatter.carbonModifiers(
                    command: f.contains(.command), option: f.contains(.option), control: f.contains(.control),
                    shift: f.contains(.shift)))
            guard KeyComboFormatter.isAcceptable(candidate) else { return nil }
            combo = candidate
            stop()
            return nil
        }
    }

    private func stop() {
        if let m = monitor { NSEvent.removeMonitor(m) }
        monitor = nil
        recording = false
    }
}

// MARK: - 金鑰

struct KeyPane: View {
    @EnvironmentObject var model: AppModel
    @State private var keyField = ""
    @State private var editing = false
    @State private var saving = false
    @State private var confirmDelete = false

    var body: some View {
        Form {
            Section("Claude API 金鑰") {
                if model.hasKey && !editing {
                    LabeledContent("目前金鑰") {
                        Text(model.fingerprint ?? "").monospaced()
                    }
                    Text("已存於鑰匙圈。完整金鑰不會再顯示或複製出來。").font(.caption).foregroundStyle(.secondary)
                    HStack {
                        Button("更換…") { editing = true }
                        Button("刪除…", role: .destructive) { confirmDelete = true }
                    }
                } else {
                    // SecureField: hidden while typing, cannot be copied out.
                    // No textContentType(.password): that would offer to
                    // save the key to iCloud Keychain passwords (INV-1).
                    SecureField("sk-ant-…", text: $keyField)
                        .onSubmit(save)
                    HStack {
                        Button(saving ? "驗證中…" : "儲存到鑰匙圈", action: save)
                            .disabled(keyField.isEmpty || saving)
                        if model.hasKey {
                            Button("取消") {
                                keyField = ""
                                editing = false
                            }
                        }
                    }
                    Text("送出後會先線上驗證，通過才存入鑰匙圈；之後只顯示末四碼。").font(.caption).foregroundStyle(.secondary)
                }
                if !model.hasKey {
                    Text("未設定金鑰時只能使用「原文」模式，完全離線。").font(.caption).foregroundStyle(.secondary)
                }
            }
            Section("安全建議") {
                SafetyGuide()
            }
        }
        .formStyle(.grouped)
        .confirmationDialog("刪除金鑰？", isPresented: $confirmDelete) {
            Button("刪除", role: .destructive) {
                model.deleteKey()
                editing = false
            }
        } message: {
            Text("刪除後只能使用原文模式。記得也到 Anthropic Console 撤銷這把金鑰。")
        }
        .onDisappear { keyField = "" }
    }

    private func save() {
        guard !keyField.isEmpty, !saving else { return }
        saving = true
        let field = $keyField
        Task {
            // submitKey clears the field before calling core (INV-2).
            let ok = await model.submitKey(from: field)
            saving = false
            if ok { editing = false }
        }
    }
}

/// PROPOSAL §12, shown in-app.
struct SafetyGuide: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("1. 在 Anthropic Console 開一個只給 keepsay 用的 workspace 與金鑰。")
            Text("2. 該帳號或 workspace 只放額度：移除付款方式、關閉自動儲值。")
            Text("3. 在 Console 設定 workspace 花費上限。")
            Text("4. 撤銷：Console → API Keys → 停用或刪除這把金鑰。")
        }
        .font(.callout)
    }
}

// MARK: - 詞彙表

struct GlossaryPane: View {
    @EnvironmentObject var model: AppModel
    @State private var newTerm = ""
    @State private var selection: String?

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            List(model.settings.glossary, id: \.self, selection: $selection) { term in
                Text(term)
            }
            .frame(minHeight: 200)
            HStack {
                TextField("新增詞彙", text: $newTerm).onSubmit(add)
                Button("＋", action: add)
                Button("−") {
                    if let s = selection { model.settings.glossary.removeAll { $0 == s } }
                    selection = nil
                }
                .disabled(selection == nil)
            }
            Text("人名、產品名、專有名詞。轉寫與潤稿時會照這裡的寫法；翻譯時也保持原樣。")
                .font(.caption).foregroundStyle(.secondary)
        }
    }

    private func add() {
        if model.settings.addGlossaryTerm(newTerm) { newTerm = "" }
    }
}

// MARK: - 預算

struct BudgetPane: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Form {
            Section {
                LabeledContent("本月用量") {
                    VStack(alignment: .trailing) {
                        Text(String(format: "US$%.2f", model.budget.estimatedUSD)).monospacedDigit()
                        Text("輸入 \(model.budget.usage.inputTokens)・輸出 \(model.budget.usage.outputTokens) token")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }
                if model.budget.hardLimited {
                    Text("已達硬上限：潤稿已停用，只輸出原文。").foregroundStyle(.red)
                } else if model.budget.softLimited {
                    Text("已達軟上限。").foregroundStyle(.orange)
                }
                Text("每月 1 日歸零・只在本機計算").font(.caption).foregroundStyle(.secondary)
            }
            Section {
                OptionalUSDField(title: "軟上限（到了提醒一次）", value: $model.settings.softLimitUSD)
                OptionalUSDField(title: "硬上限（到了停用潤稿，只出原文）", value: $model.settings.hardLimitUSD)
            }
            Section("價格表（每百萬 token，請核對官方定價）") {
                TextField("輸入 US$", value: $model.settings.prices.inputUSDPerMTok, format: .number)
                TextField("輸出 US$", value: $model.settings.prices.outputUSDPerMTok, format: .number)
                Text("用量依這張表估算。官方調價時請自行更新。").font(.caption).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

struct OptionalUSDField: View {
    let title: String
    @Binding var value: Double?

    var body: some View {
        HStack {
            Toggle(title, isOn: Binding(get: { value != nil }, set: { value = $0 ? (value ?? 5) : nil }))
            if value != nil {
                TextField("US$", value: Binding(get: { value ?? 0 }, set: { value = max(0, $0) }), format: .number)
                    .frame(width: 80)
            }
        }
    }
}

// MARK: - 模型

struct ModelPane: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Form {
            Section("語音模型") {
                ForEach(model.models) { m in
                    HStack {
                        VStack(alignment: .leading) {
                            Text(m.name)
                            Text(ByteCountFormatter.string(fromByteCount: Int64(m.sizeBytes), countStyle: .file))
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if let p = model.downloadProgress[m.id] {
                            ProgressView(value: p).frame(width: 120)
                        } else if m.installed {
                            if model.settings.sttModel == m.id {
                                Label("使用中", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
                            } else {
                                Button("使用") { model.settings.sttModel = m.id }
                            }
                        } else {
                            Button("下載") { model.download(m.id) }
                            Button("匯入…") { model.importModel(m.id) }
                        }
                    }
                }
            }
            Text("只在你按下時下載，完成後比對 SHA-256，不符就刪除。手動匯入的檔案同樣會比對。位置：~/Library/Application Support/keepsay")
                .font(.caption).foregroundStyle(.secondary)
        }
        .formStyle(.grouped)
    }
}

// MARK: - 隱私

struct PrivacyPane: View {
    @EnvironmentObject var model: AppModel

    var body: some View {
        Form {
            Section("權限") {
                LabeledContent("麥克風") {
                    permission(model.microphoneStatus == .authorized, url: Permissions.microphoneSettingsURL)
                }
                LabeledContent("輔助使用") {
                    permission(model.accessibilityTrusted, url: Permissions.accessibilitySettingsURL)
                }
                Text("「輔助使用」用來把文字插入游標位置，以及偵測 fn Z。keepsay 只辨識 fn Z 與 fn ⇧ Z，不保存、不記錄任何其他按鍵。")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("無遙測・不保存歷史・音訊不落地") {
                Text("唯一的連線：api.anthropic.com（潤稿時），以及你手動按下的模型下載。沒有帳號、同步或自動更新。")
                if !CoreBridge.isCoreLinked {
                    Text("此開發版未連結 keepsay-core：不會連網，也無法辨識語音。").foregroundStyle(.orange)
                }
            }
        }
        .formStyle(.grouped)
    }

    @ViewBuilder
    private func permission(_ granted: Bool, url: URL) -> some View {
        if granted {
            Label("已允許", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
        } else {
            HStack {
                Text("未允許").foregroundStyle(.orange)
                Button("開啟系統設定…") { NSWorkspace.shared.open(url) }
            }
        }
    }
}
