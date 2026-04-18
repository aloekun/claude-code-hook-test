# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。

---

## 現在進行中

### 1. pre-push-review の arch-review → simplicity-review 絞り込み

- **やろうとしたこと**: `pnpm push` のセルフレビューに時間がかかる問題 (ADR 1 本追加だけでも 5 分超) の解消。本来 push 時点では「コードのシンプルさ」を見たかったのに、現状は arch-review が architecture 全般を見ており、その重装備が遅さの主因になっている。別セッションで修正予定
- **現在地**: 実装済み (ADR-027)、実測検証のみ残り
  - [ ] 実測: 変更前後で `.takt/runs/*/meta.json` の duration を比較し、期待値 (5m → 2m) 通りか検証

### 2. マージ後フィードバックの定常化 (cli-merge-pipeline の post_steps 統合)

- **やろうとしたこと**: `pnpm merge-pr` 後の「ADR 記録すべきもの」「仕組みに反映すべきもの」の手動依頼を自動化。ADR-014 で提唱された `post-merge-feedback` スキルを cli-merge-pipeline から自動起動する
- **現在地**: 設計段階。未着手
  - [ ] `src/cli-merge-pipeline/src/main.rs` の `run_steps` の `"ai"` 分岐を現在の `SKIP` から実装に置き換える (takt 経由で skill を起動、または claude -p で起動)
  - [ ] `.claude/hooks-config.toml` の `[[merge_pipeline.post_steps]]` に `type = "ai"`, `prompt = "post-merge-feedback"` を設定
  - [ ] `post-merge-feedback` スキルが PR 番号とブランチ名を受け取れるよう、cli-merge-pipeline から環境変数または引数で渡す設計
  - [ ] マージ済みセッションの会話ログを参照する手段 (Claude Code Session ID 等) の検討
- **詰まっている箇所**:
  - **主要ブロッカー**: 「マージ時点のセッション会話」を post_steps 用の新セッションに引き継ぐ手段が決まっていない。会話ログがないと「何を議論した末のマージか」が失われ、フィードバック品質が下がる
    - **Why**: post-merge-feedback は ADR-014 で「セッション知見 + PR 知見の統合」を前提にしているが、merge-pipeline は別プロセスで起動されるため会話がない状態から始まる
    - **How to apply / 再開手順**: SessionStart hook (master の `src/hooks-session-start/`) で伝播した session ID を jsonl transcript に紐付けて読み取る方式が候補。ADR を書いてから実装
  - **制約**: ADR-016 (長時間コマンド) のため、post_steps の AI 起動も `run_in_background: true` + `timeout: 600000` 前提で設計する必要あり
- **依存関係**:
  - SessionStart hook は master に実装済み (`src/hooks-session-start/`)。セッション引継ぎ設計は session ID → jsonl transcript 紐付けの ADR が必要
  - takt-test-vc での試験運用を先に行い、本プロジェクトに反映

### 3. PostToolUse hook に UTF-8 整合性チェック (Layer 0) を追加

- **やろうとしたこと**: takt の自動修正が日本語文字列を破壊する問題 (PR #50 で発生) を PostToolUse hook で検出する
- **現在地**: 実装済み、ビルド+デプロイ待ち
  - [x] `check_utf8_integrity()` 関数の実装 (`std::fs::read` + `from_utf8_lossy` で U+FFFD を検出)
  - [x] `main()` の第0層として呼び出し追加 (カスタムルール・外部ツールより先に実行)
  - [x] テスト 5 本 (FFFD 検出、clean file、invalid bytes、複数行、存在しないファイル)
  - [ ] ビルド + デプロイ
- **詰まっている箇所**: なし

### 4. exe 自己修正時のビルド手順

- **運用ルール**: exe を修正する PR では、push 前に `pnpm build:all` で修正済み exe をビルドする。デプロイ (`pnpm deploy:hooks`) は各派生プロジェクト側の準備が必要なため別タイミングで実施
- **根拠**: PR #50/#51 の push 時に、修正対象のバグ (#4: bookmark 乖離、#5: 空 diff 中断) がデプロイ済み exe で再現した。修正済み exe でビルドしてから push すれば回避できた

### 5. Cargo workspace 化 + rust-test template 反映 (PR-beta、実装済み)

- **やろうとしたこと**: PR #44 のセッション知見を元に Cargo workspace 化
- **現在地**: 実装完了、PR 作成待ち
  - [ ] PR 作成 → レビュー → マージ
- **詰まっている箇所**: なし
- **参照 ADR**: ADR-026

---

## スコープ外だが将来検討

### ADR-027 / PR #47 由来

- [ ] **loop_monitor judge の軽量化**: step 間 transition で毎回 AI 呼び出しされる judge を、閾値到達前はスキップする最適化。takt 本体にオプションがあるか未調査。実測で隠れオーバーヘッドが 15-70s/遷移、17-iter run では累計 ~6 分
- [ ] **post-pr-monitor の re-push 時ポーリング問題**: re-push 後に CodeRabbit の新しいレビュー (新しい commit に対するレビュー) を待たずに旧状態で即判定している。PR 作成時は初回レビュー投稿を検出できるが、re-push 時は `new_comments: 0` で即 approved → 新レビューを見逃す。対策案: ポーリング開始前に「push 後の新しい review comment が来るまで待機」するロジックの追加 (commit SHA の比較等)
- [ ] **analyze-coderabbit.md と fix.md の read-only zone 定義の齟齬**: analyze ステップは `.takt/workflows/` を「人間が編集する源泉だから read-only zone ではない」と判断して finding を applicable とするが、fix ステップは `.takt/workflows/**` を ABSOLUTE read-only として修正不可。結果として misdirected finding が 1 iteration 分のコストを浪費する。対策案: analyze 側で `.takt/` 全体を not_applicable にするか、fix 側で `.takt/workflows/` を編集可能にするかの二者択一

### ADR-019/020 由来

ADR-019 および ADR-020 の「次ステップ」セクションで明記された未着手項目:

- [ ] **analyze instruction の強化**: ADR を自動検索して filter ルールを動的に抽出
- [ ] **Learning と ADR の双方向同期**: ADR を更新したら CodeRabbit Learning にも通知
- [ ] **他 AI レビュー統合**: Copilot review, Greptile などを ADR-019 の 3 レイヤー構成に乗せる
- [ ] **instruction 参照整合性 lint**: workflow YAML の `instruction:` 参照先と facets 実ファイルの存在を突合
- [ ] **verdict 値の整合性 lint**: workflow の `condition` 値と instruction の出力例の一致を検証 (PR #41 CodeRabbit Major 指摘の再発防止)
- [ ] **takt-test-vc への還元**: 共通 facets パターンを takt のサンプルリポジトリにも反映
