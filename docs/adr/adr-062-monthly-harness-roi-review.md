# ADR-062: 月次ハーネス ROI レビュー — telemetry 発火実績によるハーネス複雑度の棚卸し

## ステータス

試験運用 (2026-07-30)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。
> [ADR-055 (発火テレメトリ収集層)](adr-055-firing-telemetry-collection.md) の WP-12 step 2/3 の
> 実装ビークルであり、step 2/3 を本 ADR で消化する。

## コンテキスト

[ADR-055](adr-055-firing-telemetry-collection.md) の WP-12 は 3 ステップ構成で、step 1 (収集層) を
先行マージ済みである。本 ADR は残る **step 2 (ROI 棚卸し pre-step) / step 3 (bounded lifetime 判定の
発火数機械化)** を実装する。

### 動機となった問題

[ADR-053](adr-053-stop-tool-call-leak-detection.md) /
[ADR-061](adr-061-tool-call-leak-hardfail-recovery.md) の tool call leak 検知は上流 (Claude Code)
不具合への時限的な防御層であり、上流修正後も検出を続けると Stop hooks の実行時間が塵積し開発
イテレーションを遅くする (leak hook 単体で実測 83〜829ms/Stop)。ADR-053/061 の bounded lifetime は
「leak 4 週間非観測で撤去判定」と定めるが、この判定を機械的に promote する仕組みが無く、人間の記憶に
依存していた。本 ADR の月次レビューを**この判定の最初のユースケース**にする。

### 週次レビューとの役割分担

[ADR-031 (週次レビュー)](adr-031-weekly-review-pipeline.md) が whole-tree コードレビューを担うのに
対し、本 ADR の月次レビューは **telemetry / ROI 棚卸し**を担う。ADR-055 は step 2 の出力先を
「週次レビュー facet」と想定していたが、テレメトリ傾向は週次では変化が小さくノイズになり、
ADR-053 の撤去粒度「4 週間」とも月次が一致するため、**出力先を月次に変更する**
(ADR-055 に amendment、本 ADR § 決定 5)。

### warm-up 制約

telemetry はマージ後に初めてデータが溜まるため、ADR-055 収集開始 (2026-07-15) から 28 日 =
**2026-08-12** が初回の有意義な月次レビュー実行時期である。実装 land はそれ以前でよいが、初回実行は
08-12 以降が有意義。

## ユーザー決定事項

1. **非アクティブ化の提案閾値 = 連続 2 か月発火 0** (config で変更可能なデフォルト値)。減少傾向は
   1 か月目から参考情報として提示する。この「2 か月」は ADR-053/061 の「4 週間非観測」基準を
   置き換える正式基準であり (§ 決定 4)、2 か月は 4 週間より保守的な置き換えである (判定主体が
   人間の記憶から月次レビューへ移る)。
2. **採用アクションはハイブリッド**: config 1〜2 行で済む軽量変更 (`enabled = false` 等) はレビュー
   セッション内でその場で通常 push/PR フローで実施。大型作業 (rule 削除・crate 撤去 revert PR 等) は
   weekly-review と同型の todo 登録に回す。
3. **順位 309 (telemetry block 記録への infra エラー混入除去) をスコープに含める** (Phase 0 として
   先行実装済み。ADR-055 の 2026-07-29 amendment で消化)。

## 決定

### 1. 3 層構成 (ADR-030/031 パターンの 5 例目)

[ADR-030](adr-030-deterministic-post-merge-feedback.md) /
[ADR-031](adr-031-weekly-review-pipeline.md) の「L1 reminder / L2 実処理 / L3 skill」パターンを
踏襲する。ただし **L2 は takt ではなく決定論 Rust exe** とする。判断材料が数表であり AI 並列
レビューの必然性が無いため (YAGNI。必要になれば後から takt facet を追加できる)。よって
weekly-review にあった `.failed` marker / resume 機構は**不採用**とする (L2 が高速・決定論のため。
失敗時は skill がエラー報告するのみの best-effort)。

