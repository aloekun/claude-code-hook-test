# 月次ハーネス ROI レビュー (WP-12 step 2/3) 実装プラン

**本ドキュメントは実装作業の指示書である。** 実作業は本ドキュメントの内容のみを見て実施できる
ように書かれている。**最終目標は Phase 4**: 本プランの決定事項が関連 ADR に漏れなく記載されて
いることを確認した上で、**本ファイル自身を削除**することをもって作業完了とする。

## 完了条件

1. Phase 0〜3 の実装・検証がすべて完了している (PR 分割は各 Phase 参照)
2. 新規 ADR-062 (`docs/adr/adr-062-monthly-harness-roi-review.md` として新規作成) を作成済み
   (試験運用、ADR-039 3 点セット付き)
3. ADR-055 / ADR-053 / ADR-061 への追記、CLAUDE.md index、todo 整理が完了している
4. **Phase 4**: 本ドキュメントの「設計決定」「ユーザー決定事項」の各項目が ADR 群に記載済みで
   あることを 1 項目ずつ照合確認し、漏れがあれば ADR を補完した後、**本ドキュメントを削除**
5. push / PR 作成は通常フロー (`pnpm push` → prepare-pr、PR 作成は ADR-028 ゲートでユーザー承認)

## 背景 (なぜ作るか)

### 動機となった問題

[ADR-053](adr/adr-053-stop-tool-call-leak-detection.md) / [ADR-061](adr/adr-061-tool-call-leak-hardfail-recovery.md)
の tool call leak 検知は上流 (Claude Code) 不具合への時限的な防御層であり、上流修正後も検出を
続けると **Stop hooks の実行時間が塵積し開発イテレーションを遅くする** (leak hook 単体で実測
83〜829ms/Stop)。ADR-053/061 の bounded lifetime は「leak 4 週間非観測で撤去判定」と定めるが、
この判定を機械的に promote する仕組みが無く、人間の記憶に依存している。

**判定基準の関係 (新旧の統一)**: 本プランは撤去判定の正式基準を「連続 2 か月発火 0」
(ユーザー決定事項 1) に置き、ADR-053/061 の「4 週間非観測」記述は Phase 3 の追記で
「月次レビュー (ADR-062) の判定基準に従う」へ更新して新旧基準を併存させない
(2 か月は 4 週間より保守的な置き換えであり、判定主体が人間の記憶から月次レビューへ移る)。

### 既存計画との合流 (重要)

この問題は [ADR-055](adr/adr-055-firing-telemetry-collection.md) の WP-12 が既に計画していた
step 2/3 と同一である。**月次レビューを WP-12 step 2/3 の実装ビークルとし、leak 検出の
非アクティブ化を最初の判定ユースケースにする** (ユーザー承認済み方針)。本プランで以下の既存
todo を消化する:

| todo | 所在 | 内容 | 本プランでの扱い |
|---|---|---|---|
| 順位 307 | docs/todo-summary2.md:70 + docs/todo16.md「WP-12 step 2」 | ROI 棚卸し pre-step (発火 0 の rule/preset/hook を削除候補提示) | Phase 1 の集計 exe。**出力先を週次→月次に変更** (ADR-055 amendment 要) |
| 順位 308 | docs/todo-summary2.md:71 + docs/todo16.md「WP-12 step 3」 | ADR-039 bounded lifetime 判定の発火数機械化 | Phase 1 の「判定候補」機構 (マッピング + 連続 0 判定) として MVP 実装 |
| 順位 309 | docs/todo-summary2.md:72 + docs/todo16.md 該当 entry | telemetry block 記録への infra エラー混入除去 (3 hook 横断) | **Phase 0 として先行** (ユーザー指示によりスコープ内) |
| 順位 312 | docs/todo-summary2.md:75 | `.claude/telemetry/` の retention/cleanup | Phase 1 の月次 rollup + raw 削除として相乗り |

