# 0009 — 模型下載器、manifest、預算與設定（M1 工作流 D）

- Status：Accepted（工作流 D 提案，待整合時人工複核；涉及 PROPOSAL §8 的 manifest 欄位與 §6.8 預設值）
- Date：2026-10-24
- 影響：`core/src/downloader.rs`、`core/src/budget.rs`、`core/src/config.rs`、`models/manifest.json`、`core/tests/downloader.rs`、`core/tests/budget_ledger.rs`、`core/tests/config_file.rs`、ROADMAP（US-M1-14、15、18、19）。PROPOSAL §8、§6.8、§10 待整合時同步（見 Consequences）。

## Context

0005 定好了 `ModelDownloader`、`Budget`、`Config` 的簽章，本體留給工作流 D。實作時有幾件事 spec 沒寫清楚：

1. Hugging Face 的 `resolve/<revision>/…` URL 一律 302 到 CDN（實測 2026-10-24 為 `us.aws.cdn.hf.co`，依地區不同，過去也出現過 `cdn-lfs*.hf.co`、`cas-bridge.xethub.hf.co`）。§6.3 的「禁止 redirect」若原樣套到下載器，就無法從 HF 下載；但 INV-7 要求出口只能是 manifest 指定的地方。
2. manifest 的 SHA-256 要「由維護者親自下載計算」（US-M1-18）。
3. §6.8 的「每月 1 日歸零」沒有說是哪個時區；core 沒有時區資料庫，也不能新增依賴。
4. 價目表與預算上限的預設值。

## Decision

### 1. 下載器的 redirect 政策：手動跟隨、逐跳檢查

- reqwest client 設 `redirect::Policy::none()`、`no_proxy()`、connect 10 s、讀取閒置 30 s（不設總逾時，大檔在慢速網路上本來就久）。
- `download` 自己跟 redirect，最多 `MAX_REDIRECTS = 5` 跳。每一跳在連線前檢查 `Location`：
  - 必須是 `https://`，不得帶帳密。
  - 與起始 URL 同 origin（scheme、host、port 都相同），或 host 列在該筆 manifest 的 `redirect_hosts`。
  - `redirect_hosts` 的項目若以 `.` 開頭，代表該網域的子網域：`.hf.co` 允許 `us.aws.cdn.hf.co`，不允許 `hf.co`、`evilhf.co`、`hf.co.evil.com`。其餘項目完全比對。
- 完整性不靠 host，靠寫死的 SHA-256 與 size；host 檢查是為了讓出口只到 manifest 列出的地方（INV-7、§8）。
- 選 `.hf.co` 子網域而不是逐一列出 CDN host，是因為 HF 的 CDN host 依地區與時間變動，逐一列出會讓部分使用者無法下載。`hf.co` 由 Hugging Face 控制，風險與信任 `huggingface.co` 相同。
- 考慮過的替代方案：
  - reqwest `Policy::custom`：policy 綁在 client 上，拿不到「這筆 entry 允許哪些 host」，要每次下載重建 client；手動跟隨比較直接，也容易測。
  - 完全禁止 redirect：HF 無法使用。
  - 允許任何 HTTPS redirect：等於放棄 §8 的出口限制。

### 2. manifest 欄位與內容

- `ModelEntry` 新增選填欄位 `redirect_hosts`（預設空陣列，即只允許同 origin redirect）。仍是 `deny_unknown_fields`。
- `Manifest::parse` 對每筆檢查：id 只能是 `[a-z0-9._-]` 且不以 `.` 開頭（它會變成檔名）、`sha256` 為 64 位小寫 hex、`revision` 為 40 位小寫 hex 且出現在 URL 路徑中、`size > 0`、URL 為 `https://` 且不帶帳密、id 不重複。
- 三筆 whisper.cpp ggml 模型，全部釘在 `ggerganov/whisper.cpp` 的 commit `5359861c739e955e79d9a303bcbc70fb988958b1`（HF API `api/models/ggerganov/whisper.cpp` 的 `sha`，lastModified 2024-10-29）：

  | id | 檔案 | size（bytes） | sha256 |
  |----|------|---------------|--------|
  | whisper-base | ggml-base.bin | 147951465 | `60ed5bc3…a2efe` |
  | whisper-small | ggml-small.bin | 487601967 | `1be3a9b2…fea987b` |
  | whisper-large-v3-turbo | ggml-large-v3-turbo.bin | 1624555275 | `1fc70f77…2bc69` |

- 雜湊的取得方式（2026-10-24）：
  1. 從 `https://huggingface.co/api/models/ggerganov/whisper.cpp/tree/<revision>` 讀每個檔案的 `lfs.oid`（git-lfs 的 SHA-256）與 `size`。
  2. 從釘住 revision 的 resolve URL 實際下載三個檔案，在本機以 `sha256sum` 計算，三筆都與 `lfs.oid` 一致；size 也一致。
  - 這是 agent 在開發環境執行的，不是維護者本人。US-M1-18 要求「維護者親自計算」，所以標為待人工複核：維護者在自己的機器上重跑上面兩步確認即可。
- 授權：HF model card 的 `license: mit`。

### 3. 下載與匯入的檔案處理

