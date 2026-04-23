# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。

---

## 現在進行中

### マージ後フィードバックの定常化 (post-merge-feedback 自動起動)

> **全体ゴール**: `pnpm merge-pr` 後、Stop 時に `/post-merge-feedback` skill の起動を Claude に指示する自動化を本プロジェクトで dogfood 開始できる状態にする。
>
> **設計の核 (state file + 現セッション起動)**: cli-merge-pipeline が `.claude/post-merge-feedback-pending.json` を書き込み、新規 Stop hook が検出 → `additionalContext` で Claude に skill 起動を指示。新セッションを spawn しないので ADR-014 選択肢 3「skill はメイン会話内で実行」の原則を維持し、セッション知見の引き継ぎ問題を構造的に回避する。
>
> **依存関係・順序**: `1-C (hook)` を先に進める。最後に `1-D (有効化 + 試験運用開始)`。`1-E (skill 更新)` は独立タスクとして切り出し済み。
>
> **全タスク共通の参照先**: 設計の詳細は `docs/adr/adr-029-post-merge-feedback-auto-trigger.md` (PR #69 で新規作成、PR #70 で create_new 採用 / producer フィールド追加を反映)。以降のタスクはこの ADR の仕様に従う。

#### 1-C. hooks-stop-feedback-dispatch 新規 exe (コード + 配布統合、1 PR)

- **やろうとしたこと**: Stop 時に pending file を検出し、`additionalContext` で Claude に skill 起動を指示する単一責務 hook を追加。既存 `hooks-stop-quality` とは責務分離 (ADR-022 原則)
- **現在地**: 未着手
  - [ ] `src/hooks-stop-feedback-dispatch/` 新規 crate
    - `Cargo.toml` を workspace member に登録 (ADR-026)
    - `src/main.rs` を実装:
      - stdin JSON 読み取り (`stop_hook_active` 等)
      - `stop_hook_active == true` → silent exit (無限ループ防止、hooks-stop-quality と同じパターン)
      - pending 不在 → silent exit
      - 破損 (size 0 / parse 失敗 / schema_version 不一致) → 削除して silent exit
      - stale (created_at + 24h < now) → 削除して silent exit
      - `status == "pending"` → 構造化 `additionalContext` を stdout に出力 + pending file の `status` を `"dispatched"` に atomic 更新 (`dispatched_at` も設定)
      - `status == "dispatched"` → silent exit (二重通知しない)
      - `status == "consumed"` → 削除して silent exit (後片付け)
  - [ ] `Cargo.toml` (workspace root) の `members` に追加
  - [ ] `package.json` に `build:hooks-stop-feedback-dispatch` 追加、`deploy:hooks` に統合
  - [ ] `.claude/settings.json` の Stop hook エントリに 2 つ目の exe を追加 (hooks-stop-quality の**後**の順序)
  - [ ] `templates/settings.json` にも同様の設定を反映 (派生プロジェクト配布用)
  - [ ] unit test 追加:
    - pending 不在で正常 exit
    - `stop_hook_active = true` で silent exit (pending を読まない)
    - 破損 pending の削除 + silent exit
    - stale pending の削除 + silent exit
    - status=pending → additionalContext 生成 + status=dispatched へ更新
    - status=dispatched → silent exit
    - status=consumed → 削除 + silent exit
    - additionalContext 文字列フォーマット検証 (構造化タグの key 順序等)
- **完了基準**: `cargo test` 通過 + `pnpm build:hooks-stop-feedback-dispatch` / `pnpm deploy:hooks` 成功 + hooks-stop-quality と並行動作確認
- **詰まっている箇所**: なし

#### 1-D. post_steps 有効化 + 試験運用開始 (設定 + todo 更新、1 PR)

- **やろうとしたこと**: 設定を有効化し、本プロジェクトで dogfood を開始する
- **現在地**: 未着手
  - [ ] `.claude/hooks-config.toml` の `[[merge_pipeline.post_steps]]` を有効化:
    ```toml
    [[merge_pipeline.post_steps]]
    name = "post_merge_feedback"
    type = "ai"
    prompt = "post-merge-feedback"
    ```
  - [ ] `templates/hooks-config.toml` にも反映 (派生プロジェクト用、デフォルト opt-in/opt-out 方針は PR 内で判断)
  - [ ] `docs/todo.md` から本タスク群 (1-C〜1-D、および section ヘッダーと前文) を削除 (運用ルール: 完了タスクは ADR/仕組みに反映後に削除。1-A は PR #69、1-B は PR #70 で削除済)
- **完了基準**: 実マージ (別 PR) の `pnpm merge-pr` で pending file が生成され、Stop 時に Claude が構造化 `additionalContext` を受け取って skill 起動を試みるフローが走ること (skill 未対応なら手動起動で検証)
- **詰まっている箇所**: なし
- **依存関係**: 1-C (hook) の完了

#### 1-E. post-merge-feedback skill の pending file 対応 (別タスク、skill リポジトリ側で実施)

- **やろうとしたこと**: skill Phase 1 の前段に「pending file 先読み (Phase 0)」を追加し、status が `"dispatched"` の場合は引数指定と同等の最優先度で採用。skill 完了時に `status = "consumed"` に更新してからファイル削除
- **現在地**: 未着手
  - [ ] skill リポジトリの管理場所を特定 (`$CLAUDE_SKILLS_REPO` 経由 or `~/.claude/skills/` 直接) → `/skill-sync-check` で確認
  - [ ] `SKILL.md` に Phase 0 「pending file 先読み」を追加:
    - pending file を読み取り、`status == "dispatched"` ならその `pr_number` / `owner_repo` を採用 (引数・セッションコンテキスト・fallback より優先)
    - `status == "pending"` (hook 未経由の fallback 経路) も受け入れる
    - `status == "consumed"` なら無視してファイル削除
  - [ ] skill 完了時の consume 処理: `status = "consumed"` + `consumed_at` 設定 (atomic 更新) → その後ファイル削除
  - [ ] (任意) skill eval の追加: pending file ありのケース / 破損ケース / status 別の挙動
- **完了基準**: skill が pending file を正しく consume し、本プロジェクトの dogfood で Claude が自動起動した skill から Feedback Report が出力される
- **詰まっている箇所**: skill の管理場所 (本プロジェクト外) の扱いは `/skill-sync-check` の結果次第
- **依存関係**: 1-C/1-D とは並行可能だが、dogfood の完結には 1-E も必要

#### 1-F. (追って) ADR-014 試験運用フラグ解除 + takt-test-vc 反映

- **やろうとしたこと**: dogfood 1-2 週間で問題なければ ADR-014 を本採用化し、takt-test-vc へバックポート
- **現在地**: 未着手。1-D 以降 + 運用観察が前提
- **詰まっている箇所**: dogfood 結果に依存するため着手タイミングは未定
- **依存関係**: 1-D 完了 + 本プロジェクトで実マージ数回の観察

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

### Skill 運用基盤由来

- [ ] **skill evals の自動 runner 統合**: `E:\work\claude-code-skills` 配下 skill の `evals.json` / `trigger_eval.json` を skill-creator:skill-creator や `/skill-sync-check` に乗せて定期実行する仕組み。現状は手動実行のみ。prepare-pr の試験運用評価 (分離前後の発火頻度比較・フロー完了率・draft 初稿品質) の定量データ集計にも必要

### ADR-022 v3 (2026-04-21 改訂) 由来

- [ ] **takt fix による最終 commit message 草案生成機能の実装**: child commit の description が `fix(review): apply CodeRabbit fixes for #<PR>` のように「機械ログ化」して人間が読む価値が薄い問題を緩和する。takt fix の report phase で「最終的に人間が採用する統合 commit message の草案」を `.takt/runs/*/reports/final-commit-message-draft.md` 等に書き出し、`prepare-pr` skill が起動時にこれを読み込んで draft 初稿の元ネタとする。ADR-022 原則 1 改訂版の「草案生成」で正面から許可されており、別 PR で実装
- [ ] **auto-rebase / auto-squash / auto-format commit history の検討**: ADR-022 原則 1 改訂版の緩和条項 (可逆・事前ポリシー・意図不変・PR 外) を満たす範囲で将来実装可能。必要になった時点で別 ADR を作成し運用ポリシーを明示してから実装

### ADR-022 原則 5 (PR 包含 changeset の不変性) 由来

- [ ] **interactive Claude Code の amend 挙動を "PR 包含チェック" で gate する実装**: `pnpm push` (cli-push-runner) または Claude Code session 側で、`@` bookmark が open PR に紐付いているかを `gh pr list --head <bookmark> --state open --json number` で判定。紐付いている場合は `jj describe` やファイル edit による auto-amend を警告 or 自動的に child commit に切り替える。紐付いていない場合は現行通り amend 許可。takt fix は task 4 (PR #63) で既に child commit 化済のため対象は interactive 経路。設計段階、未着手