実装前に todo16.md の各 entry を必ず Read して詳細仕様 (特に順位 309 の対象 hook と分割 PR
推奨) を確認すること。

### 調査済みの事実 (2026-07-29 実測、再調査不要)

- テレメトリ (`.claude/telemetry/firings-*.jsonl`、ADR-055) は **workspace ローカル** (exe 隣の
  `.claude/` に書かれる)。実測: main workspace = 07-16〜 528 行で **leak 発火 0**、improve
  workspace (`C:\Users\owner\work\claude-code-hook-test-improve`、同一 repo の jj workspace) =
  07-20〜 200 行で **`hooks-stop-tool-call-leak` の block 発火 11 件**。
  → **workspace 横断集計が必須** (leak 発火は improve に偏在)。
- 発火レコードの形: `{ts, hook, kind(rule|preset|hook), id, decision(block|warn), session_id?}`。
  観測済み id 例: `pr_monitor_catchup` / `weekly_review_reminder` / `staleness` (session-start
  nudge 群)、`git` / `default` / `jj-message-required` (preset)、`no-docs-relative-back-to-docs`
  等 (custom rule)、`jj-op-verify` / `hooks-stop-tool-call-leak` (hook)。
- ADR-061 の回収層は id `hooks-stop-tool-call-leak/prompt-recovery` (decision=warn) で記録される
  (hard-fail 経路の観測)。leak のトレンドは **block + この warn の合算と内訳**で見る。
- ファイルは per-day × per-pid partition で削除機構なし (main 464 ファイル/13 日 ≒ 36/日)。
- **warm-up 制約**: ADR-055 step 2 の着手条件 = 収集開始 (2026-07-15) から 28 日 =
  **2026-08-12**。実装は先行してよいが、初回の月次レビュー実行は 08-12 以降が有意義。
- 派生プロジェクト (別 repo) は telemetry default OFF (ADR-055) のため集計対象は実質
  main + jj workspaces。
- ADR 採番: 現在の最大は ADR-061 → 新規は **ADR-062**。

## ユーザー決定事項 (ヒアリング済み、変更しないこと)

1. **非アクティブ化の提案閾値 = 連続 2 か月発火 0** (config で変更可能なデフォルト値)。
   減少傾向は 1 か月目から参考情報として提示する。
2. **採用アクションはハイブリッド**: config 1〜2 行で済む軽量変更 (enabled=false 等) はレビュー
   セッション内でその場で通常 push/PR フローで実施。大型作業 (rule 削除・crate 撤去 revert PR
   等) は weekly-review と同型の todo 登録に回す。
3. **順位 309 をスコープに含める** (Phase 0 として先行)。

## 設計決定

1. **3 層構成 (ADR-030/031 パターンの 5 例目)**。ただし L2 は takt ではなく**決定論 Rust exe**。
   判断材料が数表であり AI 並列レビューの必然性が無い (YAGNI。必要になれば後から takt facet を
   追加できる)。よって weekly-review にあった `.failed` marker / resume 機構は**不採用**
   (L2 が高速・決定論のため。失敗時は skill がエラー報告するのみの best-effort)。
