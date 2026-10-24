# 0015 — macOS menu bar app（M2）：專案結構、快捷鍵、插入、打包

- Status：Accepted（V2、V9 實測與手動清單待實機）
- Date：2026-10-24
- 影響：`apple/KeepsayKit/`、`apple/KeepsayMac/`、`apple/scripts/`、`.github/workflows/macos-app.yml`、ROADMAP M2。PROPOSAL §5.2 的 `Keepsay.xcodeproj` 改為 SwiftPM（見下）；PROPOSAL 本文由整合者同步。

## Context

M2 要在沒有 Xcode 的 Linux 開發環境中完成 macOS App，唯一的編譯器是 GitHub Actions 的 macOS runner。KeepsayKit 要同時給 iOS（M4a）用。core 的 UniFFI 匯出（US-M1-22）與本工作同時進行。

## Decision

### 1. 專案：SwiftPM，不用手寫 `project.pbxproj`

- `apple/KeepsayKit/`（library，macOS 14+ / iOS 17+）與 `apple/KeepsayMac/`（`KeepsayMacSupport` library、`KeepsayMac` executable、XCTest）都是 SwiftPM package。
- `.app` 由 `apple/scripts/build-mac-app.sh` 組裝：`swift build -c release` → `Keepsay.app/Contents/{MacOS,Info.plist}` → `codesign --options runtime`（Hardened Runtime）→ zip。
- 理由：手寫 pbxproj 無法在本機驗證，任何一個 UUID 或 build phase 錯誤都要靠 CI 來回試；Package.swift 是可讀、可 review 的 Swift 程式碼，diff 也清楚。Xcode 可以直接開 Package.swift 開發。缺點：沒有 asset catalog 與 Xcode 管理的簽章，簽章由腳本負責（M5 再接 Developer ID）。
- iOS App（M4a）需要的 `.xcodeproj` 或 App target 由 iOS 工作流決定；它只依賴 `KeepsayKit` product。

### 2. 連結 core：XCFramework + 條件式 binary target

- `apple/scripts/build-core.sh`：`cargo rustc -p keepsay-core --crate-type staticlib` 建出 `aarch64-apple-darwin`、`x86_64-apple-darwin`、`aarch64-apple-ios`、`aarch64-apple-ios-sim`、`x86_64-apple-ios` 的 static lib，依平台 `lipo` 合併，`xcodebuild -create-xcframework` 打包成 `KeepsayCoreFFI.xcframework`。用 `cargo rustc --crate-type`，core/Cargo.toml 的 `crate-type` 維持不變（0012：避免每次 `cargo test` 都多連結 staticlib）。
- Swift binding 用 `uniffi-bindgen-swift` 以 library mode 從 host 的 cdylib 產生一次（module `keepsay_coreFFI`）。所有 slice 必須使用相同 features（UniFFI 在載入時比對 checksum）；預設 `whisper`。腳本拒絕 `test-endpoint` / `test-support`。
- 產物（gitignore）複製到 `KeepsayKit/Frameworks/` 與 `KeepsayKit/Sources/KeepsayCoreBindings/`。`Package.swift` 只有在兩者都存在時才加入 `KeepsayCoreFFI` binary target 與 `KeepsayCoreBindings` target；否則 `CoreBridge.make()` 回傳 `UnavailableCore`（無 key、無網路、polish 一律 raw、STT 不可用）。這讓平台程式碼在 FFI 完成前就能建置與測試。
- `uniffi-bindgen-swift` 是建置工具（`cargo install uniffi@<Cargo.lock 版本> --features cli --bin uniffi-bindgen-swift`），不進 workspace 的依賴圖、不改 Cargo.lock。**待人工批准（0014 D-12）**：它需要 uniffi 的 `cli` feature（會帶入 uniffi_bindgen、clap 等）。在批准前：`build-core.sh` 把 bindgen 隔離成單一步驟（步驟 2，註明待批准），`KEEPSAY_SKIP_BINDGEN=1` 時只建 static lib 與 lipo；CI 的 `core-staticlibs` job 以此模式驗證五個 target 都能交叉編譯，並以 `nm` 確認含 UniFFI 匯出符號，但不產生 binding 與 XCFramework，也不安裝 uniffi CLI。替代方案：在 workspace 加一個不發佈的 `uniffi-bindgen` crate（需改 Cargo.lock 與 vet），或把產生的 binding 提交進 repo。
- `crate-type`：不需要改 core/Cargo.toml。`cargo rustc --crate-type staticlib|cdylib` 只影響這次建置。
- `UniffiCore.swift`（只在 binding 存在時編譯）依 UniFFI Swift 命名慣例手寫對應；FFI 函式都是阻塞呼叫（0014），一律在 `Task.detached` 執行，不在 main thread。第一次真的產生 binding 時需要再編譯確認名稱。

