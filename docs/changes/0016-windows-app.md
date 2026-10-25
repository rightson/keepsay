# 0016 — Windows Tauri 2 app（M3）：結構、Win32 FFI、插入、加固

- Status：Proposed（程式碼由 CI 編譯與測試；cargo-vet exemption 待人工批准；手動清單與簽章待實機與 §16-2）
- Date：2026-10-25
- 影響：`windows/`、`.github/workflows/windows-app.yml`（新增）、`.github/workflows/ci.yml`（unsafe-scope）、`deny.toml`、`supply-chain/imports.lock`、ROADMAP M3、PROPOSAL §7.3 與 §15 V5。

## Context

M3 要在沒有 Windows 的 Linux 開發環境完成 Tauri 2 tray app。唯一的真編譯器是 GitHub Actions 的 Windows runner。0018 批准了 `windows/src-tauri/src/win32/` 目錄使用 `unsafe` 與 `windows` crate 的七個 features，讓 V5 原生 key 對話框、SendInput 與剪貼簿備援可以實作。

## Decision

### 1. 結構

- `windows/src-tauri` 是 workspace 成員 `keepsay-windows`（bin `keepsay`）。App 只在 Windows 編譯；其他平台是 stub（`main` 印一行就結束），讓 Linux CI 不需要 WebKitGTK，同時純邏輯模組（`hotkey`、`session`、`audio`、`insert` 的順序、`ipc` 型別、`win32::plan`）在每個 OS 上測試。
- 模組：`app`（狀態、啟動、視窗、快捷鍵註冊）、`commands`（IPC）、`ipc`（跨 IPC 的型別）、`pipeline`（快捷鍵 → 錄音 → STT → 潤稿 → 插入）、`recorder`（cpal）、`audio`、`hotkey`、`session`、`insert`、`tray`、`win32/`。UI 在 `windows/ui/`（設定頁 `index.html` + `settings.js`，錄音浮條 `overlay.html` + `overlay.js`），只用 `textContent` 寫入 DOM。
- `tauri-build` 是無條件的 build-dependency：build script 看到的是 host 的 cfg，條件式依賴會讓 Linux host 無法對 Windows target 做 `cargo check`。`build.rs` 只在 `CARGO_CFG_WINDOWS` 時呼叫它。開發時可以在 Linux 上 `cargo clippy --target x86_64-pc-windows-msvc --no-default-features`（需要 clang-cl 與少量 C header stub 給 ring 的 build script；只是開發便利，CI 以 Windows runner 為準）。
- 資料目錄 `%LOCALAPPDATA%\keepsay`（`config.json`、`budget.json`、`models\<id>.bin`），不放 Roaming：詞彙表與預算不隨漫遊設定檔同步。

### 2. 加固與 capability 稽核（US-M3-02）

| 項目 | 設定 | 由誰保證 |
|------|------|----------|
| 載入的內容 | 只有內嵌的 `ui/`（`custom-protocol`，無 `devUrl`，視窗由 Rust 以 `WebviewUrl::App` 建立） | `tests/ipc_surface.rs` |
| CSP | `default-src 'self'; connect-src ipc: http://ipc.localhost`，`freezePrototype` | 同上 |
| devtools | `cfg!(debug_assertions)`；tauri 不開 `devtools` feature | 同上 |
| plugin | 只有 global-shortcut，且只從 Rust 使用；它的 JS command 沒有任何 capability 授權。shell / fs / http / updater 由 `deny.toml` 禁止，dialog 由測試禁止 | `deny.toml`、測試 |
| tauri features | `wry`、`tray-icon`、`custom-protocol`、`common-controls-v6`；不開 compression、dynamic-acl、devtools、isolation、protocol-asset、tauri 自己的 HTTP client | 測試 |
| `settings` capability | keepsay 自己的 12 個 command，沒有 `core:*` | 測試 |
| `overlay` capability | 只有 `allow-overlay-state` | 測試 |
| IPC 回傳型別 | 每個 command 的回傳型別釘死在測試的清單；新增 command 時測試失敗直到更新清單（US-M3-11） | 測試 |
| key | `KeyView` 只有 `has_key` 與 fingerprint；由 `vault::stored_fingerprint` 產生（0014）；commands 不出現 `SecretString`、`expose_secret`、vault 的 `load` | 測試、`core/tests/no_key_load.rs` |
| 錯誤 | `IpcError { code }`，固定字串，不帶使用者內容、路徑或上游訊息 | code review |