2. **L2: 新規 exe `cli-telemetry-report`** (`src/cli-telemetry-report/`、ADR-012 の `cli-`
   prefix):
   - **入力**: 各 root の `.claude/telemetry/firings-*.jsonl`。root 発見は `jj workspace list`
     の動的列挙 (パース失敗・workspace 未使用環境は現 root のみに fail-open) + config
     `extra_roots` で追加可。**root 発見が不完全な場合 (`jj workspace list` 失敗 /
     `extra_roots` の到達不能) はレポートに degraded を明示し、当該実行では判定候補の
     promote を抑止する** (集計・レポート生成は fail-open で継続するが、「発火 0」判定は
     完全な root 集合を前提とする。leak 発火が improve に偏在するため、発見漏れ + 発火 0 の
     組合せは誤 promote に直結する)。
   - **月次 rollup (順位 312)**: 月ごとの id 別集計を `.claude/telemetry/monthly-<YYYY-MM>.json`
     (main workspace 側) に永続化。raw daily ファイルは retention (config `retention_days`、
     **code default は未設定 = 削除無効**。ADR-039 opt-in。本 repo は hooks-config.toml で
     `retention_days = 90` を設定して dogfood) 超過分を削除。**複数月トレンドは rollup から
     読む**ため raw 削除後も判定可能。rollup は集計済み月を再集計しない (確定月は不変。
     当月は毎回再計算)。
   - **レポート出力** (`.claude/monthly-reviews/<YYYY-MM-DD>.md` + 機械可読 JSON):
     (a) 月別 × id 別カウント + 前月比、(b) 直近 N か月の発火 0 リスト、(c) **config enabled /
     exe 配備状態の snapshot** (「0 = 上流修正」と「0 = 無効化・未配備」の誤読防止。
     hooks-config.toml の該当 enabled 値と `.claude/*.exe` の存在を機械確認)、(d) incident 由来
     ルール (ADR-049) の「発火 0 でも維持推奨」マーク、(e) 判定候補。snapshot は集計実行時点の
     状態であり単体では月内の有効性を証明しないため、**月次 rollup 確定時に当月の snapshot を
     rollup JSON にも保存**し、判定はこの月別記録を参照する (次項)。
   - **判定候補 (step 3 MVP)**: 「機構 → 監視対象 id 群 → 成立時の提案」の静的マッピングを
     config (例: hooks-config.toml `[[telemetry_report.mechanisms]]`) に持ち、**連続
     `zero_streak_months` (既定 2) か月発火 0 で非アクティブ化候補として promote**。
     promote の成立条件には「対象の各月 rollup に enabled=true + 配備ありの snapshot 記録が
     あること」を含める (無効化・未配備の月を「発火 0」と誤読しない)。月中の一時無効化までは
     snapshot では検出できないが、最終判断が必ずユーザー採否 (AskUserQuestion) を経る前提で
     受容する (この限界は ADR-062 の留意点に明記すること)。
     初期マッピングは 1 件: ADR-053/061 (leak 検知) → ids
     [`hooks-stop-tool-call-leak`, `hooks-stop-tool-call-leak/prompt-recovery`] → 提案 =
     `[stop_tool_call_leak] enabled = false` + `prompt_recovery_enabled = false`
     (最終的な crate 撤去 revert PR は ADR-053/061 bounded lifetime の手順に従う)。
     全試験運用 ADR の網羅登録は将来拡張とし MVP はこの 1 件 + 発火 0 リスト全般で足りる。
3. **L1: SessionStart reminder**: `hooks-session-start` に `[session_start.monthly_review_reminder]`
   (enabled / threshold_days=28 / system_message_enabled)。実装は `weekly_review.rs` のパターンを
   踏襲し、以下の教訓を**必ず**適用する: (a) staleness は state file
   `.claude/monthly-review-last-run.json` の `last_run_at` **内容 timestamp** のみで判定し mtime
   に一切依存しない (ADR-031 の silent-fresh バグ教訓)、(b) state file の読み書きは
   `lib_jj_helpers::resolve_main_workspace_root` で **main workspace root に canonical 化**
   (ADR-045 分裂対策。hook 読み側と skill 書き側の両方)、(c) systemMessage は ADR-059 の
   opt-in + 1 行 (`lib_hook_output::SingleLineMessage` を使用)、(d) 発火は telemetry に
   id `monthly_review_reminder` / warn で計装 (ADR-055 amendment PR-N3 と同型)。
