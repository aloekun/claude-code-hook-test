# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。

---

## 現在進行中

### マージ後フィードバックの定常化 (post-merge-feedback 自動起動)

> **全体ゴール**: `pnpm merge-pr` 後、Stop 時に `/post-merge-feedback` skill の起動を Claude に指示する自動化を本プロジェクトで dogfood 開始できる状態にする。
>
> **設計の核 (state file + 現セッション起動)**: cli-merge-pipeline が `.claude/post-merge-feedback-pending.json` を書き込み、新規 Stop hook が検出 → `additionalContext` で Claude に skill 起動を指示。新セッションを spawn しないので ADR-014 選択肢 3「skill はメイン会話内で実行」の原則を維持し、セッション知見の引き継ぎ問題を構造的に回避する。
>
> **依存関係・順序**: `1-A (ADR 策定)` を先に完了。以降は `1-B (CLI)` と `1-C (hook)` を並行可。最後に `1-D (有効化 + 試験運用開始)`。`1-E (skill 更新)` は独立タスクとして切り出し済み (依存: `1-A` の ADR-029 確定)。
>
> **全タスク共通の参照先**: 設計の詳細は `docs/adr/adr-029-post-merge-feedback-auto-trigger.md` (1-A で新規作成)。以降のタスクはこの ADR の仕様に従う。
>
> **採否済みのフィードバック論点** (1-A の ADR-029 に反映すべき):
> 1. 多重実行耐性 → pending file に `status: "pending" | "dispatched" | "consumed"` を持たせる
> 2. atomic write / 破損耐性 → atomic rename、読み取り時は size 0 / parse 失敗で削除、ロック不要
> 3. 既存 pending との競合 → 既存 `status != "consumed"` なら新規書き込み skip + WARN (将来キュー化への拡張余地を Note に明記)
> 4. `additionalContext` は構造化タグ形式 (`[POST_MERGE_FEEDBACK_TRIGGER]` 等) にする
> 5. `run_steps` は `Option<&PipelineContext>` で後方互換を保つ

#### 1-A. ADR-029 策定 + 既存 ADR 改訂 (docs のみ、1 PR)

- **やろうとしたこと**: pending file 方式の設計を正面から確定させ、1-B/1-C/1-D/1-E の実装判断根拠を作る。ADR-014 と ADR-013 も将来の展望を更新して整合を取る
- **現在地**: 未着手
  - [ ] `docs/adr/adr-029-post-merge-feedback-auto-trigger.md` 新規作成
    - ADR-014 選択肢 3 を維持したまま自動化する論拠 (exe からの AI spawn を避け、現セッションで skill を起動する)
    - pending file JSON スキーマ (v1): `{ schema_version, pr_number, owner_repo, prompt, status, created_at, dispatched_at, consumed_at }`
    - 配置パス: `.claude/post-merge-feedback-pending.json`
    - 状態遷移: `pending → dispatched → consumed → 削除` と stale TTL (24h) による強制削除
    - 競合ポリシー: 既存 `status != "consumed"` → 新規書き込み skip + WARN (取りこぼしの可観測性を残す)。将来拡張としてキュー化 (`.claude/post-merge-feedback/<pr>.json`) 移行の余地を Note
    - 破損耐性: size 0 / JSON parse 失敗 / schema_version 不一致 → 該当ファイル削除後に silent exit。ロックは使わない (書き込みは atomic rename で十分)
    - `additionalContext` 構造化フォーマット仕様 (例: `[POST_MERGE_FEEDBACK_TRIGGER]\nschema_version: 1\npr_number: 123\nowner_repo: ...\naction: invoke_skill\ncommand: /post-merge-feedback 123\nreason: cli-merge-pipeline wrote pending artifact`)
    - ADR-022 原則 1 との整合性: pending file は「新規 artifact への自己記述」、status 更新は「自身が作成した artifact への自己更新」で両方とも許可側
    - ADR-013 / ADR-014 / ADR-016 / ADR-022 との関係説明
  - [ ] `docs/adr/adr-014-post-merge-feedback.md` 「将来の展望」更新: ADR-029 リンク追加、選択肢 1 却下と state file 方式が衝突しないことを明記
  - [ ] `docs/adr/adr-013-merge-pipeline.md` 「将来の展望」更新: `ai` ステップ実装方式を明記
  - [ ] `CLAUDE.md` の ADR リストに ADR-029 追加