- `open_mic_settings` 以 `explorer.exe ms-settings:privacy-microphone` 開系統設定，本機 URI，不連網。
- WebView2 runtime 不由安裝檔下載（`webviewInstallMode: skip`，Windows 10 / 11 內建）。

### 3. 快捷鍵（落實 0004 / V8 (c)）

- tauri-plugin-global-shortcut（global-hotkey，`RegisterHotKey` + `MOD_NOREPEAT`），沒有鍵盤 hook、沒有 Raw Input。預設 Alt+Shift+Z / Alt+Shift+X；Esc 只在錄音中註冊。
- 使用者自訂規則：恰好一個 Alt 或 Ctrl（拒絕 Ctrl+Alt，因為 AltGr = Ctrl+Alt）、Shift 選用、不收 Win、主鍵為 A–Z、0–9、F1–F24、Space；Esc 保留；兩個動作不得相同。註冊失敗時設定頁顯示橫幅請使用者改鍵。

### 4. 錄音（US-M3-05）

- cpal（WASAPI）預設輸入裝置，串流在獨立執行緒，下混成單聲道存在記憶體，上限 5 分鐘；停止後以 core 的 `stt::resample_linear` 轉 16 kHz。原始與轉換後的緩衝區用完都 zeroize。沒有任何檔案 I/O。

### 5. V5：原生 key 對話框（US-M3-01、M3-03）

- 採自製對話框，以 `CreateWindowExW` 建立、在呼叫端的阻塞執行緒跑自己的訊息迴圈（`IsDialogMessageW` 處理 Tab / Enter / Esc）。不用 `DialogBoxIndirectParamW`：記憶體內 DLGTEMPLATE 要手動排 WORD 對齊，難以 review；行為上兩者相同。不用 CredUI：無法隱藏使用者名稱欄。
- 單一 `ES_PASSWORD | ES_AUTOHSCROLL` edit，上限 512 字（core 的上限），OK 在 8 字以上才啟用（只讀長度）。`SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`，舊版 Windows 退回 `WDA_MONITOR`。`WS_EX_TOPMOST`，置中於工作區；同時只允許一個對話框（第二次按會得到 `busy`）。
- 按確定：`GetWindowTextW` 讀進 `Zeroizing<Vec<u16>>` → 就地去掉前後 ASCII 空白 → 先算精確 UTF-8 長度、以該容量建 `String`（`SecretString::from` 不重新配置，不會在釋放的記憶體留下副本）→ 以同長度字元覆寫 edit 再清空 → 關閉。之後 `enter_key` 呼叫 core 的 `vault::store_key`（格式檢查 → `GET /v1/models` 驗證 → 存入 Credential Manager），任何一步失敗都不動已存的 key。IPC 只回 `KeyView`。
- 殘餘風險：edit control 內部緩衝區可能在覆寫前已被重新配置；同權限程式仍可注入或掛鉤（T2 根本限制）。
- 文字以 DPI 縮放（`GetDeviceCaps(LOGPIXELSY)`）並使用系統訊息字型。Tauri 是 per-monitor DPI aware，跨不同 DPI 的螢幕時尺寸可能不完全正確（待實機）。

### 6. 插入（US-M3-07、M3-08、M3-09）

- 短文（≤ 600 UTF-16 單位）用 `SendInput` + `KEYEVENTF_UNICODE`（VK_PACKET），每批 32 字、批間 4 ms；換行送 VK_RETURN。送字前等使用者放開 Alt / Shift / Ctrl / Win（最多 1.5 秒），不替使用者送 key-up。事件緩衝區用完覆寫。
- 長文或 SendInput 失敗：剪貼簿。
  1. 以 message-only window 當 owner 開剪貼簿（NULL owner 時 `SetClipboardData` 會失敗）。
  2. 備份所有記憶體型格式（合計上限 64 MiB）；GDI handle 與 private handle 格式（`CF_BITMAP`、metafile、palette、`0x200–0x3FF`）不複製，`CF_BITMAP` 由系統從 `CF_DIB` 合成。備份只在記憶體並 zeroize。
  3. 寫入 `CF_UNICODETEXT` 與 `ExcludeClipboardContentFromMonitorProcessing`、`CanIncludeInClipboardHistory`（DWORD 0）、`CanUploadToCloudClipboard`（DWORD 0）。寫入失敗時先放回備份。
  4. 送 Ctrl+V，等 400 ms。
  5. 剪貼簿序號沒變（沒有別人寫入）時：把我們的文字在剪貼簿記憶體中清零、清空、放回備份並同樣標記三種格式（避免還原本身變成新的歷史紀錄）。序號變了就不動使用者的新內容。