4. **L3: skill `/monthly-review`** (skills repo `$CLAUDE_SKILLS_REPO` = 別リポジトリでの作業 +
   `~/.claude/skills/` への deploy。weekly-review skill の構成を template にする):
   - Phase 1: 起動条件確認 → Phase 2: `pnpm telemetry-report` (exe) を同期実行 → Phase 3:
     レポート提示 + AskUserQuestion で判定候補・削除候補の採否 → Phase 4: **ハイブリッド実行**
     (軽量 config 変更は即時に通常 push/PR フロー、大型は docs/todo.md 登録) + last-run 更新。
   - **候補が 4 件を超える場合は AskUserQuestion を複数質問に分割**する (1 質問 4 option の
     制約。severity / 機構種別順にグループ化。ADR-031 Phase E dogfood で確立した weekly-review
     と同方式)。
   - 自動で無効化しない。採否は必ず AskUserQuestion を経る (ADR-022/028)。
5. **週次レビューとの役割分担**: 週次 = whole-tree コードレビュー (ADR-031)、月次 = telemetry/
   ROI 棚卸し。ADR-055 が step 2 の出力先を「週次レビュー facet」と想定していた点は amendment で
   月次に変更する (根拠: テレメトリ傾向は週次では変化が小さくノイズ、ADR-053 の撤去粒度
   「4 週間」と月次が一致)。

## 実装 Phase

### Phase 0 (PR-1): 順位 309 — telemetry block 記録の infra エラー除外 ✅ 完了 (PR #329, 2026-07-29)

**実施結果**: PR #329 でマージ済み。当初 todo は「3 hook 横断で分割 PR 推奨」としていたが、
コード精査の結果 infra エラーで実際に block を記録していたのは **stop-quality のみ**と判明
(leak は `run_check` の実 leak 検出時のみ、pre-tool-validate は `validate_command` hit 時のみ
record で既に実 violation 限定)。よって分割 PR は不要で単一 PR にまとめた。stop-quality に
`BlockCause { QualityViolation, InfraError }` を導入し、`emit_block(reason, cause)` が
`cause.records_firing()` (QualityViolation のみ) の場合だけ telemetry に記録するよう変更
(closure 注入した `emit_block_with` で回帰テスト)。CodeRabbit review の追加指摘に対応し、
worker thread panic を `QualityViolation` として誤計上しないよう `StepFailure` +
`step_failure_from_join` + `aggregate_block_cause` を追加 (panic のみ = `InfraError`、実失敗
混在 = `QualityViolation`)。ADR-055 § 計装スコープに amendment 追記済み。

当初の作業計画 (すべて完了):

- docs/todo16.md の順位 309 entry を Read し、対象 3 hook と仕様を確認して実装 (infra エラー
  経路 = stdin/parse 失敗等では block を記録しない。実 quality 違反パスに限定)。
- ADR-055 に amendment を併記 (「emit 総数」定義の訂正)。todo16.md / todo-summary2.md の
  該当 entry を消化・削除。
- 分割 PR 推奨の記載が todo にあるため、大きくなる場合は hook ごとに分割してよい。

### Phase 1 (PR-2): `cli-telemetry-report` exe ✅ 実装完了 (未 push、2026-07-29)

**実施結果**: 新規 crate `src/cli-telemetry-report/` を実装。モジュール構成は
`discover`(root 発見 + degraded 判定) / `aggregate`(月次集計・rollup 確定・retention) /
`snapshot`(config enabled + exe 配備) / `verdict`(判定候補) / `incident`(維持推奨ルール抽出) /
`report`(md + JSON) / `timekit` / `model` / `config` / `main`。48 unit テスト + 実データ実測が
通過し、`cargo test --workspace` / `cargo clippy --workspace --all-targets -- -D warnings` /
`pnpm lint:md` 全通。release ビルドを `.claude/` へ deploy 済み。**push / PR 作成は未実施**
(通常フロー・ADR-028 ゲート待ち)。

実装上の決定 (プラン未指定箇所、Phase 4 で ADR-062 へ反映):

- **incident 由来ルールの真実源** = `.claude/custom-lint-rules.toml` の `[rules.incident]`
  サブテーブル (id を exe 側に複製しない、ADR-049 思想と整合)。