### 2. L2: 新規 exe `cli-telemetry-report`

`src/cli-telemetry-report/` ([ADR-012](adr-012-src-naming-convention.md) の `cli-` prefix)。

- **入力**: 各 root の `.claude/telemetry/firings-*.jsonl`。root 発見は `jj workspace list` の動的
  列挙 (読み取り専用レポートのため `--ignore-working-copy` で working copy を変異させず、テンプレートの
  `current_working_copy()` 出力で現 workspace を判定する。パース失敗・workspace 未使用環境は現 root
  のみに fail-open) + config `extra_roots` で追加可。テレメトリは workspace ローカル (exe 隣の
  `.claude/` に書かれる) で、実測上 leak 発火は improve workspace に偏在するため
  **workspace 横断集計が必須**。
- **root 発見不完全時の promote 抑止**: root 発見が不完全な場合 (`jj workspace list` 失敗 /
  現 workspace 以外の未解決 / `extra_roots` の到達不能) はレポートに **degraded を明示**し、当該
  実行では判定候補の promote を抑止する (degraded 成立の実装条件 = 現 workspace 以外で root 未解決の
  workspace が **1 件でもあれば degraded**。未解決 workspace は `self.root()` が `<Error>` = root 未知の
  ため `extra_roots` との対応を検証できず、件数比較で degraded を解除すると誤設定した extra_root が
  誤 promote を招く。対応を検証できない以上、未解決 1 件でも degraded を維持する。**現 workspace 自身の
  `self.root()` 解決失敗は exe 隣接 `.claude` の親 = 現 root で補うため degraded にしない**)。集計・
  レポート生成は fail-open で継続するが、「発火 0」判定は完全な root 集合を前提とする (発見漏れ +
  発火 0 の組合せは誤 promote に直結するため)。この環境の `ccht-improve` workspace は jj 格納パス
  不整合で `self.root()` が解決不能なため、main workspace から実行すると improve が未解決 → degraded →
  promote 抑止となる (leak 発火が improve 偏在のため誤 promote を防ぐ正しい挙動)。degraded を解消する
  運用は **improve workspace から実行する** (improve が現 root として解決される)。`extra_roots` は
  集計対象 root を追加する (improve の telemetry を取り込む) が、未解決 workspace の root は未知で
  対応を検証できないため degraded の解除には使わない。**この格納パス不整合が続く限り main workspace
  からの実行は恒久的に degraded であり、improve workspace からの実行が唯一の非 degraded 経路である**
  (last-run 更新契約 § 決定 5 により、main からは L1 reminder が止まらない)。
- **月次 rollup + retention**: 月ごとの id 別集計を `.claude/telemetry/monthly-<YYYY-MM>.json`
  (main workspace 側) に永続化。raw daily ファイルは retention (config `retention_days`。**code
  default は未設定 = 削除無効**、ADR-039 opt-in。本 repo は `retention_days = 90` で dogfood) 超過分を
  削除する。**複数月トレンドは rollup から読む**ため raw 削除後も判定可能。rollup は集計済み月を
  再集計しない (確定月は不変。当月は毎回再計算)。
- **レポート出力** (`.claude/monthly-reviews/<YYYY-MM-DD>.md` + 機械可読 JSON):
  (a) 月別 × id 別カウント + 前月比、(b) 直近 N か月 (config `trend_months`、既定 6) の発火 0 リスト、
  (c) config enabled / exe 配備
  状態の **snapshot** (「0 = 上流修正」と「0 = 無効化・未配備」の誤読防止)、(d) incident 由来ルール
  ([ADR-049](adr-049-incident-eval-regression-suite.md)) の「発火 0 でも維持推奨」マーク、
  (e) 判定候補。incident 由来ルールの真実源は `.claude/custom-lint-rules.toml` の `[rules.incident]`
  サブテーブル (id を exe 側に複製しない、ADR-049 思想と整合)。snapshot は集計実行時点の状態であり
  単体では月内の有効性を証明しないため、**月次 rollup 確定時に当月の snapshot を rollup JSON にも
  保存**し、判定はこの月別記録を参照する。