- 不用 MSIX AppContainer（§7.3，UIPI）。送進較高完整性等級的視窗時 SendInput 會被 UIPI 靜默擋下（文件記載），使用者會看到沒插入；文字不會留在剪貼簿。
- CJK IME：SendInput 走 VK_PACKET，本身繞過 IME，注音 / 新注音在任何模式下都會收到原字，不需要處理。剪貼簿路徑的 Ctrl+V 在 IME 有組字中時可能被吃掉；要切到英數模式需要 `ImmGetDefaultIMEWnd`（`Win32_UI_Input_Ime`），不在 0018 批准的 features。決定：先不實作，`ascii_ime_guard` 是 no-op、呼叫點已在剪貼簿路徑前後。**待人工批准**加開 `Win32_UI_Input_Ime`（替代方案：只在偵測到有組字時改送 Esc 結束組字——會改寫使用者的組字，不建議）。

### 7. `unsafe` 範圍（0018）

- 只在 `windows/src-tauri/src/win32/`，由 `win32/mod.rs` 開頭唯一一行 `#![allow(unsafe_code)]` 開放（涵蓋子模組）。檔案：`dialog.rs`（對話框）、`input.rs`（SendInput、GetAsyncKeyState）、`clipboard.rs`（剪貼簿、GlobalAlloc/Lock）；`plan.rs` 是純函式、沒有 `unsafe`。目前 50 個 `unsafe` 區塊，每個都有 `// SAFETY:`。
- CI `unsafe-scope`：放行 `windows/src-tauri/src/win32/*.rs`，並檢查 `win32/mod.rs` 恰好一行 `#![allow(unsafe_code)]`；其他規則不變。
- `windows` crate（與 core 同版 0.62.2）features：`Win32_Foundation`（0005 已批准）、`Win32_Graphics_Gdi`、`Win32_System_DataExchange`、`Win32_System_LibraryLoader`、`Win32_System_Memory`、`Win32_UI_Controls`、`Win32_UI_Input_KeyboardAndMouse`、`Win32_UI_WindowsAndMessaging`。V5 沒有用到 0018 清單以外的 feature。`CF_UNICODETEXT`（13）自行定義常數，避免為一個常數開 `Win32_System_Ole`。

### 8. CI

- 新增 `.github/workflows/windows-app.yml`（windows-latest）：`lint-test` 跑 `cargo clippy -p keepsay-windows --all-targets --no-default-features -D warnings` 與 `cargo test`；`build` 檢查依賴圖沒有 `test-endpoint` / `test-support`，以預設 features（含 whisper）建 release，上傳未簽章的 `keepsay.exe`（保留 14 天）。只用第一方 action、釘 SHA、`contents: read`、不用 secret。
- NSIS 安裝檔需要 tauri-cli（另一個建置工具），延到 M3-13 / M5 與簽章一起處理。
- `tauri-build` 產生的 `gen/` 與 `permissions/autogenerated/` 每次建置重產，加入 `.gitignore`。

### 9. 供應鏈