- **degraded 判定の追加条件**: 「現 workspace 以外で root 未解決の workspace 数 > 到達可能な
  extra_roots 数」でも degraded とする。現 workspace 自身の `self.root()` 解決失敗は現 root
  (exe 隣接 `.claude` の親) で補うため degraded にしない。この環境の `ccht-improve` workspace は
  jj 格納パス不整合 (`../../../ccht-improve`、実体は `-improve`) で `self.root()` が解決不能な
  ため、**main workspace から実行すると improve が未解決 → degraded → promote 抑止** となる
  (leak 発火は improve 偏在のため誤 promote を防ぐ正しい挙動)。運用上は improve workspace から
  実行するか extra_roots に improve を追加する。
- root 発見テンプレート = `jj workspace list --ignore-working-copy -T
  'name ++ \t ++ target.current_working_copy() ++ \t ++ root()'`
  (`--ignore-working-copy` で read-only 化、`current_working_copy()` で現 workspace 判定)。
- `[telemetry_report]` に `trend_months`(既定 6)、機構マッピングに `enabled_config_keys` /
  `exe_names` を追加 (snapshot の汎用化)。dogfood config: retention_days=90 /
  zero_streak_months=2 / leak 機構 1 件。

当初の作業計画 (すべて完了):

- 設計決定 2 の仕様を実装。workspace Cargo.toml members 追加、package.json に
  `build:cli-telemetry-report` + `build:all` 組み込み + `"telemetry-report"` script
  (`node scripts/run-artifact.mjs cli-telemetry-report`)。
- `.gitignore` に `.claude/monthly-reviews/` と `.claude/monthly-review-last-run.json` を追加
  (rollup は `.claude/telemetry/` 配下で既存 ignore に包含)。
- config は exe 隣の `hooks-config.toml` を読む既存パターン (`[telemetry_report]` section:
  `retention_days` / `zero_streak_months` / `extra_roots` / `[[telemetry_report.mechanisms]]`)。
  ADR-039 に従い section 不在ではレポート生成のみ・削除系 (retention) は default OFF。
- テスト: fixture JSONL での unit テスト (月跨ぎ集計 / 前月比 / 連続 0 判定 / rollup 確定月
  不変 / retention 境界 / 壊れ行 skip / snapshot) + **実データ実測** (現存の main/improve
  telemetry に対して実行し、2026-07 の improve 側に `hooks-stop-tool-call-leak` が非 0 で
  現れる・main が 0 である等、構造を確認する。件数は増え続けるため厳密値は assert しない)。
  → 実測で improve+main 横断集計・leak 13 block / recovery 2 warn の内訳分離・degraded 抑止を確認
  (実測時点では main 側にも leak 発火が蓄積し 0 ではなくなっていたが、横断集計は正しく合算)。

### Phase 2 (PR-3 前半): L1 reminder ✅ 実装完了 (未 push、2026-07-30)

**実施結果**: `hooks-session-start` に月次レビュー reminder を実装。

- `hooks_config.rs`: `MonthlyReviewReminderConfig` (`enabled` / `threshold_days` /
  `system_message_enabled`) を追加し `SessionStartConfig.monthly_review_reminder` に配線 +
  パーステスト 2 件 (section parse / system_message_enabled 省略時 None)。
- `monthly_review.rs` (新 module): last-run staleness の 1 経路のみ (weekly と異なり failed
  marker 経路は持たない、設計決定 1)。`.claude/monthly-review-last-run.json` の `last_run_at`
  内容 timestamp で判定 (mtime 非依存、`PastTime` で未来値を Stale 扱い)、
  `lib_jj_helpers::resolve_main_workspace_root` で main-root canonical 化 (ADR-045)、
  `SingleLineMessage` で systemMessage opt-in (ADR-059)、telemetry id `monthly_review_reminder`
  / warn で計装 (ADR-055)。threshold の code default = 28 日
  (`MONTHLY_REVIEW_DEFAULT_THRESHOLD_DAYS`)。unit テスト 21 件 (閾値境界 / Missing=発火 /
  Stale=発火 / Unreadable=抑制 / 未来値=Stale / default threshold / main-root canonical /
  systemMessage opt-in / tell-user 指示)。
