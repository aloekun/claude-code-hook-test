# Claude Code Web 対応可能タスクリスト

> **状態**: 試験運用 / **定期更新される管理台帳** (2026-08-06 に ephemeral artifact から改訂。列挙タスクが 0 件になっても役割は終わらない → [§ライフサイクル](#ライフサイクル))
>
> **作成経緯**: [docs/todo-summary.md](todo-summary.md) のタスク数増加に伴い、Linux 環境の Claude Code Web でも着手できるタスク（= Windows ベースの hooks/パイプラインへの実行依存がないドキュメント修正系）を抽出するため、2026-05-16 に作成。
>
> **scope 境界**: リポジトリ内のファイル編集に閉じる。当初 (2026-05-16) は Rust ビルド/テスト/Windows hook 実行が成功条件にならない **ドキュメント修正系** に限定していた ([§採用タスク](#採用タスク))。2026-07-23 に Windows/Linux クロスプラットフォーム対応 (CI = `.github/workflows/release-binaries.yml` が ubuntu-22.04 で `cargo test --workspace` をゲート実行、`scripts/cloud-setup.sh` が Linux プリビルドバイナリ + jj 0.42 を配置) が整ったため、**成功条件が `cargo test` (+ 必要に応じ `cargo clippy`) で検証完結する Rust 実装・テスト・lint タスク** も scope に追加した ([§採用タスク (2)](#採用タスク-2-cargo-test-検証タスククロスプラットフォーム対応後2026-07-23))。実 Windows hook 発火 / `pnpm push` パイプライン end-to-end / Windows 固有ランタイム挙動が成功条件になるタスクは引き続き対象外。

## 自律実行可否の 2 段階分類

本ファイルは 2026-08-06 から、Claude Code Web セッションの pickup scope に加えて**夜間 todo 消化ループ（WP-18）の選択元**を兼ねる。両者は必要な自律度が違うため、実行可否を 2 段階に分ける。

| 段階 | 意味 | 決まり方 |
|---|---|---|
| **Web 実行可** | 人間が対話で補助できる前提で着手できる。曖昧な点はセッション中に確認して詰められる | 本ファイルの各表に載っていること自体がこの段階 |
| **無人可 (= auto lane)** | そのタスクを**夜間ループに割り当てた**という表明。補助なしで完結すると人間が判断したもの | **人間が `✅` を付けたときにそうなる**。下記の判断材料を満たすことは自動的な昇格を意味しない |

**条件を満たすこと自体は `✅` を意味しない。** 下記は人間が lane を割り当てるときに使う**判断材料**であって、機械的に適用すると `✅` が決まる資格要件ではない (→ [§ 無人可列は担当割り当て（lane）である](#無人可列は担当割り当てlaneである2026-08-16adr-072-決定-18))。

**lane 割り当ての判断材料**:

1. **着手時の判断が要らない** — 台帳の「注意」欄に「再選定する」「着手時判断」「見積り」「検討」といった、人間が決める前提の記述がない
2. **実装内容が一意** — 何をどこに書くかが台帳と対象ファイルの現物から決まる。設計の選択肢が複数残っていない

**判定に迷ったら無人可にしない** — 誤って無人可にしたタスクは、夜間ループが人間の意図と違う実装で PR を作る形で失敗する。無人可にしなかったことによる損失は「Web セッションで人間が着手する」だけであり、非対称に軽い。

**判断材料 1 の確認は「注意」欄のキーワード走査に依存する。** したがって本台帳へ行を追記する者（weekly-review skill の昇格追記を含む）は、詳細エントリ（`docs/todoN.md`）にある判断留保の記述 —「再選定」「着手時判断」「見積り」「検討」など人間が決める前提の語 — を要約で圧縮・省略せず「注意」欄へ転記しなければならない。転記が落ちると、詳細エントリでは判断が残っているタスクが台帳上は判断材料 1 を満たして見え、lane を割り当てる人間の目が構造的に塞がれる。

転記する原文が上記の例示語をどれも含まない同義表現（「未定」「どちらでもよい」「複数案あり」等）の場合は、**正準タグを付して「着手時判断: <原文>」の形で転記する**。キーワード走査は例示語しか見ないため、タグ無しの同義表現は判断が残っているのに素通りする。

### 無人可列は担当割り当て（lane）である（2026-08-16、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 18）

**本台帳は担当割り当て表である。** `無人可` 列は「自動化しても壊れないかの資格判定」ではなく、**そのタスクを誰が持っているか**を表す。

| 列の値 | lane | 意味 |
|---|---|---|
| `✅` | **auto** | 夜間ループに割り当て済み。そのタスクは夜間ループの所有物 |
| `—` | **human** | ユーザー + Claude Code に割り当て。夜間ループは触らない |

上の判断材料 1・2 は、**人間が lane を割り当てるときに使うもの**であって、満たせば自動的に `✅` になる資格要件ではない。マークを付けるのは常に人間である（[ADR-022](adr/adr-022-automation-responsibility-separation.md) の責務分離）。**夜間ループは auto の先頭 1 件を機械的に取るだけで、「本当に簡単か」「他で誰かが実装していないか」を再審査しない。**

**競合は検出するのではなく、競合する割り当てをしない。** `✅` を付けたタスクを人間が引き取るなら、着手する前に `—` へ変える。この規律が守られていれば重複は起こらず、守られていなければどんな検出ゲートも取りこぼす。

> **2026-08-16 に条件 3（重複の恐れがない = 同一タスクの実装が未マージのブランチや進行中の PR に存在しない）を廃止した。** 同じ `claude/nightly-<順位>` ブランチを、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 3 は「着手済み = 再選択しない」と読み、条件 3 は「未マージ実装がある = マークを降格せよ」と読んでいた。同じ事実から逆の結論が出る状態が、誤った週次 finding（WR-2026-08-13-T01/T02、実行すれば未完了タスク 3 件が本台帳から消えるところだった）を生んだ。代替のゲートは設けない — 上記の割り当て規律がその役割を持つ。

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

**削除するのはマージした順位だけ。** クローズした夜間 PR の順位は**完了していない**ので本ファイルに残す。

### 夜間 PR を close するときの lane 操作（2026-08-16、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 20）

close は「この成果物は採らない」という判断であって、「このタスクをやめる」とも「人間がやる」とも決まっていない。**どちらなのかを `無人可` 列で表明する。**

| close 時の意図 | 操作 | その後 |
|---|---|---|
| **人間が引き取る** | `✅` を `—` へ変更する（ブランチ / 失敗マーカーがあれば削除） | 夜間ループは以後この順位を選ばない |
| **仕様を直して再投入する** | `✅` のまま。ブランチ / 失敗マーカーを削除する | 掃除後に再び選択対象へ戻る |

**`✅` のまま close する = 再投入の意思表示である。** 夜間ループは起動時に「決着済み (closed / merged) の PR に紐づく `claude/nightly-*` ブランチ」を自動で掃除するため、放置すると翌晩以降に同じ順位が再選択される。意図しない再実装を避けたいなら、close と同時に lane を `—` へ移すこと。

**PR の無い `claude/nightly-<順位>` ブランチは掃除されない。** それは夜間ループが implement 後に停止したときの**失敗マーカー**（同決定 19）で、人間が確認するまでその順位は選択されない。確認後、上表のどちらかの操作で決着させる。

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

### 「対象ファイル」列の書き方（2026-08-15 制定、機械強制あり）

この列は**後始末の完了判定に使う機械可読フィールド**である。「宣言された成果物がすべて変更されたか」で完了を判定するため、**書き漏らした成果物はそのまま判定の穴になる**。実際に夜間 PR [#394](https://github.com/aloekun/claude-code-hook-test/pull/394) は lint rule の fixture だけを追加してマージされ、rule 本体が無いまま完了扱いになりかけた。

書式（`lib-ledger` の `parse_target_files` が解釈し、`cargo test` が実台帳の全行を毎回検証する）:

- **成果物はすべてバッククォートで囲む。** 囲みの無い散文（`+ fixtures` 等）は成果物として抽出されず、**欠けていても検証を通過してしまう**ため error にする
- **リポジトリ相対パスで書く。** `main.rs` のような裸のファイル名はどの crate か決まらないため error。`src/cli-docs-lint/src/main.rs` と書く
- **複数の成果物は `+` で並べる**
- **丸括弧（全角・半角とも）の中は注釈**として無視される。行番号・「新規」・関数名などを自由に書いてよい（例: 「（`mod tests`、既存 `xxx` 拡張）」）
- **`{a,b}` は展開され、展開結果の全てが要求対象になる**（例: `tests/fixtures/incidents/{bad,good}/` は bad/good 両方の変更を要求する）
- パス区切りは `/`。絶対パス・`..` は不可

例:

```text
`src/cli-docs-lint/src/adr_consistency.rs`（新規）+ `src/cli-docs-lint/src/main.rs`（CheckMode dispatch 拡張）
```

### Batch 1: 純テスト・軽微実装（即着手推奨、◎）

`cargo test` で完結し外部依存・設計判断が最小のもの。工数昇順。

| 順位 | Tier | 無人可 | 内容 | 対象ファイル (実パス) | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 284 | T2 | — | `stale_check_enabled` (Option\<bool\>) の TOML パーステスト追加（未テストのパース経路を補完） | `src/hooks-session-start/src/hooks_config.rs`（`mod tests`、既存 `hooks_config_parses_session_start_staleness_section` 拡張） | XS | 純 deserialize。`temp_dir()` fixture で Linux CI pass 済みパターン、最もクリーン |  |
| 240 | T2 | ✅ | `takt.rs` の spawn/try_wait `Err(_)` → `Err(e)` + `eprintln!`（原因握り潰し解消、`.failed` marker debug 改善） | `src/cli-merge-pipeline/src/feedback/takt.rs`（60・68 行） | XS | pnpm/takt の実実行は成功条件外。compile + clippy 通過で足りる | fix(merge-pipeline): takt spawn/try_wait のエラー握り潰しを解消する |
| 180 | T2 | — | `escape_markdown_pipe(&str)` を pub 追加 + `format_table` の user field に適用 + 5 variant test（markdown table 破壊の防止 / prompt injection の緩和 = defense-in-depth の一層） | `src/lib-report-formatter/src/lib.rs` | XS-S | 外部依存ゼロの純 lib。既存 private `truncate()` と escape ロジック重複、DRY 整理（共通化 or 役割分担）を検討 |  |
| 228 | T2 | ✅ | `evaluate_rate_limit_shortcut` の cr_clean 判定（`new_comments` / `actionable_comments` / `unresolved_threads` 3 field × None/Some 境界）の回帰テスト | `src/cli-pr-monitor/src/stages/poll/rate_limit_signal.rs`（末尾 tests） | S | pure 関数、silent-clean 誤認保護。同 crate の `#[ignore]` 統合テストは無関係 | test(check-ci): rate-limit shortcut の cr_clean 判定をテストで固定する |
| 178 | T2 | — | `state.rs` の behavioral invariant test を ADR-041 pattern（sentinel 事前投入 + mutation 不在 assert）で 3-5 件追加 | `src/cli-pr-monitor/src/state.rs` | S | **todo 提案の invariant #1/#2 は実挙動と不一致**。`update_state_from_check_result` の実挙動を読んで実在する invariant を再選定する |  |

### Batch 2: 新規実装を伴う（○、要設計判断）

cargo test で検証完結するが、新規 module / lint rule / 軽微リファクタ / 依存追加判断を含む。

| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 340 | T2 | — | `decide.rs` の rate_limit × positive-evidence 複合境界テスト + `main.rs` の rate_limit threading テスト | `src/check-ci-coderabbit/src/{decide,main}.rs` | S | (a) は純関数で容易。(b) は `main.rs` の呼び出し側を I/O 無しでテスト可能にする小さな合成関数抽出リファクタが要る |  |
| 272 | T1 | — | cli-docs-lint に ADR 重複採番検出 + CLAUDE.md 索引整合チェック（新規 validator module） | `src/cli-docs-lint/src/adr_consistency.rs`（新規）+ `src/cli-docs-lint/src/main.rs`（CheckMode dispatch 拡張） | S-M | 中核（validator + fixture test）は cargo test で完結。「pnpm lint:docs 経由の発火確認」は Web 外だが成功条件ではない。CLAUDE.md は docs_dir の親なので TempDir で fake 構造を組む |  |
| 179 | T2 | — | rate-limit retry 境界（max_retries=0/1/3）で retry 継続 vs `action_required` 遷移の off-by-one を pin する parameterized テスト | `src/cli-pr-monitor/src/stages/poll/rate_limit.rs`（判定 L52）+ `src/cli-pr-monitor/src/config.rs`（L143-155） | S-M | **todo の「rstest 使用済」は誤り**（Cargo.lock に不在）。新 dev-dep 追加 or plain 複数 `#[test]` で代替を着手時判断。gh subprocess を踏まない早期 return 経路で構成する |  |

### 無人可としなかった理由

「注意」欄の記述と § 自律実行可否の 2 段階分類 の判定条件を突き合わせた結果。将来この判断を見直す際、根拠を再調査せずに済むよう残す。

> **見出しから件数を外した（2026-08-16）。** 行の増減のたびに見出しの数詞を直す必要があり、実際に順位 334 の retire で不整合になりかけた。

| 順位 | 満たさない条件 | 該当箇所 |
|---|---|---|
| 284 | （条件ではなく lane の割り当て） | 未マージの `claude/select-next-task-a9aiam` に同タスクの実装が乗っており、人間が決着させる。**2026-08-16 の条件 3 廃止後もこの行は human lane のまま**残す — 条件を満たすかどうかではなく、担当が人間だという記録である（[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 18）。夜間ループへ渡すなら人間が `✅` を付ける |
| 178 | 1（着手時の判断） | 「実挙動を読んで実在する invariant を**再選定する**」— 何をテストするか自体が未確定 |
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

### 2026-08-14

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 239 | 採用タスク (2) Batch 1 | マージ済みのため削除 | 夜間 PR [#391](https://github.com/aloekun/claude-code-hook-test/pull/391) が 2026-08-14 にマージ。実体も確認済み — `src/cli-merge-pipeline/src/feedback/transcript.rs` に `jsonl_paths.sort_by_key(\|path\| transcript_ordering_key(path))` が存在し、`docs/todo-summary2.md` の順位 table からも削除済み |
| 216 | 採用タスク (2) Batch 2 | 本 PR で完成させたため削除 | 夜間 PR [#394](https://github.com/aloekun/claude-code-hook-test/pull/394) は fixture 2 ファイルのみで rule 本体が無く**未完了だった**（→ [§ 未完了のままマージされた順位](#未完了のままマージされた順位)）。本 PR で rule 定義・rule test 5 件・E2E case・dogfood を実装し、完了基準（`.toml`/`.yaml`/`.yml`/`.jsonc` の `PR-` + 数字を warning 検出、`PR #NNN` は非検出）を満たしたうえで削除した |

### 2026-08-16

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 334 | 採用タスク (2) Batch 2 | **前提消滅のため retire** | 「`docs/todo*.md` 本文の順位番号表記を検出する custom lint rule」は [ADR-033](adr/adr-033-todo-numbering-simplification.md) の本文使用禁止を仕組み化するタスクだった。同 ADR § 改訂（2026-08-16）で**禁止規約そのものを緩和**したため、検出すべき違反が存在しなくなった。順位 table 行・詳細エントリ（`docs/todo14.md`）も同時に削除。**land したのではなく、やる理由が消えた**点が上 2 節と異なる |

---

## 未完了のままマージされた順位

夜間 PR がマージされても**タスクが完了しているとは限らない**。マージは「その PR の内容を取り込む」判断であって「台帳の完了基準を満たした」判定ではなく、両者を突き合わせる機構が現状どこにも無い。ここには実際に起きた事例と、そこから設けた機構を記録する。

**現在 live な事例は無い**（順位 216 は 2026-08-14 に完成させて削除済み。→ [§ 棚卸し履歴](#棚卸し履歴)）。本節を残すのは、下記の失敗モードと対処が再発防止の根拠として参照され続けるため。

### 事例: 順位 216（2026-08-14）

| PR | 入った成果物 | 欠けていた成果物 |
|---|---|---|
| [#394](https://github.com/aloekun/claude-code-hook-test/pull/394) | `tests/fixtures/incidents/{bad,good}/no-workstream-seq-names-in-config.toml`（2 ファイル・計 6 行） | `.claude/custom-lint-rules.toml` の rule 定義、rule test、`tests/incident_eval.rs` の E2E case、dogfood |

**なぜ CI を通ったのか**: custom lint rule の 3 つの機械チェックは**すべて「rule → fixture/test」の向き**にしか働かなかった。

- `rule_test_coverage_check`: toml の各 rule に対応 test が実在するか
- `incident_fixture_coverage_check`: incident 由来 rule に bad/good fixture が実在するか
- `cases_cover_every_incident_rule`: `CASES.len()` == `[rules.incident]` の個数

いずれも rule の存在を起点に検査するため、**rule を伴わない孤児 fixture は 3 チェックすべてを素通り**した。「新規 lint rule は 3 つの cargo test 群で機械強制される」という [§ 採用タスク (2)](#採用タスク-2-cargo-test-検証タスククロスプラットフォーム対応後2026-07-23) の記述は、**rule を書いた場合にのみ成立**していた。

**対処**: 逆向きの `orphan_fixture_check`（`src/hooks-post-tool-linter/src/custom_rules/coverage.rs`）を追加し、「fixture があるなら必ず rule がある」を fail-closed で強制した。この検査があれば #394 は CI で落ちてマージできなかった。

**残る一般的リスク**: 上記は lint rule クラスに固有の対処であり、「マージ ≠ 完了」という失敗モード自体は他のタスククラスに残る。完了基準を機械可読にして削除前に検証する仕組み（push 前セルフレビューでの決定論的な台帳自動削除 + 実装確認）を別途構築する。

## 昇格検査履歴 — 廃止（2026-08-16）

**本 section は廃止した。表は 1 行も記帳されないまま終わった。**

これは「週次レビューの LLM に `docs/todo-summary*.md` の全順位（251 件規模）を毎週判定させ、不適格と判定した順位を積んで検査対象を収束させる」ための収束機構だった。**2 週連続で機能しなかった** — 2026-08-13 は 164 件中約 50 件のサンプリングで「候補 0 件」、2026-08-15 は 251 件中 13 件しか判定せず同じく「候補 0 件」と報告した（instruction は完全に届いていた。強化を 2 回試みて 2 回とも失敗している）。

**廃止の理由は「動かなかったから」ではなく、lane モデルで不要になったから**である（[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 18）。昇格 = 人間の割り当て判断であり、週次レビューが出すべきものは「判定済みの証跡」ではなく**判断の材料**（台帳未掲載の順位一覧）だけになった。材料は決定論的に計算できるので、収束機構も記帳義務も要らない。

## ライフサイクル

### 2026-08-06 の改訂: ephemeral artifact → 定期更新台帳

旧 lifecycle は「採用タスクが全て land したら retire」だった。これは本ファイルが Claude Code Web セッションの pickup scope を切り出しただけの作業表だった頃の想定である。

WP-18 の夜間 todo 消化ループがここを**タスク選択元**として読むようになると、この lifecycle は成り立たない。台帳が空になった瞬間にファイルごと消えると、ループの入力が消滅する。そもそも `docs/todo-summary.md` に新しいタスクが登録され続ける以上、「全部 land して終わり」という状態は来ない。

したがって本ファイルは**空になっても retire しない**。空は「今は無人で回せるタスクが無い」という正常な状態で、夜間ループはその場合に何も作らずに終わる（fail-closed）。

### 定期更新（週次）

更新は **weekly-review と同じタイミング**で行う。専用のスケジュールを増やさないのは、台帳の鮮度が落ちる速度が todo corpus の decay と同じ周期だから。接続は `.takt/facets/instructions/review-todo-whole.md`（weekly-review workflow の観点⑤）が担い、台帳の鮮度を検査して findings として上げる。

棚卸しで見るもの:

1. **land 済み行の削除** — `docs/todo-summary.md` / `todo-summary2.md` の順位 table から消えた行を削除し、根拠を [§棚卸し履歴](#棚卸し履歴) に記帳する。対象は**現行のタスク表のみ**（Batch 1 / Batch 2 と、非空なら [§採用タスク](#採用タスク) の表）で、[§棚卸し履歴](#棚卸し履歴) と [§無人可としなかった理由](#無人可としなかった理由) は対象外 — どちらも順位列を持つが、削除済み順位を意図的に残す記録だから
2. **昇格候補の材料提示** — `docs/todo-summary.md` + `docs/todo-summary2.md` の全順位から本台帳の現行タスク表に既載の順位を引いた**差集合（台帳未掲載の順位一覧）**を提示する。**判定はしない** — どれを台帳へ載せるか、載せた行の lane を `✅` にするか `—` にするかは、いずれも人間の割り当て判断である（[§ 無人可列は担当割り当て（lane）である](#無人可列は担当割り当てlaneである2026-08-16adr-072-決定-18)）。

   **差集合の計算は決定論層が行う**（LLM に全件判定させる旧方式は 2 週連続で機能せず廃止した → [§ 昇格検査履歴 — 廃止（2026-08-16）](#昇格検査履歴--廃止2026-08-16)）。検査済み順位の記帳も、収束のための除外リストも持たない — 毎回全順位から差集合を取り直すだけで、状態を持たずに同じ結果が出る。
3. **lane の見直し** — `✅` の行が今も夜間ループの持ち物でよいかを確認する。人間が着手した / 着手する予定のタスクが `✅` のままなら `—` へ移す（この操作自体は人間が行う）。

1〜3 の実行と採否は**人間が決める**（ADR-022）。facet は read-only で findings を上げるだけで、本ファイルを編集しない。

### retire 条件

夜間ループ（WP-18）が終了し、Web セッションの pickup scope としても不要になった時点で retire する（`~/.claude/rules/common/docs-governance.md` § Retirement Workflow に従う、global path のため markdown link なし）。手順:

1. 本ファイルを読む自動化（夜間 workflow / weekly-review facet）が撤去済みであることを確認
2. permanent value の移管を確認 — 現時点で永続価値を持つのは [§自律実行可否の 2 段階分類](#自律実行可否の-2-段階分類)の判定条件のみ。retire 時に ADR へ移す
3. リポ内で本ファイルを参照する箇所を `grep -rn "claude-code-web-tasks.md" .` で洗い出し、参照を除去（検索対象パス `.` を省くと標準入力待ちになり、参照を 1 件も見つけないまま「参照なし」と誤認する）
4. 本ファイルを物理削除