- `deny.toml`（M3 前次工作加入）：Tauri 經 `tauri-utils` 的 HTML 處理帶入 MPL-2.0 的 `cssparser`、`cssparser-macros`、`dtoa-short`、`selectors`，經 `dirs` 帶入 `option-ext`，比照 0012 的 uniffi 逐 crate 例外；`RUSTSEC-2024-0370`（proc-macro-error unmaintained）只經 Tauri 的 Linux GTK 依賴，Windows 不編譯，忽略。**這兩項需要人工確認。**
- cargo-vet：已執行 `cargo vet regenerate imports`（`imports.lock` 增加 0011 那六個組織對新依賴的稽核，需 PR review）。之後仍有 220 個 crate 版本未稽核，全部來自 Tauri 2、tauri-plugin-global-shortcut 與 cpal 的依賴樹（含只在 Linux / macOS / Android 編譯的 gtk、objc2、ndk 系列，cargo-vet 不分 target）。本次程式碼（`windows` features）沒有新增 crate。
- CLAUDE.md 規定 agent 不得自行新增 exemption，所以 **本紀錄只提出、不套用**。人工批准後的步驟：`cargo vet regenerate exemptions`（只為仍未涵蓋的版本產生 safe-to-deploy exemption），提交 `supply-chain/config.toml`。在那之前 CI 的 `vet` job 是紅的，這是唯一已知的紅燈原因。
- 提議的 220 筆 exemption（`crate:version`，皆 safe-to-deploy）：alsa:0.11.0 alsa-sys:0.4.0 android_system_properties:0.1.6 atk:0.18.2 atk-sys:0.18.2 bs58:0.5.1 bytemuck:1.25.2 cairo-rs:0.18.5 cairo-sys-rs:0.18.2 cargo-platform:0.1.9 cargo_toml:1.0.1 cesu8:1.1.0 cfb:0.14.0 chrono:0.4.45 cookie:0.18.2 core-graphics:0.25.0 coreaudio-rs:0.14.2 cpal:0.18.2 cpufeatures:0.2.17 crc32fast:1.5.3 crossbeam-channel:0.5.17 crossbeam-utils:0.8.23 crypto-common:0.1.7 ctor:1.0.13 darling:0.24.1 darling_core:0.24.1 darling_macro:0.24.1 dasp_sample:0.11.0 defmt:1.2.0 defmt-macros:1.2.0 defmt-parser:1.1.0 deranged:0.5.9 derive_more:2.1.1 derive_more-impl:2.1.1 digest:0.10.7 dirs:7.0.0 dirs-sys:0.5.0 dispatch2:0.3.1 dlopen2:0.8.2 dlopen2_derive:0.4.3 dom_query:0.28.0 dpi:0.1.2 dtoa:1.0.11 dtoa-short:0.3.5 dyn-clone:1.0.20 embed-resource:3.0.12 embed_plist:1.2.2 erased-serde:0.4.11 field-offset:0.3.6 flate2:1.1.10 foreign-types-macros:0.2.4 futures-executor:0.3.34 gdk:0.18.2 gdk-pixbuf:0.18.5 gdk-pixbuf-sys:0.18.0 gdk-sys:0.18.2 gdkwayland-sys:0.18.2 generic-array:0.14.7 gethostname:1.1.0 gio:0.18.4 gio-sys:0.18.1 glib:0.18.5 glib-macros:0.18.5 glib-sys:0.18.1 global-hotkey:0.8.0 gobject-sys:0.18.0 gtk:0.18.2 gtk-sys:0.18.2 gtk3-macros:0.18.2 html5ever:0.39.0 iana-time-zone:0.1.65 ico:0.6.0 indexmap:1.9.3 infer:0.22.0 javascriptcore-rs:1.1.2 javascriptcore-rs-sys:1.1.1 jiff:0.2.38 jiff-core:0.1.1 jiff-static:0.2.38 jiff-tzdb:0.1.9 jiff-tzdb-platform:0.1.3 jni-sys:0.3.1 json-patch:4.2.0 jsonptr:0.7.1 keyboard-types:0.7.0 keyboard-types:0.8.3 libappindicator:0.9.0 libappindicator-sys:0.9.0 libloading:0.7.4 libredox:0.1.25 lock_api:0.4.14 mach2:0.6.0 markup5ever:0.39.0 memoffset:0.9.1 mime:0.3.17 miniz_oxide:0.8.9 miniz_oxide:0.9.1 muda:0.20.0 ndk:0.9.0 ndk-sys:0.6.0+11769913 new_debug_unreachable:1.0.6 num_enum:0.7.6 num_enum_derive:0.7.6 objc2-app-kit:0.3.2 objc2-audio-toolbox:0.3.2 objc2-avf-audio:0.3.2 objc2-cloud-kit:0.3.2 objc2-core-audio:0.3.2 objc2-core-audio-types:0.3.2 objc2-core-data:0.3.2 objc2-core-image:0.3.2 objc2-core-location:0.3.2 objc2-core-text:0.3.2 objc2-exception-helper:0.1.1 objc2-ui-kit:0.3.2 objc2-user-notifications:0.3.2 objc2-web-kit:0.3.2 pango:0.18.3 pango-sys:0.18.0 parking_lot:0.12.5 parking_lot_core:0.9.12 phf:0.13.1 phf_codegen:0.13.1 phf_generator:0.13.1 phf_macros:0.13.1 phf_shared:0.13.1 plist:1.10.3 png:0.18.1 portable-atomic:1.15.0 portable-atomic-util:0.2.8 powerfmt:0.2.1 proc-macro-crate:1.3.1 proc-macro-crate:2.0.2 proc-macro-crate:3.5.0 quick-xml:0.42.0 redox_syscall:0.5.18 redox_users:0.5.3 ref-cast:1.0.27 ref-cast-impl:1.0.27 schemars:0.8.22 schemars:0.9.0 schemars:1.2.3 schemars_derive:0.8.22 scopeguard:1.2.0 selectors:0.38.0 serde-untagged:0.1.9 serde_derive_internals:0.29.1 serde_repr:0.1.21 serde_spanned:0.6.9 serde_with:3.24.0 serde_with_macros:3.24.0 serialize-to-javascript:0.1.2 serialize-to-javascript-impl:0.1.2 servo_arc:0.4.3 simd-adler32:0.3.10 softbuffer:0.4.8 soup3:0.5.0 soup3-sys:0.5.0 static_vcruntime:3.0.0 string_cache:0.9.0 string_cache_codegen:0.6.1 swift-rs:1.0.9 syn:1.0.109 system-deps:6.2.2 tao:0.37.2 tao-macros:0.1.4 target-lexicon:0.12.16 tauri:2.12.5 tauri-build:2.7.4 tauri-codegen:2.7.4 tauri-macros:2.7.4 tauri-plugin:2.7.2 tauri-plugin-global-shortcut:2.4.1 tauri-runtime:2.12.2 tauri-runtime-wry:2.12.3 tauri-utils:2.10.2 tauri-winres:0.3.7 tendril:0.5.1 time:0.3.55 time-macros:0.2.32 toml:0.8.2 toml_edit:0.19.15 toml_edit:0.20.2 toml_edit:0.25.17+spec-1.1.0 tray-icon:0.25.1 typeid:1.0.3 uuid:1.29.0 version-compare:0.2.1 version_check:0.9.5 vswhom:0.1.0 vswhom-sys:0.1.3 wasm-streams:0.5.0 web_atoms:0.2.6 webkit2gtk:2.0.2 webkit2gtk-sys:2.0.2 webview2-com:0.39.1 webview2-com-macros:0.8.1 webview2-com-sys:0.39.1 winapi:0.3.9 winapi-i686-pc-windows-gnu:0.4.0 winapi-x86_64-pc-windows-gnu:0.4.0 window-vibrancy:0.8.1 windows-sys:0.45.0 windows-sys:0.59.0 windows-targets:0.42.2 windows-version:0.1.7 windows_aarch64_gnullvm:0.42.2 windows_aarch64_msvc:0.42.2 windows_i686_gnu:0.42.2 windows_i686_msvc:0.42.2 windows_x86_64_gnu:0.42.2 windows_x86_64_gnullvm:0.42.2 windows_x86_64_msvc:0.42.2 winnow:0.5.40 winreg:0.56.0 wry:0.57.1 x11rb:0.13.2 x11rb-protocol:0.13.2 xkeysym:0.2.1 zlib-rs:0.6.8
- 優先人工審視：`webview2-com*`、`wry`、`tao`、`tauri*`、`global-hotkey`、`cpal`、`muda`、`tray-icon`、`windows-sys`（Windows 上真正執行的部分）。

## Consequences

- 仍需實機（Windows 10 / 11）：§11 手動清單（瀏覽器、Electron、Office、終端機、斷網、AltGr 配置、Alt+Shift 不觸發輸入語言切換）、對話框 DPI / Narrator / 長 key 貼上 / 截圖排除、SendInput 對 UAC 視窗、剪貼簿還原與 Win+V 歷史、WASAPI 隱私設定關閉時的提示（US-M3-12）。
- 仍需人工：cargo-vet exemption 與 deny 例外確認（本紀錄第 9 節）、`Win32_UI_Input_Ime`（US-M3-09）、Authenticode 簽章（§16-2，US-M3-13）。
- 未做：push-to-talk（D-8）、登入時啟動、NSIS 安裝檔、STT 模型匯入的檔案選擇器（目前手動放檔再按「驗證」）。
- CLAUDE.md 的 unsafe 例外文字仍只寫 `core/src/vault/windows.rs`；0018 說要同步更新，但 agent 不改 CLAUDE.md，留給人工。
- PROPOSAL：§7.3 加入實作細節，§15 V5 回填結論，§17 加連結。

## Status

Proposed。