### 2 の amendment (2026-07-31, Phase A): 機構レジストリ + 発火 0 リスト再定義

初回 dogfood (2026-07-30) で、レポート (b) の発火 0 リストが「発火 0 = 削除候補」の中核シグナルとして
機能していないことが判明した。rollup の id entry は発火レコードからしか作られない (`aggregate.rs`
`count_firings` は block/warn のみ加算) ため、(1) 発火が止まって窓外に落ちた id と (2) 一度も発火して
いない機構が**どちらも不可視**だった (§ 決定 4 の「MVP は 1 件 + **発火 0 リスト全般**で足りる」の
後半が実装で満たされていなかった)。機構レジストリで母集合を静的に列挙してこの盲点を塞ぐ:

- **機構レジストリ (3 供給源、すべて exe 隣接 `config_base` 基準)**:
  - **rule**: `.claude/custom-lint-rules.toml` の全 rule id。incident 判定 (`incident.rs`) と同じ読み口
    (`RulesFile`) を共有する。telemetry の rule firing id は `hooks-post-tool-linter` の
    `lib_telemetry::record` が `id: &rule.id` で記録する (= rule の `id` フィールドそのもの)。
  - **preset**: `hooks-config.toml` の `[pre_tool_validate] blocked_patterns` 宣言。preset firing は
    `hooks-pre-tool-validate` の `record_preset_block` が `hit.source` (= blocked_patterns の宣言文字列)
    を id に記録するため、宣言をそのまま列挙すれば発火 id と突き合う。
  - **hook / nudge**: 自動列挙元が無いため config `[telemetry_report.registry] hook_ids` を新設
    (ADR-039 additive。section 不在でも rule/preset は自動列挙される)。id は hook 名と一致しない例が
    ある (`jj-op-verify` / `pr_monitor_catchup` / `hooks-stop-tool-call-leak/prompt-recovery` /
    `file-length` / nudge 群等) ため、各 hook の `record` 呼び出しから実確認して列挙する。
- **発火 0 集合 = (レジストリ ∪ 全 rollup 履歴に現れた id) − (窓内に発火した id)**。2 区分で提示:
  - **never-fired**: レジストリにあり全履歴で発火 0。
  - **went-quiet**: 履歴に発火があるが窓内 0 (**最終発火月を併記**。全 rollup 走査で導出)。

  incident (`[rules.incident]`) 維持推奨マークと mechanisms 監視対象マークは新リストにも適用する。
- **degraded 実行時は (b) 全体に「参考値 (root 発見不完全)」注記**を付す (発火が発見漏れ root に
  偏在し得るため。verdict の promote 抑止と整合)。
- 供給源単位の読取失敗は fail-open で skip しつつ**レポートに欠落を明示**する (silent fallback 排除、
  「読めなかった」と「id が 0 件」を区別)。JSON の zero_firing entry に `provenance`
  (`never_fired` / `went_quiet`) と `last_fired_month` を追加、`registry.source_failures` も出力する。
  (b) は参考情報でありユーザーゲート (自動削除しない) は不変。

### 3. L1: SessionStart reminder

`hooks-session-start` の `[session_start.monthly_review_reminder]` (`enabled` / `threshold_days`
既定 28 / `system_message_enabled`)。閾値フィールド名は `threshold_days` とする (weekly の
`reminder_threshold_days` とは非対称だが本 ADR の明示表記に合わせた。code default 28 は
`monthly_review.rs` 側に置き config 欠落時に適用)。`weekly_review.rs` のパターンを踏襲し、以下の
教訓を適用する:

- staleness は state file `.claude/monthly-review-last-run.json` の `last_run_at` **内容 timestamp**
  のみで判定し mtime に一切依存しない (ADR-031 の silent-fresh バグ教訓)。