- `main.rs`: `append_cwd_nudges` から monthly nudge を配線 (weekly の後、両 config は独立 opt-in)。
  weekly + monthly は systemMessage スロットが 1 つのため `combine_system_messages` で ` / `
  区切りの 1 行に合成。関数長 50 行ガイドライン (順位 48) 遵守のため review reminder 部分を
  `append_review_reminder_nudges` に切り出し。unit テスト 3 件 (合成 0/1/複数)。
- `.claude/hooks-config.toml`: `[session_start.monthly_review_reminder]` を `enabled = true` /
  `threshold_days = 28` / `system_message_enabled = true` で dogfood 有効化 (ADR-039: code
  default は OFF、派生 deploy では section を置かず完全 skip)。

検証: `cargo test --workspace` (全 crate green、hooks-session-start 120 件) /
`cargo clippy --workspace --all-targets -- -D warnings` / `pnpm lint:md` 全通。
**push / PR 作成は未実施** (通常フロー・ADR-028 ゲート待ち)。実 hook 発火の確認は dogfood に委ねる
(unit テストで閾値境界を固定済み、検証要件どおり)。

実装上の決定 (プラン未指定箇所、Phase 4 で ADR-062 へ反映):

- **config 閾値フィールド名 = `threshold_days`** (weekly の `reminder_threshold_days` とは
  非対称だが、設計決定 3 の明示表記 `threshold_days=28` に従う)。code default 28 は
  `monthly_review.rs` 側に置き、config 欠落時に適用。
- **weekly + monthly 同時発火時の systemMessage 合成**: 出力 JSON の systemMessage スロットは
  1 つのため ` / ` 区切りで 1 行連結 (単一行不変条件は `SingleLineMessage` が構造的に保証)。
  additionalContext は両 reminder を独立に付す。
- state file `.claude/monthly-review-last-run.json` の gitignore は Phase 1 で追加済み
  (書き手は L3 skill、exe/hook 側は読むのみ)。当初計画どおり (すべて完了):
  設計決定 3 の全教訓 (a)〜(d) を適用、hooks-config.toml で dogfood。

### Phase 3 (PR-3 後半 + skills repo): L3 skill + docs

- **skills repo 側**: `$CLAUDE_SKILLS_REPO/monthly-review/SKILL.md` を新規作成 (weekly-review
  skill を template に、設計決定 4 の Phase 構成)。skills repo の規約
  (`$CLAUDE_SKILLS_REPO/docs/adr/0002` 等) に従い、deploy 方式は既存 skill と同じにする。
  skills repo は別リポジトリのため PR フローも当該 repo の流儀に従う。
- **本 repo docs**:
  - ADR-062 新規: 本ドキュメントの「背景」「設計決定」「ユーザー決定事項」を正式記録。
    ステータス試験運用、ADR-039 3 点セット (opt-in: reminder/retention は default OFF、
    kill-switch: 各 enabled = false + telemetry 側 kill-switch が上流に存在、bounded lifetime:
    dogfood 3 回で採否判定)。
  - ADR-055 amendment: step 2/3 消化 + 出力先の週次→月次変更 + (Phase 0 の) block 記録限定。
  - ADR-053 / ADR-061 追記: 「撤去判定 (4 週間非観測) は月次レビュー (ADR-062) が機械 promote
    する」1 段落ずつ。
  - CLAUDE.md index に ADR-062 追加。todo-summary2.md / todo16.md の順位 307/308/312 entry を
    消化・削除 (harness-improvement-plan.md の WP-12 状態も更新)。

