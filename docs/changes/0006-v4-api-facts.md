# 0006 — V4：Anthropic API 現行事實與 AnthropicClient 實作

- Status：Proposed（PROPOSAL §6.3、§6.4、§12、§15 的對應修改由整合階段套用）
- Date：2026-10-24
- 影響：PROPOSAL §6.3（`anthropic-version`、錯誤對應）、§6.4（移除 temperature、新增 `thinking`）、§6.8（Haiku 5.5 價目預設值）、§12（spend limit 的 Console 位置）、§15 V4；`core/src/api/*`；`core/tests/egress.rs`、`request_type.rs`、`api_errors.rs`
- 故事：US-M1-01、US-M1-07、US-M1-08

## Context

§6.3、§6.4 的模型 ID（`claude-haiku-5-5`）與 `anthropic-version: 2023-06-01` 標為【待驗證 V4】。實作 `AnthropicClient` 前，必須用官方文件確認。查詢時間 2026-10-24。

## V4 結論

來源：

- 模型清單：Claude API skill 內建的模型表（快取日期 2026-10-06）與 `platform.claude.com/docs/en/about-claude/models/overview`
- 版本：<https://platform.claude.com/docs/en/api/versioning>、<https://platform.claude.com/docs/en/api/migration-2026-10-21>
- `/v1/models`：<https://platform.claude.com/docs/en/api/models/list>
- 錯誤：<https://platform.claude.com/docs/en/api/errors>
- spend limit：<https://platform.claude.com/docs/en/api/rate-limits>（「Spend limits」、「Setting your own spend limit」、「Reaching your spend cap」）

### 1. 模型 ID

- 現行 Haiku 為 Claude Haiku 5.5，ID 是 `claude-haiku-5-5`（不加日期後綴）。1M context、最大輸出 128K。官方定價（prompt ≤ 100K tokens）：輸入 $0.10 / MTok，輸出 $0.50 / MTok；超過 100K 為 $0.50 / $2.50。
- 與 Haiku 4.5 相比的破壞性差異：
  - 非預設的 sampling 參數（`temperature`、`top_p`、`top_k`）回 400。
  - 預設開啟 adaptive thinking，effort 預設 `medium`；`thinking: {"type": "disabled"}` 在 effort `high` 以下可接受。
  - 不支援 assistant prefill。
  - 新 tokenizer，同樣文字的 token 數約多 30%。
  - 可能回 `stop_reason: "refusal"`，而且沒有 server-side fallback。
- 結論：`DEFAULT_MODEL` 與 `MODEL_ALLOWLIST` 維持 `claude-haiku-5-5`。白名單只收能接受 `thinking: disabled` 的模型（Sonnet 5.5 不接受 `disabled`，要改用 `between_tools`，所以先不列入）。

### 2. `anthropic-version`

- 最新版本是 `2026-10-21`。官方建議用最新版。前一版是 `2023-06-01`。
- `2026-10-21` 對 Messages API 的破壞性變更：送出 `temperature`、`top_p`、`top_k` 任何一個，在所有模型上都回 400，即使是預設值也一樣。另外兩項（assistant 訊息內的 `document` 區塊、user 訊息內帶 `citations` 的 text 區塊）與本專案無關。
- `/v1/models` 等其他端點在 `2026-10-21` 的行為與 `2023-06-01` 相同。
- 結論：client 送 `anthropic-version: 2026-10-21`（`api::client::ANTHROPIC_VERSION`）。

### 3. `GET /v1/models`

- 回應：`{"data": [ModelInfo…], "first_id", "last_id", "has_more"}`。`ModelInfo` 有 `id`、`type: "model"`、`display_name`、`created_at`、`lifecycle`、`line`、`max_input_tokens`、`max_tokens`、`capabilities` 等。
- query：`limit`（1–1000，預設 20）、`after_id`、`before_id`、`lifecycle`。
- 結論：`verify_key` 呼叫 `GET /v1/models?limit=1`，只看狀態碼，不解析也不保存 body。

### 4. 錯誤形狀與對應

錯誤 body 一律是 `{"type":"error","error":{"type":…,"message":…},"request_id":…}`。429 spend cap 另有 `error.details.error_code`。

| 情況（官方文件） | 狀態碼 / 形狀 | `CoreError` |
|------------------|---------------|-------------|
| key 格式錯、撤銷、過期 | 401 `authentication_error` | `KeyInvalid` |
| key 無權限 | 403 `permission_error` | `KeyInvalid` |
| 帳務或付款問題 | 402 `billing_error` | `BudgetExhausted` |
| 使用者自設的 org / workspace spend limit 到達 | 400 `invalid_request_error`，訊息以 `You have reached your specified API usage limits` 或 `You have reached your specified workspace API usage limits` 開頭 | `BudgetExhausted` |
| 預付額度用完（舊有訊息，現行文件未列，保留比對） | 400 `invalid_request_error`，訊息含 `credit balance` | `BudgetExhausted` |
| 方案層級的每月 spend cap | 429 `rate_limit_error`，`details.error_code = "enforced_spend_limit_reached"`，沒有 `retry-after`，下月 1 日前一律失敗 | `BudgetExhausted`（不重試） |
| rate limit | 429 `rate_limit_error`，可能帶 `retry-after`（秒） | 重試一次，仍失敗為 `RateLimited` |
| 過載 | 529 `overloaded_error` | 重試一次，仍失敗為 `RateLimited` |
| 處理逾時 | 504 `timeout_error` | `Timeout` |
| 其他 | — | `ApiStatus(code)` |

