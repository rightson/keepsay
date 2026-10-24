# 0004 — 授權、網域識別字與預設快捷鍵

- Status：Accepted
- Date：2026-10-24
- 影響：PROPOSAL v0.2 → v0.3（FR-2、§6.2、§7.1、§7.3、§15 V8 / V9、§16 第 5、7 項、ADR-8）、THREAT_MODEL T7、CLAUDE.md、README、SECURITY.md、deny.toml、Cargo.toml、LICENSE、docs/mockups/desktop.html、core/src/vault/apple.rs

## Context

M0 結束時留下三個待使用者決定的項目：

1. 授權不一致：LICENSE 是 GPL-3.0，ADR-8 寫 Apache-2.0（§16 第 7 項）。GPL 與 App Store 條款的相容性也有爭議。
2. 名稱與網域：keepsay.dev 未註冊，但尚未決定是否使用；識別字暫定 `dev.keepsay.*`（0002、§16 第 5 項）。
3. 預設快捷鍵的實作方式（V8、V9，見 0001）。使用者原指定 Windows Right Alt / Right Shift + Right Alt、macOS fn + Z / fn + Shift + Z，並補充：「指定的鍵如果真的很麻煩，就換別的。」

## Decision

### 1. 授權：Apache-2.0

- LICENSE 換成 https://www.apache.org/licenses/LICENSE-2.0.txt 的全文。取得後與 SPDX license-list-data 的 `Apache-2.0.txt` 比對，忽略空白差異後內容一致；SHA-256 為 `cfc7749b96f63bd31c3c42b5c471bf756814053e847c10f3eb003417bc523d30`。
- workspace `license = "Apache-2.0"`。
- deny.toml 的允許清單本來就只有寬鬆授權，與 Apache-2.0 相容，不需要變動，只更新註解。

### 2. 網域與識別字：`keepsay.rightson.dev` / `dev.rightson.keepsay.*`

- 保留 keepsay 名稱。網域用 `keepsay.rightson.dev`（使用者自有網域的子網域），不另外註冊 keepsay.dev。
- 反向 DNS 識別字一律以 `dev.rightson.keepsay` 為前綴：Keychain service `dev.rightson.keepsay.apikey`，bundle id 例如 `dev.rightson.keepsay.mac`、`dev.rightson.keepsay.ios`、`dev.rightson.keepsay.ios.keyboard`，App Group `group.dev.rightson.keepsay`。
- crate 名維持 `keepsay-core` / `keepsay-cli`。Windows Credential Manager target `keepsay/anthropic` 不是反向 DNS 格式，不變。
- 目前還沒有任何使用者資料，所以更名不需要遷移。

### 3. 快捷鍵

判斷規則：原指定組合若有系統衝突，或需要擴大權限與攻擊面且沒有對應收益，就算「真的很麻煩」，改用其他組合。

#### macOS：保留 fn + Z / fn + Shift + Z（V9 選項 (b)）

查證結果：

- 沒有系統衝突。Apple「Mac keyboard shortcuts」（https://support.apple.com/102650）列出的 fn 組合是 fn + A、fn + Shift + A、fn + C、fn + D、fn + E、fn + F、fn + H、fn + N、fn + Q、fn + Delete、fn + 方向鍵、fn + Control + F2–F8，沒有 Z。TidBITS〈The Hidden Secrets of the Fn Key〉（https://tidbits.com/2023/11/16/the-hidden-secrets-of-the-fn-key/）另列 fn + M、fn + Space、fn + Return，也沒有 Z。macOS 15 / 26 的視窗並排快捷鍵是 fn + Control（+ Shift / Option）+ 方向鍵或 F / C / R，不衝突。
- 技術上可行。CGEventFlags `maskSecondaryFn` 的官方說明是「Indicates that the Fn (Function) key is down for a keyboard, mouse, or flag-changed event」（https://developer.apple.com/documentation/coregraphics/cgeventflags/masksecondaryfn），所以 Z 的 keyDown 事件帶得到 fn 旗標。Carbon 熱鍵與 Keyboard Maestro 這類工具都無法把 fn 當修飾鍵（TidBITS 同文；Keyboard Maestro 論壇 https://forum.keyboardmaestro.com/t/has-the-globe-fn-key-acquired-new-abilities-in-macos-12/24580），所以只能用 event tap。

決定：

- 用 active CGEventTap（kCGEventTapOptionDefault），只訂閱 keyDown / keyUp，命中 fn + Z 或 fn + Shift + Z 時吞掉事件，前景 App 不會收到 Z。
- active tap 需要「輔助使用」權限。AX 插入本來就需要這個權限，所以不新增權限類別。listen-only tap 反而要另外申請「輸入監控」，而且吞不掉 Z，所以不選 (a)。
- T7 對策：回呼只比對 Z 的 keycode 與 fn、Shift 旗標，不保存、不記 log、不轉送任何按鍵，非命中事件原封不動放行。tap 程式碼隔離在單一 Swift 檔案，列入 code review 必查項目。
- Carbon 備援組合：⌥ Space / ⌥ ⇧ Space，給沒有 fn 鍵的外接鍵盤，或不授予輔助使用權限的使用者。