- state file の読み書きは `lib_jj_helpers::resolve_main_workspace_root` で **main workspace root に
  canonical 化** ([ADR-045](adr-045-jj-workspace-parallel-sessions.md) 分裂対策。hook 読み側と
  skill 書き側の両方)。
- systemMessage は [ADR-059](adr-059-hook-system-message-visibility.md) の opt-in + 1 行
  (`lib_hook_output::SingleLineMessage` を使用)。
- 発火は telemetry に id `monthly_review_reminder` / warn で計装 (ADR-055 amendment PR-N3 と同型)。

weekly と異なり **failed marker 経路は持たない** (§ 決定 1 の marker 不採用)。weekly + monthly が
同時発火した場合、systemMessage スロットは出力 JSON に 1 つのため ` / ` 区切りで 1 行に合成する
(additionalContext は両 reminder を独立に付す)。

### 4. 判定候補 (step 3 MVP)

「機構 → 監視対象 id 群 → 成立時の提案」の静的マッピングを config
(`hooks-config.toml [[telemetry_report.mechanisms]]`) に持ち、**連続 `zero_streak_months` (既定 2)
か月発火 0 で非アクティブ化候補として promote** する (ユーザー決定事項 1)。各機構エントリは監視 id 群に
加え snapshot 用の `enabled_config_keys` / `exe_names` を持ち、config enabled / exe 配備の確認
(§ 決定 2 レポート (c)) を機構横断で汎用化する。promote の成立条件には
「対象の各月 rollup に `enabled = true` + 配備ありの snapshot 記録があること」を含める (無効化・
未配備の月を「発火 0」と誤読しない)。**snapshot が証明する範囲には限界がある** (Phase C amendment で
「月中実行があればその最後の時点」に精緻化。§ 4 の amendment 参照) が、最終判断が必ずユーザー採否
(AskUserQuestion) を経る前提で受容する。

初期マッピングは 1 件: ADR-053/061 (leak 検知) → ids
`[hooks-stop-tool-call-leak, hooks-stop-tool-call-leak/prompt-recovery]` → 提案 =
`[stop_tool_call_leak] enabled = false` + `prompt_recovery_enabled = false` (最終的な crate 撤去
revert PR は ADR-053/061 bounded lifetime の手順に従う)。ADR-061 の回収層は id
`hooks-stop-tool-call-leak/prompt-recovery` (decision = warn) で記録されるため、leak のトレンドは
block + この warn の合算と内訳で見る。全試験運用 ADR の網羅登録は将来拡張とし、MVP はこの 1 件 +
発火 0 リスト全般で足りる。

### 4 の amendment (2026-07-31, Phase C): rollup 確定時の snapshot 保持

snapshot は集計実行時点の状態でしかなく、月次カデンツでは月 M の確定を M+1 の初回実行が行う。旧実装は
確定する全月に**実行時点 (M+1) の snapshot** を刻んでいたため、月 M 中ずっと無効化していた機構を
M+1 で再有効化すると「M は enabled + 発火 0」と確定し、無効化月を誤って promote streak に算入し得た。
`aggregate.rs` `resolve_month` を修正し、**確定 rollup に「月中最後の観測」を保持**する:

- **当月**: 毎回現在 snapshot で再スタンプ (月中最後の実行時点が自然に確定値として残る)。
- **過去月の確定で prev (月中の未確定 rollup) がある**: `prev.snapshot` を保持 (現在 snapshot で
  再スタンプしない)。
- **過去月で prev が無い (月中に一度も実行が無かった)**: 現在 snapshot で代用する。

したがって **snapshot が証明するのは (a) 月中に実行があればその最後の時点、(b) 無ければ確定時点の
状態** に精緻化される。真の月中の一時無効化 (snapshot と snapshot の間のトグル) は依然検出できないが、
最終判断は必ずユーザー採否を経る前提で受容する。