- 重試的等待時間是 `min(retry-after, 2 s)`；沒有 `retry-after` 或無法解析（例如 HTTP-date）時等 1 秒。
- 比對只在記憶體內進行，錯誤 body 不記 log、不存、不放進 `CoreError`。

### 5. Workspace 與 spend limit（給 §12 onboarding）

- 每個方案層級有每月 spend cap（Start $500、Build $1,000、Scale $200,000）。
- 自訂 spend limit：Console → Billing（`platform.claude.com/settings/billing`）→ Spend limits → Adjust limit / Set limit。不能高於方案上限。
- workspace 可以設自己的 spend limit 與 rate limit，但「Default Workspace 不能設 spend limit」。所以 §12 第 1、3 步必須是「建立 keepsay 專用的非預設 workspace，再對它設 spend limit」。
- 撤銷 key、關閉自動儲值的 Console 位置本次沒有查到官方頁面，仍待人工確認（§16-4 建立測試 workspace 時一併確認）。

## Decision

1. `anthropic-version` 用 `2026-10-21`。
2. `MessagesRequest` 移除 `temperature` 欄位（§6.4 的 0.2 不再送出）。理由：`2026-10-21` 一律拒收；即使留在 `2023-06-01`，Haiku 5.5 也拒收非預設值。代價是失去低溫取樣，潤稿的一致性改靠 prompt 與 guard。
3. `MessagesRequest` 新增 `thinking: Thinking`，只有一個值 `Disabled`，序列化為 `{"type":"disabled"}`。理由：Haiku 5.5 預設開 adaptive thinking，thinking token 會算進 `max_tokens`（本專案上限 2048，短句只有 256 起跳），可能截斷輸出，也拉長延遲，違反 §1.3 的 3 秒目標。這個欄位只控制隱藏推理，不給模型任何行動能力，不影響 INV-4。`core/tests/request_type.rs` 改為斷言五個 key：`max_tokens`、`messages`、`model`、`system`、`thinking`，並斷言 `thinking` 的值固定。
4. `MessagesResponse::is_complete()`：`stop_reason` 是 `end_turn` 或 `stop_sequence` 才算完成。`max_tokens`（截斷）、`refusal`、未知值都不算。client 仍回傳 `Ok`，讓 polish 拿得到 `usage` 記帳，再由 polish 決定退回 raw（FR-8 不輸出半成品）。
5. Client 實作細節（§6.3）：
   - `redirect::Policy::none()`；3xx 以 `ApiStatus(3xx)` 回傳，不跟隨。
   - `retry::never()`，關掉 reqwest 內建的協定層重試，重試只有上面那一次。
   - `no_proxy()`；`Config::proxy` 有值時才 `Proxy::all`，解析失敗回 `Config`。
   - production 建構時 `validate_base_url` + `https_only(true)`；`referer(false)`。
   - `x-api-key` 的 `HeaderValue` 標為 sensitive；每次嘗試都從 vault 重新讀 key，建好 header 立刻 drop `SecretString`，等待重試期間不持有 key。
   - 先檢查 `MODEL_ALLOWLIST`，不在名單內回 `ModelNotAllowed`，不讀 key、不送請求。
   - `AnthropicClient` 的 `Debug` 改手寫，只印 `base_url`（reqwest 的 `Debug` 會印出 proxy 設定，可能含帳密）。
   - tracing 只記 endpoint、第幾次嘗試、狀態碼、耗時。
6. `with_test_endpoint` 只接受 `http://127.0.0.1:<port>` 或 `http://localhost:<port>`（可帶一個結尾 `/`），其餘回 `DisallowedHost`。另新增 `with_test_endpoint_timeout`（同 feature）以短逾時測試 `Timeout`。

## Consequences

- PROPOSAL 需由整合階段修改：§6.3 版本字串改 `2026-10-21` 並移除【待驗證】、錯誤對應補上 402 與 429 spend cap；§6.4 移除 `temperature: 0.2`、加入 `thinking`；§15 V4 回填本紀錄；§12 依第 5 點修正；§6.8 預設價目表可用 $0.10 / $0.50 per MTok（仍標「請核對官方定價」）。
- 工作流 B（polish）：
  - 不要設定 `MessagesRequest.temperature`（欄位已不存在）。
  - 用 `resp.is_complete()` 判斷截斷與 refusal，不完整就退回 raw。
  - 輸入 token 估計若以字元換算，Haiku 5.5 的 tokenizer 約多 30%，估算係數要調大，否則 `max_tokens` 偏小。
- 工作流 D（budget）：`Usage` 只取 `input_tokens`、`output_tokens`；本專案不用 prompt cache，`cache_*` 欄位可以忽略。
- 若日後要改用 Sonnet 5.5，`thinking` 需要改成 `between_tools`，`Thinking` enum 要加 variant，白名單同步更新。
- 測試：`core/tests/egress.rs` 的 redirect 測試解除 ignore（兩台 wiremock，307 POST 與 302 GET，第二台收到 0 個請求）；`core/tests/api_errors.rs` 覆蓋表格內每一列、重試一次、上限 2 秒、逾時、格式錯誤、模型白名單、缺 key。