- **完了基準**: ADR-029 が承認済みステータスで merge されており、以降の PR がこの仕様を参照可能
- **詰まっている箇所**: なし。仕様は採用済みフィードバックで確定済み

#### 1-B. cli-merge-pipeline の `ai` 分岐実装 (コード + テスト、1 PR)

- **やろうとしたこと**: 現状 SKIP 実装の `run_steps` の `"ai"` 分岐 ([src/cli-merge-pipeline/src/main.rs:313-322](../src/cli-merge-pipeline/src/main.rs#L313-L322)) を、ADR-029 仕様に沿った pending file 書き込みに置き換える
- **現在地**: 未着手
  - [ ] `PipelineContext` struct を新設 (`pr_number: u64`, `owner_repo: Option<String>`)
  - [ ] `run_steps` シグネチャを `Option<&PipelineContext>` で拡張 (後方互換、pre_steps は `None` を渡す)
  - [ ] `"ai"` 分岐の実装:
    - ctx が `None` → SKIP + log
    - 既存 pending 読み取り: `status != "consumed"` なら WARN + skip (ステップ自体は PASS 扱い)、破損ファイル (size 0 / parse 失敗 / schema_version 不一致) は削除して続行
    - 新規 pending 書き込み (`status = "pending"`): tmp file → `fs::rename` で atomic
    - pending file パス: `config_path().parent() / "post-merge-feedback-pending.json"`
  - [ ] unit test 追加:
    - 正常書き込み (ctx ありで新規作成)
    - ctx なしで SKIP
    - 既存 consumed 上書き成功
    - 既存 pending/dispatched で skip + WARN
    - 破損 pending (parse 失敗) で削除後書き込み
    - tmp → rename の atomicity (partial file が残らない)
- **完了基準**: `cargo test` 通過 + ローカルで `pnpm merge-pr` 手動実行 → 正しい pending file が生成される
- **詰まっている箇所**: なし
- **依存関係**: 1-A (ADR-029) のスキーマ確定後に着手

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
- **依存関係**: 1-A (ADR-029) のスキーマと additionalContext フォーマット確定後に着手

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
  - [ ] `docs/todo.md` から本タスク群 (1-A〜1-D) を削除 (運用ルール: 完了タスクは ADR/仕組みに反映後に削除)
- **完了基準**: 実マージ (別 PR) の `pnpm merge-pr` で pending file が生成され、Stop 時に Claude が構造化 `additionalContext` を受け取って skill 起動を試みるフローが走ること (skill 未対応なら手動起動で検証)
- **詰まっている箇所**: なし
- **依存関係**: 1-B (CLI) + 1-C (hook) 両方の完了

#### 1-E. post-merge-feedback skill の pending file 対応 (別タスク、skill リポジトリ側で実施)

- **やろうとしたこと**: skill Phase 1 の前段に「pending file 先読み (Phase 0)」を追加し、status が `"dispatched"` の場合は引数指定と同等の最優先度で採用。skill 完了時に `status = "consumed"` に更新してからファイル削除
- **現在地**: 未着手。1-A (ADR-029) 確定後に仕様参照可能
  - [ ] skill リポジトリの管理場所を特定 (`$CLAUDE_SKILLS_REPO` 経由 or `~/.claude/skills/` 直接) → `/skill-sync-check` で確認
  - [ ] `SKILL.md` に Phase 0 「pending file 先読み」を追加:
    - pending file を読み取り、`status == "dispatched"` ならその `pr_number` / `owner_repo` を採用 (引数・セッションコンテキスト・fallback より優先)
    - `status == "pending"` (hook 未経由の fallback 経路) も受け入れる
    - `status == "consumed"` なら無視してファイル削除
  - [ ] skill 完了時の consume 処理: `status = "consumed"` + `consumed_at` 設定 (atomic 更新) → その後ファイル削除
  - [ ] (任意) skill eval の追加: pending file ありのケース / 破損ケース / status 別の挙動
- **完了基準**: skill が pending file を正しく consume し、本プロジェクトの dogfood で Claude が自動起動した skill から Feedback Report が出力される
- **詰まっている箇所**: skill の管理場所 (本プロジェクト外) の扱いは `/skill-sync-check` の結果次第
- **依存関係**: 1-A (ADR-029 スキーマ確定)。1-B/1-C/1-D とは並行可能だが、dogfood の完結には 1-E も必要

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
