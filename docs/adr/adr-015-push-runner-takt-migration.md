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

### 設定ファイルの分離

push-runner の設定は `push-runner-config.toml` (リポジトリルート) に配置し、hooks-config.toml の `[push_pipeline]` セクションは削除する。理由:

- push-runner は Claude Code hooks ではなく CLI exe であり、hooks-config.toml の管轄外
- takt 固有の設定 (`[takt]` セクション) が hooks-config.toml の関心事と合わない
- takt-test-vc との設定互換性を維持しやすい

## 影響

### 廃止

- `src/cli-push-pipeline/` — cli-push-runner に完全置き換え
- `hooks-config.toml` の `[push_pipeline]` セクション — push-runner-config.toml に移行
- `pnpm review:ai` スクリプト — takt が内部で AI レビューを処理

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

## 次ステップ (スコープ外)

- **cli-pr-monitor の takt 化**: daemon ポーリング完了後に takt ワークフローで CodeRabbit 指摘の自動分析・修正 (Phase 2)
- **review-rules ディレクトリの整備**: プロジェクト固有のレビュールールを外部ファイルとして管理