### 4 の amendment (2026-07-31, Phase D): promote 判定を確定月に限定

「連続 2 か月発火 0」(ユーザー決定事項 1) の忠実実装として、**閾値到達判定は確定月のみで数える**。
未確定の当月は `zero_streak` 表示と `current_month_partial` フラグには含める (参考表示) が、閾値到達の
カウントからは除外する。旧実装は未確定当月も算入していたため、実効閾値が「確定 1 か月 + 当月 20 数日」で
成立し得た (系全体の保守的設計 = degraded 抑止・snapshot AND 意味論 と不整合だった)。

これにより leak の promote 最早時期は **「2 つの完全なゼロ月が確定した後の初回実行」** になる
(例: 8・9 月がゼロなら 10 月の初回実行。現挙動比で約 1 か月後ろ倒し = 仕様の忠実化であり意図的)。

### 5. L3: skill `/monthly-review`

skills repo (`$CLAUDE_SKILLS_REPO`) で作成し `~/.claude/skills/` へ deploy する
(weekly-review skill の構成を template にする):

- Phase 1: 起動条件確認 → Phase 2: `pnpm telemetry-report` (exe) を同期実行 → Phase 3: レポート
  提示 + AskUserQuestion で判定候補・削除候補の採否 → Phase 4: **ハイブリッド実行** (軽量 config
  変更は即時に通常 push/PR フロー、大型は `docs/todo.md` 登録) + last-run 更新 (ユーザー決定事項 2)。
  大型作業の登録先は weekly-review と同じ `docs/todo.md` で、優先度 table の採番 (行追加) は skill
  では行わずユーザー判断に委ねる。
- **last-run 更新契約 (L1 reminder の誤抑制防止)**: `.claude/monthly-review-last-run.json` の
  `last_run_at` は、**exe が完全な (degraded でない) レポート生成に成功し、Phase 3 (レビュー) に
  到達した場合にのみ**更新する。**exe 失敗** (Phase 2 で非 0 exit = レポート不在。skill は Phase 4
  に到達しない) と **degraded** (root 発見不完全で promote 抑止) の場合は last-run を**更新せず
  stale のまま**にし、次回セッションで L1 reminder を再発火させる。degraded を「レビュー完了」と
  みなして更新すると、root 発見漏れ (leak 発火が improve に偏在) のまま催促が止まるため。degraded は
  **improve workspace 実行で解消するまで催促を継続する** (`extra_roots` は集計対象 root を追加する
  のみで degraded は解除しない)。
- **候補が 4 件を超える場合は AskUserQuestion を複数質問に分割**する (1 質問 4 option の制約。
  severity / 機構種別順にグループ化。ADR-031 Phase E dogfood で確立した weekly-review と同方式)。
- 自動で無効化しない。採否は必ず AskUserQuestion を経る
  ([ADR-022](adr-022-automation-responsibility-separation.md) /
  [ADR-028](adr-028-pnpm-create-pr-gate.md))。

## ADR-039 3 点セット

### Config opt-in (default OFF)

- **L1 reminder**: `[session_start.monthly_review_reminder]` の code default は
  `enabled = unwrap_or(false)`。section 省略で完全 skip。本 repo は dogfood のため `enabled = true`。
- **L2 retention**: `[telemetry_report] retention_days` 未設定で削除無効 (レポート生成のみは継続)。
  本 repo は `retention_days = 90` で dogfood。
- 派生プロジェクトへの deploy 時は各 section 省略で OFF を継承する。

### Kill-switch

| 停止手段 | 影響範囲 |
|---|---|
| `[session_start.monthly_review_reminder] enabled = false` | L1 reminder を恒久停止 |
| `[session_start.monthly_review_reminder] system_message_enabled = false` | systemMessage のみ停止 (additionalContext の nudge は継続) |
| `[telemetry_report]` section 削除 | L2 集計を停止 (収集層 `[telemetry]` とは独立) |
| 上流 `[telemetry]` の kill-switch (`enabled = false` / env `CLAUDE_TELEMETRY_DISABLE`) | 収集層ごと停止 = 本集計の入力が枯れる |

