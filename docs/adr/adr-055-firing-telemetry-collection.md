# ADR-055: 発火テレメトリ収集層 — ハーネス ROI 棚卸しの決定論的観測基盤

## ステータス

試験運用 (2026-07-15)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。
>
> 採番は land 時に確定する ([順位 135/140](../todo-summary.md) の placeholder 方式)。
> 現時点で ADR-054 が最新のため 055 を仮採番している。

## コンテキスト

2026-07-04 策定のハーネス改善計画の WP-12 は「ハーネス複雑度 (hooks 7 本・custom rule 12 本・
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
- **残りの nudge-only hook** (stop-feedback-dispatch / user-prompt-feedback-recovery)。
  本 PR のスコープ外で、計装は各 hook を触る PR で個別に行う。session-start nudge は当初
  この除外に含めていたが、後述の Amendment (2026-07-19) で除外根拠 (「nudge は block/warn に
  乗らない」) を撤回し計装対象に加えた。

`decision` は「hook がツールを実際に停止したか」ではなく「発火の重み」を表す軸である。
custom rule / jj-op-verify は additionalContext の助言層で実際には block しないが、severity
に応じて block/warn を記録する。stop-quality は当初 infra エラー (stdin/parse 失敗) の
fail-closed 経路でも block を emit するため「hook が block を emit した総数」として記録して
いたが、後述の Amendment (2026-07-29) でこの定義を撤回し、実 quality 違反 (品質ステップ
失敗) の block のみ記録するよう限定した (WP-12 ROI 信号から infra ノイズを除外)。
file-length gate の fail-closed 経路 (jj 失敗の判定不能 block) は当初から ROI 信号を汚さない
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

## Amendment (2026-07-18): push-run メトリクス record kind (R3、todo 順位 325)

本 ADR のスコープは hook 発火イベント (`firings-*.jsonl`) だが、push パイプラインの
per-run メトリクス永続化 (push-pipeline-fix-plan2 §3 R3) が同じ器を必要としたため、
**別 record kind** として相乗りさせる。判断と設計は以下。

### 別 record kind (別ファイル) を採用 — amendment ではなくスキーマ分離

`firings-*.jsonl` は WP-12 step 2 の集計が glob 走査する前提で、行の shape (hook/kind/id/
decision) が固定されている。push-run メトリクスは shape が全く異なる (stage 別 elapsed の
object・exit_code・os 等) ため、同一ファイルに混ぜると firing 集計を壊す。よって
**`push-runs-<YYYY-MM-DD>-<pid>.jsonl` という別 prefix** に書き、firing 集計と分離する。
opt-in (`[telemetry] enabled`) / kill-switch (`CLAUDE_TELEMETRY_DISABLE`) / fail-open /
per-pid×日次 partition / `WRITE_LOCK` 直列化の既存原則には相乗りする (グローバル telemetry
スイッチ 1 つで両 record kind を統べる)。

### 責務分離 — lib-telemetry はドメイン中立の器のまま

push-run 固有スキーマ (`RunRecord`) は消費側 `cli-push-runner` (`src/metrics.rs`) が保持し、
lib-telemetry には**任意 `Serialize` を prefix 付き partition へ書く汎用 writer**
(`record_metric` / `record_metric_to` / `record_metric_gated_to`) だけを追加した。これにより
「観測層を特定ドメインに結合させない」という本 ADR の設計思想 (§欠点 の UTC ヘルパー複製の
論点と同根) を保つ。`ts` は writer が書き込み時に差し込むため、UTC ヘルパーの消費者は
lib-telemetry 内に留まり、§欠点 の「3 crate 目で lib-time 抽出」トリガには到達しない。

### 記録フィールドとプライバシー

os / exit_code / total_secs / docs_only / skipped_groups / post_takt_regate 判定
(skip / run-pass / block を区別) / takt_workflow / bookmarks / stage 別 elapsed。**メタデータ
のみ**の原則 (§プライバシー) は維持する。bookmark 名は branch 識別子 (= メタデータ、
session_id と同性質) であり、ファイルパス・コマンド本文ではないため run の識別鍵として載せる。

**deferred (別 PR、完了基準の必須外)**: takt run slug (`run_cmd_inherit` が takt 出力を
捕捉しないため `.takt/runs/` との join 鍵を clean に取得できない)・pr_size 行数
(`run_pr_size_check` が総行数を返さない)。

### 消費者

ADR-057 / ADR-058 の採否判定 (期限 2026-08-15) と R5/R6 の after 計測。これらの効果検証が
「push 時コンソール出力の手動保存」に依存していたのを、機械集計可能な JSONL に置き換える。

## Amendment (2026-07-19): session-start nudge 群の計装 (PR-N3)

初版は §計装スコープ で **session-start reminder を含む nudge-only hook を除外**し、根拠を
「decision 語彙が block/warn の 2 値のため nudge (助言出力) は乗らない」とした。weekly-review
reminder が約 4 週間ユーザーに気付かれず発火し続けていた incident
([ADR-059](adr-059-hook-system-message-visibility.md)) を受け、**この除外根拠を撤回し
session-start hook の 5 nudge を firing 計装 (`firings-*.jsonl`) に加える**。

### 除外根拠の撤回 — warn は「発火の重み」であり nudge に整合する

初版の「nudge は乗らない」判断は `decision` を「hook が実際に停止したか」と暗黙に捉えていた。
本 ADR は §計装スコープ 末尾で既に **`decision` は「発火の重み」を表す軸**と定義しており、
additionalContext の助言層で実際には block しない custom rule / jj-op-verify も warn/block を
記録している。nudge (助言出力) はこの warn (= 助言的発火) に自然に対応するため、語彙拡張を
待たず `warn` で記録できる。よって初版の除外根拠は不成立で撤回する。

### 計装対象と id

| 対象 | hook | kind | decision |
|---|---|---|---|
| session-start nudge 群 (5 種) | hooks-session-start | hook | warn |

`id` は nudge 種別の 5 値: `weekly_review_reminder` / `pr_monitor_catchup` / `reaper` /
`staleness` / `workspace_stale`。各 nudge が発火 (context 追記) した点で
`lib_telemetry::record` を 1 回呼ぶ (`hooks-session-start/src/main.rs` の `record_nudge_firing`)。
session_id は SessionStart hook 入力から直接渡す。fail-open / opt-in / kill-switch / per-pid×日次
partition は既存原則に相乗りする。

### 動機 — ADR-059 bounded lifetime の観測基盤

ADR-059 は systemMessage 可視化を weekly reminder 限定で dogfood し、行動要求系 nudge
(PR catch-up / post-merge recovery / failed marker) への段階展開の採否を発火実績で判定する
(期限 2026-08-16)。本計装が「どの nudge が実際に発火したか」を供給してその判定を支える。
同時に WP-12 step 2 の ROI 棚卸し (発火 0 の機構を削除候補提示) にも寄与する。

### スコープ外に残す nudge-only hook

stop-feedback-dispatch / user-prompt-feedback-recovery は本 PR では計装しない。撤回した根拠は
これらにも当てはまるが、計装は各 hook を触る PR で個別に行う (ADR-059 段階展開に連動)。
§計装スコープ の除外リストは本 amendment に合わせて更新した。

## Amendment (2026-07-29): block 記録を実 quality 違反に限定 (順位309、WP-12 ROI 信号の精度)

初版 § 計装スコープ は stop-quality の block 記録を「hook が block を emit した総数」と
定義し、infra エラー (stdin 読込 / JSON parse 失敗) の fail-closed 経路も計上していた。
WP-12 step 2/3 (発火数で hook の維持・削除を判断する ROI 棚卸し) の観点では、この infra
エラー混入が発火数を歪め「実際に品質違反を捕捉した回数」と乖離する。CodeRabbit Major 指摘
(順位309) を受け、**block 記録を実 quality 違反 (品質ステップ失敗) パス限定に絞り込む**。

### 3 hook の状態 (計装スコープ表のうち block を emit する hook)

| hook | 修正前 | 修正後 |
|---|---|---|
| hooks-stop-quality | stdin/parse 失敗の fail-closed block でも記録 | `emit_block(reason, cause: BlockCause)` に変更し、`cause.records_firing()` (QualityViolation のみ真) の場合だけ telemetry に記録する。infra エラー経路 (`read_stdin_or_block` / `parse_hook_input_or_block`) と worker thread panic (`run_quality_steps` の join 失敗) は `InfraError` 扱いで block decision のみ emit し記録しない。実 quality 違反 (品質ステップ失敗) のみ `QualityViolation` |
| hooks-stop-tool-call-leak | (変更なし) | `emit_block` は `run_check` の実 leak 検出時のみ呼ばれ、transcript 読取失敗・連続数 0・上限到達は fail-open で return するため既に実 leak 限定。ADR-061 の回収層 (`prompt-recovery`) は `should_recover` 成立時のみ warn 記録 |
| hooks-pre-tool-validate | (変更なし) | `record_preset_block` は `validate_command` が hit を返す (= preset にマッチした実 violation) 経路のみ。stdin/parse 失敗は `ExitCode::FAILURE` で return し記録しない。既に preset match 限定 |

### 設計: 経路を型で区別しテストで固定

stop-quality に `BlockCause { QualityViolation, InfraError }` を導入し、`emit_block(reason, cause)`
が `cause.records_firing()` (QualityViolation のみ真) のときだけ telemetry に記録する。
telemetry 記録を closure 注入した `emit_block_with` をテストの芯とし、「QualityViolation は
recorder を発火 / InfraError は発火しない」を副作用で観測して回帰ガードにする。leak / preset
は record 呼び出しが既に violation 経路にしか存在しないため、コード構造でこの不変条件が保たれる
(追加のテストは設けない)。

また `run_quality_steps` の各失敗は `StepFailure { message, cause }` で由来を保持し、worker
thread panic (join の `Err`) は実 quality 違反ではないため `InfraError`、ステップの実失敗は
`QualityViolation` とする。`block_on_failures` は `aggregate_block_cause` で全体の cause を
決め (1 件でも実失敗があれば `QualityViolation`、全て panic なら `InfraError`)、`emit_block`
に渡す。これにより内部障害 (panic) を品質違反として ROI 信号に誤計上しない (CodeRabbit Major 指摘)。

### 帰結

- 「発火 0 = 削除候補」の ROI 信号が infra エラーの fail-closed block で汚染されなくなり、
  発火数は「hook が実際に品質違反を捕捉した回数」を表す。
- fail-open 原則は不変。telemetry 記録の有無に関わらず block decision 自体は emit するため、
  infra エラー時も Claude への block 通知は従来どおり行われ、ゲート挙動は変わらない。

## Amendment (2026-07-30): WP-12 step 2/3 消化 + 出力先の週次→月次変更 (ADR-062)

初版 § コンテキスト / § Bounded lifetime は、step 2 (ROI 棚卸し pre-step) の出力先を
**週次レビュー ([ADR-031](adr-031-weekly-review-pipeline.md)) の facet** と想定していた。
[ADR-062 (月次ハーネス ROI レビュー)](adr-062-monthly-harness-roi-review.md) の実装にあたり、
この想定を **月次レビューへ変更**し、WP-12 step 2/3 を ADR-062 で消化する。

### 出力先を週次→月次に変更

テレメトリ傾向は週次では変化が小さくノイズになり、ADR-053/061 の leak 撤去粒度「4 週間」とも
月次が一致する。よって step 2 の集計は週次 facet ではなく **月次の決定論 exe
`cli-telemetry-report`** (ADR-062 § 決定 2) が担い、出力は `.claude/monthly-reviews/` +
月次 rollup (`.claude/telemetry/monthly-<YYYY-MM>.json`) とする。週次 = whole-tree コード
レビュー / 月次 = telemetry/ROI 棚卸し、の役割分担で ADR-031 と併存する。

### step 2 / step 3 の消化内容 (ADR-062 に詳細)

- **step 2 (棚卸し pre-step)**: `cli-telemetry-report` が workspace 横断で `firings-*.jsonl` を
  集計し、月別 × id 別カウント + 発火 0 リスト + config enabled / exe 配備 snapshot + incident 由来
  ルール ([ADR-049](adr-049-incident-eval-regression-suite.md)) の維持推奨マークを出力する。
  incident 由来の区別は初版 § Bounded lifetime の想定どおり `[rules.incident]` を真実源とする。
- **step 3 (bounded lifetime 判定の機械化)**: config `[[telemetry_report.mechanisms]]` の静的
  マッピングで「連続 `zero_streak_months` (既定 2) か月発火 0 → 非アクティブ化候補として promote」を
  MVP 実装。初期マッピングは ADR-053/061 の leak 検知 1 件。自動無効化はせず採否は
  `/monthly-review` skill の AskUserQuestion を経る (ADR-022/028)。

### retention (順位 312) の相乗り

初版 § Windows 並行書き込み安全性 の per-pid × 日次 partition は削除機構が無かった。
`cli-telemetry-report` に retention (`[telemetry_report] retention_days`、code default 未設定 =
削除無効の opt-in) を相乗りさせ、rollup 確定後の raw daily ファイルを削除する (複数月トレンドは
rollup から読むため判定に影響しない)。

なお、初版 (別 § に既述) の 2026-07-29 amendment「block 記録を実 quality 違反に限定 (順位309)」は、
ADR-062 の Phase 0 (ユーザー決定事項 3) として先行実装したものであり、本 step 2/3 の ROI 信号
精度向上の一部である。

## 関連 ADR

- [ADR-062](adr-062-monthly-harness-roi-review.md) — 月次ハーネス ROI レビュー (WP-12 step 2/3 の実装、出力先を月次に変更)
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン (opt-in / kill-switch / bounded lifetime)
- [ADR-043](adr-043-security-gates-fail-closed.md) — fail-closed 原則 (本 telemetry は observation 層で適用外 = fail-open)
- [ADR-044](adr-044-subprocess-utility-extraction-boundary.md) — utility extraction 境界 (UTC ヘルパー抽出トリガ到達)
- [ADR-031](adr-031-weekly-review-pipeline.md) — 週次レビューパイプライン (step 2 の棚卸し出力先)
- [ADR-049](adr-049-incident-eval-regression-suite.md) — incident→eval 回帰スイート (発火 0 でも維持する incident 由来ルールの区別)
- [ADR-012](adr-012-src-naming-convention.md) — src/ 命名規約 (`lib-` prefix)
- [ADR-026](adr-026-cargo-workspace.md) — Cargo workspace (新 crate の members 追記)
- [ADR-041](adr-041-test-isolation-patterns.md) — テスト隔離 (env kill-switch テストの serial 化)
- [ADR-059](adr-059-hook-system-message-visibility.md) — systemMessage 可視化 (session-start nudge 計装が bounded lifetime 判定の観測基盤)
