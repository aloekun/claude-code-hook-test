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

**未掲載タスクから新たに `✅` 行を起こすときの手順は [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) に従う。** 上記 2 点は既存行を読むときの判断材料であり、ADR-074 は候補の列挙から除外クラスの適用、対象パスの実在確認までの手順を定める。特に **ADR-074 決定 4（宣言する対象ファイルの実在を機械確認する）は本台帳へ行を追記する前に必ず実施する** — 漂流したパスを宣言すると、実装が正しくても完了検証で停止する。

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

1. 編集対象がリポ内ファイル（`docs/` 配下 / `config/custom-lint-rules.toml` 内コメント / Rust ソースのコメント）
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

**掃除されるのは「決着済み PR の head commit を指したままのブランチ」だけ**（2026-09-05 改訂、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 20）。次の 2 つは掃除されない。

- **PR の無い `claude/nightly-<順位>` ブランチ** — 夜間ループが implement 後に停止したときの**失敗マーカー**（同決定 19）
- **決着済み PR と同名でも、ref がその PR の head と別 commit を指すもの** — 過去にその順位で PR を出したあとに作られた失敗マーカーがこれに当たる。**PR の履歴はブランチ名で永続する**ため、名前だけで束ねると後から作られたマーカーを消してしまう（順位 324 が 2026-08-31 / 09-01 の 2 晩これで消され、同じ順位が再選択され続けた）

どちらも人間が確認するまでその順位は選択されない。**過去に PR を出した順位でも、マーカーは自動では消えない** — 確認後、上表のどちらかの操作で決着させること。`cli-stale-branch-scan` のレポートでは「参考: ref が PR の head を指していないブランチ」節に出る。

### 夜間 PR を人間名義で作り直す（2026-10-12 からの暫定運用）

**夜間ループが作った PR は、CodeRabbit のレビューを受けられない。** CodeRabbit は 2026-09 下旬から、seat を持たない bot 作者の PR をレビューしなくなった (スキップ通知は「This PR was authored by a bot without an assigned CodeRabbit review seat」)。人間が `@coderabbitai review` / `full review` を投稿しても覆らず、Free プランでは seat を手動で割り当てることもできない。一方、本リポジトリは public なので、人間が作った PR は seat 無しでレビューされる。そこで、夜間 PR は人間名義で作り直してレビューを受ける ([ADR-085](adr/adr-085-third-party-review-means.md) 決定 2)。初回の適用は #557 → #559 (順位 433) で、CodeRabbit のレビューが完了することを確認した。

**作り直しの合図**: 夜間 PR の `Request` (review-request workflow) が red になり、ログに `[REVIEW_REQUEST_SEAT]` の案内が出る。案内には、その PR の番号とブランチを埋めたコマンドが並ぶ (2026-10-14 から、[ADR-085](adr/adr-085-third-party-review-means.md) 決定 6)。

**手順**:

1. 夜間 PR を close する。理由と「ブランチは削除しない」旨をコメントに残す
2. **続けてすぐに**、同じブランチから人間名義で PR を作る。`RANK` に順位、`TITLE` に夜間 PR と同じタイトル、`BODY_FILE` に本文のファイルを入れてから実行する: `pnpm create-pr --head "claude/nightly-$RANK" --base master --title "$TITLE" --body-file "$BODY_FILE"`。本文は夜間 PR のものを引き継ぎ、冒頭に作り直しの旨を書く
3. close した夜間 PR に、作り直した PR への参照をコメントで残す
4. 台帳の `無人可` 列は `✅` のままにする。タスクは作り直した PR で進行中であり、ブランチがある限り再選択されない
5. 作り直した PR をマージするときは、夜間ループが作った PR と同じく「夜間ループ（無人実装）でマージしたタスクの後始末」の手順で、台帳の行・順位表の行・詳細エントリを削除する。夜間 PR のブランチが台帳の削除を含んでいれば (#559 はそうだった) 追加の作業は無い。削除しないままブランチが消えると、同じ順位が再選択される

**守ること**:

- **先に人間名義の PR を作ることはできない。** GitHub は同じ head と base の組み合わせで open な PR を 2 本作らせない (PR #558 の CodeRabbit 指摘)
- **close と作り直しの間に夜間 run (18:10 UTC 起動) を挟まない。** 夜間ループは起動時に「決着済み PR の head を指したままのブランチ」を掃除する ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 20)。1 本でも open な PR が同じブランチを指していれば掃除しない (`cli-stale-branch-scan` の `a_single_open_pr_outweighs_any_number_of_closed_ones` で固定済み)
- **別名のブランチから作らない。** 着手済み判定は `claude/nightly-<順位>` の名前で行うため別名では数えられず、close した元ブランチが掃除されて同じ順位が再選択される
- 作り直した PR も `claude/` で始まるブランチなので、[ADR-071](adr/adr-071-draft-pr-backpressure.md) の背圧には従来どおり数えられる
- `pnpm merge-pr` で PR 番号を指定するときは `pnpm merge-pr --pr "$PR"` と書く (`PR` に番号を入れる)。`--` を挟むと引数エラーで止まる

この運用は [ADR-085](adr/adr-085-third-party-review-means.md) で当面の手段として採用した。ADR-085 の再評価で手段が変わったら、この節を廃止するか置き換える。

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
- **末尾 `/` はディレクトリ宣言**で、配下のいずれか 1 ファイルの変更で充足する
- パス区切りは `/`。絶対パス・`..` は不可

**粒度（2026-09-04 追記、[ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 4）**: 宣言は「独立した成果物」ごとに、**確実に予測できる最も細かい単位**で書く。完了判定が実効的に守るのは rule 本体 / fixture / test のような独立成果物の欠落（#394）であり、同一 crate 内でどのファイルに変更が着地するかは守れない（宣言先に無意味な 1 行を足せば通るため）。着地ファイルが実装判断で変わり得るときは、ファイルを当て推量で書かずに crate の `src/` をディレクトリで宣言する。当て推量が外れると**正しい実装だけが落ちる**（2026-08-17〜09-03 に順位 228 / 356 / 310 で 3 回発生。同時期に順位 162 も同じ `[LEDGER_CLEANUP_BLOCK]` で停止したが、原因はファイルの当て推量ではなく既存ファイルが移動した参照パスの漂流であり、本粒度原則の対象外）。

**ディレクトリ宣言の注釈に、ファイル名を書かない**（2026-09-06 追記、2026-09-18 に「既存」の限定を撤去）。列挙は「どこに着地するか」の予測を宣言から注釈へ移すだけで、外れる余地は消えない。2026-09-05 の順位 356 は注釈が挙げた `staleness.rs`（順位 136 の working-copy staleness、本タスクと無関係）を agent が最初に読んで「台帳が現物と合わない」と判断し、2 ターンで停止した。配置は詳細エントリの対処案が持つ（そこが一意に定めていなければ auto lane に置けない）。**注釈に書くのは、宣言の粒度を変えた理由に限る。**

**未実在のファイル名も書かない**（2026-09-18 追記）。初版の機械強制は「実在するものだけ」を弾いており、順位 426 の「例 `facade_reexports.rs`」はその隙間を通っていた。2026-09-17 の run 35257221949 で agent が例示どおりの名前でファイルを作り、**成果物そのものが列挙として報告されて** `verify=failure` で停止した。**「実在しない」は状態であって性質ではない** — 例示した名前は agent が採用した瞬間に実在化する。検査は実在を見なくなり、台帳の文字列だけで判定する。

**「変更してはならないもの」の明示は「注意」列へ書く**（2026-09-18 追記、移設）。これは本来ファイル名を名指す用途であり、注釈に残すと機構は「候補として挙げた名前」と区別できない。区別しようとすれば語彙の意味解釈が機構に入り、規約が「書くな」から「正しく書け」へ後退する。順位 426 は既に注意列へ書いている（「統合テストなので `lib.rs` 自体は変更しない」）。

**判定はセル単位である。** ディレクトリ宣言が 1 つでもあるセルでは、ファイル宣言側に付けた注釈も検査対象になる。混在セル（順位 236 の形）では、どの注釈にもファイル名を書けない。

**バッククォート引用の有無は問わない**（2026-09-18 追記、[#506](https://github.com/aloekun/claude-code-hook-test/pull/506) CodeRabbit 指摘）。初版の機械強制は引用符付きの名前しか見ておらず、`（helper.rs を参照）` のように素で書いた名前が素通りしていた。agent が読むのは prompt に載る生の文字列で、markdown の装飾は害の成立に関係ない。**詳細エントリへのポインタ（markdown リンクで `todoN.md` を指す形）も注釈には書かない** — リンク記法だけを除外すると、同じ記法でソースファイル名を書けば素通りできるうえ、所在は注意列が持てばよい（夜間 workflow の prompt が `docs/todoN.md` を読むよう agent に指示済み）。

**拡張子を持たない語は対象外である。** `tempfile` のような語は crate 名・関数名・識別子とテキスト上区別できず、本節の書式が注釈にそれらを書くことを明示的に許している。検出側へ倒すと正当な注釈を落とし、規約が「書くな」から「正しく書け」へ後退する。拡張子は「英数のみ・英字を 1 文字以上含む」に限る（版番号 `0.42.0` をファイル名と取り違えないため）。

例:

```text
`src/cli-docs-lint/src/adr_consistency.rs`（新規）+ `src/cli-docs-lint/src/main.rs`（CheckMode dispatch 拡張）
`src/<crate>/src/`（ディレクトリ宣言。着地が実装判断で変わり得るとき。候補ファイルは書かない）
```

**詳細エントリが着地ファイルを固定しているなら、ディレクトリではなくそのファイルを宣言する。** ディレクトリ宣言は配下 1 ファイルの変更で充足するため、独立成果物が 2 つ以上あるタスク（例: 2 つの test module に同じ fixture を置く）では片方だけの実装が完了扱いになる（#394 型）。粒度原則の「確実に予測できる最も細かい単位」は、予測が確実になった時点でファイルへ戻ることを含む（順位 356、2026-09-06、CodeRabbit #480）。

### Batch 1: 純テスト・軽微実装（即着手推奨、◎）

`cargo test` で完結し外部依存・設計判断が最小のもの。工数昇順。

| 順位 | Tier | 無人可 | 内容 | 対象ファイル (実パス) | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 180 | T2 | — | `escape_markdown_pipe(&str)` を pub 追加 + `format_table` の user field に適用 + 5 variant test（markdown table 破壊の防止 / prompt injection の緩和 = defense-in-depth の一層） | `src/lib-report-formatter/src/lib.rs` | XS-S | 外部依存ゼロの純 lib。既存 private `truncate()` と escape ロジック重複、DRY 整理（共通化 or 役割分担）を検討 |  |
| 178 | T2 | — | `state.rs` の behavioral invariant test を ADR-041 pattern（sentinel 事前投入 + mutation 不在 assert）で 3-5 件追加 | `src/cli-pr-monitor/src/state.rs` | S | **todo 提案の invariant #1/#2 は実挙動と不一致**。`update_state_from_check_result` の実挙動を読んで実在する invariant を再選定する |  |
| 162 | T2 | — | fail-closed error path（`Option::None`）の個別テストを追加（`check_todo_staleness` / `build_todo_staleness_message` の None ケース独立検証） | `src/hooks-pre-tool-validate/src/todo_staleness.rs` | S | PR #177 の `behind.unwrap_or(0)` fail-closed 漏れ修正の回帰テスト。純関数。**2026-08-23 auto → human へ変更**: 本ファイル [§ 対象外](#対象外web-では完了不能--残価値枯渇) が本順位を「着手前に『残りは 1 件のみ』である点をユーザーに確認」と記録しており、`✅` と矛盾していた (PR #441 CodeRabbit 指摘)。対象外側の記述は 2026-08-23 の実測で 3 点とも裏が取れた — (a) `behind.is_none_or(...)` の fix は適用済み (todo_staleness.rs:136,189)、(b) `build_todo_staleness_message` の None ケースは 3 テストで既存、(c) 残るのは `check_todo_staleness` の early-return 以降で、同関数は DI 口を持たないため DI refactor が要る。着手前のユーザー確認と DI refactor の要否判断は [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 5 に当たるため human lane とする。**2026-08-23 宣言パス修正**: 旧記載 `main.rs` は漂流していた — 対象 2 関数 `check_todo_staleness` / `build_todo_staleness_message` は `todo_staleness.rs` にあり `main.rs` には 1 度も現れない (module 分割で移動したまま台帳が追随していなかった)。2026-08-23 の dry_run で `[LEDGER_CLEANUP_BLOCK]` により実際に停止した。順位 176 / 145 と同型 | test(pre-tool-validate): fail-closed の None ケースをテストで固定する |
| 199 | T2 | — | multi-byte 文字を含む string window test を標準 coverage requirement 化（境界テストの追加） | `src/cli-docs-lint/src/priority_inversion.rs` | S | 既存の `RESOLUTION_WINDOW_CHARS` は文字数基準。multi-byte で境界が崩れないことを固定する。**2026-09-04 auto → human へ変更**: 台帳の再スコープ（本ファイルへのテスト追加）と [todo10.md](todo10.md) の詳細エントリ（`~/.claude/rules/common/testing.md` へのチェックリスト追記 = リポジトリ外の文書タスク）が矛盾しており、agent は詳細エントリを読んで完了条件を確認する手順のため実装が一意に決まらない（[ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 5）。2026-08-24 の run 32760716062 が 0 変更で停止していた（当時は順位 488 のバグで run が green に見えた）。詳細エントリを再スコープに合わせて書き直したら `✅` へ戻す。マーカー `claude/nightly-199` は lane 変更に伴い削除する | test(docs-lint): multi-byte 文字を含む window の境界をテストで固定する |
| 143 | T2 | — | 複言語 fixture helper（日本語 / emoji / combining chars の 3 関数）を標準化して string-processing の境界テストを書きやすくする | `src/hooks-post-tool-linter/src/main.rs` | S | helper の追加のみ。既存テストの書き換えは範囲外。**2026-08-23 auto → human へ変更**: 宣言先 `main.rs` は 73 行の起点のみでテスト 0 件、string 処理のテストは `utf8_integrity.rs` 等の別モジュールにある (漂流)。ただし「複数モジュールが使う共有 fixture helper をどこに置くか」は台帳から一意に決まらず ([ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 5)、パスの機械的な差し替えでは塞げないため人間が実装先を決める | test(post-tool-linter): 複言語 fixture helper を標準化する |
| 428 | T2 | — | PR 番号を取る CLI の不正値（`--pr 0` 等）を弾く検査を足す | `src/cli-merge-pipeline/src/main.rs` | S | 既に `pr_number_zero_is_rejected` があるため、**未カバーの入口（他 exe の同種フラグ）を洗ってから足す**。gh の実実行は成功条件外。**2026-08-23 auto → human へ変更**: 夜間ループでは完了不能と判明したため。理由は 2 つで、いずれも 2026-08-23 の実測にもとづく。(1) **宣言先が既に完成している** — 宣言成果物 `src/cli-merge-pipeline/src/main.rs` は `parse_pr_flag` で `--pr` / `--feedback-only` の 0 を拒否済みで、テスト `pr_number_zero_is_rejected` / `feedback_only_pr_number_zero_is_rejected` も存在する。(2) **洗った結果、未カバーの入口が見つからない** — PR 番号フラグを持つもう一方の入口 `src/check-ci-coderabbit/src/main.rs` も `--pr` 経路で `pr == 0` を弾いており (main.rs:299)、`src/cli-pr-monitor` はそもそも PR 番号フラグを取らない (PR URL から取得する)。この状態で夜間ループが着手すると、正しい実装ほど「足すものが無い」に到達して `[NIGHTLY_DENY] 変更がありません` で停止し、無理に何かを足せば宣言先以外を触って `[LEDGER_CLEANUP_BLOCK]` で停止する。どちらに転んでも完了できない。残る価値は `check-ci-coderabbit` の 0 拒否に専用テストが無い点だが、それは宣言先と別ファイルであり、対象の再定義は人間が行う ([ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 5 = 実装内容が一意に定まらない) | fix(cli): PR 番号フラグの不正値を弾く |
| 368 | T3 | — | `describe_axes()`（deny 行の 4 軸表示）と `evaluate()`（実際の allow/deny 判定）が同一入力で食い違わないことを assert する regression test を追加 | `src/cli-fix-push-gate/src/checks.rs`（既存 `mod tests` 内） | S | 純関数どうしの一貫性検査。Allow / Denied の双方向を書く。誤 allow は起きず観測性の劣化のみなので Severity は中。**2026-08-23 auto → human へ変更**: 成果物 `src/cli-fix-push-gate/src/checks.rs` が [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パス `src/cli-fix-push-gate/` に当たる（[ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 3）。機械検査は順位 486 | test(fix-push-gate): describe_axes と evaluate の一貫性を固定する |
| 360 | T2 | — | push 経路の `cargo test` と CI の `cargo test --workspace` が同じ対象集合を回すことを assert するテストを追加 | `src/lib-autonomy-policy/tests/cargo_test_scope_parity.rs`（新規ファイル。既存の `workflow_awk_parity.rs` と同じ crate に置く。読む対象は `push-runner-config.toml` の `[[quality_gate.groups]]` `name = "rust-lint-test"`、`.github/workflows/ci.yml` の `cargo test --workspace`、ルート `Cargo.toml`） | S | 361 と同型。素の `cargo test` が `--workspace` と等価なのはルート `Cargo.toml` に `default-members` が無いため。**その不在を assert する**のが実質の検査。3 ファイルとも読むだけ。意図的に `default-members` を足すと fail することまで確認する。**2026-08-23 auto → human へ変更**: 成果物 `src/lib-autonomy-policy/tests/cargo_test_scope_parity.rs` が [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パス `src/lib-autonomy-policy/` に当たる（[ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 3）。同 ADR は本順位を「成果物が Rust ファイルなので誤除外するところだった」と記録しているが、その Rust ファイル自身が deny リストの別エントリに当たることを見ていなかった。機械検査は順位 486 | test(workspace): push と CI の cargo test 対象集合の一致を固定する |
| 361 | T2 | — | jj のバージョン文字列が `.github/workflows/ci.yml` と `scripts/cloud-setup.sh` で一致することを assert するテストを追加 | `src/lib-autonomy-policy/tests/jj_version_parity.rs`（新規ファイル。既存の `workflow_awk_parity.rs` と同じ crate に置く。読む対象は `.github/workflows/ci.yml` の `JJ_VERSION: "0.42.0"` と `scripts/cloud-setup.sh` の `readonly JJ_VERSION="${CLOUD_SETUP_JJ_VERSION:-0.42.0}"`） | S | **2 ファイルは読むだけで書き換えない**（`.github/**` は Guard の禁止パス）。ADR-051 の cross-system coupling 検査にあたる。片方だけ変えた状態で fail することまで確認する。**2026-08-23 auto → human へ変更**: 注意欄は `.github/**` が禁止パスであることを見ていたが、成果物 `src/lib-autonomy-policy/tests/jj_version_parity.rs` 自身も [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パス `src/lib-autonomy-policy/` に当たる（[ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 3）。**deny リストの一部だけと照合したのが誤り**。機械検査は順位 486 | test(jj-helpers): jj バージョンの ci.yml / cloud-setup.sh 一致を固定する |
| 447 | T2 | — | 台帳の `✅無人可` と判断留保キーワード (再選定 / 着手時判断 / 見積り / 検討) の矛盾を決定論層で検出する | `src/lib-ledger/src/deployed_ledger.rs` | S | 転記規約 (本ファイル § 自律実行可否の 2 段階分類) の機械強制。検査は台帳を読むだけで書き換えない。順位 486 と同一ファイル・同一層なので実装順の調整が要る。**`src/lib-ledger/` は Guard 禁止パス**のため human lane (順位 383 と同じ理由、ADR-074 除外クラス 3) | test(ledger): 無人可と判断留保キーワードの矛盾を検出する |
| 486 | T2 | — | auto lane の対象ファイルが Guard 禁止パスに当たる行を決定論的に弾く | `src/lib-ledger/src/deployed_ledger.rs` | S | **着手時判断: 禁止パスの読み先が未決定** (順位 454 は単一定義を作らず 3 箇所の一致テストで決着した)。deny リストをどこから読むかを着手時に決める必要がある。順位 447 と同一ファイル・同一層。**`src/lib-ledger/` は Guard 禁止パス**のため human lane (ADR-074 除外クラス 3) | test(ledger): auto lane の対象パスが Guard 禁止パスに当たる行を弾く |
| 540 | T3 | ✅ | stale-branch-scan の分類が scan → cleanup → マーカー作成 → 再 scan を 3 周しても変わらないことを、ls-remote 出力と PR 一覧を fixture にした純関数テストで固定する | `src/cli-stale-branch-scan/src/classify.rs`（`mod tests`） | S | git / jj / gh は起動しない。CI step にはしない (Guard 禁止パス)。決定事項は [docs/todo28.md](todo28.md) § 順位 540 (2026-10-12 確定) | test(stale-branch-scan): scan の冪等性を純関数テストで固定する |
| 541 | T3 | ✅ | transcript 縮約の e2e に、日本語・マルチバイトを含む entry とエラーの tool_result を含む entry を通し、出力にそのまま残ることを確かめる | `src/cli-merge-pipeline/src/feedback/transcript.rs`（既存 e2e と同じ `mod tests`） | S | 期待値は `transcript_compact.rs` の unit テストが固定済みの性質に合わせる。決定事項は [docs/todo28.md](todo28.md) § 順位 541 | test(merge-pipeline): 縮約 e2e で日本語とエラー全文の保持を確かめる |
| 547 | T3 | ✅ | `parse_tier` のテストに Tier 4 / 5 を足す。旧書式 (絵文字つき) のテストにも Tier 5 を足す | `src/cli-docs-lint/src/priority_inversion.rs`（`mod tests`） | XS | 旧書式は派生プロジェクトの台帳を読むための互換で、本リポジトリの台帳からは絵文字を外してある。決定事項は [docs/todo28.md](todo28.md) § 順位 547 | test(docs-lint): parse_tier の Tier 4 / 5 を固定する |
| 553 | T3 | ✅ | custom-lint の rule⑲ (`gh-json-files-truncated`) と rule⑳ (`ref-destroying-push-without-lease`) に、リポジトリ全体の clean-baseline テストを rule ごと 1 本ずつ足す | `src/hooks-post-tool-linter/src/custom_rules/deployed_tests.rs` | S | 走査は `src` / `scripts` / `.github` / `.takt`、除外は `target` / `node_modules` / `tests/fixtures` / `.takt/runs` とほかのドット始まり。rule は `config/custom-lint-rules.toml` から読み、書き写さない (順位 534)。既存違反は 0 件 (2026-10-12 実測)。決定事項は [docs/todo29.md](todo29.md) § 順位 553 | test(custom-lint): rule⑲⑳ のリポジトリ全体 baseline を固定する |
| 569 | T3 | ✅ | lib-telemetry のテストの残り 3 点を埋める: タイムスタンプのリテラルを命名済み定数に置き換える / ワークスペース走査の除外が効いていることを確かめる / reason の newtype の doctest を足す | `src/lib-telemetry/tests/id_contract.rs` + `src/lib-telemetry/src/reason.rs` | XS | 定数名は同じ crate の慣習 (`T_2026_04_01_1200` の形) に合わせる。決定事項は [docs/todo29.md](todo29.md) § 順位 569 | test(telemetry): id_contract の定数化と除外テスト、reason の doctest |
| 551 | T3 | ✅ | analyze-session facet の md をテキストとして読み、スキーマ例が縮約の実装 (落とす block type / 残すフィールド / 上限値) と一致することを cargo test で確かめる | `src/cli-merge-pipeline/src/feedback/transcript_compact.rs`（`mod tests`） | S | 読む対象は `.takt/facets/instructions/analyze-session.md` (変更しない)。抽出の目印は「各 entry のスキーマ (Rust 側で縮約済みの形):」の直後の json フェンス。決定事項は [docs/todo29.md](todo29.md) § 順位 551 | test(merge-pipeline): facet のスキーマ例と縮約の実装を照合する |
| 568 | T2 | ✅ | pre-tool-validate の exe を staging し、ブロック時の telemetry jsonl を検査する e2e を足す (custom regex と allowlist に無い名前は `custom-block` になり元の文字列が残らない / 名前付き preset はその名前になる / kill-switch で書かれない) | `src/hooks-pre-tool-validate/tests/telemetry_e2e.rs`（新規） | S | `tests/smoke.rs` には混ぜない。staging と ETXTBSY 回避のロックは smoke.rs から複製する (ADR-084)。config はテスト内で最小の toml を書く。決定事項は [docs/todo29.md](todo29.md) § 順位 568 | test(pre-tool-validate): telemetry を exe 経由で検査する |

### Batch 2: 新規実装を伴う（○、要設計判断）

cargo test で検証完結するが、新規 module / lint rule / 軽微リファクタ / 依存追加判断を含む。

| 順位 | Tier | 無人可 | 内容 | 対象ファイル | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 340 | T2 | — | `decide.rs` の rate_limit × positive-evidence 複合境界テスト + `main.rs` の rate_limit threading テスト | `src/check-ci-coderabbit/src/{decide,main}.rs` | S | (a) は純関数で容易。(b) は `main.rs` の呼び出し側を I/O 無しでテスト可能にする小さな合成関数抽出リファクタが要る |  |
| 179 | T2 | — | rate-limit retry 境界（max_retries=0/1/3）で retry 継続 vs `action_required` 遷移の off-by-one を pin する parameterized テスト | `src/cli-pr-monitor/src/stages/poll/rate_limit.rs`（判定 L52）+ `src/cli-pr-monitor/src/config.rs`（L143-155） | S-M | **todo の「rstest 使用済」は誤り**（Cargo.lock に不在）。新 dev-dep 追加 or plain 複数 `#[test]` で代替を着手時判断。gh subprocess を踏まない早期 return 経路で構成する |  |
| 536 | T2 | ✅ | jj-op-verify で、`cd` / `-R` / `--repository` を含むコマンドと background 実行のコマンドを照合しないようにする。quote 内の文言と破壊的コマンドを組み合わせた tokenizer テストも足す | `src/hooks-post-tool-jj-op-verify/src/main.rs` + `src/hooks-post-tool-jj-op-verify/src/tests.rs` | S | 判定は既存の tokenizer で行い、`cd` 先を解析して照合先を切り替えることはしない。background は hook 入力の `tool_input.run_in_background` で判定する。module doc の既知の限界へ 1 項足す。決定事項は [docs/todo28.md](todo28.md) § 順位 536 (2026-10-12 確定) | fix(jj-op-verify): cd 後と background のコマンドを照合しない |
| 542 | T2 | ✅ | testability gate の rename 行の解決を、括りがちょうど 1 組 (`{`・` => `・`}` が各 1 回、この順) のときだけにし、それ以外は Err にする | `src/cli-push-runner/src/stages/testability_gate/mod.rs` | S | Err の経路と分類 (`unparsable-status`) は変えない。決定事項は [docs/todo28.md](todo28.md) § 順位 542 | fix(testability-gate): 曖昧な rename の括りを推測で解決しない |
| 544 | T3 | ✅ | todo preamble の範囲表記で退役を書いたとき、範囲全体を退役と読むように直し、範囲の書式 × 退役の位置 × 実在の組合せを表駆動テストで固定する | `src/cli-docs-lint/src/todo_routing.rs` | S | 範囲全体を退役と読むのはユーザー判断 (2026-10-12)。テストは `mod tests` 内の表駆動で、proptest は使わない。ファイルが 800 行を超えるならテストを別ファイルへ分ける (ADR-080)。決定事項は [docs/todo28.md](todo28.md) § 順位 544 | fix(docs-lint): 範囲で書いた退役を範囲全体に効かせる |
| 552 | T3 | ✅ | mod 宣言の並び順を検査する cargo test を足し、既存違反 1 件を並べ直す | `src/cli-docs-lint/tests/mod_order.rs`（新規） + `src/cli-pr-monitor/src/stages/mod.rs` | S | 比較の単位は空行かコメントで区切られた連続ブロック。属性付きの mod は比較から除外し、可視性は区別しない。この条件の既存違反は宣言した 1 ファイルだけ (2026-10-12 実測)。決定事項は [docs/todo29.md](todo29.md) § 順位 552 | test(lint): mod 宣言の並び順を検査する |
| 554 | T2 | ✅ | custom-lint rule の exception を外したとき、incident fixture の good 側で 1 件以上の違反が出ることを確かめる test module を足す (exception を持つ全 rule が対象) | `src/hooks-post-tool-linter/src/custom_rules/` | M | mutation は exception の除去だけに限る。rule は `config/custom-lint-rules.toml` から読む。2026-10-12 時点で exception を持つ rule は 8 個。決定事項は [docs/todo29.md](todo29.md) § 順位 554 | test(custom-lint): exception が fixture で正当化されているか検査する |
| 555 | T2 | ✅ | deploy-hooks.ts が探す config の候補と lib-config-path の候補が一致することを、TS をテキストとして読むテストで固定する。post-tool-linter が `config/` の設定を読む e2e も足す | `src/lib-config-path/src/lib.rs`（`mod tests`） + `src/hooks-post-tool-linter/tests/` | S | 比較の仕様は「新しい置き場に移せるファイルは `config/` と `.claude/` の 2 候補、それ以外は `.claude/` だけ」。`scripts/deploy-hooks.ts` は読むだけ。直書き検出の lint は作らない。決定事項は [docs/todo29.md](todo29.md) § 順位 555 | test(config-path): TS と Rust の config 候補の同期を固定する |
| 567 | T2 | ✅ | CodeRabbit のレビュー済み判定から gh 呼び出しを注入できる版を切り出し、呼び出し経路の数と 1 回の判定の照会数を固定する。head に固定した照会の配線テストも足す | `src/cli-pr-monitor/src/stages/coderabbit_reviewed.rs` | M | 注入の形は `src/cli-pr-monitor/src/runner.rs` の既存の前例に合わせる。経路の数は `include_str!` で数える (3 経路)。workflow の bash 側は対象外。決定事項は [docs/todo29.md](todo29.md) § 順位 567 | test(pr-monitor): レビュー済み判定の照会回数と head 固定を検査する |
| 546 | T2 | — | 試験運用 ADR の索引タグの期限が過ぎていれば報告する検査と、採用・却下・Superseded の ADR の H1 に `(仮)` が残っていれば違反にする検査を lint:docs に足す | `src/cli-docs-lint/src/adr_index.rs` + `src/cli-docs-lint/src/lib.rs` + `src/cli-docs-lint/src/main.rs` | S | **順位 545 (期限の記入) の完了後に auto へ移す。** 今日の日付は引数で注入する。`(仮)` は片方向だけ検査する。決定事項は [docs/todo28.md](todo28.md) § 順位 546 | feat(docs-lint): 試験運用 ADR の期限切れと (仮) の外し忘れを検出する |
| 557 | T3 | — | `--no-install` の無い `npx` を、custom-lint rule と package.json を読む clean-baseline テストの 2 層で検出する | `config/custom-lint-rules.toml` + `src/hooks-post-tool-linter/src/custom_rules/` + `tests/fixtures/incidents/{bad,good}/` + `src/hooks-stop-quality/tests/` | S | **順位 556 (devDependencies の導入) の完了後に auto へ移す。** 既存の違反 3 件が残っていると検査を入れられない。決定事項は [docs/todo29.md](todo29.md) § 順位 557 | feat(custom-lint): --no-install の無い npx を検出する |
| 561 | T3 | — | package.json の名前に test を含む script がすべて ci.yml の run 行に現れることを検査する cargo test を足す | `src/hooks-stop-quality/tests/` | S | **順位 560 (残骸 script の撤去) の完了後に auto へ移す。** ci.yml は読むだけ。決定事項は [docs/todo29.md](todo29.md) § 順位 561 | test(ci): package.json の test script が CI で走ることを検査する |
| 563 | T3 | — | lint-workflows に jq の PCRE 略記の検出と marker 照合のコメント除去を足し、本体を export 関数に分けて vitest のテスト (契約検査 4 の mutation を含む) を書く | `scripts/lint-workflows.mjs` + `scripts/lint-workflows.test.mjs`（新規） | S | **順位 562 (夜間 verify への vitest 追加) の完了後に auto へ移す。** 決定事項は [docs/todo29.md](todo29.md) § 順位 563 | feat(lint-workflows): jq の PCRE 略記と marker 照合を締める |
| 564 | T3 | — | exe-fingerprint のテストに cargo metadata の例外系と状態遷移の表駆動テストを足し、cargo-check-for-file の timeout のコメントを実設定に合わせる | `scripts/exe-fingerprint.test.mjs` + `scripts/cargo-check-for-file.mjs` | S | **順位 562 の完了後に auto へ移す。** コメントは 30 秒 → 35 秒 (`.claude/settings.json` の値)。決定事項は [docs/todo29.md](todo29.md) § 順位 564 | test(scripts): exe-fingerprint の例外系と状態遷移を固定する |

### 無人可としなかった理由

「注意」欄の記述と § 自律実行可否の 2 段階分類 の判定条件を突き合わせた結果。将来この判断を見直す際、根拠を再調査せずに済むよう残す。

> **見出しから件数を外した（2026-08-16）。** 行の増減のたびに見出しの数詞を直す必要があり、実際に順位 334 の retire で不整合になりかけた。

| 順位 | 満たさない条件 | 該当箇所 |
|---|---|---|
| ~~284~~ | **解消 (2026-08-17)** | human だった理由は未マージの `claude/select-next-task-a9aiam` に同タスクの実装が乗っていたこと。**同ブランチは 2026-08-15 に削除済みで、根拠だった条件 3 も 2026-08-16 に廃止された**ため、理由が失効した。人間の判断で auto lane (`✅`) へ移した。**lane の変更理由が消えても行だけが残る**ことがある実例として記録を残す |
| 178 | 1（着手時の判断） | 「実挙動を読んで実在する invariant を**再選定する**」— 何をテストするか自体が未確定 |
| 179 | 1（着手時の判断） | 「新 dev-dep 追加 or plain `#[test]` で代替を**着手時判断**」— 依存を増やす判断は無人でしない |
| 180 | 2（実装内容が一意でない） | 「既存 private `truncate()` と escape ロジック重複、DRY 整理（共通化 or 役割分担）を**検討**」 |
| 340 | 2（実装内容が一意でない） | 「`main.rs` の呼び出し側を I/O 無しでテスト可能にする小さな**合成関数抽出リファクタ**が要る」— 抽出の切り方が未確定 |
| 199 | 2（実装内容が一意でない） | 台帳の再スコープ（`priority_inversion.rs` へのテスト追加）と [todo10.md](todo10.md) の詳細エントリ（`~/.claude/rules/common/testing.md` への文書追記）が矛盾している。2026-08-24 の run 32760716062 が 0 変更で停止。詳細エントリの書き直しが先（2026-09-04） |
| 546 / 557 / 561 / 563 / 564 | 前提タスクが未完了 | 実装内容はヒアリングで確定済み (2026-10-12)。前提 (順位 545 / 556 / 560 / 562) が終わるまでは、夜間 verify で検証できないか、既存違反で即座に落ちる。前提が完了した時点で auto へ移す |

### 対象外（Web では完了不能 / 残価値枯渇）

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

### 2026-08-23

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 199 | 対象外 | **記述が陳腐化したため削除** | 「主成果物が `~/.claude/rules/common/testing.md` = リポジトリ外の global config」という記述が再スコープに追随していなかった。実測 (2026-08-23): 挙げられた global ファイルは**存在せず**、現在の宣言成果物は `src/cli-docs-lint/src/priority_inversion.rs` (`RESOLUTION_WINDOW_CHARS` が 3 箇所) でリポジトリ内・`cargo test` で完結する。**対象外である理由が消えた**ので本節から削除し、採用タスク表の `✅` を正とする |

順位 162 も同じ照合で `✅` と対象外記録の矛盾が見つかったが、そちらは**対象外側が正しかった**ため採用タスク表を `—` へ変更した (削除ではないので上表には載せない)。

### 2026-09-02

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 324 | 採用タスク | マージ済みのため削除 | 夜間 PR [#427](https://github.com/aloekun/claude-code-hook-test/pull/427) が 2026-08-30 にマージ。ブランチの台帳削除コミット (親) を人間のリベース (`jj rebase -r` で先端のみ移動) が置き去りにしたため行が残り、2026-09-01 の夜間 run が再選択して空 diff red になった |
| 412 | 採用タスク | マージ済みのため削除 | 夜間 PR [#459](https://github.com/aloekun/claude-code-hook-test/pull/459) が 2026-08-30 にマージ。原因は 324 と同一 (3/3 で同じリベース操作ミス) |
| 457 | 採用タスク | マージ済みのため削除 | 夜間 PR [#461](https://github.com/aloekun/claude-code-hook-test/pull/461) が 2026-08-30 にマージ。原因は 324 と同一 |

3 件の削除は `cli-ledger-cleanup --apply` を順位ごとに実行して行った (台帳行 + 順位 table 行 + 詳細エントリの 3 点セット)。再発防止は (1) `claude/nightly-<順位>` の PR が当該順位の台帳行削除を diff に含むことの CI 検査、(2) merged PR と台帳の照合を weekly-review / 夜間 preflight へ組み込み、として別 PR で実装する。

### 2026-09-04

夜間 run が 08-30 から 5 晩連続で red だったため、agent 到達後に停止した夜を 2026-08-17 以降で全件分類した。「宣言した成果物が未変更」(`[LEDGER_CLEANUP_BLOCK]` exit 3) が 4 回 (順位 228 / 162 / 356 / 310) で最多、空 diff が 5 回 (199、324 ×3、455)、台帳削除の計画失敗が 2 回 (193)。未変更の 4 件のうち 228 / 356 / 310 の 3 件は宣言が「変更が着地するファイルの予測」になっており、外れると正しい実装が落ちていた。162 は原因が異なり、既存ファイルが移動した参照パスの漂流（順位 176 / 145 と同型）で、宣言粒度の規約では防げない種類だった。ゲート側は変えず、宣言粒度の規約を § 「対象ファイル」列の書き方 と ADR-074 決定 4 に追加した。

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 310 | 採用タスク | 宣言をディレクトリ粒度へ | 2026-09-03 の run 33788964662 が停止。実装は `blocked_patterns.rs` + `presets/mod.rs` に着地して正しく、その run では読むだけだった `handlers.rs` を宣言していた。正規化の置き場は 3 ファイルのどこでもあり得るため crate の `src/` を宣言する |
| 356 | 採用タスク | 宣言をディレクトリ粒度へ | 2026-08-27 の run 33116990702 が同じ形で停止。着地ファイルは実装判断 |
| 426 | 採用タスク | 宣言をディレクトリ粒度へ | 新規ファイル名を当て推量で固定していた。同型の停止を予防 |
| 199 | 採用タスク | `✅` → `—` | 台帳の再スコープと詳細エントリが矛盾し、2026-08-24 の run 32760716062 が 0 変更で停止していた (当時は順位 488 のバグで green) |

### 2026-09-06

09-04 のディレクトリ宣言化の後、310 は 09-04 の run で通り (#477)、356 は 09-05 の run 33983134567 で 2 ターン・0 変更 (`guard=failure`) で停止した。両行の注釈は同じ形 (候補ファイルの列挙 + 判断委譲の文言) だったので、差は列挙したファイルの正誤にある — 310 の 3 ファイルは全部本物、356 は先頭の `staleness.rs` が順位 136 の working-copy staleness で本タスクと無関係だった。**判断委譲の文言は 310 が通っている以上、失敗の原因ではない。**

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 356 | 採用タスク | 注釈の候補列挙を除去し、宣言を独立成果物 2 件のファイル宣言へ | 誤ったファイルを挙げていた。todo14.md の対処案を実測へ直して着地 2 ファイルを固定した結果、ディレクトリ宣言では片方だけの実装が完了扱いになるため (#394 型、CodeRabbit #480)、`monthly_review.rs` + `weekly_review/tests.rs` の 2 件をファイルで宣言する |

規約側は § 「対象ファイル」列の書き方 と ADR-074 決定 4 に「ディレクトリ宣言の注釈で既存ファイルを列挙しない」を追加した。これは「正しく書け」ではなく「書くな」の規約であり、予測を書く場所を消すことで予測が外れる余地を消す。

### 2026-09-09

2026-09-08 の run 34261503122 が順位 145 で `verify=failure` 停止した。agent は `CONFIG_SELECTABLE_PRESETS` という const 表を新設したが**どこからも参照せず**、`cargo clippy -- -D warnings` の `dead_code` で落ちた (`cargo test` は全スイート通過)。

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 145 | 採用タスク | 却下して削除 | **前提が既に別の形で満たされていた。** 本タスクは `presets/mod.rs` に「preset 分類の const 表 + matrix test」を求めるものだが、[#485](https://github.com/aloekun/claude-code-hook-test/pull/485) (2026-09-07 マージ) が同ファイルへ `KNOWN_PRESET_NAMES` (14 件の allowlist) と `every_known_preset_name_resolves_to_itself` (dispatch arm との 1:1 照合テスト) を入れており、「dispatch arm と整合させる」という目的は達成済み。順位が離れている (145 対 485) ぶん #485 の方が現状を反映している |

**この失敗は agent の実装ミスだけが原因ではない。** 台帳の記述は #485 より前に書かれており、両者の関係が示されていなかった。agent は「const 表が無い」前提で着手し、着地時点では類似の表が既にあった。**無人可 lane の「実装内容が一意に決まる」条件は、台帳を書いた時点ではなく着手時点で満たされている必要がある** — 同じ領域に別の変更が入ると条件は静かに崩れる。

### 2026-09-13

決着済みなのに本ファイルに残っていた 3 行を削除した。いずれも人間側の PR で決着した順位で、§ 夜間ループ（無人実装）でマージしたタスクの後始末 の手順 1 (本ファイルの行削除) が抜けていた。283 / 499 は summary 行・詳細エントリも残っていたため手順 2〜3 も同時に行い、302 は summary 行・詳細エントリが既に無かったため行削除のみ。

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 283 | Batch 1 | 完了により削除 | 順位 476 へ統合済み (2026-08-25、不具合修正計画 (2026-09-28 退役) の着手前に片付ける 3 件)。476 は夜間 PR [#494](https://github.com/aloekun/claude-code-hook-test/pull/494) でマージ済み。todo15.md のエントリと todo-summary2.md の行も削除 |
| 302 | Batch 1 | 対象消滅により削除 | 対象の `takeover_stale_lock_skips_remove_when_snapshot_is_stale` は [#312](https://github.com/aloekun/claude-code-hook-test/pull/312) の takeover 再設計で置き換えられ、現在の `pipeline_lock/tests.rs` に存在しない。後継の各テストは `///` doc コメントに前提を持つ。詳細エントリ・summary 行は既に無い |
| 499 | Batch 2 | 完了により削除 | [#464](https://github.com/aloekun/claude-code-hook-test/pull/464) で実装 ([ADR-078](adr/adr-078-takt-verdict-gate.md))。todo25.md のエントリと todo-summary3.md の行も削除 |


### 2026-09-15

`custom-lint-rules.toml` を `.claude/` 直下から `config/` へ移設した ([ADR-006](adr/adr-006-config-driven-hooks.md) § 改訂 2026-09-15)。夜間 agent の `Edit(work/**)` はドット始まりディレクトリに当たらず、`.claude/` 配下を成果物とする行は構造的に完了できなかった (2026-09-02 の run 33665621808 が 34 ターン・0 変更・`permission_denials_count: 2`)。**権限を広げるのではなく設定を出す**ことで解消した。

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 455 | Batch 1 | human → auto | 成果物が `config/custom-lint-rules.toml` になり権限 glob に届く**見込み**。他の除外条件には当たらない。**agent 側の実走確認は順位 519 で待つ** — 実走で確かめたのは hooks が `config/` から設定を読むことまでで、`Edit(work/**)` が `work/config/**` に届くかは glob の意味からの推論にとどまる ([ADR-067](adr/adr-067-phase-b-unattended-fix-push.md)) |
| 281 | Batch 1 | human → auto | 同上。検出対象を「`lib_config_path::resolve_config` を経由せず自前で config パスを組み立てる」形に確定させ、判断留保を除いた |
| 236 | Batch 1 | human → auto (2026-09-18) | 455 との統合可否を「統合しない」で確定し、判断留保 ([ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 5) を除いた。境界は**時刻成分の有無** — 455 は固定名のみを見るので PID+時刻形は素通りし、重複しない。PID 単独は 455 の推奨形 (`src/` に 64 箇所) なので対象外と明記した |
| 519 | Batch 1 | 完了により削除 | 2026-09-16 の夜間 run (`rank=281` → PR #504) が `config/custom-lint-rules.toml` と fixtures を編集して PR 作成まで到達し、agent の `Edit(work/**)` が `work/config/` に届くことを実走で確認した。実測は [ADR-006](adr/adr-006-config-driven-hooks.md) § 実走確認 (2026-09-16) に記録し、`docs/todo26.md` のエントリと `docs/todo-summary3.md` の行を削除 |
| 492 | Batch 1 | 完了により削除 | agent プロンプトの禁止パス列挙を本移設と同じ PR で 10 件へ揃えた (Guard 正規表現 / プロンプト / ADR-072 決定 6 の 3 箇所が一致)。`docs/todo26.md` のエントリと `docs/todo-summary3.md` の行も削除 |
| 507 | (台帳外) | 実装により削除、確認は順位 519 へ | ドット始まりディレクトリの権限問題は本移設で解消した**見込み**。**agent 側の実走確認は未了**なので、`docs/todo26.md` に順位 519 (Tier 4) を起こして確認方法と失敗時の戻し先を残した ([ADR-067](adr/adr-067-phase-b-unattended-fix-push.md): agent の権限経路は実走でしか検証できない) |

**移したのは `custom-lint-rules.toml` だけ。** `hooks-config.toml` は `.claude/` に残した — `[pre_tool_validate]` の block preset (secret-detection / `git` / `rm -rf`) と `[stop_quality]` を持ち、agent が到達できる場所へ出すと「セッション中に無効化 → 作業 → 復元」が最終 diff に残らず、Guard の deny-list (diff しか見ない) では捕まえられないため (pre-push security review の指摘、[ADR-006](adr/adr-006-config-driven-hooks.md) § 改訂 決定 1)。Guard 禁止パスは変更していない。

### 2026-09-18

2026-09-17 の run 35257221949 が順位 426 で `verify=failure` 停止した。**agent の実装は正しく、追加した統合テスト 9 件も通っていた。** 落ちたのは台帳の書き方を検査する検査 C (`lib-ledger` の `annotation_check`) だけで、失敗メッセージは順位 426 の注釈が挙げた名前が `src/lib-jj-helpers/tests/` 配下に実在する、というものだった — **agent が台帳の注釈にあった例示どおりの名前でファイルを作ったため、その成果物が「既存ファイルの列挙」として報告された**。

原因は 2 つある。**(1) 規約と機構がずれていた** — 規約は実在を問わず候補を禁じているのに、機構だけ「実在するものだけ」に緩めており、426 の「例 `facade_reexports.rs`」はその隙間にあった。**(2) 実在を見る限り検査の結果は評価するツリーに依存する** — 夜間 verify step はそれを agent の成果物ツリーで評価するため、「実在する」の意味が「着手前からあった無関係なファイル」から「このタスクの成果物」へ入れ替わる。

| 順位 | 節 | 判定 | 根拠 |
|---|---|---|---|
| 426 | Batch 1 | 注釈から例示ファイル名を除去 | 対象ファイル欄の「例 `facade_reexports.rs`」を削除した。実装の指示は変わらない (ファイル名は agent が決める)。handoff marker `claude/nightly-426` を削除して再投入する |

機構側は実在確認を撤去し、注釈のファイル名を実在不問で弾くようにした (§ 「対象ファイル」列の書き方、[ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 4 の 2026-09-18 追記)。検査は台帳の文字列だけを見る純粋な判定になり、評価時点への依存が構造的に消えた。併せて「変更してはならないものの明示」は注釈から**注意**欄へ移した。

**台帳検査を agent 成果物の verify から外す案は今回採らなかった。** 台帳は agent にとって Guard 禁止パスで不変なので、agent verify で検査 A / C を回しても新しい情報は生まれない。ただし外すと (a) 検査 B の「agent の変更が他行の前提を壊す」検出が PR の CI まで遅れ、(b) verify と CI の `cargo test --workspace` の対象集合がずれ、その除外自体を機械で固定する層が別に要る。規約違反の注釈を消す方が本筋なので、除外は独立した設計判断として切り離した。

### 2026-10-11

**自動 lane が空のまま 3 週間経っていた。** 2026-09-20 から 10-10 まで 21 晩連続で、夜間ループは
「実装可能な無人可タスクがありません」で止まり、何もせずに green で終わっていた (`✅` 行が 0 件)。
決定 10 で「agent を回していない夜」は green なので、run 一覧からは気づけない。順位 487 (PR #551) の
実走確認をしようとして判明した。監視の穴は順位 510 に追記した。

補充のため、未掲載 204 件に [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) の基準を当てた。
語彙で判定できるクラス 1 / 2 / 4 / 5 と決定 3 (文書・並行性) で 180 件を落とし、残る 24 件は詳細エントリの
本文を読んで判定した。

| 順位 | 判定 | 根拠 |
|---|---|---|
| 433 | **✅ で追加** | 除外クラスと決定 3 のどれにも当たらず、作業計画 6 項目が一意。前提 (素の引き算 / 二重計算) を実測で確認 |
| 390 / 382 | 除外 (クラス 3) | 成果物が `src/lib-ledger/` 配下 |
| 423 / 425 | 除外 (クラス 3) | 成果物が `.github/workflows/` 配下 |
| 479 / 483 / 503 / 407 / 362 / 471 | 除外 (クラス 5) | 実装の分岐 (lint か一覧廃止か / 検出範囲 / cargo test か docs-lint か / 語彙と寿命 / regex と除外条件 / 着手時に再確認) が本文で決まっていない |
| 363 / 332 / 286 | 除外 (クラス 2) | jj の colocated repo を組む統合テスト / Windows 実機 / OS 依存の exe 起動テスト |
| 525 | 除外 (クラス 1 / 4) | skill と ADR-028 周辺の記述も変える |
| その他 9 件 | 除外 | 判断を含む作業 (棚卸し / 2 段の判断 / 設計の置き換え) または文書タスク |

**1 件しか残らなかった。** 補充の速度 (人間が判断して `✅` を付ける) が消化の速度 (1 晩 1 件) に
追いつかない構造は変わっていない。順位 510 の完了基準に「`✅` 行の残数を週次で出す」を加えたのはこのため。

### 2026-10-12

post-merge feedback の一括採否 (順位 535〜570) から、auto lane に 15 行を割り当てた (順位 536 / 540 / 541 / 542 / 543 / 544 / 547 / 551 / 552 / 553 / 554 / 555 / 567 / 568 / 569)。

**曖昧さが理由で human に残りそうな行は、判断点をヒアリングで確定してから auto に置いた。** 着手時判断・設計の二択・期待挙動の未定で auto にできない行について、現物から選択肢を洗い出し、ユーザーに決めてもらった。結論は各詳細エントリの「決定事項」にある。夜間ループはそれに従い、読み替えない。

前提タスクが終われば auto に移せる行が 5 つある (順位 546 / 557 / 561 / 563 / 564)。いまは `—` のまま表に載せ、理由を § 無人可としなかった理由 に書いた。

同日、判定基準 1 の旧パス (`.claude/custom-lint-rules.toml`) を、移設後の `config/custom-lint-rules.toml` に直した。

### 2026-10-14

**順位 543 を auto lane から human lane へ引き取り、Web セッションで完了したため行を削除した。** 順位 383 の push が、`pnpm push` の品質ゲート (`lint:docs`) で止まったのが発端。`rustdoc_links.rs` の doc comment にあるインラインコード内の例示リンク (`../../../docs/adr/...`) が、Linux では実在しないパスとして違反になっていた。CI は `lint:docs` を回していないため、Linux で落ちることが見えていなかった。Windows で通っていたのは、末尾のドットを落としてパスを解決するためと推測している (Windows 実機では未確認)。

例示リンクを書き換える対処はコード内に例を書くたびに再発するため、検査の除外を入れる順位 543 を先に PR にし、383 をその上に積んだ (ユーザー判断)。引き取り時点で `claude/nightly-543` は無く、夜間ループとは競合していない。

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
