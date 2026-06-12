# ADR-039: Experimental feature 標準パターン (config opt-in + kill-switch + bounded lifetime)

## ステータス

試験運用 (2026-05-10)

## コンテキスト

本プロジェクトでは試験運用 ADR が systemic に蓄積している (本表は **lineage = 過去の試験運用 ADR の網羅列挙**、本 ADR を land する際の遡及 cross-link は **本 ADR では行わない** = §帰結 「欠点 / 留意点」参照):

| ADR | 試験対象 | 開始 |
|---|---|---|
| [ADR-014](adr-014-post-merge-feedback.md) | post-merge-feedback ループ | 2026-04-22 |
| [ADR-023](adr-023-coderabbit-reject-thread-skill.md) | CodeRabbit reject thread skill | — |
| [ADR-025](adr-025-cwd-restore-drop-guard.md) | CwdRestore Drop guard | — |
| [ADR-029](adr-029-post-merge-feedback-auto-trigger.md) | Post-Merge Feedback 自動起動 | — |
| [ADR-030](adr-030-deterministic-post-merge-feedback.md) | takt 経由の同期実行 | — |
| [ADR-031](adr-031-weekly-review-pipeline.md) | 週次プロジェクト全体レビュー | 2026-04-27 |
| [ADR-033](adr-033-todo-numbering-simplification.md) | todo 採番管理の簡素化 | — |
| [ADR-034](adr-034-coderabbit-auto-monitoring.md) | CodeRabbit 監視自動化 | — |
| [ADR-036](adr-036-bundle-z-three-layer-review.md) | Bundle Z 3 層 review | — |
| [ADR-037](adr-037-takt-fix-trust-shortcut.md) | takt fix-trust shortcut | — |
| [ADR-038](adr-038-local-llm-finding-classification.md) | ローカル LLM finding classification | 2026-05-06 |

> **本 PR で back-link を追加した範囲**: 上記 11 ADR のうち、**ADR-031 / ADR-036 / ADR-038** の 3 件のみに本 ADR への blockquote 参照を冒頭に追加した (**§ 既存試験運用 ADR で観測される共通パターン** で適合状況を分析した 3 ADR)。残り 8 ADR への遡及更新は **後続 PR で個別追補** とする (§ 帰結 / 欠点 参照)。

各試験運用 ADR は個別判断で導入されてきたが、PR #123 (ADR-038 Phase 5: P-0 classifier opt-in + §10 ブランチ分離運用) の post-merge-feedback で、**3 点セット** (config opt-in / kill-switch / bounded lifetime) が systemic に反復していることが確認された (Tier 3 #1 採用)。

### 既存試験運用 ADR で観測される共通パターン

| 観点 | ADR-031 | ADR-036 | ADR-038 |
|---|---|---|---|
| **Config opt-in** | 週次トリガはデフォルト disabled | gate を flag で制御 | `[lint_screen] enabled = false` (default OFF) |
| **Kill-switch** | レビューパイプ停止可能 | gate 経路を revert で停止 | revert PR で `enabled = false` |
| **Bounded lifetime** | 「採用判定で本採用に昇格」 | dogfood 完了で判定 | ADR-038 採用昇格 = 2026-05-15 (Phase D 6 PR / 9 data points で採用条件充足) |

3 点とも個別 ADR で都度設計されてきたが、**新規試験運用 ADR を策定するたびに同じ判断を再発明している**。

## 決定

試験運用 feature を導入する際の **標準パターン** として 3 点セットを以下の通り規定する。新規試験運用 ADR は本 ADR を **参照** し、3 点を満たすことを default とする。

### 1. Config opt-in (デフォルト無効) — 適用対象を明示

本 § は **behavior の妥当性が不確定な** experimental feature に適用する。具体的には:

- 挙動が後の dogfood で「失敗 / 却下 / 方向転換」されうるもの
- false positive 発生時に多数 user / session に影響するもの
- 採否判定 (採用 / 却下 / 継続) のフェーズが必要なもの

該当する場合:

- 設定ファイル (`*.toml`) または env var で `enabled = false` をデフォルトとする
- 明示有効化 (`enabled = true`) で feature 発動
- env var / config 値での切り替えを必ず提供 (config-only より env override 可能な方が望ましい)
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc 等) への deploy 時にも default OFF が継承されるよう、`[feature]` section の追加を必須化

### 1.b 適用対象外: 決定論的 mechanical lint (default ON 許容、PR #203 post-merge-feedback 由来)

以下条件をすべて満たす機能は § 1 (default OFF) の対象外とし、**default ON で配布してよい**:

