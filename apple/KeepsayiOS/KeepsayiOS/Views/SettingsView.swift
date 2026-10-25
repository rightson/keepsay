// mobile.html settings. Key section shows the fingerprint only (INV-2):
// no reveal, no copy.

import SwiftUI

struct SettingsView: View {
    @Environment(AppModel.self) private var model
    @State private var showKeySheet = false
    @State private var confirmDelete = false
    @State private var newTerm = ""

    var body: some View {
        @Bindable var model = model
        Form {
            if !model.coreLinked {
                Section {
                    Label("此開發版未連結 keepsay-core：只能用原文模式，無法儲存金鑰、下載模型或轉寫。",
                          systemImage: "wrench.and.screwdriver")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
            }
            Section {
                HStack {
                    Text("Claude 金鑰")
                    Spacer()
                    if let fp = model.fingerprint {
                        Text(fp).font(.system(.subheadline, design: .monospaced))
                            .padding(.horizontal, 8).padding(.vertical, 3)
                            .background(Color(.tertiarySystemFill), in: RoundedRectangle(cornerRadius: 6))
                    } else {
                        Text("未設定").foregroundStyle(.secondary)
                    }
                }
                Button(model.hasKey ? "更換金鑰" : "加入金鑰") { showKeySheet = true }
                if model.hasKey {
                    Button("刪除金鑰", role: .destructive) { confirmDelete = true }
                }
            } header: {
                Text("Claude 金鑰")
            } footer: {
                Text("完整金鑰只存在鑰匙圈，不會再顯示或複製出來。")
            }

            Section("預設模式") {
                ModePicker(selection: $model.settings.defaultMode, isAvailable: { _ in true })
                    .frame(maxWidth: .infinity)
                    .listRowBackground(Color.clear)
            }

            Section {
                Picker("目標語言", selection: $model.settings.targetLanguage) {
                    ForEach(TargetLanguage.allCases) { Text($0.label).tag($0) }
                }
            } header: {
                Text("潤飾並翻譯")
            } footer: {
                Text("先潤飾再翻譯，在同一次請求內完成。沒有金鑰或離線時輸出原文。")
            }

            Section {
                ForEach(model.settings.glossary, id: \.self) { term in
                    Text(term)
                }
                .onDelete { model.settings.glossary.remove(atOffsets: $0) }
                HStack {
                    TextField("新增詞彙", text: $newTerm)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .onSubmit(addTerm)
                    Button("加入", action: addTerm)
                        .disabled(newTerm.trimmingCharacters(in: .whitespaces).isEmpty)
                }
            } header: {
                Text("詞彙表")
            } footer: {
                Text("人名、產品名、專有名詞。轉寫與潤稿時會照這裡的寫法。")
            }

            BudgetSection(settings: $model.settings, budget: model.budget)

            Section {
                ForEach(ModelCatalog.all) { info in
                    ModelRow(info: info, selected: model.settings.sttModel == info.id)
                }
                if let err = model.downloadError {
                    Text(err).font(.footnote).foregroundStyle(.red)
                }
            } header: {
                Text("語音模型")
            } footer: {
                Text("只在你按下時下載。完成後比對 SHA-256，不符就刪除。")
            }

            Section {
                VStack(spacing: 6) {
                    Text("無遙測・不保存歷史・音訊不落地").font(.subheadline.weight(.semibold))
                    Text("唯一的連線：api.anthropic.com（潤稿時）與你手動按下的模型下載。")
                        .font(.caption).foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                    Text("keepsay \(Self.version)・開放原始碼").font(.caption).foregroundStyle(.tertiary)
                }
                .frame(maxWidth: .infinity)
            }
        }
        .navigationTitle("設定")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $showKeySheet) { ReplaceKeySheet() }
        .confirmationDialog("刪除金鑰？", isPresented: $confirmDelete, titleVisibility: .visible) {
            Button("刪除", role: .destructive) { Task { await model.deleteKey() } }
        } message: {
            Text("刪除後只能用原文模式。Console 上的金鑰仍有效，要停用請到 Console 撤銷。")
        }
    }

    private func addTerm() {
        model.settings.glossary = Glossary.adding(newTerm, to: model.settings.glossary)
        newTerm = ""
    }

    static var version: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "0.1.0"
    }
}

struct BudgetSection: View {
    @Binding var settings: AppSettings
    let budget: BudgetSnapshot

