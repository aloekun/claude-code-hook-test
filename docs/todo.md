# TODO

> **運用ルール**: 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。

---

## 現在進行中

### マージ後フィードバック機構の決定論化 (ADR-030 起案 + 実装)

> **動機**: PR #74 マージ後の dogfood で、ADR-029 設計の **silent loss 問題** が顕在化した。Stop hook + skill ベースの auto-trigger は Claude のターン取得次第で機能せず、決定論的実行が成立しない。skill 機構は本質的に "ask-based" であり must-run 要件には不適合という設計上の知見が得られた。
>
> **本タスクの位置づけ**: ADR-029 を partial supersede する新 ADR-030 を起案し、takt 経由の決定論的フィードバック機構へ移行する。本タスク完了で post-merge-feedback skill / pending file / Stop hook (hooks-stop-feedback-dispatch) はすべて廃止される。

#### 背景: ADR-029 の構造的欠陥 (PR #74 dogfood で実証)

ADR-029 は 4 層のトリガー機構を直列に積んでいるが、後半 2 層が非決定的:

| 層 | 機構 | 決定論性 | PR #74 で何が起きたか |
|---|------|---------|---------------------|
| 1 | cli-merge-pipeline が pending file を書き込む | ✅ 決定論的 | 正常動作 (status=pending) |
| 2 | Stop hook が pending を読み additionalContext を出力 | ✅ 決定論的 | 正常動作 (status→dispatched) |
| 3 | Claude が次ターンで additionalContext を読む | ❌ **非決定的** | ユーザー入力先行で次ターン消失 |
| 4 | Claude が skill 起動命令と解釈・実行 | ❌ **非決定的** | (3 で詰まったので未到達) |

層 3 はセッションライフサイクル依存 (ユーザー入力 / VSCode 終了 / Claude Code 再起動 で容易に壊れる)。層 4 は skill design philosophy が ask-based (AskUserQuestion で中止可、命令無視可)。**must-run 要件に skill を主動線で使うのは設計ミスと判定**。

dogfood では PR #74 マージ後、pending file が `dispatched` で stuck した状態で session が終了 → 24h 後に stale 削除 → フィードバック silent loss という最悪の経路が再現可能。

#### 設計決定: 2 層アーキテクチャ (新 ADR-030)

| 層 | 機構 | 保証レベル | 失敗時 |
|---|------|-----------|--------|
| **L1 Floor** (決定論) | cli-merge-pipeline → takt workflow `post-merge-feedback` を **同期実行** | Exactly-once (takt の決定論実行) | soft: merge 成功、`<pr>.md.failed` marker 残存 |
| **L2 Recovery** (safety net) | UserPromptSubmit hook が `*.md.failed` を検出 → additionalContext で再実行指示 | At-least-once (ユーザーが何か入力すれば必ず発火) | hook 自体は決定論的、Claude の応答は best-effort (ただし floor は既存なので silent loss は起きない) |

- **失敗ポリシー**: soft (merge 成功 + marker 残存。後続 prompt 入力で L2 が拾う)
- **skill enrichment 層 (旧案 L3) は廃止** (ask-based の弱点を再導入してしまうため)
- **入力源**: PR data (gh API) + pre-push reports (`.takt/runs/`) + transcript (`~/.claude/projects/<id>/*.jsonl`、commit 時刻 range filter)
- **出力**: `.claude/feedback-reports/<pr>.md`

#### Phase 0 調査結果 (実施済 — 2026-04-25)

##### transcript ファイル所在 (確認済)

`~/.claude/projects/<project-id>/<session-id>.jsonl` (1 session = 1 file, UUID 命名)