### 3. 快捷鍵（落實 0004）

- `KeepsayMacSupport/FnKeyTap.swift` 是唯一的全域按鍵 hook：active `CGEventTap`（`.cgSessionEventTap`、`.defaultTap`），只訂閱 keyDown / keyUp；回呼只讀 keycode、`maskSecondaryFn`、`maskShift`。fn+Z keyDown 吞掉並觸發；fn+Z keyUp 吞掉不觸發；其他事件原封不動。唯一 import 是 CoreGraphics。純函式 `FnKeyTapRules.decide` 有單元測試。
- 自動檢查（THREAT_MODEL T7 待補項）：`apple/scripts/check-fn-tap.sh`，在 `macos-app.yml` 的 `fn-tap-scan` job 執行。檢查：`apple/` 內只有這個檔案出現 `tapCreate` / `addGlobalMonitorForEvents` 等 hook；import 只有 CoreGraphics；不出現檔案、網路、剪貼簿、logging、UserDefaults、Keychain、IPC 等 API；沒有集合或累加（append、`+=`、`[CGEvent]`…）；只讀 `.keyboardEventKeycode` 與兩個 flag、不修改 flags；event mask 只有 keyDown / keyUp。
- 自動重複：回呼不讀 autorepeat 欄位（維持「只讀 keycode 與 fn / Shift」）。改由 `RepeatSuppressor` 在 tap 之外判斷：同一快捷鍵距離上一個事件不到 0.25 秒就不觸發，所以按住只觸發一次。
- Z 的 keycode 在安裝 tap 時以 `UCKeyTranslate` 依目前鍵盤配置換算（AZERTY），不在回呼內做。
- Carbon `RegisterEventHotKey`：⌥Space / ⌥⇧Space。在 tap 建立失敗（沒有輔助使用權限）時自動註冊，或使用者開啟「同時啟用備援組合」時註冊。註冊失敗時在設定頁提示改鍵；設定頁可錄製新組合（本機 `NSEvent` local monitor，只看得到送往自己視窗的按鍵；必須含 ⌘⌥⌃ 其一）。
- Esc：只在錄音中以 Carbon 註冊（無修飾鍵），結束錄音即取消註冊。
- toggle 語意與取消由 `DictationStateMachine`（KeepsayKit，純邏輯，有測試）決定：開始時的組合決定是否翻譯；處理中忽略快捷鍵；錄音中 Esc 取消並丟棄音訊。

### 4. 插入

- AX：`kAXFocusedUIElementAttribute` → 確認 `kAXSelectedTextAttribute` 可寫 → 設定。
- 失敗時走剪貼簿（`PasteboardTransaction`）：備份所有 item 的所有 type → 寫入文字加 `org.nspasteboard.ConcealedType`、`org.nspasteboard.TransientType` → `CGEvent` Cmd+V（V 依目前配置換算 keycode）→ 300 ms 後還原。若期間 changeCount 被別人改過，保留較新的內容、不還原。
- 剪貼簿路徑上，若目前輸入來源是 CJK 輸入法（或非 ASCII-capable），先切到 `TISCopyCurrentASCIICapableKeyboardLayoutInputSource`，貼上並還原剪貼簿後切回（US-M2-12）。判斷規則 `InputSourceRules.needsSwitch` 有測試。
- 已知限制：部分 App（Electron、部分瀏覽器欄位）對 AX 寫入回報成功但沒有效果，需手動清單確認，必要時加 per-app 強制剪貼簿清單。

