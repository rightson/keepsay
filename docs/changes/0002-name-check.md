# 0002 — 名稱可用性初查

- Status：Proposed（名稱最終決定待使用者）
- Date：2026-10-24

## Context

開工前先確認 `keepsay` 這個名稱是否已被使用，避免日後被迫更名（crate 名、bundle id、Keychain service 都綁定名稱，越晚改越貴）。

## Findings

| 項目 | 結果 | 依據 |
|------|------|------|
| crates.io `keepsay` / `keepsay-core` / `keepsay-cli` | 可用 | API 回 404 |
| npm `keepsay` | 可用 | registry 回 404 |
| npm `@keepsay` scope | 大概可用 | 搜尋 0 筆；org 頁面 403，無法排除空 org |
| keepsay.dev | 未註冊 | Google Registry RDAP 404，無 DNS 紀錄 |
| keepsay.app | 已註冊 | RDAP 200（OVH） |
| keepsay.com | 已註冊 | Verisign RDAP 200，2014 年註冊，解析到 AWS |
| GitHub 同名 repo / `github.com/keepsay` 帳號 | 未確認 | 開發環境 proxy 回 403 |
| 同名產品 / 商標 | 未發現 | 只做了網路搜尋，未直接查 USPTO / TIPO / EUIPO |

相近名稱：Keepsy（相簿 App）、Keepsayk、Keepsake（多個 App）、Keepsaker。

## Decision

暫時保留 `keepsay`。識別字一律以 `dev.keepsay.*` 為前綴，與可取得的 keepsay.dev 一致。剩下的人工確認項目列在 PROPOSAL §16 第 5 項。

## Consequences

- 若商標查詢或 GitHub 查詢發現衝突，必須在 M1 結束前更名。M1 之後 Keychain service 與 Credential Manager target 一旦有使用者資料，更名就需要遷移。
- 不使用 keepsay.app。若之後要做官網，優先 keepsay.dev。