1. **失敗 mode が non-blocking**: block ではなく additionalContext / warning のみ (ユーザー操作を妨げない)
2. **判定が決定論的**: 閾値 (例: 50KB) / 文字列 match (例: regex) / metadata 演算で discretionary 判断を含まない
3. **影響範囲が宣言的に限定**: scope filter (`paths` glob / extension match 等) で適用箇所が config or const で限定済み
4. **recovery hint が明確**: 違反検出時に「次にやるべきこと」が message に含まれる

該当する例 (本リポジトリで既に default ON 稼働中):

- **順位 147 file_length lint** (`hooks-post-tool-comment-lint-rust`、Rust source 800 行 max): `const MAX_FILE_LINES = 800` で固定、config 不在 = ON 固定
- **順位 177 file_size_check** (`hooks-post-tool-linter` § Layer 0.5、metadata-only 50KB threshold): touch-trigger ratchet で grandfather + paths glob で scope 限定

該当**しない**例 (default OFF が正しい):

- post-merge-feedback (ADR-014/030): 挙動が dogfood で確定する experimental
- weekly-review (ADR-031): 採否判定要、reminder 頻度や observation rubric が dogfood で進化
- local-llm-finding-classification (ADR-038): classification 精度が dogfood で判定

**過去の誤適用**: PR #197 で順位 177 file_size_check を ADR-039 § 1 機械適用で default OFF にしたが、本 PR (PR #203 post-merge-feedback 由来) で「決定論的 mechanical lint = § 1.b 例外で default ON」へ訂正。順位 147 と同様の扱いに統一。

### 2. Kill-switch (停止経路の事前明文化)

