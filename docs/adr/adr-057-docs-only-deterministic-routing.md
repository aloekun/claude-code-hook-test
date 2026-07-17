# ADR-057: docs-only / 空 diff の決定論 routing — instruction 規約から決定論機構への昇格

## ステータス

試験運用 (2026-07-18) / **dogfood 中 (判定期限 2026-08-15)**

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) の
> 対象。ランタイム機能なので 3 点セット (config opt-in / kill-switch / bounded lifetime) を
> そのまま適用する (後述「ADR-039 3 点セットの適用」)。

## コンテキスト

`docs/push-pipeline-fix-plan.md` の T11。docs-only push の所要時間短縮を目的とする。

### 問題: docs-only push が Rust の重い gate を毎回払っていた

`pnpm push` の quality_gate は `rust-lint-test` group
(`cargo clippy --workspace` + `cargo test` + `cargo test -- --ignored`) を含み、
これが gate の律速だった (2026-07-18 実測 ~50s、warm target)。

`docs/**` / `*.md` だけを変更する PR (ADR / todo / 計画文書の編集) でも、この Rust group を
毎回実行していた。**docs の変更は Rust のコンパイル・テスト対象に一切含まれない**ため、
PR 範囲が docs-only なら working copy の Rust ソースは base branch と同一であり、
`cargo clippy` / `cargo test` の結果は base が緑である限り**変わり得ない**
(演繹的に不変)。この group の実行は docs-only push では純粋な待ち時間だった。

### 既存の関連実装