本プロジェクト: `C:\Users\HIROKI\.claude\projects\e--work-claude-code-hook-test\`

##### transcript スキーマ (確認済)

```json
{
  "parentUuid": "...",
  "isSidechain": false,
  "type": "user" | "assistant" | "attachment" | "queue-operation",
  "timestamp": "2026-04-25T05:44:35.040Z",
  "sessionId": "<uuid>",
  "cwd": "E:\\work\\claude-code-hook-test",
  "gitBranch": "HEAD",
  "message": {
    "role": "user" | "assistant",
    "content": [
      { "type": "text", "text": "..." },
      { "type": "thinking", "thinking": "", "signature": "<encrypted>" },
      { "type": "tool_use", "name": "Bash", "input": {...} }
    ]
  }
}
```

##### 重要な制約 (実装時に必ず参照)

| 観察 | 影響 |
|---|---|
| timestamp は ISO 8601 ms 精度 | commit 時刻からの逆引き filter が容易 |
| `thinking` content は encrypted (`signature` のみ可視、`thinking` field は空) | chain-of-thought は抽出不可。user/assistant text + tool calls/outputs で十分 |
| `gitBranch` は `HEAD` 固定 (jj detached state のため) | branch 名 filter は **使えない**。**時刻 range で filter する必要** |
| 1.7 MB / 621 行 (現セッション例) | takt context window 圧迫の可能性。filter 後の絞り込みが必須 |
| `type: queue-operation` はノイズ | parsing で skip すべき |

##### transcript 抽出戦略 (Q1 = commit 時刻逆引きの具体化)

```
入力: <pr_number>
1. gh pr view <pr> --json commits,mergedAt → first_commit_time, end_time 取得
2. ~/.claude/projects/<project-id>/*.jsonl の全ファイルを mtime ∈ [first_commit_time, end_time + 1day buffer] でフィルタ
3. 該当 file 内で entry.timestamp ∈ [first_commit_time, end_time] かつ type ∈ {user, assistant} を抽出 (queue-operation, attachment は除外)
4. 合成 in-memory log を analyze-session facet に渡す
```

#### ユーザー判断記録 (本タスク策定時に合意済)

| 質問 | 回答 |
|---|---|
| 失敗時の挙動 | **soft** (merge 成功、後続 prompt で L2 が再実行) |
| レイテンシ許容 | **数分の追加レイテンシ OK** |
| Anthropic API 直接呼出し | **禁止** (pre-push-review / post-pr-monitor と同じ takt 経由パターンに統一) |
| transcript 紐付け方針 | **PR commit 時刻から逆引きして transcript 区間を抽出** |
| takt facets 構造 | **4 facets に分離** (`analyze-pr` / `analyze-session` / `analyze-prepush-reports` / `aggregate-feedback`) |
| PR 分割 | **PR 1 (ADR) → PR 2 (B) → PR 3 (C) → PR 4 (E)** の 4 段階 |
| 旧機構廃止 | 本作業計画に **Phase E として含める** (dogfood 数回後に実施) |

#### 作業計画

##### Phase B: takt workflow + 4 facets — L1 Floor (PR 2)

- [ ] `.takt/workflows/post-merge-feedback.yaml` 新規作成
  - 入力: PR 番号 (cli-merge-pipeline から渡される)
  - facets を順次 chain
- [ ] facets を新設 (`.takt/facets/` 下):
  - **`analyze-pr.md`**: PR diff + reviews を分析 (既存 `analyze-pr` skill から port。`E:\work\claude-code-skills\analyze-pr\SKILL.md` を参照)
  - **`analyze-session.md`** (新規): transcript range filter で抽出した user/assistant 履歴から 実装時の学び・トラブル修正・ユーザー指示 を抽出。**Phase 0 調査結果の transcript 抽出戦略を参照**
  - **`analyze-prepush-reports.md`** (新規): `.takt/runs/<latest>/reports/*.md` (pre-push-review の simplicity / security レポート) を集約
  - **`aggregate-feedback.md`** (新規): 上記 3 facets の出力 + Plankton 優先度で統合 → ADR 提案 / 仕組み改善案を生成 (旧 post-merge-feedback skill の Phase 4 統合フィードバックロジックを port、`E:\work\claude-code-skills\post-merge-feedback\SKILL.md` を参照)
- [ ] `src/cli-merge-pipeline/` の post_steps `type = "ai"` 分岐を変更:
  - 旧: pending file 書き込み (ADR-029)
  - 新: takt workflow を spawn して同期実行 (push-runner / cli-pr-monitor の takt 起動方法に倣う)
  - 出力 `.claude/feedback-reports/<pr>.md` を生成
  - 失敗時 `<pr>.md.failed` marker 書き込み (soft fail、merge は成功扱い)
- [ ] `.gitignore` に `.claude/feedback-reports/` を追加 (artifact、コミット対象外)
- [ ] テスト追加 (workflow 成功/失敗ケース、transcript 抽出正常性、cli-merge-pipeline 統合)

##### Phase C: UserPromptSubmit hook — L2 Recovery (PR 3)

- [ ] `src/hooks-user-prompt-feedback-recovery/` 新規 crate
  - `Cargo.toml` を workspace member に追加 (ADR-026)
  - `src/main.rs`:
    - stdin から UserPromptSubmit event JSON を読む
    - `.claude/feedback-reports/*.md.failed` を検索
    - 見つかれば additionalContext で「未完了 feedback あり、再実行: `pnpm feedback-retry <pr>`」を出力
    - 見つからなければ silent exit
- [ ] `Cargo.toml` (workspace root) の `members` に追加
- [ ] `package.json` の `build:hooks-user-prompt-feedback-recovery` 追加、`deploy:hooks` に統合
- [ ] (任意) `pnpm feedback-retry <pr>` script 追加 (cli-merge-pipeline の post_steps を単独再実行する thin wrapper)
- [ ] settings.local.json + `templates/settings.json` の UserPromptSubmit hook エントリ登録
- [ ] テスト追加 (failed marker 検出、additionalContext フォーマット)

##### Phase D: ❌ 廃止 (skill enrichment 不要)

##### Phase E: 旧機構廃止 (PR 4 — Phase B/C dogfood 数回後)

- [ ] post-merge-feedback skill 削除
  - `~/.claude/skills/post-merge-feedback/` 削除
  - `E:\work\claude-code-skills\post-merge-feedback\` 削除
- [ ] hooks-stop-feedback-dispatch.exe / `src/hooks-stop-feedback-dispatch/` crate 削除
  - workspace member から外す
  - `package.json` の build/deploy script 削除
  - `.claude/hooks-stop-feedback-dispatch.exe` 配布物削除
- [ ] `src/lib-pending-file/` crate 廃止 (cli-merge-pipeline からの依存削除、`pending_file.rs` も整理)
- [ ] `.claude/hooks-config.toml` から Stop hook の `hooks-stop-feedback-dispatch` 登録を削除 (既に明示登録されていない可能性あり、要確認)
- [ ] settings.local.json + `templates/settings.json` から Stop hook 登録解除
- [ ] `.gitignore` から `.claude/post-merge-feedback-pending.json` 行を削除 (もう使わない)
- [ ] ADR-029 のステータスを `Superseded by ADR-030` に変更
- [ ] ADR-014 のステータスを `Superseded by ADR-030` に変更
- [ ] memory `feedback_*.md` / `project_*.md` で post-merge-feedback skill / pending file に言及している記述を更新
- [ ] CLAUDE.md の Architecture Decisions リストで ADR-014 / ADR-029 のステータス記載を更新

##### Phase F: dogfood 検証 (PR 4 マージ後 / 継続観察)

- [ ] 3-5 回の実マージで feedback report が **必ず生成** されることを確認 (silent loss 0 を証明)
- [ ] L2 recovery を人為的失敗で発火確認 (cli-merge-pipeline で takt fail を inject、`<pr>.md.failed` marker 残存 → 次 prompt で UserPromptSubmit hook 発火 → 再実行成功)
- [ ] transcript からの session 知見抽出が想定通りか確認 (実装時の学び・トラブル・ユーザー指示が拾えるか)
- [ ] feedback report の品質評価 (ADR 提案 / 仕組み改善案が出るか、Plankton 優先度が機能しているか)

#### 作業可能になるための前提情報 (新セッションで必読)

##### 既存コンポーネントとの参照関係

- **既存 takt workflow 例** (新 workflow `post-merge-feedback` も同じパターンで構築):
  - `pre-push-review`: simplicity-review + security-review facets (ADR-020)
  - `post-pr-review` (cli-pr-monitor 内): analyze-pr-review-comments + supervise / fix facets (ADR-018)
- **既存 cli-merge-pipeline**: post_steps `type = "ai"` 分岐の現行実装 (pending file 書き込み) を Phase B で takt workflow 起動に置き換える
- **既存 skill `analyze-pr`** (`E:\work\claude-code-skills\analyze-pr\SKILL.md`): facet `analyze-pr.md` への port 元
- **既存 skill `post-merge-feedback`** (`E:\work\claude-code-skills\post-merge-feedback\SKILL.md`): Phase 4 統合フィードバックロジックを `aggregate-feedback.md` facet への port 元

##### 重要な既存 ADR (実装時に必ず参照)

| ADR | 関係 |
|---|---|
| **ADR-014** | post-merge-feedback skill 自体の起案 (試験運用)。本タスク完了で **Superseded** |
| **ADR-015** | push-runner takt 移行。本タスクの設計パターン (CLI exe → takt workflow) の **先行事例** |
| **ADR-018** | cli-pr-monitor takt 移行。同上 |
| **ADR-020** | takt facets (fix/supervise) 共通化戦略。本タスクの **4 facets 分離方針の根拠** |
| **ADR-022** | 自動化コンポーネントの責務分離原則。L1 takt 経由は本原則に整合 (Claude 不在でも動く) |
| **ADR-026** | Cargo workspace。新 crate (`hooks-user-prompt-feedback-recovery`) はこのワークスペースに追加 |
| **ADR-028** | 外部可視成果物ゲート。本タスクは内部 artifact のみで対象外 |
| **ADR-029** | 本タスクで **partial supersede** (層 3-4 廃止、層 1 流用) |

##### memory 参照

- `feedback_side_effect_integration.md`: cleanup / consume 処理を新 phase ではなく既存 phase 末尾に統合する原則 (本設計の Phase D 廃止判断にも適用)
- `feedback_verify_edit_results.md`: 大きな Edit 後は grep で見出し検証 (Phase B での takt workflow / facets ファイル作成時に有用)
- `project_takt_push_runner_learnings.md`: takt 導入の知見 (バージョン固定、ハイブリッド構成等)
- `project_takt_pre_push_iterations.md`: takt fix の child commit 自動収束パターン

##### 残存する PR #74 の pending file の扱い

- 現状 `.claude/post-merge-feedback-pending.json` に PR #74 の pending file が残存している可能性 (status=dispatched のまま consume されていない)
- **対処方針**: Phase E で pending file 機構ごと廃止するため明示対処不要。Phase A 着手前に手動 `rm` でもよい (新セッション開始時の状態を整理する目的なら推奨)

##### 新セッションで最初に確認すべきこと

1. `git log --oneline -5` で master の最新状態を確認
2. `docs/todo.md` の本セクション (本記録) を読む
3. `docs/adr/adr-029-post-merge-feedback-auto-trigger.md` を読む (supersede 元の理解)
4. `docs/adr/adr-014-post-merge-feedback.md` を読む (skill 自体の元設計)
5. `docs/adr/adr-015-push-runner-takt-migration.md` / `docs/adr/adr-018-pr-monitor-takt-migration.md` を読む (takt 移行の先行事例として参考)
6. `docs/adr/adr-020-takt-facets-sharing.md` を読む (facets 共通化方針の根拠)
7. Phase A から着手: `docs/adr/adr-030-deterministic-post-merge-feedback.md` の起案

#### 完了基準

- Phase A〜F すべて完了
- dogfood で 3-5 回連続マージ → 全 PR にレポート生成 (silent loss 0 を実証)
- ADR-029 / ADR-014 のステータスが `Superseded by ADR-030` に更新済
- 旧 skill / hook / lib-pending-file が repository から削除済
- 本 todo.md エントリを削除 (運用ルール: 完了タスクは ADR/仕組みに反映後に削除)

#### 詰まっている箇所

なし (全方向確定済、Phase A から着手可能)

### (追って) ADR-030 の takt-test-vc 反映

> **参照**: 上位タスク「マージ後フィードバック機構の決定論化」の Phase F 完了が前提。元の 1-F (ADR-014 本採用化 + takt-test-vc 反映) は ADR-014 が ADR-030 で Superseded されるため scope 変更。

- **やろうとしたこと**: 本プロジェクトで ADR-030 機構が安定稼働 (Phase F dogfood 完了) した後、takt-test-vc へ機構ごとバックポート
- **現在地**: 上位タスクの Phase F 完了待ち
- **詰まっている箇所**: ADR-030 実装 + dogfood 完了に依存

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