### 5. Key UI（INV-2）

- `SecureField`，送出時 `AppModel.submitKey` 先把欄位清空，再把值交給 `store_key` 一次；之後只顯示 fingerprint。沒有顯示或複製 key 的功能。
- 不設 `textContentType(.password)`：那會讓 macOS 提議把 key 存進 iCloud 鑰匙圈密碼（INV-1）。
- 金鑰頁顯示 §12 四步驟。

### 6. 打包、entitlements、V2

- Hardened Runtime 開啟。`KeepsayMac.entitlements`（正式）：`com.apple.security.device.audio-input`、`keychain-access-groups = <TeamID>.dev.rightson.keepsay`（0008 的 data protection keychain 需要，屬受限 entitlement，要 provisioning profile，§16 第 1 項）。`KeepsayMac.adhoc.entitlements`（CI）只有 audio-input，所以 CI 產出的 App `store_key` 會得到 `Unsupported`（0008 預期行為）。
- App Sandbox：暫不開啟。V2（sandbox 下 AX 寫入其他 App、CGEvent 貼上、active event tap 是否可用）只能在實機測，CI 無法驗證，結論為「待實機驗證」。不開 sandbox 時網路出口完全由程式碼層（INV-3、INV-7）限制。若 V2 結果為不可用，SECURITY.md 需加一段說明（SECURITY.md 不在本工作流範圍，交給整合者）。
- bundle id `dev.rightson.keepsay.mac`；`LSUIElement`（沒有 Dock 圖示）；最低 macOS 14（`MenuBarExtra`、`SettingsLink`）。
- CI artifact：`Keepsay-macOS-adhoc-nocore`（未連結 core）。D-12 批准後再加連結 core 的 App 與 `KeepsayCoreFFI-xcframework`（給 iOS 工作流）。Notarization 待 §16 第 1 項。

### 7. 設定

- core 的 `config.json` 是唯一來源。Swift 的 `AppSettings` 只是鏡像，經 `KeepsayCore.loadSettings` / `saveSettings` 讀寫，Swift 不另寫設定檔。`UnavailableCore` 只存在記憶體。

## Consequences

- 0014 的 FFI 已涵蓋 key、polish、transcribe、budget、download（進度）、import。**仍需要 core 補上**（App 無法自己做，Swift 也不另寫設定檔）：
  1. 讀寫 config：例如 `get_config() -> Config record`、`set_config(Config)`（mode、translate_to、stt_model、glossary、prices、budget、hotkeys），走 `Config::save` 的驗證。沒有它，設定頁的變更只在記憶體，`transcribe` 也無法切換 `stt_model`。
  2. 模型清單與安裝狀態：例如 `models() -> Vec<ModelStatus { id, name, size, installed }>`（`installed` = 檔案存在；完整 SHA-256 驗證仍在載入時）。
  3. （可選）`Unsupported` / `VaultNotEntitled` 在 Swift 端目前都顯示為「未簽章的開發版」，若要分開顯示，`CoreFailure` 再加一個 case 即可，不需改 core。
- 待實機（US-M2-02、03、10、11、12、16）：V9 四項（🌐 鍵行為、AZERTY、Secure Input、sandbox 交互）、V2、AX 插入相容性、注音 / 倉頡 / 日文輸入法的切換、無 fn 鍵外接鍵盤、斷網與 key 失效提示。
- 推按住說話（push-to-talk）到 v1.1，設定頁只顯示為停用選項。
