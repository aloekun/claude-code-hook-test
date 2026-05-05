# CodeRabbit 監視機能改善

> **目的**: 個人開発で CodeRabbit 無課金ユーザーの rate-limit (1 時間あたり 3 reviews 上限) で発生する待ち時間を、ユーザー手動介入なしに自動回復させる。「定期的に PR コメントを確認する」手間をなくし、快適な個人開発環境を整備する。
>
> **本ドキュメントの役割**: CodeRabbit 監視機能の改善に関する task 分類・bundle 案を集約する index。各 task の作業詳細は `docs/todo*.md` 系列に置き、本ファイルは概要 + リンクに留める。
>
> **状態**: 試験運用 (本ドキュメントは "計画書" であり、bundle が消化されたら役割を終える)
>
> **想定読者**: 本リポジトリで CodeRabbit と連携する自動 review 環境を運用するユーザー。「rate-limit に引っかかった時に手動介入を最小化したい」目的を持つ。

---

## 着手決定 (2026-05-05)

### 採用案

**Bundle b (CR auto-monitoring core、3 PR 構成) を採用**。`max_duration_secs` 拡大による sleep 延長案 (案 C) は **却下**。

### 着手順 / 設計方針

- **着手は Bb-1 (順位 53) から**
- **CronCreate は `durable: true` で利用** — `.claude/scheduled_tasks.json` に永続化され、Claude Code session 再起動を跨ぐ
- **PR description で ADR-030 の責務分離パターン (L1 floor + L2 recovery) を引用** — post-merge-feedback で実証済の「機械的 Rust + Claude Code 定期チェック」分離原則を CR rate-limit に適用 (3 例目)。新 ADR で本パターン適用を明文化する想定

### 設計判断時に確認した CronCreate の重要事実 (今後の再分析を防ぐため記録)

- 標準 5 フィールド cron 構文 (`MM HH DoM Mon DoW`) — **時間制約なし**。「60 分上限」は **別ツール `ScheduleWakeup` (`/loop` 動的モード) の `clamped to [60, 3600]` 制約** であり、CronCreate には適用されない
- One-shot (`recurring: false`) で任意の reset 時刻に 1 度だけ wakeup 可能 → 47 分でも 90 分でも cron で直接予約可能
- `durable: true` で session 跨ぎが native サポート — 旧 ADR-009 設計の「session 終了で wakeup 発火しない」問題は durable 化で解消可能 (Bb-3 SessionStart catch-up は補完層として位置づけ)
- recurring task は 7 日 auto-expire (rate-limit context では十分長い)

### ADR-018 との整合性

ADR-018 が廃止したのは「同一プロセス内で 4 段間接連携 (daemon → state file → CronCreate → Claude → skill) する旧設計」であり、**プロセス分離 + 責務分離原則そのものは ADR-018 流儀と整合**。Bundle b は ADR-030 の L1+L2 パターン (機械的 Rust 状態管理 + Claude Code 定期チェックの分離) を踏襲する。

### 案 C を却下した理由

- **`max_duration_secs` 1h cap 延長は短期的には有効だが、Claude Code session 終了時の silent loss を解消できない** — cli-pr-monitor は Claude Code Bash tool の child process として動作するため、session 終了で sleep 中の subprocess が kill される
- **CronCreate `durable: true` は session 跨ぎを native サポート** — 案 C 単体より UX が確実に良い
- 90+ 分 rate-limit に対する構造的限界 (案 C は cap 超過で fail、案 A は cron で任意時刻予約可能)
- 「30 分ごとに確認 → 残時間再計算で reschedule」chain pattern は CronCreate native 機能で実現可能 (ユーザー指摘、本決定の根拠の一つ)

### 進捗

