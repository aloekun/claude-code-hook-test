# TODO (Part 28)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: **新規タスクの追加先** (2026-09-28 から)。2026-09-19 に `docs/todo.md` から「週次レビュー採用」節 4 節を移した先として新設したが、2026-09-28 に移送分 10 件を実測で仕分けて空にし (削除・既存順位への統合・新規採番)、同日 `docs/todo26.md` が 48.8KB = 閾値接近だったため新規追加先へ転用した。50KB に到達するまでは本ファイルへ追加する。
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
