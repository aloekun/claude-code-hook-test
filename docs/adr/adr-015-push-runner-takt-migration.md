# ADR-015: Push Pipeline を takt ベースの push-runner に移行

## ステータス

承認済み (2026-04-14)

Supersedes: ADR-008 (Push Pipeline ハーネスの実装) の push 前パイプライン部分

## コンテキスト

### 問題

ADR-008 で導入した `cli-push-pipeline.exe` は hooks-config.toml の `[push_pipeline]` セクションからステップを読み込み、順次実行する構成だった。AI レビューは `pnpm review:ai` (`claude -p "/pre-push-review"`) として Claude Code に「お願いベース」で実行していたが、以下の問題が顕在化した:

1. **非決定論的実行**: Claude Code がスキルを正しく呼び出すかどうかはセッション状態に依存し、確実性に欠ける
2. **fix loop の欠如**: レビュー結果に基づく修正 → 再レビューのループが自動化されておらず、人手介入が必要
3. **品質ゲートの直列実行**: lint → test → build が順次実行されるため、全体のスループット時間が長い
4. **post-push ポーリングとの断絶**: push 成功後の CodeRabbit ポーリング (cli-pr-monitor) が独立プロセスで動くが、その結果を Claude Code に伝達する CronCreate も「お願いベース」

### 検証結果

