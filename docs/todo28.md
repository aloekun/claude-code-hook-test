# TODO (Part 28)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: 既存タスクの編集・完了削除専用。**新規タスクは追加しない** (2026-10-12 に 41.9KB へ達し、新規追加先は [docs/todo29.md](todo29.md) へ移った)。以下は追加先だった時期の経緯: **新規タスクの追加先** (2026-09-28 から)。2026-09-19 に `docs/todo.md` から「週次レビュー採用」節 4 節を移した先として新設したが、2026-09-28 に移送分 10 件を実測で仕分けて空にし (削除・既存順位への統合・新規採番)、同日 `docs/todo26.md` が 48.8KB = 閾値接近だったため新規追加先へ転用した。50KB に到達するまでは本ファイルへ追加する。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 順位の無いエントリの仕分けから採番 (2026-09-28)

> `docs/todo.md` と本ファイルに順位を持たないまま残っていた 32 件を実測で仕分け、なお有効だった分だけを採番したもの (順位 530 / 531)。残りは削除・取り下げ・既存順位への統合で処理した。**順位 532 は 32 件の中から来たものではない** — 仕分けの途中で見つけた別件 (analyze-coderabbit facet の「Windows only」前提) で、同じ日に同じ節へ起票した。処置と根拠は下表のとおりで、採否はユーザー承認済み (2026-09-28)。本節の順位がすべて消化されたら、表ごと削除してよい (経緯は git log に残る)。