- **post-PR 側の先行実装**: `cli-pr-monitor` の auto-push 前 gate
  ([ADR-043](adr-043-security-gates-fail-closed.md) 由来、PR #224 対策) は、takt fix 後の
  fix diff が docs-only なら gate を skip する。docs-only 判定 (`is_docs_only_summary`) は
  そこに既に存在していた。
- **[ADR-035](adr-035-doc-evaluation-policy.md) (docs-only PR 評価ポリシー)**: docs-only の
  path 基準 (どのパスを docs とみなし、どの除外パスを code-equivalent とするか) の
  single source of truth。ただし従来は **AI reviewer の instruction (prompt) に対する規約**
  であって、pre-push の決定論層には未適用だった。

### 空 diff は既に処理済み

diff が完全に空 (レビュー対象なし) のケースは `main.rs` の
`run_diff_and_lint_screen` が `DiffResult::Empty` で takt を skip する経路が既にある。
本 ADR は **docs はあるが code は無い** ケース (空 diff ではない) を扱う。

## 決定 (試験運用)

### docs-only 判定を決定論層に昇格する

PR 範囲 `<default_branch>..@` の `jj diff --summary` を quality_gate の**前**に取得し、
ADR-035 の path 基準で docs-only 判定する。docs-only なら Rust の quality_gate group
(既定 `rust-lint-test`) を skip する。ADR-035 の instruction 規約を決定論機構へ昇格させる
形 ([ADR-042](adr-042-rule-vs-mechanism-boundary.md) の「ルール → 仕組み化」の方向)。

### takt (AI レビュー) は skip しない — skip の範囲は演繹可能な部分に限る

**path 判定から演繹できるのは「Rust テスト結果が不変」までで、「レビュー不要」は演繹できない。**
docs の内容・cross-ref 整合性・trust boundary は誤り得る (ADR-035 §適用する criteria が
docs-only にも trust boundary / cross-reference / markdown lint を**適用する**と明記)。
[ADR-056](adr-056-review-policy-anomaly-shadow.md) の T10 dogfood では、reviewer が
docs diff の事実誤り (行数の記載ミス) を checklist ノイズ 0 件で検出した実績がある。
したがって skip するのは決定論的に結果が変わらない group だけとし、**takt は維持する**。
JS 系 group (`lint` の `pnpm lint:docs`) は docs の markdown lint そのものなので維持される。

この「演繹できる範囲だけを落とす」方針は ADR-043 (fail-closed) の精神でもある:
判定に確信が持てない部分はフル実行に倒す。

### 判定範囲は PR 範囲 (`<base>..@`)、単一コミット (`@`) ではない

`[diff]` stage は `jj diff -r @` (単一コミット) を使うが、本 stage は
`pr_size_check` と同じ `<default_branch>..@` (PR 範囲) を使う。quality_gate は
working copy 全体をビルド・テストするので、判定すべきは「push される差分全体が
docs-only か」である。`@` 単独が docs-only でも祖先コミットが Rust に触れていれば
gate は必要で、単一コミット判定は祖先の code 変更を見逃す穴になる。

### ADR-035 path 基準を単一実装に集約 (`lib-docs-policy`)

判定を必要とする決定論層が pre-push (本 stage) と post-PR (`cli-pr-monitor` gate) の
2 箇所になったため、`is_docs_only_summary` を新 crate `lib-docs-policy` に集約した。
ADR-035 は「docs-only 判定が facet ごとに分散して drift した」ことを問題として起案された
ADR であり、その判定の実装が複数箇所に増えることは ADR-035 が防ごうとした drift の
再生産にあたる。`cli-pr-monitor` の既存実装は本 crate の呼び出しに置き換え、重複を解消した。

内容基準 (doc comment のみの `.rs` 変更等、ADR-035 の 2 つ目の判定軸) は AST 解析を要し
path 文字列からは判定できないため本 crate は扱わない。該当ケースは docs-only でないと
判定され、フル実行に倒れる (fail-closed)。

## ADR-039 3 点セットの適用

- **Config opt-in**: `push-runner-config.toml` の `[docs_only_routing]` section で
  default OFF。section 不在 / `enabled != true` は完全 skip (= 従来どおり全 group 実行)。
  本リポジトリは `enabled = true` で dogfood。派生 repo の templates は section を置かない。
- **Kill-switch**: `enabled = false` で完全停止。env `DOCS_ONLY_ROUTING_DISABLE=1` で
  個別 push の意図的バイパス (docs-only でも Rust gate を強制実行したいとき)。
- **Bounded lifetime**: dogfood 開始 (2026-07-18) から約 4 週間 = **判定期限 2026-08-15**。
  3-5 docs-only PR の dogfood で **誤 skip** (docs-only と判定されたが実は Rust に影響していた)
  が無いことを確認できたら default-ON 昇格 (templates へ反映)、観測されたら却下。判定結果は
  本 ADR のステータス行 + `push-runner-config.toml` の `[docs_only_routing]` コメント +
  `src/cli-push-runner/src/stages/docs_only_routing.rs` module doc に反映する。

## 影響

### 期待効果

- docs-only push で `rust-lint-test` group (実測 ~50s) を落とす。2026-07-18 実測の warm gate
  内訳は rust-lint-test 50s / JS 系 (lint/test/build) 計 ~6s であり、この group が gate の
  律速。docs-only push の quality_gate は数秒台に短縮される。
- takt は維持されるため docs のレビュー品質 (cross-ref / trust boundary / 事実確認) は不変。

> **期待効果の見積もりに関する注意**: `push-pipeline-fix-plan.md` §5 T11 の当初見積もり
> 「docs-only push -6〜8 分」は §1 の古いベースライン由来で、T1/T4 の実測 (docs 相当 push は
> 既に合計 ~151s = quality_gate 49.7s + takt 97.8s) と乖離していた。本 ADR は実測に基づき
> 効果を **-~50s (quality_gate の rust group 分)** と見積もる。T1 (Ollama eval) / T2 (crate 削除)
> と同型の「計画時見積もりが実測で下方修正される」例。

### リスク

- **base branch が緑でないと skip の前提が崩れる**: skip の演繹は「base branch の Rust が
  緑」を前提とする。これは post-PR 側 gate (`cli-pr-monitor`) が既に置いている前提と同一で、
  CI と CodeRabbit が安全網として残る (skip されるのは pre-push の Rust gate のみ)。
- **誤 skip (path 基準の穴)**: ADR-035 の path 基準が code-equivalent なパスを docs と
  誤判定すると、Rust に影響する変更で gate を skip し得る。除外パス (`.takt/` / `.claude/`)
  を明示し、判定不能はフル実行に倒すことで抑止するが、新 path pattern 出現時は ADR-035 側の
  更新が必要。dogfood の観測対象はまさにこの誤 skip。
- **`default_branch` の cross-config coupling**: `[pr_size_check]` と `[docs_only_routing]`
  が別々に `default_branch` を持ち、食い違うと一方が誤った PR 範囲を見る
  ([ADR-051](adr-051-cross-system-config-coupling.md))。両 section のコメントに同期義務を明記。

### 検証

- 実装の全分岐 (docs-only / code / mixed / 除外パス / jj 失敗 / 空 / kill-switch) を
  純関数の unit test で固定 (jj 呼び出しは closure 注入、ADR-021 原則 3)。
- 配布 exe による実 jj repo での before/after: docs-only working copy では
  `rust-lint-test` が skip され gate 通過、code 変更では実行され gate 停止、
  kill-switch env / `enabled = false` では routing が効かずフル実行 (fail-safe 方向) を確認。
- dogfood 期間 (〜2026-08-15) で誤 skip が観測されないこと。

## 関連

- [ADR-035: docs-only PR 評価ポリシー](adr-035-doc-evaluation-policy.md) — path 基準の
  single source of truth。本 ADR はそれを決定論層に適用する
- [ADR-042: ルール vs 仕組み化の境界基準](adr-042-rule-vs-mechanism-boundary.md) — instruction
  規約 → 決定論機構への昇格という本 ADR の位置づけ
- [ADR-043: Security/Quality Gate での Fail-Closed 原則](adr-043-security-gates-fail-closed.md)
  — 判定不能はフル実行に倒す
- [ADR-039: Experimental feature 標準パターン](adr-039-experimental-feature-standard-pattern.md)
  — 本 ADR の 3 点セット
- [ADR-051: クロスシステム設定 coupling](adr-051-cross-system-config-coupling.md)
  — `default_branch` の 2 section 間 coupling
- [ADR-056: takt builtin review policy の shadow](adr-056-review-policy-anomaly-shadow.md)
  — takt を skip しない判断の根拠 (docs でも reviewer が事実誤りを検出した実績)
- [ADR-021: jj 変更検出ロジックの設計原則](adr-021-jj-change-detection-principles.md)
  — revset 合成と jj 呼び出しの closure 注入