    var body: some View {
        Section {
            VStack(alignment: .leading, spacing: 4) {
                HStack {
                    Text("本月用量")
                    Spacer()
                    Text("\(Usd.format(budget.estimatedUSD)) ／ \(settings.hardLimitUSD.map { Usd.format($0) } ?? "無上限")")
                        .foregroundStyle(.secondary).monospacedDigit()
                }
                ProgressView(value: budget.fraction).tint(budget.softLimited ? .orange : .accentColor)
                Text("每月 1 日歸零・只在本機計算").font(.caption).foregroundStyle(.tertiary)
            }
            UsdField(title: "軟上限", subtitle: "到了提醒一次", value: $settings.softLimitUSD)
            UsdField(title: "硬上限", subtitle: "到了停用潤稿，只出原文", value: $settings.hardLimitUSD)
        } header: {
            Text("預算")
        }
        Section {
            UsdField(title: "輸入", subtitle: nil, value: optional($settings.inputPricePerMTok))
            UsdField(title: "輸出", subtitle: nil, value: optional($settings.outputPricePerMTok))
        } header: {
            HStack(spacing: 6) {
                Text("價格表（每百萬 token）")
                Text("請核對官方定價").font(.caption2).foregroundStyle(Color.accentColor)
            }
        } footer: {
            Text("用量是依這張表估算的。官方調價時請自行更新。")
        }
    }

    private func optional(_ b: Binding<Double>) -> Binding<Double?> {
        Binding(get: { b.wrappedValue }, set: { b.wrappedValue = max(0, $0 ?? 0) })
    }
}

struct UsdField: View {
    let title: String
    let subtitle: String?
    @Binding var value: Double?

    var body: some View {
        HStack {
            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                if let subtitle { Text(subtitle).font(.caption).foregroundStyle(.tertiary) }
            }
            Spacer()
            Text("US$").foregroundStyle(.secondary)
            TextField("—", value: $value, format: .number.precision(.fractionLength(0...2)))
                .keyboardType(.decimalPad)
                .multilineTextAlignment(.trailing)
                .frame(width: 76)
                .monospacedDigit()
        }
    }
}

struct ModelRow: View {
    @Environment(AppModel.self) private var model
    let info: SttModelInfo
    let selected: Bool

    var body: some View {
        let installed = model.installedModels.contains(info.id)
        HStack(spacing: 10) {
            Image(systemName: selected ? "largecircle.fill.circle" : "circle")
                .foregroundStyle(selected ? Color.accentColor : Color.secondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(info.name)
                Text("\(info.sizeLabel)・\(info.summary)").font(.caption).foregroundStyle(.tertiary)
            }
            Spacer()
            if installed {
                Text("已下載").font(.footnote).foregroundStyle(Color.accentColor)
            } else if let p = model.downloadProgress, downloadingThis {
                ProgressView(value: p).frame(width: 60)
            } else {
                Button("下載") { downloading = info.id; model.download(info) }
                    .buttonStyle(.bordered)
                    .disabled(model.downloadProgress != nil)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture { if installed { model.settings.sttModel = info.id } }
    }

    @State private var downloading: String?
    private var downloadingThis: Bool { downloading == info.id }
}

struct ReplaceKeySheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.dismiss) private var dismiss
    @State private var keyText = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(model.hasKey ? "更換金鑰" : "加入金鑰").font(.title2.bold())
            Text("新金鑰會取代舊的。儲存後只顯示末四碼。驗證失敗時不會儲存。")
                .font(.subheadline).foregroundStyle(.secondary)
            KeyEntryField(text: $keyText, onSubmit: save)
            if let err = model.keyError {
                Text(err).font(.footnote).foregroundStyle(.red)
            }
            KeySafetyGuide()
            Spacer()
            Button(action: save) {
                if model.keyBusy { ProgressView() } else { Text("儲存") }
            }
            .buttonStyle(PrimaryButtonStyle())
            .disabled(!KeyInput.isPlausible(keyText) || model.keyBusy)
            Button("取消") { keyText = ""; dismiss() }
                .frame(maxWidth: .infinity, minHeight: 44)
        }
        .padding(24)
        .presentationDetents([.large])
        .onDisappear { keyText = "" }
    }

    private func save() {
        guard KeyInput.isPlausible(keyText), !model.keyBusy else { return }
        let key = keyText
        keyText = ""
        Task {
            if await model.submitKey(key) {
                model.showToast("已儲存・只顯示末四碼")
                dismiss()
            }
        }
    }
}