- ✅ **Bb-1 (順位 53) — マージ済 (PR #113、2026-05-05)**: `cli-pr-monitor` の rate-limit retry を CronCreate park モデルに切り替え。post-merge-feedback で T2-2 のみ採用 (順位 75)、T3-1/T3-2 はユーザー判断で却下 (memory: feedback_no_unenforced_rules)。
- ✅ **Bb-2 (順位 54) + 順位 75 (T2-2 Bb-1 follow-up) — マージ済 (PR #114、2026-05-05)**: review 完了待ちを CronCreate park モデルに展開し、polling 完全排除 + 二重 polling (45s gh API + 5s observer) を撤廃。T2-2 の sibling parity 回帰テストを同 PR で bundled。CR Major 2 件 (head 不一致時の wakeup 誤判定 / fresh push での recheck count 持ち越し) は fold-in 修正で land。post-merge-feedback で 2 件採用 / 5 件様子見 / 3 件却下 (詳細レポート: `.claude/feedback-reports/114.md`)。
- ✅ **Bb-3 (順位 55) — 実装完了 (2026-05-05、PR 起票準備中)**: `[review_recheck]` config セクションを `pr-monitor-config.toml` に追加し旧 hard-coded const (INITIAL_REVIEW_WAIT_SECS / REVIEW_RECHECK_WAIT_SECS / MAX_REVIEW_RECHECKS) を `PollContext` 経由で thread。SessionStart hook (`hooks-session-start`) に catch-up nudge を追加 (state.action が `parked_*` かつ `next_wakeup_at_unix <= now` のとき additionalContext に `[PR_MONITOR_CATCHUP]` を注入)。MonitorConfig::poll_interval_secs (Bb-2 で未使用化) を削除し既存 toml は legacy フィールド無視で後方互換。T2-2 follow-up として `finalize_park_siblings_have_symmetric_write_state_handling` を 3 sibling 化 (`finalize_initial_review_park` を追加)。
  - **設計判断: SessionStart catch-up は別プロセス spawn せず additionalContext で nudge** (advisor 推奨 option b)。Windows 環境での handle 継承 / stdout 可視性問題を回避し、PARK signal flow を session 内に保つ。
  - **false-positive 抑制**: terminal action (`action_required` / `passed_clean` / `continue_monitoring`) では `next_wakeup_at_unix` が古い park 由来で残っていても nudge を出さない (action prefix `parked_` を gate に使用)。

---

## 現状の課題

- **CodeRabbit 無課金**: 1 時間あたり 3 reviews 上限
- **上限到達時の挙動**: rate-limit comment が PR に投稿され、最大 60 分の `wait_minutes` が記載される
- **既存実装** (`cli-pr-monitor`、PR #97 で land 済) の限界:
  - rate-limit 自動検出 + 再トリガー機構あり
  - ただし `std::thread::sleep` 同プロセス内待機 + `max_duration_secs=600s` (10 分) cap で**長時間 rate-limit にバウンス**する
  - PR #104 で 47 分 rate-limit を実観測、auto-retry が機能せず `action_required` 通知 → ユーザーは手動で PR コメントをチェックして再投稿する必要

## ボトルネック分析

| ボトルネック | 現状 | 改善方向 |
|---|---|---|
| 長時間 rate-limit (>10 分) の auto-retry | `std::thread::sleep` + 600s budget cap でバウンス | **CronCreate ベース wakeup** に置換、長時間待機を構造的に可能化 |
| review 完了待ちの polling 負荷 | 45s 間隔で gh API polling + observer 5s 間隔 polling | 計算時刻 wakeup に置換、**polling 完全排除** |
| AI 離席中の silent loss | wakeup 発火しない期間に rate-limit 解除しても検知できない | **SessionStart hook で pending wakeup catch-up** |
| structured findings 取得 | CodeRabbit comment text を grep ベース parse | `check-ci-coderabbit --list-findings` で構造化取得 (Sub-PR 1 で API 提供済) |
| 自動 trigger の信頼性 | error-path test infra なし、silent fail (`unwrap_or_else` で空配列 fallback) 検出機構なし | parse_findings の error-path test 追加 |
| ポリシーの暗黙化 | rate-limit retry の判断基準が ADR-018 / ADR-009 で散在 | ADR で明文化 |

---

## 改善 task 分類

各 task の詳細 (動機 / 設計決定 / 作業計画 / 完了基準) は `docs/todo*.md` 系列を参照すること。本セクションは概要 + 効果のみ記載。

### 🎯 HIGH IMPACT — rate-limit 自動回復の中核

| 順位 | Tier | タスク概要 | 効果 | 作業詳細 |
|---|---|---|---|---|
| 53 | 🚀 Tier 1 | rate-limit retry の CronCreate 化 (Bundle b PR-1) | **致命点解消**。47 分 rate-limit を auto-retry 可能に。同プロセス常駐モデル → スケジュール起動モデルへ転換 | [todo5.md](todo5.md) |
| 42 | 🔧 Tier 2 | cli-pr-monitor rate-limit auto-retry + `@coderabbitai review` auto-trigger 実装 (Bundle a Sub-PR 2) | `check-ci-coderabbit --list-findings` (Sub-PR 1 land 済) を消費して構造化 retry に進化 | [todo4.md](todo4.md) |
| 54 | 🔧 Tier 2 | review 完了待ちの CronCreate 化 + observer 廃止 (Bundle b PR-2) | polling 完全排除、固定値 wakeup 化 | [todo5.md](todo5.md) |

### 🛠 MEDIUM IMPACT — 信頼性 / silent loss 防止

| 順位 | Tier | タスク概要 | 効果 | 作業詳細 |
|---|---|---|---|---|
| 46 | 🔧 Tier 2 | CodeRabbit rate-limit auto-retry の integration test (Bundle a Sub-PR 2) | rate-limit auto-retry の信頼性確保 | [todo4.md](todo4.md) |
| 49 | 🔧 Tier 2 | `parse_findings` 系の error-path test infrastructure (Bundle a Sub-PR 2) | silent fail (`unwrap_or_else(\|_\| empty)`) を抑止、cli-pr-monitor mock infra も流用 | [todo5.md](todo5.md) |
| 55 | 💎 Tier 3 | config 拡張 + SessionStart catch-up (Bundle b PR-3) | AI 不在時の silent loss 防止、固定値 (`monitor.toml` 化) で調整可能 | [todo5.md](todo5.md) |
| 15 | 🔧 Tier 2 | cli-pr-monitor 通知 Recovery 経路 (SessionStart hook 拡張) ★ silent loss prevention | ADR-030 L2 recovery パターンを cli-pr-monitor に適用 | [todo3.md](todo3.md) |
| 11 | 🔧 Tier 2 | cli-pr-monitor プロセス正常終了の integration test (PR #85 T2-2) | プロセス挙動の信頼性確保 (auxiliary) | [todo2.md](todo2.md) |

### 📋 LOW IMPACT — ドキュメント整備

| 順位 | Tier | タスク概要 | 作業詳細 |
|---|---|---|---|
| 43 | 💎 Tier 3 | ADR-018 / ADR-009 の rate-limit retry ポリシー明文化 (Bundle a Sub-PR 2) | [todo4.md](todo4.md) |

---

## 推奨 bundle

### Bundle "CR auto-monitoring core" (Bundle b、最優先)

ユーザー目的「rate-limit 待ち時間に手動介入なし」の最短達成パス。

| 含む順位 | 概要 | 工数 |
|---|---|---|
| 53 | 中核: rate-limit retry の CronCreate 化 (PR-1) | M |
| 54 | review 完了待ちの CronCreate 化 + observer 廃止 (PR-2) | M |
| 55 | config 拡張 + SessionStart catch-up (PR-3) | S |

**依存関係**: 54 は 53 land 後、55 は 53 + 54 land 後。PR を 3 本に分けて順次 land 推奨。

**期待効果**:

- 順位 53 land 後: 47 分 rate-limit でも auto-retry が動作 (= ユーザー手動介入なしに review が再開)
- 順位 54 land 後: polling 排除で Claude Code セッション稼働中の overhead 削減
- 順位 55 land 後: AI 離席時の silent loss 解消 (起動時に pending wakeup を catch-up)

### Bundle "CR rate-limit auto-retry robustness" (Bundle a Sub-PR 2、補完)

| 含む順位 | 概要 | 工数 |
|---|---|---|
| 42 | auto-retry + `@coderabbitai review` auto-trigger 実装 | M |
| 43 | ADR-018 / ADR-009 明文化 | S |
| 46 | integration test | M |
| 49 | `parse_findings` error-path test infra | M |

**依存関係**: Sub-PR 1 (順位 44/45 = PR #100/#101 で land 済) の `check-ci-coderabbit --list-findings` API を消費。1 PR で 4 順位を統合する設計 (cli-pr-monitor mock infra を 4 順位で共有)。

**期待効果**: 既存 grep ベース parse から構造化 findings 駆動の auto-retry へ進化。`parse_findings` の silent fail を test 化することで、rate-limit 検出漏れによる手動介入リスクを抑止。

### 並行進行可能性

- **Bundle b** と **Bundle a Sub-PR 2** は別領域 (Cron 機構 vs structured findings 消費)、**並行進行可**
- ただし dogfood 観測の signal 純度を保つため 1 PR ずつ land を推奨

### 推奨実行順序 (ユーザー目的の最短達成)

1. **順位 53** (Bundle b PR-1): 致命点解消、即座に user benefit
2. **順位 42 + 43 + 46 + 49** (Bundle a Sub-PR 2): auto-retry の信頼性 + 構造化 findings 駆動化
3. **順位 54** (Bundle b PR-2): polling 排除
4. **順位 55** (Bundle b PR-3): silent loss 解消、polish

並行可: 1 と 2 は別領域なので、片方の dogfood 中にもう一方を着手できる。

---

## 関連ドキュメント

- [docs/todo.md](todo.md) — 推奨実行順序サマリ表 (priority table)
- [ADR-035: docs-only PR 評価ポリシー](adr/adr-035-doc-evaluation-policy.md) — docs PR iteration 改善の核 (旧 docs-pr-iteration-efficiency.md は retire 済、本 ADR に集約)
- [ADR-036: Bundle Z 3 層アーキテクチャ](adr/adr-036-bundle-z-three-layer-review.md) — pre-push-review 改善 (旧 pipeline-token-efficiency.md #B セクション)
- [ADR-037: takt fix-trust shortcut](adr/adr-037-takt-fix-trust-shortcut.md) — post-pr-review 短縮 (旧 #C-2)
- [ADR-009: Post-PR Monitor — push/PR作成後の CI・CodeRabbit 自動監視](adr/adr-009-post-pr-monitor.md)
- [ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止](adr/adr-018-pr-monitor-takt-migration.md) — Bundle b で再導入する CronCreate の設計根拠 (廃止判断と整合)
- [ADR-019: CodeRabbit レビュー運用のハイブリッド構成](adr/adr-019-coderabbit-review-hybrid-policy.md) — CodeRabbit 運用根拠
- [ADR-034: CodeRabbit 監視・対話の自動化戦略 — Bundle a 設計根拠](adr/adr-034-coderabbit-auto-monitoring.md) — Bundle a の設計根拠
