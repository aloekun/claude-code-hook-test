# ADR-049: incident→eval 回帰スイート (カスタムルールの由来 incident 再現テスト)

## ステータス

試験運用 (2026-07-06)

## コンテキスト

カスタムリントルール ([.claude/custom-lint-rules.toml](../../.claude/custom-lint-rules.toml)、`hooks-post-tool-linter` が適用) は
12 本あり、うち **11 本は実 incident (過去 PR で発生した具体的な事故) を由来**とする ([ADR-007](adr-007-custom-linter-layer-boundary.md) の正規表現層)。
既存の `rule_test_coverage_check` は「各ルールが対応 test 関数を宣言していること」をゲート化するが、
**ルールが由来 incident を今も検出できるか (= ハーネス自身の退行)** を機械検出する仕組みは無かった。

WP-08 (`docs/harness-improvement-plan.md`) は、各ルールを生んだ実 incident を再現する fixture を整備し、
それを hook に食わせて block/warn を assert する回帰スイートで「ハーネス自体の退行」を機械検出する。

## 決定

**incident→eval 回帰スイート**を導入する。標準的な 2 つの手法 — 回帰テスト (バグ修正には再現テストを付ける) と
linter の fixture corpus (ESLint / Clippy 等の「引っかかる例 / clean な例」) — の応用で、これに
**provenance ポリシー** (各ルールは由来 incident とその再現 fixture を機械可読に持つ) を重ねる。

### 1. provenance の構造化 ([rules.incident])

`[.claude/custom-lint-rules.toml](../../.claude/custom-lint-rules.toml)` の incident 由来 11 ルールに
`[rules.incident]` meta field を追加し、`CustomRule.incident: Option<CustomRuleIncident>` として parse する:

```toml
[rules.incident]
pr = 75                             # 由来 incident の PR 番号
bad_fixture = "no-personal-paths.md"    # tests/fixtures/incidents/bad/ 配下 (fire すべき入力)
good_fixture = "no-personal-paths.md"   # tests/fixtures/incidents/good/ 配下 (fire しない clean 入力)
adr = "adr-007"                     # 設計根拠 ADR (任意)
```

追跡鎖: **incident (PR) → rule (id) → fixture (bad/good) → regression test → ADR**。数年後に
「このルールは消してよいか」を問う開発者が全て辿れる。section を持たないルール (rule① `no-console-log`
= 汎用サンプル、incident 由来でない) は fixture 要求から免除する (`NON_INCIDENT_RULES` allowlist)。
これにより「12 ルール中 11 が incident 由来」という齟齬も明示的に扱える。

### 2. fixture 設計 (1 fixture = 1 failure mode + good/bad)

`tests/fixtures/incidents/{bad,good}/` に配置。設計原則:

- **1 fixture = 1 failure mode**: 各 bad fixture は該当ルールの incident パターン**のみ**含む
  (LLVM / rustc の UI test 流儀)。将来「何が検出できなくなったか」を一点に絞れる。
- **good (negative) fixture 必須**: bad は fire する、good (clean な対応) は fire しないことを両方保証し、
  検出退行だけでなく **false positive 退行**も防ぐ (linter では同等に重要)。
- **テストデータ明示**: 各 fixture 冒頭コメントで synthetic test data であることと由来 PR を明示。

### 3. Hook E2E test (実 exe spawn)

[src/hooks-post-tool-linter/tests/incident_eval.rs](../../src/hooks-post-tool-linter/tests/incident_eval.rs) は
内部関数呼び出しではなく **実 exe を `CARGO_BIN_EXE_*` で spawn** し、`PostToolUse` JSON を stdin に流して
stdout を parse する。これで **arg/stdin パース → config → rule → feedback JSON → stdout** の全経路を通す
真の E2E になる (内部 API だけ呼ぶと exe の shell を通らない)。assert は **`type` / `severity` / `line` のみ**に
限定し (feedback 全文は固定しない)、文言修正で test が壊れない。

- パス filter付きルール (rule⑨ `takt-workflow-persona-without-model`、`paths = [".takt/workflows/*.yaml"]`)
  は temp CWD 配下の `.takt/workflows/` に fixture を stage し相対パスで invoke して path filter も検証する。
- exe は `custom-lint-rules.toml` を自身の隣から解決するため、test は deploy 済 toml を build 先へ copy する。

### 4. coverage gate (fail-closed)

既存 `rule_test_coverage_check` と同じ crate 内 cargo test として `incident_fixture_coverage_check` を追加。
各 incident 由来ルールが `[rules.incident]` を持ち bad/good fixture が**実在**することを **fail-closed** で強制する
(hard assert、[ADR-043](adr-043-security-gates-fail-closed.md))。fixture 欠落 / ルールが incident を検出しなくなった
場合は **cargo test が失敗**する — これは開発者向けの test 失敗であって、本番の Edit/Write を止めるものではない。

### 5. fixture の隔離 (ハーネス運用を壊さない)

fixture は意図的に「悪い」内容を含むため、ハーネス自身のゲートから隔離する:

- `src/**` の外 (repo-root `tests/fixtures/`) に置き、`deployed_tests.rs` の clean-baseline 走査 (`src/**/*.rs` /
  `.takt/workflows/*.yaml`) に触れない。
- `[.markdownlint-cli2.jsonc](../../.markdownlint-cli2.jsonc)` の ignores に追加。
- `.rs` fixture のヘッダは comment-lint ([ADR-036](adr-036-bundle-z-three-layer-review.md) #B-α) に触れないよう
  doc コメント (`//!`) を使う。
- カスタムリンター自体は非致命 (exit 0、feedback のみ) のため、fixture を編集しても運用は止まらない。

## ADR-039 との関係

prompt/test 資産の追加であり、[ADR-039](adr-039-experimental-feature-standard-pattern.md) の config opt-in は
そのままは適用しない。bounded lifetime として、dogfood 期間で「fixture 追加忘れ / ルール検出退行を実際に
機械検出できた」ことを確認したら `試験運用` を解除する。可逆性は fixture / gate の revert で担保。

## 帰結

### 利点

- ルールの検出力退行と false positive 退行を cargo test で機械検出 (ハーネス自身の回帰スイート)。
- 各ルールが由来 incident と再現 fixture を機械可読に持ち、削除可否判断が追跡可能。
- 実 exe E2E で hook の全経路 (stdin/config/feedback/exit) を保証。
- 本 repo 初の exe-spawn integration test パターンを確立 (WP-16 CI smoke test で流用可能)。

### 欠点 / 留意点

- fail-closed のため、incident 由来 11 ルール分の fixture を揃えて一括で land する必要がある (部分導入は gate 赤)。
- exe-spawn は内部関数テストより遅い (22 spawn)。回帰網羅性とのトレードオフとして許容。
- ルールの extensions / pattern を変更する際は対応 fixture も更新が必要 (既存 `rule_test_coverage_check` と同じ保守義務)。

### 関連 ADR

- [ADR-007](adr-007-custom-linter-layer-boundary.md) — custom-linter 正規表現層 (対象 11 ルールの居所 + per-rule test checklist)
- [ADR-036](adr-036-bundle-z-three-layer-review.md) — Bundle Z 3 層 review (comment-lint #B-α、fixture ヘッダの制約源)
- [ADR-042](adr-042-rule-vs-mechanism-boundary.md) — rule vs 仕組み化 (「11 custom lint rule」の由来カウント)
- [ADR-043](adr-043-security-gates-fail-closed.md) — ゲートの fail-closed 原則