別リポジトリ (E:\work\takt-test-vc) で takt (https://github.com/nrslib/takt) を組み込んだ Rust パイプラインを検証し、以下の知見を得た:

- **takt の適材適所**: takt は AI ステート管理 (fix loop, escalation, structured judgment) に強いが、機械的な処理 (品質ゲート, diff 取得, push) では Phase 1/2/3 のオーバーヘッドにより実行時間が膨らむ (takt-test-vc ADR-0001)
- **Rust + takt のハイブリッド**: 機械的ステップを Rust exe で、AI レビューを takt で処理する分離により、クリーンパスで 97-99% の実行時間削減を達成 (takt-test-vc ADR-0003)
- **supervise conditional skip**: `all("approved")` 時に supervise をスキップすることで、クリーンパス 16m30s → 5m30s に短縮

## 決定

### cli-push-pipeline.exe を cli-push-runner.exe (takt ベース) に置き換える

**パイプライン構成:**

```text
pnpm push = cli-push-runner.exe && cli-pr-monitor.exe --monitor-only
             |                       |
             |                       +-- 現行維持: daemon spawn -> polling -> state file
             |
             +-- 新規 (takt-test-vc 方式)
                  +-- Stage 1:   quality_gate  [Rust 並列実行]
                  +-- Stage 1.5: diff          [jj diff -> file]
                  +-- Stage 2:   takt          [AI review + fix loop]
                  +-- Stage 3:   push          [jj git push]
```

### 設計原則

1. **機械的ステップは Rust**: quality_gate, diff, push は Rust exe 内で直接実行。takt のオーバーヘッドを回避
2. **AI ステップは takt**: レビュー (arch + security 並列) → fix loop → supervise は takt ワークフローで deterministic に制御
3. **関心の分離**: push-runner は push まで、post-push ポーリングは cli-pr-monitor が担当。pnpm スクリプトでチェーン

### 品質ゲートの並列実行

`push-runner-config.toml` で lint / test / build を独立グループとして定義し、`parallel = true` で同時実行する。PostToolUse hooks で lint が随時修正されるため、構文レベルの壊滅的エラーが test/build に波及するケースはほぼ発生しない。並列実行により1つでも失敗した場合は全結果が同時に得られ、修正 → 再実行のイテレーションが高速化される。

### clippy の lint スコープ (2026-07-06 追記、順位259 / PR #247 post-merge-feedback T2-1 採用)

`rust-lint-test` グループの clippy は当初 `cargo clippy --workspace -- -D warnings` で lib/bin ターゲットのみを検査しており、`#[cfg(test)]` ユニットテスト・integration test のコードが lint 対象外になる gap があった。PR #247 で `useless_format` が clippy を素通りして `cargo test` 段階まで顕在化し手戻りが発生したことを受け、`cargo clippy --workspace --all-targets --all-features -- -D warnings` に変更する。

- `--all-targets` (= `--lib --bins --tests --benches --examples`) でテストコードも clippy 対象にする。
- `--all-features` は現時点で全 crate に `[features]` 定義がなく no-op だが、将来の feature-gated コードを取りこぼさないための保険。
- 実測コスト (2026-07-05): ウォームで +1〜3s。`rust-lint-test` グループは `cargo test -- --ignored` (~80s) 支配で誤差、かつ 4 グループ並列のため総時間への影響なし。
- 移行に伴い既存違反 1 件 (`cli-merge-pipeline` の `ORPHAN_THRESHOLD_SECS` 不変条件を検証する runtime test の `clippy::assertions_on_constants`) を `const _: () = assert!(...)` のコンパイル時検証へ移行した (定数条件は test 実行時 assert より compile-time assert の方が強い保証)。
- 派生プロジェクト向け `templates/push-runner-config.toml` の Rust グループ例も同スコープに同期。

### 設定ファイルの分離

push-runner の設定は `push-runner-config.toml` (リポジトリルート) に配置し、hooks-config.toml の `[push_pipeline]` セクションは削除する。理由:

- push-runner は Claude Code hooks ではなく CLI exe であり、hooks-config.toml の管轄外
- takt 固有の設定 (`[takt]` セクション) が hooks-config.toml の関心事と合わない
- takt-test-vc との設定互換性を維持しやすい

## 影響

### 廃止

- `src/cli-push-pipeline/` — cli-push-runner に完全置き換え。**crate 削除済み (2026-07-17、PR: push パイプライン改善 T2)** — 下記「cli-push-pipeline crate の削除」参照
- `hooks-config.toml` の `[push_pipeline]` セクション — push-runner-config.toml に移行
- `pnpm review:ai` スクリプト — takt が内部で AI レビューを処理

### cli-push-pipeline crate の削除 (2026-07-17 追記)

本 ADR で「廃止」とした `src/cli-push-pipeline/` は、pnpm scripts / `build:all` / `.claude/*.exe` からは移行時点で参照が切れていたが、**Cargo workspace の member としては残存**していた (ADR-026 が「削除は別 PR」としてスコープ外に置いたため)。結果、毎 push の `cargo clippy --workspace` / `cargo test` が dead crate をビルド・実行し続けていた。本追記の PR で crate ディレクトリごと削除し、workspace member (22 → 21 crate) からも除去した。

- **dead code であることの根拠**: 実装は既に存在しない `hooks-config.toml` の `[push_pipeline]` セクションを読む前提 (`main.rs` の `Config`) で、設定移行 (上記「設定ファイルの分離」) 後は**仮に実行しても動作しない**状態だった。他 crate からの path 依存もゼロ (削除前に `Cargo.toml` 全 grep で確認)。
- **履歴の所在**: 実装は git 履歴に残る。復元が必要な場合は本 ADR の移行前 revision を参照する。
- **残置した参照**: ADR-008 / 009 / 010 / 012 は当時の設計を記録した歴史的文書のため、`cli-push-pipeline` の記述をそのまま残す (本 ADR が置換と削除の経緯を持つ)。生きたコード側の参照 (`lib-subprocess` の doc コメント) と、未実施タスクの crate 一覧 (ADR-044 / `docs/todo10.md`) は本 PR で更新した。

### 維持

- `cli-pr-monitor.exe` — post-push/post-PR ポーリングは現行のまま
- `check-ci-coderabbit.exe` — cli-pr-monitor から呼ばれる
- `lib-report-formatter` — cli-pr-monitor / check-ci-coderabbit が使用
- PreToolUse の `jj-push-guard` — pnpm push への誘導は継続

### 新規追加

- `src/cli-push-runner/` — takt ベースの push パイプライン
- `push-runner-config.toml` — push-runner の設定ファイル
- `.takt/` — takt ワークフロー・facets
- `takt` devDependency (package.json)

## push 戦略の 2 層管理原則 (2026-07-13 追記)

push 戦略 (どの bookmark を、どの経路で push してよいか) は **hook 層と exe 実装層の 2 層で独立に管理されており、片方だけでは守れない**。ADR-045 の並列 workspace 事故 (lost-update incident) の調査で、`pnpm push` (cli-push-runner) が内部で `jj git push --all` を無条件実行しており、hook 層のガードが自動化経路に一切効いていないことが判明した実例を原則化する。

| 層 | 担当範囲 | 実装 |
|---|---|---|
| hook 層 (hooks-pre-tool-validate `jj-push-guard`) | **対話操作のみ**: Claude が Bash tool で直接打つ `jj git push` / `jj push` を全 block し `pnpm push` へ誘導 | `src/hooks-pre-tool-validate/src/presets/jj.rs` |
| exe 実装層 (cli-push-runner push stage) | **自動化経路**: pipeline 内部の push コマンド。hook のスコープ外 (Bash tool を経由しない subprocess) のため、push 範囲の制御は exe/config 側で行う | `src/cli-push-runner/src/stages/push.rs` `build_push_command` — bookmark_check の検出名から `-b <name>` を組み立て、`--all` を廃止 (2026-07-13) |

原則: **push 戦略を変更するときは両層を確認する**。「hook で block したから安全」は対話操作にしか成立しない。逆に exe 層の制御は Claude の直接操作には効かない。片層のみの変更は保護の非対称 (asymmetric guard coverage、review-security-whole の観点) を生む。

補足 — ADR-011 との整合: ADR-011 (jj 0.37 前提) は新規 bookmark push を `remotes.origin.auto-track-bookmarks` 設定で解決する戦略だったが、その後の実装は config コメントベースで `--all` を採用し、ADR-011 と乖離していた。jj 0.42 では `jj git push -b <name>` が未 tracking の新規 bookmark を自動 track するため、auto-track 設定も `--all` も不要になり、本追記の `-b` 明示方式で両者の課題が解消される。

## push パイプラインの所要時間 — before / after (2026-09-03 記録)

`docs/push-pipeline-fix-plan.md` / `push-pipeline-fix-plan2.md` (ephemeral 計画、2026-09-03 に削除条件充足で削除) が持っていた**唯一の実測記録**をここへ移した。同計画の削除条件 3 が「after 計測とベースラインの 3 点比較を関連 ADR に記録すること」を求めていたため、その記録先が本節である。

### 計測方法

- **before**: 2026-07-16 時点の `.takt/runs/` 直近 20 run を、各 run の `meta.json` の startTime/endTime と `trace.md` の `- Started:` / `- Completed:` 行から集計
- **after**: 2026-08-20 以降の 108 run を `.claude/telemetry/push-runs-*.jsonl` (T0/R3 で追加した stage 別計測ログ) から集計。書式の定義元は `src/cli-push-runner/src/log.rs` の `format_stage_elapsed()`

**計測基盤そのものが本計画の成果である** — before は手作業の run 走査、after は永続化された JSONL からの集計で、同じ手順では取れていない。値の比較は「同じ対象を測っている」範囲で読むこと。

### 3 点比較

| 指標 | before (2026-07-16、n=20) | after (2026-08-20〜、n=108) | 目標 | 判定 |
|---|---|---|---|---|
| takt 部分 中央値 | 3.8 分 | **3.3 分** | — | 改善 |
| コード変更 push (fix あり) | 5.5〜14.6 分 (**範囲**) | **中央値 4.7 分** (n=95) | 7 分以下 | **達成** |
| docs-only push | (計測なし) | **中央値 3.0 分** (n=13) | 1 分台 | **未達** |

**before と after で統計量が違う。** before の原典 (削除した計画の §1) は fix あり run を**範囲**でしか持たず、中央値は残っていない。したがって上表の 2 行目は「範囲の下端 5.5 分」と「中央値 4.7 分」の比較であり、**中央値どうしの比較ではない**。目標 (7 分以下) との判定は after の中央値で行っている。takt 部分だけは before/after とも中央値で揃っている。

### docs-only の目標は再解釈する

**「docs-only push 1 分台」は総時間としては達成しない。** [ADR-057](adr-057-docs-only-deterministic-routing.md) の routing を入れた後も「docs-only でも takt レビューは skip しない」ことをユーザーが決めており (docs の事実誤りを takt が実際に検出した実績があるため)、**takt が支配項として残る**。

したがって目標は「**決定論ゲート部分の短縮**」へ読み替える。gate 部分の実測は 3.0 秒で、総時間 3.0 分の大半は takt が占める。旧計画の学び「見積も目標も実測で見直す」をここに適用した。

### 残る観測

- takt の最大値は 21.1 分 (after)。中央値は改善したが**裾は伸びている** — fix step の iteration 数に律速され、REJECT が続いた run で長くなる
- `post_takt_regate` は集計対象 108 run すべてで発火している ([ADR-058](adr-058-post-takt-regate.md))

## 検討して採らなかった方向 (2026-09-03 移送)

削除した ephemeral 計画 (`push-pipeline-fix-plan.md` § 7) が持っていた**非推奨判断**。いずれも「やらないと決めた理由」であり、同じ提案が再び出たときに再検討のコストを払わないために残す。

| 方向 | 採らない理由 |
|---|---|
| **takt 離脱** (Rust 直オーケストレーション + `claude -p` 直呼び) | 判断材料が未取得。[ADR-055](adr-055-firing-telemetry-collection.md) の telemetry と `check-ci-coderabbit --list-findings` で「pre-push APPROVE 後に CodeRabbit が何を出したか」を突合してから判断する |
| **pre-push AI レビューの廃止** (CodeRabbit 全面依存) | CodeRabbit の rate-limit ([ADR-019](adr-019-coderabbit-review-hybrid-policy.md) 記録: 解除待ち 20〜40 分が頻発) により、push は速くなっても**PR マージまでの総時間が悪化する公算が大きい** |
| **review + fix の単一エージェント統合** | [ADR-036](adr-036-bundle-z-three-layer-review.md) が特定した self-review 盲点 (6 iteration アウトライアの根因) を再導入するため |

## 次ステップ (スコープ外)

- **cli-pr-monitor の takt 化**: daemon ポーリング完了後に takt ワークフローで CodeRabbit 指摘の自動分析・修正 (Phase 2)
- **review-rules ディレクトリの整備**: プロジェクト固有のレビュールールを外部ファイルとして管理
