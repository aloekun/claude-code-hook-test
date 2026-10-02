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

### 順位 533: `network-spawn-without-timeout` が `timeout: 0` を timeout 指定として通す

> **動機**: custom-lint の rule㉒ `network-spawn-without-timeout` (`config/custom-lint-rules.toml`) は、
> exception で options に `timeout:` というキーがあるかだけを見ている。Node の `spawnSync` /
> `execFileSync` は `timeout: 0` を「timeout なし」として扱うので、`timeout: 0` と書くと無期限に
> 待つ呼び出しが検査を通る。PR #534 の pre-push review (simplicity facet、非ブロッキング) が指摘した。
>
> **由来**: `[defect:G2]`。証拠 = PR #534。判定層にテストはあったが、値の入力空間 (`0`) を覆っていなかった。

- [ ] exception を「値が 1〜9 で始まる数値か、定数名 (識別子)」に絞り、`timeout: 0` を検出する
- [ ] `timeout: undefined` / `null` のような識別子の形は lookahead の無い Rust regex では除外できない。
  rule の注記に限界として書く (実在するかは `scripts/` を grep して確かめる)
- [ ] `timeout: 0` の positive test を `spawnSync` と `execFileSync` の両方で足し、`timeout: GH_TIMEOUT_MS` /
  `timeout: 30_000` の negative test を足す

#### 完了基準

`timeout: 0` を付けた通信系の `spawnSync` と `execFileSync` が、どちらも rule㉒ で検出されること
(Node はどちらの API でも `timeout` が 0 より大きいときだけ timeout を効かせる)。