已知風險：

- SwiftUI 在 macOS 12 beta 的 release note 註明「Fn (Function) shortcut modifier is deprecated and reserved for system usage」（引自上述 Keyboard Maestro 論壇）。Apple 未來可能把 fn + Z 收為系統快捷鍵，屆時改以 Carbon 備援組合為預設。
- ⌥ Space 可能和 Alfred、ChatGPT 等 App 的預設快捷鍵衝突；Carbon 註冊失敗時提示使用者改鍵。

#### Windows：改為 Alt + Shift + Z / Alt + Shift + X（V8 選項 (c)）

Right Alt 屬於「真的很麻煩」：

- 單獨偵測 Right Alt 需要 WH_KEYBOARD_LL 低階 hook，要 Win32 unsafe FFI，而 workspace 禁止 `unsafe_code`。
- 許多歐洲鍵盤配置的 Right Alt 是 AltGr，會直接衝突。
- 單按 Alt 放開會啟用選單列，不遮蔽就會干擾前景程式。
- 前景是高權限視窗時，UIPI 會讓 hook 收不到事件。
- 行為與 keylogger 相同，容易被防毒誤判，也擴大 T1 被利用時的損害（T7）。

選擇 Alt + Shift + Z（潤飾）/ Alt + Shift + X（潤飾並翻譯），以 RegisterHotKey 註冊並加 MOD_NOREPEAT。理由：

- 不含 Ctrl + Alt。Windows 把 AltGr 視為 Ctrl + Alt，而 RegisterHotKey 要求修飾鍵完全相符，所以 AltGr + Shift + Z（例如波蘭文配置的 Ż）不會被攔截。
- 不含 Win。Win + 字母多數被系統保留，例如 Win + H 是系統語音輸入。
- 避開注音 / 新注音的 Ctrl + Space、Shift、Shift + Space、Ctrl + Shift。Alt + Shift 本身是切換輸入語言，但切換只在兩鍵單獨按下放開時觸發，中間夾第三個鍵就不會。Windows 內建的語音存取也使用 Alt + Shift + B / C（https://support.microsoft.com/en-us/accessibility/windows/voice-access/get-started-with-voice-access），可以佐證這類組合可用。
- 避開 Alt + Space（系統視窗選單、Copilot、PowerToys Run）與 Alt + Z（NVIDIA overlay、VS Code 自動換行）。
- 已知衝突：Word 的 Alt + Shift + X（標記索引項目）屬冷門功能，可接受。VS Code、Chrome、Edge、Firefox、Excel 預設沒有用到 Alt + Shift + Z / X。
- Z 與 macOS 的 fn + Z 是同一顆鍵，X 在它旁邊，跨平台好記。
- 註冊失敗（被其他程式佔用）時提示使用者在設定中改鍵。

Right Alt 延到 v1.1 以後，作為使用者主動開啟的進階選項（背後走 V8 選項 (a)）。v1 不實作。要實作時必須先另開變更紀錄，並取得放寬 `unsafe_code` 的批准。

## Consequences

- PROPOSAL 升為 v0.3：FR-2、§6.2、§7.1、§7.3、§15 V8 / V9 結論、§16 第 5 項（名稱與網域已決定）與第 7 項（授權已解決）、ADR-8。
- THREAT_MODEL T7 狀態改為已決定：Windows 不監聽；macOS 監聽，對策如上。M2 要補上 tap 模組的自動化檢查（例如文字掃描，確認該檔案不呼叫檔案、網路、剪貼簿、logging API）。
- SECURITY.md 把「event tap 讀取或保存 fn + Z 以外的按鍵」列為範圍內問題。
- CLAUDE.md 移除授權 gotcha，快捷鍵 gotcha 改寫成已決定的規則。
- 0001 的實作方式狀態改為 Accepted；0002 的識別字決定被本紀錄取代。
- M2 spike 要實測：
  - fn + Z 不會觸發「按下 🌐 鍵」設定的動作（切換輸入法、表情符號、聽寫）。
  - 非 QWERTY 配置（例如 AZERTY）的 Z 位置不同。要在設定時依目前配置換算 keycode，不在回呼內做。
  - Secure Input（密碼欄位）期間 tap 收不到事件，屬預期行為。
  - 與 V2 的交互：sandbox 下能否建立 active tap。
- M3 手動測試：AltGr 配置下 AltGr + Shift + Z / X 仍可正常輸入字元；開啟注音時 Alt + Shift + Z 不會切換輸入語言。
