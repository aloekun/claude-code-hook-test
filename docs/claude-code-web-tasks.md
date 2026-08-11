# Claude Code Web 対応可能タスクリスト

> **状態**: 試験運用 / **定期更新される管理台帳** (2026-08-06 に ephemeral artifact から改訂。列挙タスクが 0 件になっても役割は終わらない → [§ライフサイクル](#ライフサイクル))
>
> **作成経緯**: [docs/todo-summary.md](todo-summary.md) のタスク数増加に伴い、Linux 環境の Claude Code Web でも着手できるタスク（= Windows ベースの hooks/パイプラインへの実行依存がないドキュメント修正系）を抽出するため、2026-05-16 に作成。
>
> **scope 境界**: リポジトリ内のファイル編集に閉じる。当初 (2026-05-16) は Rust ビルド/テスト/Windows hook 実行が成功条件にならない **ドキュメント修正系** に限定していた ([§採用タスク](#採用タスク))。2026-07-23 に Windows/Linux クロスプラットフォーム対応 (CI = `.github/workflows/release-binaries.yml` が ubuntu-22.04 で `cargo test --workspace` をゲート実行、`scripts/cloud-setup.sh` が Linux プリビルドバイナリ + jj 0.42 を配置) が整ったため、**成功条件が `cargo test` (+ 必要に応じ `cargo clippy`) で検証完結する Rust 実装・テスト・lint タスク** も scope に追加した ([§採用タスク (2)](#採用タスク-2-cargo-test-検証タスククロスプラットフォーム対応後2026-07-23))。実 Windows hook 発火 / `pnpm push` パイプライン end-to-end / Windows 固有ランタイム挙動が成功条件になるタスクは引き続き対象外。

## 自律実行可否の 2 段階分類

本ファイルは 2026-08-06 から、Claude Code Web セッションの pickup scope に加えて**夜間 todo 消化ループ（WP-18）の選択元**を兼ねる。両者は必要な自律度が違うため、実行可否を 2 段階に分ける。

| 段階 | 意味 | 前提 |
|---|---|---|
| **Web 実行可** | 人間が対話で補助できる前提で着手できる。曖昧な点はセッション中に確認して詰められる | 本ファイルの各表に載っていること自体がこの段階 |
| **無人可** | 補助なしで完結する。実装内容が台帳の記述だけで一意に決まり、着手時の設計判断が要らない | 上に加えて下記 3 条件をすべて満たす |

**無人可の判定条件**:

1. **着手時の判断が要らない** — 台帳の「注意」欄に「再選定する」「着手時判断」「見積り」「検討」といった、人間が決める前提の記述がない
2. **実装内容が一意** — 何をどこに書くかが台帳と対象ファイルの現物から決まる。設計の選択肢が複数残っていない
3. **重複の恐れがない** — 同一タスクの実装が未マージのブランチや進行中の PR に存在しない

3 は台帳だけでは判定できないため、定期棚卸し（→ § ライフサイクル）で確認する。**判定に迷ったら無人可にしない** — 誤って無人可にしたタスクは、夜間ループが人間の意図と違う実装で draft PR を作る形で失敗する。無人可にしなかったことによる損失は「Web セッションで人間が着手する」だけであり、非対称に軽い。

マークは**人間が付ける**（ADR-022 の責務分離）。夜間ループは無人可マークの有無を機械的に読むだけで、自分でこの判定をしない。

## 採用タスク

判定基準:

1. 編集対象がリポ内ファイル（`docs/` 配下 / `.claude/custom-lint-rules.toml` 内コメント / Rust ソースのコメント）
2. Rust ビルド / Windows hook / pnpm パイプラインの実行が成功条件に **ならない**
3. [docs/todo-summary.md](todo-summary.md) の表で採用判定済み（`feedback_no_unenforced_rules.md` 例外として既存実践の明文化に該当）

**現在 0 件**（2026-08-06 の棚卸しで最後の 2 件が land 済みと確認され削除。→ § 棚卸し履歴）。docs-only の候補が再び出た場合は上記 3 基準で本表へ追加する。

### 着手フロー

1. Claude Code Web で本ファイルを起点に対象タスクを 1 つ選ぶ
2. 該当ファイルを Read で確認し、編集内容を [docs/todo-summary.md](todo-summary.md) と該当 `docs/todoN.md` の詳細エントリに照らして固める
3. 編集後、本ファイルの該当行と [docs/todo-summary.md](todo-summary.md) の該当順位行を削除する（todo-summary.md の table 更新方針に従う）
4. 詳細エントリが置かれた `docs/todoN.md` の該当 section も削除する
5. PR を作成（commit 単位は task 単位、複数 task を 1 PR に束ねる場合は理由を PR description に明記）

### 夜間ループ（無人実装）でマージしたタスクの後始末

> **2026-08-10 制定。** 夜間ループ ([ADR-072](adr/adr-072-nightly-todo-loop.md)) が作った PR をマージするときは、**マージと同じタイミングで本ファイルから該当順位を削除する**。

**なぜ必要か**: 夜間ループの「着手済み」判定は **`claude/nightly-*` ブランチの存在だけ**を見る（[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 3）。本ファイルに完了を示す列は無く、`無人可` 列の `✅` は着手可否の印であって進捗ではない。したがって、後始末をしないと次の 2 つが起きる。

- **PR をマージしてブランチを削除すると、除外マーカーが消えて同じ順位が再選択される**
- 夜間ループは自分では後始末できない。Guard step が台帳の書き換えを禁止しているため（決定 6）、**完了の記録は人間側の責務**として残っている

実際に [#365](https://github.com/aloekun/claude-code-hook-test/pull/365) のブランチを手で削除した際、順位 203 が再選択されて [#373](https://github.com/aloekun/claude-code-hook-test/pull/373) が作られた。あれはクローズ由来だったが、**マージ由来でも同じことが起きる**（しかも完了済みタスクの重複実装になる）。

**手順**（上記「着手フロー」3〜4 と同じ削除を、マージ時に行う）:

1. 本ファイルの該当順位行を削除する
2. [docs/todo-summary.md](todo-summary.md) / [docs/todo-summary2.md](todo-summary2.md) の該当順位行を削除する
3. 詳細エントリが置かれた `docs/todoN.md` の該当 section を削除する
4. そのうえで PR をマージし、ブランチも削除する

**削除するのはマージした順位だけ。** クローズした夜間 PR の順位は**完了していない**ので本ファイルに残す。その場合はブランチが残る限り再選択されず、ブランチを整理した時点で再び選択対象へ戻る（[ADR-031](adr/adr-031-weekly-review-pipeline.md) § 残存ブランチ検出 が週次で棚卸しし、再挑戦は許容する方針）。

---

## 採用タスク (2): cargo test 検証タスク（クロスプラットフォーム対応後、2026-07-23〜）

判定基準（docs-only の 3 基準に代えて、実装・テスト・lint タスク向け）:

1. 成功条件が `cargo test --workspace`（+ 必要に応じ `cargo clippy`）で検証完結する。CI (`release-binaries.yml`, ubuntu-22.04) が同一ゲート（同一コマンド・同一 toolchain）を持つため、Web セッションのローカル `cargo test` 結果は CI ゲートと整合する（乖離は環境差に限られ、最終判定は CI に委ねる）
2. 実際の Windows hook 発火 / `pnpm push` パイプライン end-to-end / Windows 固有ランタイム挙動が成功条件に **含まれない**
3. 対象ソース/テストに cwd 依存の `#[ignore]` 統合テスト・`cmd.exe` 依存がない（`#[ignore]` は `cargo test` デフォルトで skip されるため、同一 crate 内に存在してもデフォルト実行の検証には影響しない。ただし対象コードが当該 `#[ignore]` テストの被験対象なら着手時に `--ignored` でも確認する）

> **lint rule 追加の検証**: 新規 custom lint rule も `rule_test_coverage_check` / `incident_fixture_coverage_check` / `incident_eval.rs` E2E（`CARGO_BIN_EXE` = cargo ビルド exe を spawn し fixture を stdin JSON で投入、deployed exe パスや cmd.exe に非依存）の 3 つの cargo test 群で機械強制される。**実 hook 発火は不要**なので Web で検証完結する。
>
> **着手フロー**: [上記 §着手フロー](#着手フロー)に同じ（完了後に該当順位を収録する `docs/todo-summary.md` または `docs/todo-summary2.md`（順位 220 以降）の該当行 + `docs/todoN.md` の詳細エントリを削除）。加えて DoD として `cargo test --workspace`（+ 該当 crate の `cargo clippy`）green を PR description に記載する。詳細エントリ内の対象ファイルパスがリファクタで stale なことがあるため、着手時に実パス（下表「対象ファイル(実パス)」列）を優先する。

### 「PRタイトル」列の書き方（2026-08-11 追加、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 17）

夜間ループが作る PR のタイトルは `<この列の値> (nightly-todo 順位 NNN)` になる。**翌朝 PR 一覧を見た人間が、中身を開かずに何が入っているか判断できる 1 行**を書くこと。

- **conventional commits の prefix を含める**（`feat(scope): ` / `test(scope): ` / `fix(scope): `）。`内容` 列から機械的に決められないため、ここで人間が選ぶ
- **60 文字以内**。超えると `…` で切り詰められる（PR 一覧で読む 1 行のため）
- **空でよい**。未記入なら従来の `feat: 順位 NNN の無人実装 (nightly-todo)` にフォールバックする。`無人可` が `—` の行は選ばれないので空のままでよい
- `内容` 列とは**用途が違う**。あちらは agent への依頼文で長くてよい。こちらはタイトル

### Batch 1: 純テスト・軽微実装（即着手推奨、◎）

`cargo test` で完結し外部依存・設計判断が最小のもの。工数昇順。

| 順位 | Tier | 無人可 | 内容 | 対象ファイル (実パス) | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 284 | T2 | — | `stale_check_enabled` (Option\<bool\>) の TOML パーステスト追加（未テストのパース経路を補完） | `src/hooks-session-start/src/hooks_config.rs`（`mod tests`、既存 `hooks_config_parses_session_start_staleness_section` 拡張） | XS | 純 deserialize。`temp_dir()` fixture で Linux CI pass 済みパターン、最もクリーン |  |
| 203 | T2 | ✅ | GitHub token `ghu_` / `ghr_` の secret 検出ブロックテスト 2 件追加 | `src/hooks-pre-tool-validate/src/presets/safety/secret.rs` | XS | todo 記載の `main.rs` は module split でパスドリフト、実体は `secret.rs`。純 regex 判定 | test(pre-tool-validate): GitHub token の secret 検出ブロックをテストする |
| 240 | T2 | ✅ | `takt.rs` の spawn/try_wait `Err(_)` → `Err(e)` + `eprintln!`（原因握り潰し解消、`.failed` marker debug 改善） | `src/cli-merge-pipeline/src/feedback/takt.rs`（60・68 行） | XS | pnpm/takt の実実行は成功条件外。compile + clippy 通過で足りる | fix(merge-pipeline): takt spawn/try_wait のエラー握り潰しを解消する |
| 180 | T2 | — | `escape_markdown_pipe(&str)` を pub 追加 + `format_table` の user field に適用 + 5 variant test（markdown table 破壊の防止 / prompt injection の緩和 = defense-in-depth の一層） | `src/lib-report-formatter/src/lib.rs` | XS-S | 外部依存ゼロの純 lib。既存 private `truncate()` と escape ロジック重複、DRY 整理（共通化 or 役割分担）を検討 |  |
| 228 | T2 | ✅ | `evaluate_rate_limit_shortcut` の cr_clean 判定（`new_comments` / `actionable_comments` / `unresolved_threads` 3 field × None/Some 境界）の回帰テスト | `src/cli-pr-monitor/src/stages/poll/rate_limit_signal.rs`（末尾 tests） | S | pure 関数、silent-clean 誤認保護。同 crate の `#[ignore]` 統合テストは無関係 | test(check-ci): rate-limit shortcut の cr_clean 判定をテストで固定する |
| 178 | T2 | — | `state.rs` の behavioral invariant test を ADR-041 pattern（sentinel 事前投入 + mutation 不在 assert）で 3-5 件追加 | `src/cli-pr-monitor/src/state.rs` | S | **todo 提案の invariant #1/#2 は実挙動と不一致**。`update_state_from_check_result` の実挙動を読んで実在する invariant を再選定する |  |
| 239 | T2 | ✅ | `filter_transcripts` の `read_dir` 非決定順を timestamp ソートで決定論化 + 回帰テスト | `src/cli-merge-pipeline/src/feedback/transcript.rs`（`filter_transcripts` + tests） | M | temp-dir に複数 jsonl 生成 → 順序 assert で完結。実 hook 発火不要 | fix(merge-pipeline): transcript の読み取り順を timestamp ソートで決定論化する |

### Batch 2: 新規実装を伴う（○、要設計判断）

cargo test で検証完結するが、新規 module / lint rule / 軽微リファクタ / 依存追加判断を含む。

| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 340 | T2 | — | `decide.rs` の rate_limit × positive-evidence 複合境界テスト + `main.rs` の rate_limit threading テスト | `src/check-ci-coderabbit/src/{decide,main}.rs` | S | (a) は純関数で容易。(b) は `main.rs` の呼び出し側を I/O 無しでテスト可能にする小さな合成関数抽出リファクタが要る |  |
| 216 | T2 | ✅ | `no-workstream-seq-names-in-config` lint rule 追加（config comment 内 `PR-[0-9]+` を検出、`#NNN` は除外） | `.claude/custom-lint-rules.toml` + `src/hooks-post-tool-linter/src/custom_rules/rule_tests_extras.rs` + `tests/incident_eval.rs` + `tests/fixtures/incidents/{bad,good}/` + (dogfood) `.claude/hooks-config.toml` | S | 確立 12 rule / 11 incident パターン踏襲。Rust regex lookaround 不要（`\bPR-[0-9]+\b`）。dogfood は数行の text 編集 | feat(post-tool-linter): config の workstream 連番名を lint する |
| 272 | T1 | — | cli-docs-lint に ADR 重複採番検出 + CLAUDE.md 索引整合チェック（新規 validator module） | `src/cli-docs-lint/src/adr_consistency.rs`（新規）+ `main.rs`（CheckMode dispatch 拡張） | S-M | 中核（validator + fixture test）は cargo test で完結。「pnpm lint:docs 経由の発火確認」は Web 外だが成功条件ではない。CLAUDE.md は docs_dir の親なので TempDir で fake 構造を組む |  |
| 334 | T1 | — | docs/todo\*.md 本文の順位番号表記を検出する custom lint rule（ADR-033 仕組み化、`paths=["docs/todo*.md"]` scope、table 行除外） | `.claude/custom-lint-rules.toml` + fixtures（216 と同基盤） | M | 検証経路は 216 と同じ cargo test。**regex FP 精緻化**（preamble の「順位 220 以降」等）+ **本文 dogfood cleanup の規模**を着手前に grep 見積り（todo 記載 S だが M 見込み） |  |
| 179 | T2 | — | rate-limit retry 境界（max_retries=0/1/3）で retry 継続 vs `action_required` 遷移の off-by-one を pin する parameterized テスト | `src/cli-pr-monitor/src/stages/poll/rate_limit.rs`（判定 L52）+ `config.rs`（L143-155） | S-M | **todo の「rstest 使用済」は誤り**（Cargo.lock に不在）。新 dev-dep 追加 or plain 複数 `#[test]` で代替を着手時判断。gh subprocess を踏まない早期 return 経路で構成する |  |

### 無人可としなかった 7 件の理由

「注意」欄の記述と § 自律実行可否の 2 段階分類 の 3 条件を突き合わせた結果。将来この判断を見直す際、根拠を再調査せずに済むよう残す。

| 順位 | 満たさない条件 | 該当箇所 |
|---|---|---|
| 284 | 3（重複の恐れ） | 未マージの `claude/select-next-task-a9aiam` に同タスクの実装が乗っている。タスク自体は 1・2 を満たすので、ブランチが決着したら無人可へ昇格しうる |
| 178 | 1（着手時の判断） | 「実挙動を読んで実在する invariant を**再選定する**」— 何をテストするか自体が未確定 |
| 334 | 1（着手時の判断） | 「regex FP 精緻化 + 本文 dogfood cleanup の規模を着手前に**grep 見積り**」 |
| 179 | 1（着手時の判断） | 「新 dev-dep 追加 or plain `#[test]` で代替を**着手時判断**」— 依存を増やす判断は無人でしない |
| 180 | 2（実装内容が一意でない） | 「既存 private `truncate()` と escape ロジック重複、DRY 整理（共通化 or 役割分担）を**検討**」 |
| 340 | 2（実装内容が一意でない） | 「`main.rs` の呼び出し側を I/O 無しでテスト可能にする小さな**合成関数抽出リファクタ**が要る」— 抽出の切り方が未確定 |
| 272 | 2（実装内容が一意でない） | 新規 validator module の設計（検査項目の分割・エラー表現）が台帳から一意に決まらない |

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

## 棚卸し履歴

台帳の鮮度は「行が消えていること」でしか表現されないため、削除の根拠をここに残す。削除理由が追えないと、後から「なぜこのタスクは消えたのか」を再調査する羽目になる。

### 2026-08-06

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 120 | 採用タスク | land 済みのため削除 | `docs/todo-summary.md` / `todo-summary2.md` の順位 table から消えている。実体も確認済み — [ADR-007](adr/adr-007-custom-linter-layer-boundary.md) に negation by enumeration の case study（Rust regex が lookahead 非対応である旨と代替案 3 択の比較）が存在する |
| 134 | 採用タスク | land 済みのため削除 | 同上。[ADR-035](adr/adr-035-doc-evaluation-policy.md) に docs-only PR の適用外基準リスト（mutation / DRY / YAGNI 等）が存在する |

これで docs-only の採用枠は 0 件になった。ただし本ファイルは retire しない（→ § ライフサイクル）。

---

## ライフサイクル

### 2026-08-06 の改訂: ephemeral artifact → 定期更新台帳

旧 lifecycle は「採用タスクが全て land したら retire」だった。これは本ファイルが Claude Code Web セッションの pickup scope を切り出しただけの作業表だった頃の想定である。

WP-18 の夜間 todo 消化ループがここを**タスク選択元**として読むようになると、この lifecycle は成り立たない。台帳が空になった瞬間にファイルごと消えると、ループの入力が消滅する。そもそも `docs/todo-summary.md` に新しいタスクが登録され続ける以上、「全部 land して終わり」という状態は来ない。

したがって本ファイルは**空になっても retire しない**。空は「今は無人で回せるタスクが無い」という正常な状態で、夜間ループはその場合に何も作らずに終わる（fail-closed）。

### 定期更新（週次）

更新は **weekly-review と同じタイミング**で行う。専用のスケジュールを増やさないのは、台帳の鮮度が落ちる速度が todo corpus の decay と同じ周期だから。接続は `.takt/facets/instructions/review-todo-whole.md`（weekly-review workflow の観点⑤）が担い、台帳の鮮度を検査して findings として上げる。

棚卸しで見るもの:

1. **land 済み行の削除** — `docs/todo-summary.md` / `todo-summary2.md` の順位 table から消えた行を削除し、根拠を [§棚卸し履歴](#棚卸し履歴) に記帳する。対象は**現行のタスク表のみ**（Batch 1 / Batch 2 と、非空なら [§採用タスク](#採用タスク) の表）で、[§棚卸し履歴](#棚卸し履歴) と [§無人可としなかった 7 件の理由](#無人可としなかった-7-件の理由) は対象外 — どちらも順位列を持つが、削除済み順位を意図的に残す記録だから
2. **新規候補の昇格** — todo-summary 側に増えたタスクのうち、[§採用タスク (2) の判定基準](#採用タスク-2-cargo-test-検証タスククロスプラットフォーム対応後2026-07-23)**または** [§採用タスク の判定基準](#採用タスク)（docs-only）のいずれかを満たすものを、対応する表へ追加する。docs-only 枠は現在 0 件だが再追加を許しているため、両方の経路を見ないと候補を取りこぼす
3. **無人可マークの見直し** — [§自律実行可否の 2 段階分類](#自律実行可否の-2-段階分類)の 3 条件を再確認する。特に条件 3（重複の恐れ）は台帳の外にある未マージブランチ・進行中 PR を見ないと判定できないため、この棚卸しでしか確認できない

1〜3 の実行と採否は**人間が決める**（ADR-022）。facet は read-only で findings を上げるだけで、本ファイルを編集しない。

### retire 条件

夜間ループ（WP-18）が終了し、Web セッションの pickup scope としても不要になった時点で retire する（`~/.claude/rules/common/docs-governance.md` § Retirement Workflow に従う、global path のため markdown link なし）。手順:

1. 本ファイルを読む自動化（夜間 workflow / weekly-review facet）が撤去済みであることを確認
2. permanent value の移管を確認 — 現時点で永続価値を持つのは [§自律実行可否の 2 段階分類](#自律実行可否の-2-段階分類)の判定条件のみ。retire 時に ADR へ移す
3. リポ内で本ファイルを参照する箇所を `grep -rn "claude-code-web-tasks.md" .` で洗い出し、参照を除去（検索対象パス `.` を省くと標準入力待ちになり、参照を 1 件も見つけないまま「参照なし」と誤認する）
4. 本ファイルを物理削除