### Bounded lifetime

telemetry の ROI 棚卸しは dogfood で有用性を検証する。明示的な decision trigger:

- **dogfood 3 回** (月次のため約 3 か月) で採否判定 (本採用化 or 撤去)。
- 撤去時は L1 reminder + L2 exe + L3 skill をまとめて revert する。
- 本 ADR が消化する ADR-053/061 の leak 撤去判定は、初回有意義な実行 (2026-08-12 以降) から連続
  2 か月発火 0 の成立をもって promote する。

## 帰結

### 利点

- 試験運用機構 (rule/preset/hook) の維持・撤去判断を、人間の記憶ではなく発火実績で機械化する
  基盤ができる。ADR-053/061 の「4 週間非観測」の記憶依存判定が月次レビューの機械 promote に移る。
- L2 が決定論 exe (LLM 不使用) で高速なため、takt facet の失敗/resume 機構 (`.failed` marker) を
  持たずに済み、weekly-review より簡素。
- workspace 横断集計 + degraded 時の promote 抑止により、telemetry が workspace ローカル
  (leak 発火が improve に偏在) でも誤 promote を構造的に防ぐ。

### 欠点 / 留意点

- **telemetry はローカル運用データ** (gitignore): rollup もローカル。マシン移行でトレンドが
  消える点は ADR-055 と同じ位置づけで受容する。
- **発火 0 の解釈**は snapshot (config enabled + exe 配備の月別記録) で緩和するが確定はしない。
  Phase C 以降 **snapshot が証明するのは「月中実行があればその最後の時点、無ければ確定時点の状態」**
  (§ 4 amendment) で、真の月中の一時無効化 (snapshot 間のトグル) は依然検出できない限界があり、
  最終判断は必ずユーザー採否を経る (自動無効化しない)。
- **初回実行は 2026-08-12 以降を推奨** (warm-up)。実装 land はそれ以前でよい。
- root 発見が不完全な実行では degraded を明示し promote を抑止するが、集計・レポート自体は
  fail-open で継続する ([ADR-043](adr-043-security-gates-fail-closed.md) の fail-closed は
  ゲート限定であり、本 observation 層は適用外)。

## 関連 ADR

- [ADR-055](adr-055-firing-telemetry-collection.md) — テレメトリ収集層 (WP-12 step 1、本 ADR の土台。step 2/3 消化 + 出力先の週次→月次変更を amendment)
- [ADR-031](adr-031-weekly-review-pipeline.md) — 週次レビュー (3 層パターンの直接の先例、役割分担 = whole-tree コードレビュー)
- [ADR-030](adr-030-deterministic-post-merge-feedback.md) — 決定論的 post-merge feedback (3 層パターンの先例)
- [ADR-053](adr-053-stop-tool-call-leak-detection.md) / [ADR-061](adr-061-tool-call-leak-hardfail-recovery.md) — 第一ユースケース (leak 検知の bounded lifetime、撤去判定を本 ADR が機械 promote)
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用 3 点セット
- [ADR-045](adr-045-jj-workspace-parallel-sessions.md) — workspace 状態分裂 (main-root canonical 化 / workspace 横断集計の根拠)
- [ADR-059](adr-059-hook-system-message-visibility.md) — systemMessage / SingleLineMessage (L1 reminder の可視化チャネル)
- [ADR-049](adr-049-incident-eval-regression-suite.md) — incident 由来ルールの維持推奨区別
- [ADR-022](adr-022-automation-responsibility-separation.md) / [ADR-028](adr-028-pnpm-create-pr-gate.md) — 承認ゲート (自動無効化しない根拠)
- [ADR-012](adr-012-src-naming-convention.md) — src/ 命名規約 (`cli-` prefix)
