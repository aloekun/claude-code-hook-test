# ADR-055: 発火テレメトリ収集層 — ハーネス ROI 棚卸しの決定論的観測基盤

## ステータス

試験運用 (2026-07-15)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。
>
> 採番は land 時に確定する ([順位 135/140](../todo-summary.md) の placeholder 方式)。
> 現時点で ADR-054 が最新のため 055 を仮採番している。

## コンテキスト

`harness-improvement-plan.md` の WP-12 は「ハーネス複雑度 (hooks 7 本・custom rule 12 本・
pre-tool preset 群) の維持判断を発火実績で機械化する」ことを目的とする。ルールや preset は
「実 incident 由来で追加された」履歴 (ADR-049) は追えるが、**追加後に実際に発火しているか**
の観測データが無いため、不要になった機構の削除判断を人間の記憶に依存していた。

WP-12 は 3 ステップ構成である:

1. **収集層 (本 ADR)** — 各 hook の block/warn 発火を JSONL に append する共通基盤
2. **ROI 棚卸し pre-step** — 直近 N 日で発火 0 の rule/preset/hook を削除候補として提示
3. **卒業/廃止判定の機械化** — ADR-039 の bounded lifetime 判定を発火数で機械化

telemetry はマージ後に初めてデータが溜まるため、収集層だけを先行してマージし、28 日の
warm-up 後に実データで棚卸し (step 2/3) を後続 PR で行う。本 ADR / PR のスコープは step 1 に
限定する。step 2/3 は [todo-summary.md](../todo-summary.md) に登録した。

## 決定

新規共有ライブラリ `lib-telemetry` (`src/lib-telemetry/`、ADR-012 の `lib-` prefix) を作成し、
各 hook から `lib_telemetry::record(&Firing { .. })` を呼んで発火イベントを
`.claude/telemetry/firings-<YYYY-MM-DD>-<pid>.jsonl` に 1 行 append する。

### JSONL レコード (メタデータのみ)

各行は 1 発火イベントで、以下のフィールドを持つ:

| フィールド | 内容 |
|---|---|
| `ts` | UTC ISO 8601 timestamp |
| `hook` | 発火した hook 名 (例 `hooks-pre-tool-validate`) |
| `kind` | `rule` / `preset` / `hook` (発火主体の種類) |
| `id` | rule id / preset 名 / hook 名 |
| `decision` | `block` / `warn` (発火の重み) |
| `session_id` | 相関用 (任意、`.claude/.session-id` から補完) |

**プライバシー**: 記録はメタデータのみとし、**ファイルパス・編集内容・コマンド本文は記録
しない**。custom rule ② no-personal-paths (PII パス混入禁止) と同じ思想で、ローカル運用
データであっても個人情報を残さない。

### 計装スコープ — 裁量発火に限定

記録対象は「削除候補になり得る裁量的な発火」に限定する:

| 対象 | hook | kind | decision |
|---|---|---|---|
| custom rule 12 本 | hooks-post-tool-linter | rule | error→block / warning→warn |
| pre-tool preset 群 | hooks-pre-tool-validate | preset | block |
| Stop 品質ゲート | hooks-stop-quality | hook | block |
| tool call leak 検知 | hooks-stop-tool-call-leak | hook | block |
| file-length gate | hooks-post-tool-comment-lint-rust | hook | block |
| jj operation 未記録警告 | hooks-post-tool-jj-op-verify | hook | warn |

**除外**したもの:

- **常時 ON の構造チェック** (comment-lint-rust の非 doc コメント / 関数長、post-tool-linter
  の file_size_check / utf8_integrity)。これらは編集のたびに発火するコア機構で削除候補に
  ならず、記録すると ROI 信号 (「発火 0 = 削除候補」) を希釈するノイズになるため。
- **nudge-only hook** (session-start reminder / stop-feedback-dispatch /
  user-prompt-feedback-recovery)。decision 語彙が block/warn の 2 値のため、nudge (助言
  出力) は乗らない。将来 decision 語彙を拡張する際に再検討する。

`decision` は「hook がツールを実際に停止したか」ではなく「発火の重み」を表す軸である。
custom rule / jj-op-verify は additionalContext の助言層で実際には block しないが、severity
に応じて block/warn を記録する。逆に stop-quality は infra エラー (stdin/parse 失敗) の
fail-closed 経路でも block を emit するため、「hook が block を emit した総数」として記録
する。file-length gate の fail-closed 経路 (jj 失敗の判定不能 block) は ROI 信号を汚さない
よう記録しない。

### 副作用注入によるテスト可能性

[ADR-024 (共通 jj ヘルパー)](adr-024-shared-jj-helpers-library.md) の
`acquire_pipeline_lock_at` と同思想で、純粋 writer `record_to(base_dir, firing, now_epoch)` に
base_dir と now を引数注入し、テストが temp dir へ確定的に書けるようにする。prod 入口
`record()` は exe 隣の `.claude/` を解決し、opt-in 判定を `OnceLock` で 1 プロセス 1 回に
キャッシュしてから `record_to` を呼ぶ。