- revert PR で `enabled = false` に戻す経路を **PR body / ADR で明文化**
- crate / module の物理削除は **dogfood 失敗判定後にまとめて実施**。途中段階での部分削除は機械的損傷の risk が高い
- ADR-038 §10.6 の C 案 (採用 / 簡易版 / 完全版の階層化) が良いテンプレ
- kill-switch 経路の table を ADR / PR body に必ず含める (項目: 起動経路 / 停止コマンド / 影響範囲)
- **診断メッセージは実装の受理値を網羅する**: kill-switch 発動時 (SKIP / 停止) の診断メッセージは、判定関数 (`is_*_value` 等) が受理する全 value variant を反映する。判定ロジックが複数値 (`1` / `true` / `TRUE` / `True` 等) を受理するのに固定文字列 (例: `"{}=1 detected"`) で 1 値のみ表記すると spec-impl drift となり、user が受理値を誤解する診断 UX 低下を招く。(a) 全受理値を列挙 (例: `"{}=1 (or =true) detected"`) するか、(b) 実際の env var 値を動的取得して表示 (例: `format!("{}={} detected", env_name, raw_value)`) のいずれかを採用する。本 ADR はテンプレートとして参照されるため、原則の欠落は全 experimental feature に波及する (PR #179 で `CLI_DOCS_LINT_DISABLE` の kill-switch message が `=1` 固定で実受理値 `true` / `TRUE` / `True` を反映しなかった spec-impl drift を pre-push simplicity reviewer が指摘した実例)

### 3. Bounded lifetime (試験期限と採否判定基準)

- 試験期限を **ADR 冒頭** または **計画書冒頭** に明記
  - 例: 「6 ヶ月経過しても採用判定未達なら却下とみなす」
  - 例: 「3-5 PR で dogfood 後に採否判定」(ADR-038 / Phase d)
- retirement workflow (`~/.claude/rules/common/docs-governance.md`) との接続を明示
  - **採用**: 試験運用 → 本採用に昇格 (新規 ADR 不要、本 ADR の status 更新)
  - **却下**: revert PR で feature 削除 + 本 ADR を「却下」に更新 + 計画書 (`docs/<topic>-analysis.md`) を retirement workflow で削除
  - **継続**: 期限内に判定が出ない場合、計画書側に新たな期限と判定基準を記述 (1 回まで延長可)
- bounded lifetime を欠いた試験運用は「永遠の試験運用」化し、累積複雑度の温床になる

#### 明示的 decision trigger の必須化 (PR #174 採用、Bundle 1 実例)

試験期限は「期限 OR 条件 OR PR 数」のいずれかで **明示的 decision trigger 化** する。任意の "未来の判定" (= 形式不明の「いずれ判断する」記述) は禁止し、reviewer / 後継 Claude session が判定タイミングを一意に決定できる構造にする:

- **PR 数ベース**: 「N-M PR の dogfood 後に判定」 (例: PR #174 `scratch_file_warning` = 「3-5 PR の dogfood 後に default-ON 昇格 or 却下を判定」)
- **期限ベース**: 「YYYY-MM-DD 経過で却下とみなす」「6 ヶ月経過しても採用判定未達なら却下」
- **条件ベース**: 「false positive 5 件以上で却下」「latency p95 が X ms 超で却下」

decision trigger は **config (TOML コメント) / code comment (module doc) / PR body** のいずれかで永続記録する。ephemeral 計画書 (`docs/<topic>-analysis.md` 等) だけに記述すると retire 時に dead pointer 化するため不可 ([coding-style.md § Cross-File Reference Lifecycle](../../CLAUDE.md) 参照)。

参考実装: PR #174 では `push-runner-config.toml` の `[scratch_file_warning]` section コメント + `scratch_file_warning.rs` module doc の 2 箇所で trigger を明示 (永続性確保 + 検索容易性)。

既存試験運用 ADR (014/023/025/029/030/031/033/034/036/037/038) の bounded lifetime 記述に formless な箇所があれば、後続 PR で個別に追補する。

### 新規 experimental feature 追加時の self-review checklist

新規 feature を追加する PR では、push 前 self-review で以下 5 点 (上位 1 件 + mechanical 4 件) の整合を確認する。**上位判定で「§ 1.b 例外」に該当した場合は 4 点 checklist を skip し、default ON で配布**する:

#### 0. 上位判定: そもそも § 1 適用対象か? (PR #203 post-merge-feedback 由来)

§ 1.b の 4 条件 (non-blocking / 決定論 / scope 限定 / recovery hint 明確) をすべて満たすか self-check する:

- **すべて満たす** → § 1.b 例外、default ON で配布 (順位 147 file_length lint / 順位 177 file_size_check と同 pattern)
- **1 つでも欠ける** → § 1 適用、以下 4 点 checklist を実施

判断に迷う場合は 4 点 checklist を実施する側 (default OFF) を選択 (= conservative default)。本判定を skip して機械的に 4 点 checklist を実施すると、決定論的 mechanical lint を誤って opt-in 化する over-application が発生する (PR #197 順位 177 で実観測、PR #203 で訂正)。

#### 1-4. § 1 適用時の mechanical 4 点 (config / code / docs / test)

各点は discretionary 判断を含まず、config / code / docs / test の差分を機械的に照合できる:

1. **config schema**: 該当 hook / module の config struct (例: `WeeklyReviewReminderConfig`) が `enabled: Option<bool>` field を持つ
2. **feature flag default OFF**: 該当 config の `enabled` の default が **OFF** (= `unwrap_or(false)`) になっている。`unwrap_or(true)` は § 決定 1 (Config opt-in) 違反
3. **docs / config example**: `.claude/hooks-config.toml` 等の repo config example で `enabled = false` を明示し、enable 時の挙動をコメントで添える (= opt-in 運用 guidance)
4. **test coverage**: `enabled = false` (disabled state) の test case があり、feature が完全 skip されることを assert する (= kill-switch が機能することの regression gate)

実例 (PR #184 CR Major M-2 = `weekly_review_reminder` の `enabled = true` が opt-in 契約違反として CR re-review で検出された事例):

- **NG** (fix 前): `enabled = true` で (2) と (3) が違反、CR Major finding で検出
- **OK** (fix 後): `WeeklyReviewReminderConfig::enabled` Option + `unwrap_or(false)` + `.claude/hooks-config.toml` で `enabled = false` + `compute_weekly_review_reminder_nudge_returns_none_when_disabled` test

本 checklist は **新規 feature 追加時** の self-review 手順であり、既存 grandfathered case (例: `[session_start.staleness]` の pre-existing な `enabled = true`) の retro-cleanup は scope 外 (別 PR で個別判断)。

### 設計段階 pre-check: config struct 設計時の 6 点 (PR #194 T3-#1 採用、2026-06-04)

PR #194 で `SweepConfig` の初版が 3 点セット (config opt-in / kill-switch / bounded lifetime) のうち kill-switch + bounded lifetime の **設計時考慮** を欠いた状態で実装され、CodeRabbit Major #4 で指摘 → takt-fix で `enabled = false` default + config-driven gate を追加して修正された。前 section の self-review checklist (4 点) は code 完成後の整合確認だが、本 section は **config struct を書く前** に確認する設計段階チェックリスト。両者の関係は「設計時 6 点 (本 section)」→「実装後 4 点 (前 section)」の sequential gate。

新規 experimental feature の config struct を書く前に以下 6 点を確認する:

1. **`enabled: bool` field の存在**: feature 有効化フラグ。`#[serde(default)]` で default = false に明示。型は `Option<bool>` か `bool` のどちらでも可だが、`Option` は「未指定 = OFF」の意図を明示できて self-review 4 点目との整合が取りやすい
2. **`Default` impl の明示**: `Default::default()` で `enabled = false` が確実に出ることを `impl Default` で書く。`#[derive(Default)]` だと bool default が false なので結果は同じだが、`impl Default` の方が後の field 追加時に明示性が保たれる
3. **kill-switch 経路**: 即時停止が必要なとき、(a) config の `enabled = false` toggle で停止できるか、(b) feature を呼び出す上位 module で early-return できるか、(c) 別 process (daemon 等) なら kill signal で停止できるか — のいずれかを ADR / PR body で **明文化**。新規 config field (`kill_switch: bool`) を追加する代わりに既存 `enabled = false` toggle を kill-switch として併用する場合は、その明示が必要
4. **bounded lifetime decision trigger**: 「N PR 後 / YYYY-MM-DD / 条件 X」のいずれかで採否判定タイミングを明文化。形式不明の「いずれ判断する」は不可 (§ 3 § "明示的 decision trigger の必須化" 参照)
5. **3 段 gate の単一箇所集約**: 実行経路で `config.enabled && !is_kill_switched() && !is_expired(&config)` のような 3 段 check を **単一関数** に集約。call site で 3 段をバラバラに書くと条件追加時に漏れる risk あり。SweepConfig の場合は call site が 1 箇所のみのため `if !config.enabled { return; }` で十分だが、複数 call site がある feature は `fn should_run(config: &Self) -> bool` 関数を生やす
6. **off-state integration test の事前計画**: `enabled = false` でのバイパス test を **config struct 実装と同 commit** で書く。後追いで test を書くと、disable path の絶縁が確認されずに 本採用昇格 PR で初めて気づく risk あり

実例 (PR #194 SweepConfig):

- **NG** (初版): `enabled` field 不在 → 常時 run → CodeRabbit Major #4 指摘
- **OK** (takt-fix 後): `pub(crate) enabled: bool` with `#[serde(default)]` + `impl Default { enabled: false }` + `if !config.enabled { return; }` 単一 gate + integration test (本 PR 同梱の `integration_sweep_*` 系で `enabled = false` skip を assert する追加 test は PR #194 T2-#2 で完了)

本 6 点は **設計段階の** 確認手順であり、code 完成後は前 section の 4 点 self-review に進む。両 section が「設計 → 実装」の 2 段 gate を成す。

## 帰結

### 利点

- 新規試験運用 ADR の判断が標準化され、設計議論の重複が削減される
- kill-switch 経路の事前明文化で、dogfood 失敗時のロールバックが decision-free に進む
- bounded lifetime で試験運用の「忘却された負債化」を防ぐ

### 欠点 / 留意点

- 既存試験運用 ADR (014/023/025/029/030/031/033/034/036/037/038) の 3 点セット適合状況は再評価対象。本 ADR の land 後、各 ADR の reflect は **後続 PR での追補** として進める (本 ADR では遡及更新しない)
- 本 ADR 自体も試験運用扱い: 3-5 個の新規試験運用 ADR で本パターンを適用し、適合率と運用負荷を確認後に本採用に昇格する

### 想定される運用

新規試験運用 ADR (例: 仮称 ADR-040) を策定する際は ADR 冒頭近くに以下を記載:

```markdown
## ステータス

試験運用 (YYYY-MM-DD)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。
```

PR body にも kill-switch table を含める (起動経路 / 停止コマンド / 影響範囲)。

## 関連

- [ADR-031](adr-031-weekly-review-pipeline.md) — 承認済み (2026-06-01 本採用昇格)。§ 採用判定の閾値 (本採用化条件) が本 ADR § 3 (Bounded lifetime) の「採用 / 却下 / 継続」3 値判定基準の具体化例として参照可能 (5 閾値: 採用率 / wall-clock / FP / context 圧迫 / systemic 検出力)
- [ADR-036](adr-036-bundle-z-three-layer-review.md) — 試験運用、3 点セット部分適合
- [ADR-038](adr-038-local-llm-finding-classification.md) — 試験運用、3 点セット完全適合 (本 ADR の trigger 事例)
- `~/.claude/rules/common/docs-governance.md` — Document Lifecycle Classification / Retirement Workflow
- `~/.claude/CLAUDE.md` — グローバルルール index