### Phase 4 (最終): ADR 記載漏れ確認 + 本ドキュメント削除

1. 本ドキュメントの「ユーザー決定事項」3 項目と「設計決定」5 項目を 1 つずつ、ADR-062 (および
   ADR-055/053/061 の追記) と照合し、**すべて ADR 側に記載済みであることを確認**する。
   照合の観点: 閾値 2 か月 (config 可変) と「4 週間」基準の置き換え関係・ハイブリッド実行・
   workspace 横断と fail-open・root 発見不完全時の promote 抑止 (degraded 明示)・
   rollup/retention (retention default OFF)・snapshot による発火 0 誤読防止 (月別 rollup 記録 +
   月中一時無効化の検出限界)・incident 由来の維持推奨区別・takt 不採用 (YAGNI) と marker 不採用・
   AskUserQuestion 4 件超の分割・週次との役割分担・warm-up 初回時期。
2. 漏れがあれば ADR を補完する (本ドキュメントにしか書かれていない決定を残さない)。
3. 確認完了後、**本ドキュメント (`docs/monthly-harness-roi-review-plan.md`) を削除**し、
   削除を含む最終 PR を通常フローで作成する。

## 検証要件 (各 PR 共通)

- `cargo test --workspace` / `cargo clippy --workspace -- -D warnings` / `pnpm lint:md` 全通。
- Rust の非 doc コメントは禁止 (Bundle Z、PostToolUse lint が block する)。意図は識別子名と
  doc comment で表現する。
- push は `pnpm push` (push-runner: quality gate + takt レビュー)。PR 作成は prepare-pr フロー
  でユーザー承認 (ADR-028)。マージ後の post-merge-feedback は自動起動する。
- **実測検証を省略しない**: Phase 1 は実データ、Phase 2 は実 hook 発火 (新セッション起動で
  reminder が期待どおり silent / 発火することの確認は dogfood に委ねてよいが、unit テストで
  閾値境界を固定する)。

## リスク / 留意点

- **telemetry はローカル運用データ** (gitignore): rollup もローカル。マシン移行でトレンドが
  消える点は ADR-055 と同じ位置づけで受容 (ADR-062 に明記)。
- **発火 0 の解釈**は snapshot で緩和するが確定はしない。最終判断は必ずユーザー採否を経る
  (自動無効化しない)。
- **初回実行は 2026-08-12 以降を推奨** (warm-up)。実装 land はそれ以前でよい。
- 順位 309 (Phase 0) が遅れても Phase 1 以降は独立して進められる (leak 判定は fail-open 設計の
  ため 309 の影響を受けない)。その場合レポートに「stop-quality 等の block 数は infra エラーを
  含み得る」注記を入れる。

## 参照

- [ADR-055](adr/adr-055-firing-telemetry-collection.md) — テレメトリ収集層 (WP-12 step 1、本プランの土台)
- [ADR-031](adr/adr-031-weekly-review-pipeline.md) — 週次レビュー (3 層パターンの直接の先例、教訓の出典)
- [ADR-053](adr/adr-053-stop-tool-call-leak-detection.md) / [ADR-061](adr/adr-061-tool-call-leak-hardfail-recovery.md) — 第一ユースケース (leak 検知の bounded lifetime)
- [ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) — 試験運用 3 点セット
- [ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) — workspace 状態分裂 (main-root canonical 化の根拠)
- [ADR-059](adr/adr-059-hook-system-message-visibility.md) — systemMessage / SingleLineMessage
- [ADR-049](adr/adr-049-incident-eval-regression-suite.md) — incident 由来ルールの維持推奨区別
- [ADR-022](adr/adr-022-automation-responsibility-separation.md) / [ADR-028](adr/adr-028-pnpm-create-pr-gate.md) — 承認ゲート
- docs/todo16.md / docs/todo-summary2.md — 順位 307/308/309/312 の詳細仕様
- docs/harness-improvement-plan.md — WP-12 全体像