| 元の置き場 | 項目 | 処置 | 根拠 (2026-09-28 実測) |
|---|---|---|---|
| todo.md WR-08-15 | A01 800 行閾値の SSOT 化 | 取り下げ | Rust 側の定数は `file_length.rs` の `MAX_FILE_LINES` 1 か所で、`modified_files_check.rs` はそれを `use` している。`pr_size_check` の 800 は PR diff 行数の警告閾値 (`push-runner-config.toml` `warning_threshold`) で別の事実。2 種の閾値の役割差は ADR-039 に、watchlist facet の逆参照も既にある |
| 同 | A02 reminder 閾値の定数化 | 削除 (完了) | #396 で `WEEKLY_REVIEW_DEFAULT_THRESHOLD_DAYS = 7` に定数化し、テストで値を固定済み |
| 同 | A03 lint rule ⑥ の義務を ADR-007 へ | 取り下げ | 前提の「ローカル `cargo test` では検知されない」が誤り。`rule_test_coverage_check` は `#[ignore]` なしの通常テストで、TOML の `extensions` を読んでテストの実在を検査する (順位 137)。ADR-007 にも `[rules.test_coverage]` の記述がある |
| 同 | A04 ADR-031「既定 30 日」 | 順位 371 へ統合 | |
| 同 | J01 / J02 `CARGO_MANIFEST_DIR` | 順位 530 | |
| 同 | S01 eval E2E の閾値 assertion | 削除 (完了) | 完了基準「計測専用か検証ゲートかが決まっている」は ADR-038 § eval の起動 (PR #279) が「assert を持たない計測専用」と決定済み。ADR-038 は採用済みで「試験運用が解けたら」の条件も成り立たない |
| 同 | C03 INJECTION_SIGNALS の拡充 | 取り下げ | 「観測実例から育てる」方針は ADR-054 にある。観測の機会になる fix step の実行実績が 0 件で (順位 521)、着手できる作業が無い |
| 旧 todo28.md | WR-08-13-A01 / WR-06-01-A01 | 削除 (完了) | 統合先の「ADR-032 欠番を CLAUDE.md 索引へ反映」で 2026-09-30 に完了 (ADR-032 の欠番行を追加し、ADR-030 行の撤回済み Supersedes 注記を除去) |
| 同 | 撤回記録 T01 / T02 | 削除 | 教訓は ADR-072 決定 18 が持つ |
| 同 | WR-08-13-M01 / A02 | 削除 (完了) | todo23.md は実在し routing も更新済み。ADR-031 → ADR-070 の前方参照は 2026-08-04 に追記済み |
| 同 | WR-08-13-T03 | 削除 | 中身は順位 28 の退役 (下) |
| 同 | WR-07-19-J01 / J02 | 順位 493 / 509 へ統合 | |
| 同 | WR-07-01-A01 | 順位 531 | 「両方残す」で決着 (ユーザー判断) |
| 同 | WR-06-01-C02 `owner_repo` 検証 | 削除 (完了) | 本番の入口 2 か所 (`pipeline.rs` の AI ステップ入力ガード #230 / `--feedback-only` #268) が事前に `is_valid_owner_repo` を通す。`owner_repo` の出所は pending file でなく `detect_owner_repo()` で、前提も成り立たない |
| todo.md 旧チェックリスト | re-push 時のポーリング問題 | 削除 (解消) | ADR-064 の陽性証拠 (push_time で絞った今サイクルの出力) と、HEAD 単位のレビュー済み判定 (`coderabbit_reviewed.rs`) で解消 |
| 同 | read-only zone の齟齬 | 削除 (解消) | `analyze-coderabbit.md` が `.takt/` を read-only zone として not_applicable に分類し、`fix.md` と揃った |
| 同 | instruction 参照 lint / verdict 値 lint | 順位 465 へ統合 | |
| 同 | ADR からの動的抽出 / Learning 同期 / 他 AI レビュー統合 / takt-test-vc 還元 / skill evals runner / commit message 草案 / auto-rebase 検討 / PR 包含 gate | 取り下げ | いずれも観測された害が無い (ADR-042)。草案生成は squash マージで不要。skill evals は skills リポジトリ側の話。PR 包含 gate は ADR-045 が「hook からは判定できない」と判断済み |
| todo.md | 順位 28 (takt-test-vc への反映) | 取り下げ | 反映先が作業マシンに無く、2026-04-13 から更新なし。ADR-017 に注記した |
| todo26.md | 順位 522 (Stop の docs-only routing) | 取り下げ | 順位 531 の § 順位 522 を取り下げる根拠 |

### 順位 530: テストのリポジトリルート解決をコンパイル時の値から実行時の探索へ移す

> **動機**: `src/lib-ledger/src/deployed_ledger.rs` の `repo_root()` と `src/hooks-post-tool-linter/src/custom_rules/coverage.rs` (23 / 68 / 247 行) が `env!("CARGO_MANIFEST_DIR")` からリポジトリルートを組み立てている。コンパイル時に焼き込まれるため、workspace を移動・改名するとテストが古いパスを読んで panic しうる。週次レビュー WR-2026-08-15-J01 / J02 で採用し、WR-2026-09-18-J02 でも high として再検出された。どちらも `#[cfg(test)]` 配下のテスト専用コードで、本番 exe には影響しない。
>
> **参照**: `src/lib-ledger/src/deployed_ledger.rs` (`repo_root`)、`src/hooks-post-tool-linter/src/custom_rules/coverage.rs`、`src/lib-jj-helpers/src/workspace.rs` (`resolve_main_workspace_root`)

**範囲はこの 2 ファイルに絞る** (2026-09-28 ユーザー判断)。同じ `env!("CARGO_MANIFEST_DIR")` はテストコードに計 13 ファイルあるが、発症は実測していない — cargo の fingerprint は path を含むため、`target/` ごと移動する通常のケースでは再ビルドされて発症しない可能性がある。

**探索の目印は `.git` にしない。** 元エントリの案は「`.git` / `.claude` を上へ探す」だったが、`.git` は非 colocated の jj workspace に無い (ADR-045)。`.jj` を目印にするか、`resolve_main_workspace_root` に寄せる。

- [ ] 着手前に、workspace の移動で実際に panic するかを実測する (発症しないなら負の結果として記録して閉じる — ADR-042 § 追記 2026-09-13)
- [ ] 2 ファイルのルート解決を実行時の探索へ移す
- [ ] 移動したディレクトリからでもテストが通ることを確認する

#### 完了基準

2 ファイルのテストがコンパイル時のパスに依存せず、workspace を移動しても通ること。または、発症しないことを実測して負の結果として閉じていること。

---

### 順位 531: Stop hook と push gate の二重検査を意図した二層として明文化する

> **動機**: `.claude/hooks-config.toml` の `[stop_quality]` と `push-runner-config.toml` の `[quality_gate]` は `pnpm lint` / `pnpm test` / `pnpm test:e2e` / `pnpm build` / clippy を重ねて回している。週次レビュー WR-2026-07-01-A01 が重複の解消を提案したが、2026-09-28 の調査で**両方残す**と決めた (ユーザー判断)。一方で `push-runner-config.toml` の rust-lint-test group のコメントは「Stop hook では実行せず」のままで、順位 175 (PR #185) で Stop に clippy を足した後の実態と合っていない。
>
> **参照**: [ADR-004](adr/adr-004-stop-hook-quality-gate.md)、[ADR-058](adr/adr-058-post-takt-regate.md)、`src/hooks-stop-quality/src/main.rs` (skip は fail-open の説明)、`.claude/telemetry/push-runs-*.jsonl` (`stages.quality_gate`)

**両方残す根拠** (2026-09-28 実測):

1. **Stop は助言層** — `hooks-stop-quality` 自身が「本物のゲートは push pipeline 側」と書く。2 回目の Stop (`stop_hook_active`)、pipeline 実行中の skip、step timeout 60 秒のいずれでも失敗したまま通る
2. **Stop は push する中身を見ている保証が無い** — 編集と push が同じターンなら Stop は挟まらない (2026-09-28 の PR #526 の再 push が実例)。ターミナルからの push には Stop が無い。push 中の takt fix step は Stop より後にコードを書き換え、その再ゲート (ADR-058) は quality_gate の group を再実行する
3. **削っても待ち時間は縮まない** — group は並列実行で、直近 15 回の quality_gate は 79〜91 秒。律速は push にしか無い `cargo test` / `--ignored` 統合テストと推定され、重複分はその裏で走る
4. clippy は push 側が `--all-targets --all-features` で範囲が広く、同一ではない

**順位 522 を取り下げる根拠**: 522 は「Option B (両方残す) なら Stop に docs-only 判定を足す」としていたが、前提の「docs だけの変更でも Stop で `cargo clippy` / `cargo test` が走り、毎ターン待ち時間が乗る」が実測と合わない。[ADR-004](adr/adr-004-stop-hook-quality-gate.md) § ステップ並列実行による高速化 (WP-05 の実測) によれば、Stop は `cargo test` を実行せず、Rust は `cargo clippy --workspace` だけで warm cache なら 0.4〜0.8 秒である。step は並列実行で、全体は中央値で約 2.0 秒 (最遅 step で決まる)。docs-only 判定を足して clippy を飛ばしても最遅 step が別の node 系 step に移るだけで、縮むのは 1 秒未満になる。判定を Stop 側へ共有する実装コスト (ADR-081 の写経回避) に見合わない。cold cache の初回だけは clippy が長くなるが、これは docs-only かどうかと無関係に起きる。

- [ ] ADR-004 に「Stop = 頻度の高い早期検出、push = 外へ出す前の強制点」を役割として追記し、上の根拠を残す
- [ ] `push-runner-config.toml` の「Stop hook では実行せず」のコメントを実態に合わせる

#### 完了基準

ADR-004 から二層の役割と両方残す根拠が読めること。push-runner-config.toml のコメントが Stop の実際の step と矛盾しないこと。

---

### 順位 532: analyze-coderabbit facet の「Windows only」前提を Linux 対応後の実態に合わせる

> **動機**: `.takt/facets/instructions/analyze-coderabbit.md` は「This project targets Windows only」として、cross-platform 互換の指摘 (`.exe` の決め打ち等) を `Info` へ落とす規則を持っている (25 行 / 59 行の例)。だが ADR-063 で Linux バイナリを配布し、ADR-065 で両 OS の CI matrix を回すようになった後は、Linux で壊れる指摘こそ拾うべきものになっている。2026-09-28 の仕分け中に発見した (未検証。実際に正当な指摘が落とされた事例はまだ探していない)。
>
> **参照**: `.takt/facets/instructions/analyze-coderabbit.md`、[ADR-063](adr/adr-063-linux-portability-release-binaries.md)、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)

- [ ] `.claude/feedback-reports/` と post-pr-review の run から、Platform scope を理由に Info へ落とした指摘を探し、正当だったものがあるかを確かめる
- [ ] Platform scope の規則を「Windows と Linux の両方を対象とする」へ改め、例の行も直す
- [ ] facet の出力言語検査 (`pnpm lint:takt-facets`) が通ること

#### 完了基準

analyze-coderabbit facet が cross-platform の指摘を理由なく Info に落とさないこと。

---

## PR #548 (順位 227) のレビューから起票 (2026-10-08)

### 順位 533: allow-without-reason を属性単位の判定 (構文ベース) へ移す再評価 (トリガー付きの見送り follow-up)

> **動機**: custom lint rule㉕ `allow-without-reason` (順位 227、PR #548) は、行頭の `#[allow]` に
> 理由を要求する軽量な正規表現 lint として対象範囲を定義した。同じ行の 2 つ目以降の allow 属性と、
> 他のコードの後ろにある allow 属性は意図的に対象外にしている (PR #548 で CodeRabbit が前者を指摘)。
> 対象範囲・対象外にした理由・テストで固定した挙動は `config/custom-lint-rules.toml` の rule㉕ コメントが持つ。
> 完全に対応するには属性単位で理由を判定する構文ベースの実装が要り、これは
> [ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の判定木で AST 層にあたる。
> 2026-10-08 時点で対象外の形はリポジトリに 0 件で、rule は warning の助言層のため、移行は見送った。
> 正規表現のまま範囲を広げる案 (1 行 1 属性の別 rule / 次の `#[` で範囲を区切る) は、コメントや理由の文中の
> `#[` で新しい誤検知を持ち込むため採らない。
> **「現時点では見送り」を表現するための行であり、着手対象ではない。**
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 5 — トリガーが観測されるまで着手しない。

#### 再評価トリガー (いずれかが観測されたときのみ)

- **対象外の形がリポジトリに現れた**: rule は対象外の形を検出しないので、実害を待たずに次のコマンドで
  出現を確かめる (1 件以上で再評価)。週次・月次のレビューで随時確認する。rule㉕ 自身のテストは
  文字列リテラルの中に対象外の形の例を持つので除外している (2026-10-08 時点で、除外後はどちらも 0 件)。
  人が目で確かめる前提の確認であり、1 つ目は理由の文中に `#[allow(` を書いた行も拾う。
  - 同じ行に allow 属性が 2 つ以上: `rg -n --type rust -g '!**/rule_tests_allow_without_reason.rs' '^[ \t]*#!?\[(?:cfg_attr\([^\n]*?,\s*)?allow\([^\n]*?#!?\[(?:cfg_attr\([^\n]*?,\s*)?allow\(' src tests`
  - 他のコードの後ろにある allow 属性: `rg -n --type rust -g '!**/rule_tests_allow_without_reason.rs' '^[ \t]*[^#/ \t][^\n]*#!?\[(?:cfg_attr\([^\n]*?,\s*)?allow\(' src tests`
- **書き方の方針が変わった**: 1 行に複数の allow 属性を書く書き方をリポジトリとして認めることになったとき。
- **正規表現方式の限界が繰り返し問題になった**: 属性を対象にする同種の custom lint rule が増え、
  同じ限界 (属性単位で判定できない) が別の rule でも問題になったとき。

再評価では、構文ベースの実装 (Rust の lexer / parser の導入、violation の位置 = 属性) の費用を、
観測された出現・実害と比べて判断する。

#### 完了基準

- トリガーが観測されて再評価し、移行する / しないの判断と根拠が rule㉕ のコメントか ADR-007 に記録されている
- または、トリガーが観測されないまま次の月次 ROI レビュー (ADR-062) 2 回分を経過し、本行を削除した

## PR D (順位 498) の作業から起票 (2026-10-11)

### 順位 534: rule を書き写したテストを設定ファイル読み込みへ移し、写経の再発を検査で止める

> **動機**: custom lint rule のテストの一部は、`config/custom-lint-rules.toml` の rule ではなく、
> テスト内に書き写したコピー (`make_test_rule("<rule id>", "<pattern>", &[<拡張子>])`) を検査している。
> コピーと設定ファイルがずれても、テストは green のまま残る。これは
> [ADR-081](adr/adr-081-single-fact-dispersion.md) の写経禁止に反し、PR #507 で実際に踏んだ形でもある
> (新しめのテストファイルは既に設定ファイルを読む方式で、古いファイルだけが取り残されている)。
>
> 2026-10-10 の実測では 25 rule 中 16 rule がコピー方式だった。設定ファイルと比べると、ずれていたのは
> `no-console-log` の 1 件 (pattern と拡張子の両方)。順位 498 (PR D) で、非主要拡張子の coverage を
> 数えるテストを持つ 9 rule を移行した (`no-console-log` はテストを書き直した)。**本行は残りの移行と
> 再発防止を扱う。**
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — 2026-10-10 時点で残りのコピーは設定ファイルと一致しており、実害は無い。

#### 作業計画 (2 段。順序を守る)

1. **残りのコピーを設定ファイル読み込みへ移す**。2026-10-10 時点の対象:
   - 主要拡張子側の 7 rule: `no-time-field-strict-greater` / `no-write-result-discard` /
     `takt-workflow-persona-without-model` / `no-hardcoded-jj-revset-range` / `no-unbounded-child-wait` /
     `no-weak-temp-uniqueness` / `no-manual-hooks-config-path`
   - `deployed_tests.rs` に残るコピー (`no-ephemeral-todo-reference` / `no-write-result-discard` /
     `takt-workflow-persona-without-model`)
   - 着手時に `make_test_rule` の呼び出しを全件洗い出し、設定ファイルにある rule id を作っているものを
     数え直す (上の一覧は 2026-10-10 の静的な照合で、pattern を実行時に組み立てる呼び出しは見落としうる)
   - `rule_from_repo_config` は [ADR-084](adr/adr-084-test-helper-no-sharing.md) に従いファイルごとに複製する
2. **1 が全部終わってから**、設定ファイルにある rule id を `make_test_rule` で作ったら落とす検査を足す。
   途中で入れると、残っているコピーで CI が落ちる。検査を正規表現で済ませられるかは、実際の呼び出し方
   (id を定数経由で渡す / 呼び出しが複数行にまたがる / コメントや説明文字列に id が出る) を確かめてから
   決める。呼び出し方が限られていれば、Rust の構文解析までは導入しない

#### 完了基準

- 設定ファイルにある rule を、テスト側で書き写しているテストが 0 件であること
- 書き写しを足すと CI が落ちること (検査を外す変異で確認する)

## post-merge feedback (#472, #486〜#555) の一括採否から起票 (2026-10-12)

> **経緯**: 機4b (順位 523) を待って保留していた feedback レポートを一括で採否した ([ADR-079](adr/adr-079-defect-origin-tagging.md) § post-merge feedback の採否)。docs-only PR のレポートは対象外。採用候補のうち現 master で未実装のものだけを残し、様子見は却下した。Tier 3 (規約・文書) は、ADR にしか置き場が無い判断基準と事実誤りの訂正だけを残した。
>
> **auto lane へ割り当てた行は、着手時の判断をヒアリングで確定済みである。** 各エントリの「決定事項」がその結論で、実装はそれに従う。読み替えや再選定はしない。

### 順位 535: push-runner / pr-monitor がハンドオフマーカー bookmark を前進させる

> **動機**: push 時の bookmark 前進は trunk bookmark だけを除外している (`src/lib-jj-helpers/src/bookmarks.rs` の `is_trunk_bookmark`)。target に bookmark が無いときの fallback 経路は、ローカルに非 trunk の bookmark が 1 件だけあればそれを前進させるため、base commit を指す夜間ループのハンドオフマーカー (`claude/nightly-<順位>`) が動きうる。同じロジックが `cli-push-runner` と `cli-pr-monitor` の `push_jj_bookmark.rs` に重複している。
>
> **由来**: `[defect:G2]`。証拠 = #506 (セッションでマーカーの前進を観測)。前進のテストは在ったが、マーカーが混ざる入力を覆っていなかった。
>
> **実行優先度**: Tier 2 — Effort M。

#### 決定事項 (2026-10-12)

- 除外は**名前ではなく構造で判定する**: bookmark が指すコミットが `::trunk()` に含まれていれば前進対象から外す。マーカーと夜間 PR ブランチは同じ名前空間 ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 19/20) なので、接頭辞で外すと rebase 後の夜間 PR を人手で push できなくなる
- 判定は `lib-jj-helpers` に共有関数として置き、両 crate の fallback 経路に適用する。`is_trunk_bookmark` は変えない (stale-branch-scan の保護判定と夜間 PR 検出が依存している)。`bookmark_check.rs` の検出側には入れない
- 回帰テストは実 jj を使う `#[ignore]` テストになる。夜間 verify は `--ignored` を回さないため human lane とする (2026-10-12 ユーザー判断)

#### 完了基準

- `::trunk()` 上を指す bookmark が fallback 経路で前進しないことを、両 crate の統合テストで確認できること

### 順位 536: jj-op-verify が `cd` 後・background 実行のコマンドを誤判定する

> **動機**: `src/hooks-post-tool-jj-op-verify/src/main.rs` は `jj op log` を hook 自身の cwd で起動する。`cd <dir> && jj ...` や `-R` で別リポジトリを操作したコマンド、background 実行で完了前に照合したコマンドは、別の op log と比べて誤警告を出す。
>
> **由来**: `[defect:G2]`。証拠 = #545 (セッションで誤判定を 2 件観測)。tokenizer のテストは在ったが、cwd を変える入力を覆っていなかった。
>
> **実行優先度**: Tier 2 — Effort S。

#### 決定事項 (2026-10-12)

- コマンドに `cd` (区切り `&&` / `;` の後を含む) か `-R` / `--repository` があれば照合しない。判定は既存の `tokenize_respecting_quotes` で行う。コマンド文字列から `cd` 先を解析して照合先を切り替える案は採らない (相対パス・`cd -`・サブシェルで誤る)
- hook 入力の `tool_input.run_in_background` が `true` なら照合しない。`HookInput` に `Option<bool>` を足し、JSON fixture でテストする
- 「誤警告より沈黙」は `Nothing changed.` を照合しない既存方針と同じ。module doc の既知の限界へ 1 項足す
- 同じ行で、quote 内の文言と `jj bookmark delete` などの破壊的コマンドを組み合わせた tokenizer テストを足す (#494 の採用分。既存の `jj abandon` 版と同じ形)

#### 完了基準

- `cd` / `-R` / background の各入力で警告が出ないこと、従来の照合対象では従来どおり警告が出ることをテストで確認できること

### 順位 537: merge-pr 後の空コミットを push 前検査で止める (段 1+2)

> **動機**: `pnpm merge-pr` は同期の最後に `jj new <branch>@origin` を実行し、@ を説明なしの空コミットにする (`src/cli-merge-pipeline/src/pipeline.rs` の `sync_local`。post_steps の残骸検知の基準線でもある)。その上で ADR-045 の規約どおり `jj new -m X` で作業を始めると、空で説明なしの親が push 範囲に残る。push-runner の事前検査 (`bookmark_check.rs`) は @ の説明しか見ないため、takt レビューを通過した後の `jj git push` で初めて拒否される。**操作ミスではなく、2 つの設計の組合せで毎回決定的に起きる。**
>
> **由来**: `[defect:G1]`。証拠 = #484 (2026-09-07 に 2 回連続で発生) / #485 (feedback で再提案)。push 範囲を検査する場が無かった。
>
> **実行優先度**: Tier 1 — Effort M。レビュー時間を毎回 2〜3 分捨てている。

#### 決定事項 (2026-10-12)

対策は 4 段に分け、本項は段 1+2 を扱う (段 3 = 順位 538、段 4 = 順位 539)。**自動 abandon は初期実装に含めない** — 空でも履歴上の役割 (子の親、他 workspace や bookmark からの参照) があり、空であることだけでは安全な削除条件にならない。

1. **段 1**: `bookmark_check.rs` の責務を広げ、push 対象の範囲全体で説明なしコミットを takt レビューの前に検出して止める
   - push 対象に説明なしコミットがある、または @ に説明はあっても祖先に説明なしがある → 停止
   - 検査コマンドが失敗した → 停止 (fail-closed)
   - push 対象外の説明なしコミットでは止めない
   - 最終 push の拒否条件 (説明なし) をそのまま再現する。空かどうかはメッセージで区別して示す
   - 対象コミットの ID・説明・親子関係と修復コマンド (`jj abandon` / `jj squash -u` など) を表示する
2. **段 2**: merge-pipeline の後に `jj new -m X` した状態を再現し、段 1 の検査が捕まえる回帰テスト (実 jj を使う `#[ignore]` テスト)

#### 完了基準

- 再現状態で、push-runner が takt を起動する前に停止し、修復手順を表示すること

### 順位 538: 空の基準コミットに `jj new -m` するのを PreToolUse で止める (段 3)

> **動機**: 順位 537 は混入を push 前に止めるが、発生そのものは防がない。入口で止めれば、空コミットが作られない。
>
> **由来**: `[improvement]`。発生の証拠は順位 537 にある。本項は同じ不具合の入口側の対策である。
>
> **実行優先度**: Tier 2 — Effort M。順位 537 の後に行う。

#### 決定事項 (2026-10-12)

- 止める条件は次のすべてを満たすときに限る: revision 指定の無い `jj new -m X` である / @ が空 / @ に説明が無い / **@ が merge-pipeline の作った基準コミットである**。最後の条件は状態から推測しない。merge-pipeline が基準コミットを作るときに change_id を記録するなど、機械的に識別できる手段を用意する
- 止めたときは `jj describe -m X` を案内する
- pre-tool-validate で jj を起動する初めての検査になる。次をテストで確かめる: 検査コマンドが作業ツリーを snapshot しないこと (読み取り専用に見える jj コマンドも op を書きうる) / jj の失敗・timeout・想定外の出力で安全側に倒れること / `jj new -m X <revision>` など対象外の操作を止めないこと / 再実行しても判定が変わらないこと

#### 完了基準

- merge-pr 直後の `jj new -m X` が止められ、それ以外の `jj new` は止められないこと

### 順位 539: post_steps 残骸検知を空の基準コミットから切り離せるか調べる (段 4)

> **動機**: 空の基準コミットは `src/cli-merge-pipeline/src/stray.rs` が post_steps の残骸を検知する基準線として作られている。検知に要るのが「空のコミット」ではなく「post_steps 実行前の作業ツリーの状態」なら、基準コミットを履歴に残さない設計にでき、順位 537 / 538 の原因になっている結合そのものが消える。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort M。調査タスク。順位 537 の回帰テストが入ってから着手する。

#### 作業計画

- `stray.rs` が扱う条件 (未追跡ファイル・削除・変更・競合・snapshot のタイミング) を列挙し、作業ツリーの状態比較で同じ検知ができるかを確かめる
- できるなら置き換えの設計を、できないなら理由を [ADR-013](adr/adr-013-merge-pipeline.md) に残す

#### 完了基準

- 置き換えの可否が根拠つきで記録されていること

### 順位 540: stale-branch-scan の冪等性を純関数テストで固定する

> **動機**: scan → cleanup → マーカー作成 → 再 scan を繰り返したとき、分類が変わらないことを保証するテストが無い (#476 の提案)。CI step での実走案は Guard 禁止パス (`.github/workflows/`) を書き換えるため採らない。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。

#### 決定事項 (2026-10-12)

- `src/cli-stale-branch-scan/src/classify.rs` の `mod tests` に、ls-remote 出力と PR 一覧を fixture にした純関数テストを足し、scan → cleanup → マーカー作成 → 再 scan を 3 周させて分類が変わらないことを確かめる。git / jj / gh は起動しない

#### 完了基準

- 3 周目の分類が 1 周目と一致することを assert するテストがあること

### 順位 542: `rename_target` を曖昧な括りで推測解決しないようにする

> **動機**: `src/cli-push-runner/src/stages/testability_gate/mod.rs` の `rename_target` は、最初の `{` と次の `}` で括りを決める。ファイル名自体が `{` / `}` / ` => ` を含むと、Err にならずに別のパスへ解決し、走査対象を取り違える (#509 の提案。コードからの推論で、実害の観測はまだ無い)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort S。

#### 決定事項 (2026-10-12)

- `{`・` => `・`}` がそれぞれちょうど 1 回、この順に並ぶときだけ解決し、それ以外は Err にする。module doc の「推測で解決せず Err」(ADR-043) の延長である
- Err の扱いは現在の経路 (`changed_paths` 全体を Err にし、`unparsable-status` に分類する) のまま変えない
- jj のエスケープ規則を実測して正確にパースする案は採らない

#### 完了基準

- ファイル名に `{` / `}` / ` => ` を含む rename 行が Err になり、通常の rename 行は従来どおり解決されることをテストで確認できること

### 順位 544: 範囲で書いた退役を範囲全体の退役と読み、表駆動テストで固定する

> **動機**: `src/cli-docs-lint/src/todo_routing.rs` は「`todo3.md 〜 todo5.md` は退役」を、末尾の 5 だけが退役で 3・4 は宣言と解釈する。人が読む意味と食い違う。`expand_between` と退役判定の相互作用のテストは 1 ケースしかない (#500 の提案)。
>
> **由来**: `[improvement]`。現在の preamble は退役を個別に書いているため実害は無い。
>
> **実行優先度**: Tier 3 — Effort S。

#### 決定事項 (2026-10-12)

- 範囲で退役を書いたら、範囲全体を退役と読む (2026-10-12 ユーザー判断)。実装を直す
- テストは `mod tests` 内の表駆動 (`&[(説明, 本文, 実ファイル, 期待)]` を回す) にする。proptest や crate 横断の helper は使わない (ADR-084)
- 網羅する軸: 範囲の書式 (`〜` / `～` / `…` / `...` / 圧縮形 `todoA-B.md`) × 退役の位置 (範囲内 / 範囲の終端 / 範囲外) × 退役ファイルの実在 (有 / 無)
- `todo_routing.rs` は 727 行ある。800 行を超えると Stop hook が止めるので、超えるならテストを別ファイルへ分ける (ADR-080)

#### 完了基準

- 範囲で書いた退役が範囲全体に効くこと、上の軸の組合せが表で固定されていること

### 順位 545: 試験運用 ADR の再評価期限を索引タグに記入する

> **動機**: 試験運用の ADR は 42 件あるが、期限を日付で書いているのは 8 件 (書式 5 種類)、条件で書いているのが 18 件、期限の記述が無いのが 16 件 (2026-10-12 実測)。期限切れを機械で検出しようにも、読む値が無い (#549 の提案)。既に期限切れが 3 件ある (ADR-023 = 2026-07-31、ADR-045 = 2026-09-29、ADR-052 = 2026-10-11)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 2 — Effort M。ユーザーと二人三脚で期限を決める作業。

#### 決定事項 (2026-10-12)

- 期限は CLAUDE.md の ADR 索引タグに `*(試験運用 — 期限 YYYY-MM-DD)*` の形で持たせる。ADR 本文は触らない (`adr_index.rs` の索引タグ判定は `試験運用` の前方一致なので互換)
- 条件型の 18 件も、確認する日付を決めて記入する
- 期限切れ 3 件は、採用・却下・延長のいずれかを決める

#### 完了基準

- 試験運用の全 ADR の索引タグに期限があること

### 順位 546: 試験運用 ADR の期限切れと `(仮)` の外し忘れを lint:docs で検出する

> **動機**: 順位 545 で索引タグに期限が入れば、期限切れを決定論で検出できる。あわせて、H1 の `(仮)` が採用・却下・Superseded の ADR に残っていないかも検査する (#549 の提案。「試験運用なら `(仮)` 必須」の向きは、41 件が違反になり前提が崩れているので採らない)。
>
> **由来**: `[improvement]`。`(仮)` の外し忘れは 2026-10-12 時点で 0 件で、今後の予防である。
>
> **実行優先度**: Tier 2 — Effort S。**順位 545 の完了が前提**。それまでは human lane に置く。

#### 決定事項 (2026-10-12)

- 期限切れ検査: 索引タグの `期限 YYYY-MM-DD` が今日より前の試験運用 ADR を報告する。今日の日付は引数で注入し、テストの決定性を保つ。新規 module にして `src/cli-docs-lint/src/lib.rs` と `main.rs` の check 一覧に登録する
- `(仮)` 検査: `adr_index.rs` で、本体ステータスが採用・却下・Superseded の ADR の H1 に `(仮)` / `（仮）` が残っていれば違反にする。片方向だけ

#### 完了基準

- 期限切れの試験運用 ADR と、`(仮)` の外し忘れが `pnpm lint:docs` で検出されること

### 順位 547: `parse_tier` の Tier 4 / 5 をテストで固定する

> **動機**: `src/cli-docs-lint/src/priority_inversion.rs` のテストは Tier 1〜3 しか見ていない。台帳には Tier 5 の行が実在する (#488 の提案)。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort XS。

#### 決定事項 (2026-10-12)

- `parse_tier_reads_the_plain_form` に Tier 4 / 5 を足す。派生プロジェクトの旧書式を読めることを確かめる `parse_tier_still_reads_the_legacy_emoji_form` にも、旧書式の Tier 5 (絵文字つき) を足す。本リポジトリの台帳からは絵文字を 2026-10-12 に外した

#### 完了基準

- Tier 1〜5 の現書式と旧書式が読めることをテストで確認できること

### 順位 548: lane マーカーを絵文字から文字列へ置き換える (絵文字の全面禁止 段 2)

> **動機**: ドキュメントの絵文字は環境によって表示が変わり、主張を散らかし、他のコーディングエージェントへ渡すときのノイズになる。2026-10-12 に全面禁止を決め、3 段で進める (段 1 = 台帳の Tier 表記から外す。同日実施済み)。docs の絵文字は 43 ファイルに 263 個あり (2026-10-12 実測)、そのうち `docs/claude-code-web-tasks.md` の `無人可` 列のチェックマークは `lib-ledger` が lane マーカーとして機械的に読んでいる。段 3 の一掃の前に、これを文字列へ置き換える必要がある。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort M。Guard 禁止パス (`src/lib-ledger/`、台帳) の改修なので human lane。

#### 作業計画

- lane の値を `auto` / `human` などの文字列にし、`lib-ledger` と `cli-nightly-task-select` の判定、台帳の表、ADR-072 / ADR-074 の記述を追随させる
- 派生プロジェクトの台帳を読めなくしないか (旧書式の互換) を決める

#### 完了基準

- lane の判定が文字列で行われ、台帳から lane の絵文字が消えていること

### 順位 549: ドキュメントの絵文字を一掃し、検出したら即時エラーにする (段 3)

> **動機**: 順位 548 の動機を参照。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort M。**順位 548 の完了が前提**。

#### 作業計画

- `docs/`・`CLAUDE.md`・`.takt/facets/` の絵文字を消す。facet の出力契約 (feedback レポートの採用候補の印など) は文字列の表記へ置き換え、読む側の takt facet と skill を追随させる
- 絵文字を検出したら即時エラーにする検査を入れる。編集時に止める custom-lint rule と、既存違反を捕まえる cargo test の clean-baseline の 2 層にする (custom-lint は編集したファイルにしか発火しないため)
- 旧書式の例示などで絵文字を書く必要がある箇所の扱いを決める

#### 完了基準

- 対象範囲に絵文字が 0 件で、追加すると検出されること

### 順位 550: post-merge feedback の判断基準を ADR へ追記する

> **動機**: 一括採否で残した Tier 3 のうち、機械化できない判断基準で、置き場が ADR しか無いもの。
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 3 — Effort S。ADR 編集なので human lane。

#### 作業計画

- [ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md): 複数 crate で値が同じ定数を lib へ集約する判定基準 (#545。lint 案は却下されたので ADR しか置き場が無い)
- [ADR-049](adr/adr-049-incident-eval-regression-suite.md): incident 由来の lint rule を起票する前に、静的 lint で本当に捕まえられるか (実行時に生成される値ではないか) を確かめる (#532)
- [ADR-041](adr/adr-041-test-isolation-patterns.md): 新規 lint rule では入力空間の境界 (単語境界・パスの段数・`timeout: 0` のような外部 API の意味論) を明示的にテストする (#537 / #538)
- [ADR-041](adr/adr-041-test-isolation-patterns.md) または [ADR-025](adr/adr-025-cwd-restore-drop-guard.md): プロセス global state を触るテストより、純粋関数の抽出による隔離不要化を優先する (#552)
- [ADR-043](adr/adr-043-security-gates-fail-closed.md): `find_first` 系のループ内で `Option` の `?` が走査全体を早期終了させる形を、原則 4 の適用範囲に含める (#490)
- [ADR-064](adr/adr-064-monitor-success-positive-evidence.md): 外部 SaaS の応答の自然文 (description) を判定に使わず、構造化フィールドだけを使う (#514)

#### 完了基準

- 上の 6 項目が各 ADR に入っていること