### Windows 並行書き込み安全性

hook は並行実行され得るため、書き込み競合を 3 重で排除する:

1. **per-process partition** (ファイル名に pid) — プロセス間で別ファイルに書く
2. **日次 partition** (ファイル名に日付) — 集計は `firings-*.jsonl` を glob 走査する前提
3. **プロセス内 `Mutex` + 単一 `write_all`** — 同一プロセス内マルチスレッドの行
   インターリーブを排除

## ADR-039 3 点セット

### Config opt-in (default OFF)

`hooks-config.toml` の `[telemetry]` section:

```toml
[telemetry]
enabled = true   # code default は false (unwrap_or(false))
```

section 不在 / `enabled` 未設定 / `false` では完全 skip (何も記録しない)。本リポジトリは
dogfood のため `enabled = true`。派生プロジェクトへの deploy 時は section 省略で OFF を継承
する (`pnpm deploy:hooks` の配布先で意図せぬ有効化を避ける)。

### Kill-switch

| 停止手段 | 影響範囲 |
|---|---|
| `enabled = false` (or section 削除) | 恒久停止。一切記録しない |
| env `CLAUDE_TELEMETRY_DISABLE=1` (truthy 値) | 緊急バイパス。受理集合 `1｜true｜yes｜on` |

### Bounded lifetime

収集層単体では価値が出ず、step 2/3 とセットで初めて ROI 棚卸しが成立する。明示的な
decision trigger:

- **収集開始から 28 日**の warm-up 後、WP-12 step 2 (集計 pre-step) を実装して
  「発火 0 の rule/preset/hook」を削除候補として週次レビュー ([ADR-031](adr-031-weekly-review-pipeline.md))
  に出力する。incident 由来ルール (ADR-049) は抑止力として発火 0 でも維持推奨とし、
  非 incident のみ削除候補にする区別を step 2 で入れる。
- step 3 で ADR-039 の卒業/廃止判定 (試験運用機能の採否) を発火数で機械化する。
- 上記が実装されず telemetry が死蔵する場合は、収集層ごと撤去する revert PR を作成する。

## 帰結

### 利点

- ルール/preset/hook の維持・削除判断を、人間の記憶ではなく発火実績で機械化する基盤ができる
- 記録が決定論的 (LLM 不使用) で高速 (数十バイトの append)、fail-open のため hook 本来の
  判定を一切妨げない
- 計装が各 hook の choke point (emit_block / run_custom_rules の per-rule / validate_command
  の hit) に 1 行 record を差すだけで、opt-in 判定は lib 内部に集約され侵襲が小さい

### 欠点 / 留意点

- 「発火 0」は「不要」と「抑止力として機能 (違反が起きなかった)」の区別がつかない。削除
  候補の最終判断は人間に委ね、incident 由来ルールは発火 0 でも維持推奨とする (step 2 で区別)
- 本 PR 単体ではデータが溜まらないため、initial run は必ず「観測期間中・全維持」になる
  (28 日 warm-up 後に step 2 で初めて棚卸しが機能する)
- `fail-open` の観点は [ADR-043 (fail-closed 原則)](adr-043-security-gates-fail-closed.md) の
  適用対象外である。ADR-043 の fail-closed は「block/allow を決めるゲート関数」限定であり、
  telemetry は observation 層でゲートではないため、記録失敗で hook を止めない fail-open が
  正しい。stop-tool-call-leak (ADR-053) が既に「ゲートでない UX 装置は fail-open」の先例
- UTC ヘルパーを `lib-pending-file` から最小複製した。観測層が post-merge-feedback ドメイン
  特化 crate に依存して責務結合するのを避けるため意図的に複製したが、これで UTC ヘルパーの
  消費者が 2 crate 目に到達した。[ADR-044 (utility extraction 境界)](adr-044-subprocess-utility-extraction-boundary.md)
  層 1 の「2 つ目の使用例待ち」トリガに到達したため、将来 3 crate 目が現れたら中立 crate
  (例 `lib-time`) への抽出候補とする

## 関連 ADR

- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン (opt-in / kill-switch / bounded lifetime)
- [ADR-043](adr-043-security-gates-fail-closed.md) — fail-closed 原則 (本 telemetry は observation 層で適用外 = fail-open)
- [ADR-044](adr-044-subprocess-utility-extraction-boundary.md) — utility extraction 境界 (UTC ヘルパー抽出トリガ到達)
- [ADR-031](adr-031-weekly-review-pipeline.md) — 週次レビューパイプライン (step 2 の棚卸し出力先)
- [ADR-049](adr-049-incident-eval-regression-suite.md) — incident→eval 回帰スイート (発火 0 でも維持する incident 由来ルールの区別)
- [ADR-012](adr-012-src-naming-convention.md) — src/ 命名規約 (`lib-` prefix)
- [ADR-026](adr-026-cargo-workspace.md) — Cargo workspace (新 crate の members 追記)
- [ADR-041](adr-041-test-isolation-patterns.md) — テスト隔離 (env kill-switch テストの serial 化)