- 最終路徑固定為 `<dest_dir>/<id>.bin`（`downloader::model_path`），STT 端用同一個函式找檔。
- 下載與匯入都先寫到同目錄的 `.<id>.bin.part-<pid>-<nanos>`（`create_new`），邊寫邊算 SHA-256。size 與 hash 都相符才 `sync_all` 並 rename 到最終路徑；任何失敗或 future 被 drop（取消）時，`Drop` 刪掉暫存檔。舊的模型檔只有在新檔驗證通過後才被取代。
- 下載時 `Content-Length` 或累計位元組超過 manifest 的 `size` 就中止（`ModelTooLarge`）；結束時位元組數不等於 `size` 或 hash 不符為 `ModelHash`。
- 手動匯入不連網，hash 是對「實際寫入的位元組」計算，來源檔在複製途中被改掉也擋得住。
- `test-endpoint` feature 下才有 `ModelDownloader::with_test_endpoint()`，額外允許 `http://127.0.0.1` / `localhost` 給 wiremock；`new()` 與 release build 只接受 HTTPS。release build 啟用 `test-endpoint` 會 `compile_error!`（0005）。
- 檔案 I/O 用 `std::fs`（tokio 沒有開 `fs`），在 async 裡是同步寫入；下載是使用者主動觸發、單一任務，可以接受。

### 4. 預算

- `budget.json` 帶自己的 `BUDGET_SCHEMA_VERSION = 1`，與設定檔的版本分開。`deny_unknown_fields`。寫入走 temp + fsync + rename。
- 月份由呼叫端注入 `YearMonth`。「本地時間 1 日歸零」：平台層自己算本地月份，或把當下的 UTC 偏移傳給新增的 `YearMonth::from_unix_secs_with_offset`。core 不碰時區資料庫。
- 只往前歸零：時鐘倒退到前一個月時，用量繼續累加在已存的月份，不會被清掉，避免改時鐘就繞過硬上限。
- `budget.json` 損毀或版本不明時回 `CoreError::Config`，不自動重設；呼叫端應把當月視為已達硬上限，而不是視為零用量。
- token 累加用 saturating add。

### 5. 價目表與上限預設值

- 價目表預設為 Claude Haiku 5.5 的 Anthropic 官方 API 定價：input $0.10、output $0.50 / 百萬 token（輸入 100K token 以內；來源為 claude-api 參考資料，快取日期 2026-10-06）。UI 仍須標示「請核對官方定價」；模型 ID 仍屬【待驗證 V4】。
- 預算上限預設：軟上限 $5、硬上限 $10 / 月。依上述價格約等於數千萬 token，遠高於聽寫用量，作用是擋住程式錯誤造成的失控請求；真正的損失上限仍由帳號端（§12）保證。

### 6. 設定

- `Config` 與子結構都是 `#[serde(default, deny_unknown_fields)]`：缺欄位補預設值、未知欄位拒絕。
- `load` 先讀 `schema_version`：缺少或非整數 → `Config`；大於目前版本 → `Config`，且不覆寫檔案（降版不會毀掉新版設定）；小於目前版本 → `config::migrate` hook（v1 是第一版，目前任何舊版號都拒絕）。
- 驗證：`model` 必須在 `MODEL_ALLOWLIST`（否則 `ModelNotAllowed`），價格與上限必須是有限的非負數。`save` 也先驗證。
- 預設：mode `polish`、translate_to `en`、model `DEFAULT_MODEL`、stt_model `whisper-small`、glossary 空、proxy 無。快捷鍵欄位 `None` 代表平台預設，預設組合以常數提供：`WINDOWS_HOTKEYS`（Alt+Shift+Z / Alt+Shift+X）、`MACOS_HOTKEYS`（Fn+Z / Fn+Shift+Z）、`MACOS_FALLBACK_HOTKEYS`（Option+Space / Option+Shift+Space），由 `Hotkeys::resolve` 合併使用者覆寫（0004）。
- 詞彙表與 proxy 不進 `Debug`（只印長度與是否有值），錯誤只有固定的 `CoreError` variant。

## Consequences

- 新增的 `pub` 項目：`downloader::{MAX_REDIRECTS, model_path, ModelEntry::validate, ModelEntry::redirect_hosts, ModelDownloader::with_test_endpoint}`、`budget::{BUDGET_SCHEMA_VERSION, YearMonth::from_unix_secs_with_offset, YearMonth::is_valid, estimate_usd}`、`config::{DEFAULT_STT_MODEL, HotkeyDefaults, WINDOWS_HOTKEYS, MACOS_HOTKEYS, MACOS_FALLBACK_HOTKEYS, Hotkeys::resolve, Config::from_json, Config::validate, migrate}`。0005 的既有簽章都沒改。
- `PriceTable`、`BudgetLimits` 的 `Default` 由 derive 改為手寫（非零預設值）。
- 待整合時同步 spec（docs/specs 在平行實作期間凍結，所以本紀錄先不改）：
  - PROPOSAL §8：manifest 欄位加上 `redirect_hosts`，並寫明下載器「手動跟隨 redirect、逐跳檢查 host」，與 §6.3 AnthropicClient 的「禁止 redirect」區分。
  - PROPOSAL §6.8：每月歸零以本地時間計，由平台層提供月份或 UTC 偏移；時鐘倒退不歸零；預設價目表與上限。
  - PROPOSAL §10：未知欄位拒絕、較新版本拒絕且不覆寫。
  - THREAT_MODEL T1 / §8：下載器出口包含 `*.hf.co`。
- §16 新增人工項目（建議）：維護者在自己的機器重算三個模型的 SHA-256（見上方方法），並在 PR 註明。
- `downloader.rs` 的文字掃描測試仍通過；模組文件改寫成不直接出現被掃描的字。
