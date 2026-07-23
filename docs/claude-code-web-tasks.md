# Claude Code Web 対応可能タスクリスト

> **状態**: 試験運用 (本ドキュメントは「Claude Code Web セッションで着手するタスクのピックアップ scope」を切り出した ephemeral artifact であり、列挙された全タスクが land したら役割を終える)
>
> **作成経緯**: [docs/todo-summary.md](todo-summary.md) のタスク数増加に伴い、Linux 環境の Claude Code Web でも着手できるタスク（= Windows ベースの hooks/パイプラインへの実行依存がないドキュメント修正系）を抽出するため、2026-05-16 に作成。
>
> **scope 境界**: リポジトリ内のファイル編集に閉じる。当初 (2026-05-16) は Rust ビルド/テスト/Windows hook 実行が成功条件にならない **ドキュメント修正系** に限定していた ([§採用タスク](#採用タスク))。2026-07-23 に Windows/Linux クロスプラットフォーム対応 (CI = `.github/workflows/release-binaries.yml` が ubuntu-22.04 で `cargo test --workspace` をゲート実行、`scripts/cloud-setup.sh` が Linux プリビルドバイナリ + jj 0.42 を配置) が整ったため、**成功条件が `cargo test` (+ 必要に応じ `cargo clippy`) で検証完結する Rust 実装・テスト・lint タスク** も scope に追加した ([§採用タスク (2)](#採用タスク-2-cargo-test-検証タスククロスプラットフォーム対応後2026-07-23))。実 Windows hook 発火 / `pnpm push` パイプライン end-to-end / Windows 固有ランタイム挙動が成功条件になるタスクは引き続き対象外。

## 採用タスク

判定基準:

1. 編集対象がリポ内ファイル（`docs/` 配下 / `.claude/custom-lint-rules.toml` 内コメント / Rust ソースのコメント）
2. Rust ビルド / Windows hook / pnpm パイプラインの実行が成功条件に **ならない**
3. [docs/todo-summary.md](todo-summary.md) の表で採用判定済み（`feedback_no_unenforced_rules.md` 例外として既存実践の明文化に該当）

| 順位 | Tier | 内容 | 編集ファイル | 工数 |
|---|---|---|---|---|
| 120 | T3 | `takt-workflow-persona-without-model` rule コメント拡張（field 拡張手順 4-5 行）+ ADR-007 case study 追記（enumeration-based 正規表現層、Rust regex lookahead 非対応の pragmatic 対処）(PR #150 T1-#1、実体 Tier 3) | [.claude/custom-lint-rules.toml](../.claude/custom-lint-rules.toml) ルール⑨ + [docs/adr/adr-007-custom-linter-layer-boundary.md](adr/adr-007-custom-linter-layer-boundary.md) | XS |
| 134 | T3 | ADR-035 に docs-only PR 評価の適用外基準リスト追加（mutation / error handling / DRY / YAGNI / function length / test coverage / magic-number 等）(PR #156 T3 #2) | [docs/adr/adr-035-doc-evaluation-policy.md](adr/adr-035-doc-evaluation-policy.md) | S |

### 着手フロー

1. Claude Code Web で本ファイルを起点に対象タスクを 1 つ選ぶ
2. 該当ファイルを Read で確認し、編集内容を [docs/todo-summary.md](todo-summary.md) と該当 `docs/todoN.md` の詳細エントリに照らして固める
3. 編集後、本ファイルの該当行と [docs/todo-summary.md](todo-summary.md) の該当順位行を削除する（todo-summary.md の table 更新方針に従う）
4. 詳細エントリが置かれた `docs/todoN.md` の該当 section も削除する
5. PR を作成（commit 単位は task 単位、複数 task を 1 PR に束ねる場合は理由を PR description に明記）

---

## 採用タスク (2): cargo test 検証タスク（クロスプラットフォーム対応後、2026-07-23〜）

判定基準（docs-only の 3 基準に代えて、実装・テスト・lint タスク向け）:

1. 成功条件が `cargo test --workspace`（+ 必要に応じ `cargo clippy`）で検証完結する。CI (`release-binaries.yml`, ubuntu-22.04) が同一ゲート（同一コマンド・同一 toolchain）を持つため、Web セッションのローカル `cargo test` 結果は CI ゲートと整合する（乖離は環境差に限られ、最終判定は CI に委ねる）
2. 実際の Windows hook 発火 / `pnpm push` パイプライン end-to-end / Windows 固有ランタイム挙動が成功条件に **含まれない**
3. 対象ソース/テストに cwd 依存の `#[ignore]` 統合テスト・`cmd.exe` 依存がない（`#[ignore]` は `cargo test` デフォルトで skip されるため、同一 crate 内に存在してもデフォルト実行の検証には影響しない。ただし対象コードが当該 `#[ignore]` テストの被験対象なら着手時に `--ignored` でも確認する）

> **lint rule 追加の検証**: 新規 custom lint rule も `rule_test_coverage_check` / `incident_fixture_coverage_check` / `incident_eval.rs` E2E（`CARGO_BIN_EXE` = cargo ビルド exe を spawn し fixture を stdin JSON で投入、deployed exe パスや cmd.exe に非依存）の 3 つの cargo test 群で機械強制される。**実 hook 発火は不要**なので Web で検証完結する。
>
> **着手フロー**: [上記 §着手フロー](#着手フロー)に同じ（完了後に該当順位を収録する `docs/todo-summary.md` または `docs/todo-summary2.md`（順位 220 以降）の該当行 + `docs/todoN.md` の詳細エントリを削除）。加えて DoD として `cargo test --workspace`（+ 該当 crate の `cargo clippy`）green を PR description に記載する。詳細エントリ内の対象ファイルパスがリファクタで stale なことがあるため、着手時に実パス（下表「対象ファイル(実パス)」列）を優先する。

### Batch 1: 純テスト・軽微実装（即着手推奨、◎）

`cargo test` で完結し外部依存・設計判断が最小のもの。工数昇順。

| 順位 | Tier | 内容 | 対象ファイル (実パス) | 工数 | 注意 |
|---|---|---|---|---|---|
| 284 | T2 | `stale_check_enabled` (Option\<bool\>) の TOML パーステスト追加（未テストのパース経路を補完） | `src/hooks-session-start/src/hooks_config.rs`（`mod tests`、既存 `hooks_config_parses_session_start_staleness_section` 拡張） | XS | 純 deserialize。`temp_dir()` fixture で Linux CI pass 済みパターン、最もクリーン |
| 203 | T2 | GitHub token `ghu_` / `ghr_` の secret 検出ブロックテスト 2 件追加 | `src/hooks-pre-tool-validate/src/presets/safety/secret.rs` | XS | todo 記載の `main.rs` は module split でパスドリフト、実体は `secret.rs`。純 regex 判定 |
| 240 | T2 | `takt.rs` の spawn/try_wait `Err(_)` → `Err(e)` + `eprintln!`（原因握り潰し解消、`.failed` marker debug 改善） | `src/cli-merge-pipeline/src/feedback/takt.rs`（60・68 行） | XS | pnpm/takt の実実行は成功条件外。compile + clippy 通過で足りる |
| 180 | T2 | `escape_markdown_pipe(&str)` を pub 追加 + `format_table` の user field に適用 + 5 variant test（markdown table 破壊の防止 / prompt injection の緩和 = defense-in-depth の一層） | `src/lib-report-formatter/src/lib.rs` | XS-S | 外部依存ゼロの純 lib。既存 private `truncate()` と escape ロジック重複、DRY 整理（共通化 or 役割分担）を検討 |
| 228 | T2 | `evaluate_rate_limit_shortcut` の cr_clean 判定（`new_comments` / `actionable_comments` / `unresolved_threads` 3 field × None/Some 境界）の回帰テスト | `src/cli-pr-monitor/src/stages/poll/rate_limit_signal.rs`（末尾 tests） | S | pure 関数、silent-clean 誤認保護。同 crate の `#[ignore]` 統合テストは無関係 |
| 163 | T2 | cross_ref validator に percent-encode / GFM heading slug / relative path normalize の edge case fixture test 追加 | `src/cli-docs-lint/src/cross_ref.rs`（`#[cfg(test)]` mod） | S | 全て `tempfile::TempDir` 上で完結。実コードは canonicalize 非使用・percent-decode 仕様を実装から確認して期待値決定 |
| 339 | T2 | CR rate-limit 3 世代 format × 4 parse path（old/new/next/fallback）× 主要 CR state の複合マトリックステスト | `src/check-ci-coderabbit/src/decide.rs`（既存 `mod tests`） | S | 既存 helper（`pr309_incident_*` 等）と世代別書式を組み合わせるだけ。純 parse + decide |
| 178 | T2 | `state.rs` の behavioral invariant test を ADR-041 pattern（sentinel 事前投入 + mutation 不在 assert）で 3-5 件追加 | `src/cli-pr-monitor/src/state.rs` | S | **todo 提案の invariant #1/#2 は実挙動と不一致**。`update_state_from_check_result` の実挙動を読んで実在する invariant を再選定する |
| 239 | T2 | `filter_transcripts` の `read_dir` 非決定順を timestamp ソートで決定論化 + 回帰テスト | `src/cli-merge-pipeline/src/feedback/transcript.rs`（`filter_transcripts` + tests） | M | temp-dir に複数 jsonl 生成 → 順序 assert で完結。実 hook 発火不要 |

### Batch 2: 新規実装を伴う（○、要設計判断）

cargo test で検証完結するが、新規 module / lint rule / 軽微リファクタ / 依存追加判断を含む。

| 順位 | Tier | 内容 | 対象ファイル | 工数 | 注意 |
|---|---|---|---|---|---|
| 340 | T2 | `decide.rs` の rate_limit × positive-evidence 複合境界テスト + `main.rs` の rate_limit threading テスト | `src/check-ci-coderabbit/src/{decide,main}.rs` | S | (a) は純関数で容易。(b) は `main.rs` の呼び出し側を I/O 無しでテスト可能にする小さな合成関数抽出リファクタが要る |
| 216 | T2 | `no-workstream-seq-names-in-config` lint rule 追加（config comment 内 `PR-[0-9]+` を検出、`#NNN` は除外） | `.claude/custom-lint-rules.toml` + `src/hooks-post-tool-linter/src/custom_rules/rule_tests_extras.rs` + `tests/incident_eval.rs` + `tests/fixtures/incidents/{bad,good}/` + (dogfood) `.claude/hooks-config.toml` | S | 確立 12 rule / 11 incident パターン踏襲。Rust regex lookaround 不要（`\bPR-[0-9]+\b`）。dogfood は数行の text 編集 |
| 272 | T1 | cli-docs-lint に ADR 重複採番検出 + CLAUDE.md 索引整合チェック（新規 validator module） | `src/cli-docs-lint/src/adr_consistency.rs`（新規）+ `main.rs`（CheckMode dispatch 拡張） | S-M | 中核（validator + fixture test）は cargo test で完結。「pnpm lint:docs 経由の発火確認」は Web 外だが成功条件ではない。CLAUDE.md は docs_dir の親なので TempDir で fake 構造を組む |
| 334 | T1 | docs/todo\*.md 本文の順位番号表記を検出する custom lint rule（ADR-033 仕組み化、`paths=["docs/todo*.md"]` scope、table 行除外） | `.claude/custom-lint-rules.toml` + fixtures（216 と同基盤） | M | 検証経路は 216 と同じ cargo test。**regex FP 精緻化**（preamble の「順位 220 以降」等）+ **本文 dogfood cleanup の規模**を着手前に grep 見積り（todo 記載 S だが M 見込み） |
| 179 | T2 | rate-limit retry 境界（max_retries=0/1/3）で retry 継続 vs `action_required` 遷移の off-by-one を pin する parameterized テスト | `src/cli-pr-monitor/src/stages/poll/rate_limit.rs`（判定 L52）+ `config.rs`（L143-155） | S-M | **todo の「rstest 使用済」は誤り**（Cargo.lock に不在）。新 dev-dep 追加 or plain 複数 `#[test]` で代替を着手時判断。gh subprocess を踏まない早期 return 経路で構成する |

### 対象外（Web では完了不能 / 残価値枯渇）

- **順位 199**（multi-byte test coverage requirement）: 主成果物が `~/.claude/rules/common/testing.md` = リポジトリ外の global config で、Web の ephemeral Linux home では実配置・派生プロジェクト波及を検証できず `cargo test` ゲートも無い → ローカル（Windows 実環境）で実施
- **順位 162**（fail-closed error path test）: 実装 fix は既に適用済（`behind.is_none_or(...)`）+ 提案 3 テストのうち 2 件が既存で、残作業は `check_todo_staleness` の DI テスト 1 件のみ（DI refactor 前提）で残価値が低い。着手前に「残りは 1 件のみ」である点をユーザーに確認

---

## 周辺情報

### 採用しなかったタスク群 (1): グローバル `~/.claude/*` 編集が必要なタスク

[docs/todo-summary.md](todo-summary.md) で採用判定済みかつ純 docs 修正だが、編集対象が **ユーザーグローバル設定**（`~/.claude/rules/common/*.md` や `~/.claude/CLAUDE.md`）であるため、本リストには含めない。

**理由**:

- ローカル PC と Claude Code Web の作業環境が異なり、`~/.claude/` ディレクトリは Web の per-repo workspace には含まれない
- グローバル `CLAUDE.md` / `~/.claude/rules/*` をバージョン管理する仕組みを本リポジトリでは用意していないため、Web 側で編集しても本リポの PR には反映できない
- ローカル PC 側で着手するのが構造的に妥当

該当する順位（参考、本リストでは取り扱わない）: 44, 66, 79, 84, 93, 100, 105, 107, 108, 110, 111, 117, 122, 128, 133

### 採用しなかったタスク群 (2): 実装系 / CI/script 系 / 判断作業混在系

以下は本リストの対象外。Windows ローカル環境または別途調整が必要。

- **Rust 実装系（cargo test で検証完結しないもの）**: 順位 1, 2, 5, 8, 11, 19, 39, 49, 57, 81, 83, 91, 97, 121, 124, 125, 130, 131, 132 等
  - 実 Windows hook 発火 / `pnpm push` パイプライン end-to-end / deployed exe の dogfood が成功条件になるもの
  - **(2026-07-23 更新)** クロスプラットフォーム対応に伴い、成功条件が `cargo test --workspace` で完結する Rust 実装・テスト・lint タスクは [§採用タスク (2)](#採用タスク-2-cargo-test-検証タスククロスプラットフォーム対応後2026-07-23) へ移した（順位 162, 163, 178, 179, 180, 199, 203, 216, 228, 239, 240, 272, 284, 334, 339, 340 を精査、うち 199/162 は対象外判定）。残る候補（順位 16, 17, 36, 37, 42–46, 51, 52, 92, 145, 148–150 等）も cargo test 検証可能なら順次 §採用タスク (2) へ昇格しうる
- **CI/script 実装系**: 順位 6, 10, 95, 96
  - `gh` CLI / GitHub Actions workflow 整備で Web からも実行可能だが、本リポ初の `.github/workflows/*` 追加など影響範囲があり、ローカル dogfood と組み合わせる方が安全
- **判断作業混在系**: 順位 118
  - rule⑧ の paths filter 検討は ADR amendment との整合判断を含み、純 docs 修正には閉じない

---

## ライフサイクル

- 採用タスクが全て land したら本ファイルを retire する（`~/.claude/rules/common/docs-governance.md` § Retirement Workflow に従う、global path のため markdown link なし）
- retire 時の手順:
  1. 採用タスク欄が空になっていることを確認
  2. permanent value の移管は不要（本ファイルは scope 整理のための作業表で、永続価値となる decision はない）
  3. リポ内で本ファイルを参照する箇所を `grep -rn "claude-code-web-tasks.md"` で洗い出し、参照を除去
  4. 本ファイルを物理削除
