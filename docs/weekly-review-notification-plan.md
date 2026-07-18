# Weekly Review 通知可視化改善 (PR-N1 〜 PR-N3)

> **状態**: 計画書 (PR-N1 〜 PR-N3 が全て land + dogfood 完了で **本ファイルを削除** して役割を終える)
>
> **目的**: weekly-review reminder (ADR-031) が「発火しているのにユーザーに見えない」問題を解消し、
> 状態ファイルの jj workspace 分裂と発火の観測不能も併せて修正する
>
> **削除条件**: 以下 4 条件を全て満たしたら、本ファイルを削除する (削除は doc-only 変更のため、
> ユーザーの運用に従い他の doc 変更とまとめた PR で良い)
>
> 1. PR-N1 〜 PR-N3 が全て master に land 済
> 2. PR-N1 の systemMessage が新セッション起動時に **ユーザーの画面に表示されること** を目視確認済
> 3. PR-N2 land 後、secondary workspace (ccht-improve) からのセッションで経過日数が「未実行」ではなく実日数で表示されることを確認済
> 4. PR-N3 land 後、`.claude/telemetry/firings-*.jsonl` に session-start nudge の発火行が記録されることを確認済

## 背景 (2026-07-19 調査結果)

前提知識なしで読めるよう、調査で確定した事実を列挙する:

- ADR-031 の weekly-review reminder は SessionStart hook (`src/hooks-session-start/src/weekly_review.rs`) が
  `.claude/weekly-review-last-run.json` の `last_run_at` を見て 7 日超過で発火する設計。
  2026-06-23 (PR #216) に `.claude/hooks-config.toml` で enable された。
- **reminder は正しく発火している**。しかし hook の出力は `hookSpecificOutput.additionalContext` のみで、
  これは **Claude のコンテキストに注入されるだけでユーザーの画面には表示されない**。
  Claude がセッション冒頭で言及しない限りユーザーは気付けず、実際に約 4 週間気付かれなかった (根本原因)。
- `.claude/weekly-review-last-run.json` は gitignore 済み untracked ファイルのため
  **jj workspace 間で共有されない** (ADR-045 並列 workspace 運用との相互作用)。
  前回実行 (2026-07-01) は improve workspace (`claude-code-hook-test-improve`) 側で行われ、
  メイン workspace には状態ファイルが無い → メイン側では常に「未実行」判定で発火し続けていた。
  `weekly_review.rs` の doc comment「`last_run_at` は workspace 不変の値」は mtime リセット問題への
  対処としては正しいが、ファイル自体が workspace ローカルである点が盲点だった。
- `hooks-session-start` は lib-telemetry (ADR-055) に未統合で、nudge の発火実績が観測できず、
  問題の発見が遅れた。

### 裏取り済みの Claude Code hooks 仕様 (公式ドキュメント確認済、2026-07-19)

- `systemMessage` は hook JSON 出力の **トップレベル共通フィールド** (string 型) で、
  **全 hook イベント (SessionStart 含む) で使用可能**。ユーザーに表示される。
- `hookSpecificOutput.additionalContext` と同一 JSON で **併用可能**。
- UI 上の表示スタイル (警告色か通常か等) はドキュメント未明記のため、
  PR-N1 の dogfood で目視確認する (削除条件 2)。

## 確定済み設計判断 (2026-07-19 ユーザー承認)

| # | 論点 | 決定 |
|---|---|---|
| 1 | systemMessage の適用範囲 | **weekly reminder のみ → dogfood 後に段階展開** (行動要求系 nudge = PR catch-up / post-merge recovery / failed marker が第 2 弾候補) |
| 2 | last-run 状態の置き場所 | **メイン workspace を canonical** とし、secondary からは `.jj/repo` ファイルでメイン root を解決 (新しい置き場を増やさない、移行不要) |
| 3 | PR 分割 | **3 PR 段階投入** (PR-N1 → PR-N2 → PR-N3 の直列) |
| 4 | ADR | **新 ADR 1 本** (systemMessage 可視化チャネル) + **ADR-031 / ADR-045 追記** (状態分裂) |

---

## 各 PR 共通の前提

- ブランチは **master から** 新規作成する (調査時の checkout は `refactor/adr-047-retire-refute` だった。
  master には hooks-config.toml のコメント修正 b1da57fd などが入っている)。
- Rust コードの関数 body 内 `// foo` 非 doc コメントは Stop hook (Bundle Z) で block される。
  許可は `///` / `//!` / `// SAFETY:` / `// NOTE:` のみ。
- ビルド: `pnpm build:all` (Windows では Git の `usr/bin` (cp.exe) が PATH に必要)。
  hook exe は `.claude/` 配下に配置される。動作確認は **新セッション起動** で行う
  (SessionStart hook はセッション開始時のみ発火するため)。
- push は `pnpm push` (直接の `jj git push` は PreToolUse hook で block される)。

---

## PR-N1: systemMessage によるユーザー可視通知 (weekly 限定) + additionalContext 文言強化

最優先・即効性最大。これが本計画の本丸。

### 変更内容

1. **新 ADR 起案**: `docs/adr/adr-059-hook-system-message-visibility.md` (番号は起案時点の最新+1 に読み替え)
   - 決定: hook 通知を 2 層に分離する。`additionalContext` = モデル向け (行動指示・詳細)、
     `systemMessage` = ユーザー向け (1 行サマリー)。ユーザーの行動を要求する nudge は両方に出す。
   - ADR-039 3 点セット: config opt-in (`system_message_enabled`、ソース default OFF /
     本リポジトリ config で ON)、kill-switch (`enabled = false`)、bounded lifetime
     (weekly での dogfood 観測後に行動要求系 nudge へ展開 or 却下を判定。PR-N3 の telemetry が観測基盤)。
   - 段階展開ロードマップ: weekly reminder → PR monitor catch-up / post-merge recovery /
     failed marker → その他 (staleness 系は Claude が自律対処できるため対象外の見込み)。
   - `CLAUDE.md` の ADR 一覧にリンク追記。
2. **`src/hooks-session-start/src/hooks_config.rs`**:
   `WeeklyReviewReminderConfig` に `system_message_enabled: Option<bool>` を追加 (+ parse テスト)。
3. **`src/hooks-session-start/src/weekly_review.rs`**:
   - `compute_weekly_review_reminder_nudge` の戻り値を struct 化
     (例: `WeeklyReviewNudge { additional_context: String, system_message: Option<String> }`)。
   - `system_message` は `system_message_enabled` が真かつ nudge 発火時のみ `Some`。
     文言は 1 行: 「週次レビュー: 前回実行から N 日経過 (threshold 7 日)。`/weekly-review` の実行を検討してください」
     (未実行時は「実行記録なし」、failed marker 時は resume 促し文言)。
   - additionalContext 側の文言に **「セッション最初の応答でこの reminder をユーザーに一言伝えること」**
     という明示指示を追加 (提案 3 の吸収。systemMessage が効かない場合の defense-in-depth)。
4. **`src/hooks-session-start/src/main.rs`**:
   - `emit_session_start_output` の JSON 組み立てを pure な builder 関数
     (`fn build_session_start_json(context: &str, system_message: Option<&str>) -> serde_json::Value` 等)
     に切り出し、`system_message` が `Some` のときトップレベルに `"systemMessage"` を付与。
   - builder のユニットテスト (systemMessage 有り/無しの JSON 形状)。
5. **`.claude/hooks-config.toml`**:
   `[session_start.weekly_review_reminder]` に `system_message_enabled = true` を追記 + コメント更新。

### テスト・検証

- `cargo test` (新規: config parse / system_message 生成の有効・無効・Missing・ElapsedDays・failed marker 各分岐 / JSON builder 形状)
- `pnpm build:all` → 新セッション起動 → **UI に systemMessage の 1 行が表示されることを目視確認** (削除条件 2)。
  表示されない場合は ADR-059 の前提が崩れるため、実装を revert せず表示経路を再調査してから判断する。

---

## PR-N2: last-run 状態のメイン workspace canonical 化

**2 リポジトリ横断** (本リポジトリ + claude-code-skills) に注意。

### 変更内容

1. **`src/lib-jj-helpers/src/lib.rs`**:
   `pub fn resolve_main_workspace_root(workspace_root: &Path) -> Option<PathBuf>` を追加。
   - `.jj/repo` が **ファイル** → 内容がメイン repo store へのパス (相対なら `<root>/.jj/` 基準。
     既存 `resolve_git_dir` と同じレイアウト解釈) → メイン root = store パス (`<main>/.jj/repo`) の 2 階層上。
   - `.jj/repo` が **ディレクトリ** → 現 root 自身がメイン workspace → `Some(workspace_root)`。
   - 読み取り失敗 / `.jj` 不在 → `None` (caller は現 root に fail-open)。
   - テストは既存 `resolve_git_dir` のパターンを流用 (fixture + 実 jj E2E。secondary の `.jj/repo` が
     ファイルであるレイアウトは 2026-07-03 に実機確認済みと lib 内コメントにあり)。
2. **`src/hooks-session-start/src/weekly_review.rs`**:
   - last-run 読込パスを `resolve_main_workspace_root(cwd).unwrap_or(cwd)` 基準に変更。
   - **failed marker (`.claude/weekly-reviews/*.md.failed`) と pending JSON は workspace ローカルのまま**
     (レビュー成果物は実行した workspace に属する、という線引きを doc comment に明記)。
   - doc comment の「`last_run_at` は workspace 不変の値」という誤記述を訂正
     (正: mtime と違い内容 timestamp は checkout で変わらないが、ファイル自体は workspace ローカルだったため
     本 PR でメイン root canonical 化した)。
3. **claude-code-skills リポジトリ**: `weekly-review/SKILL.md` の Step 5.3 (last-run timestamp 書込) を
   メイン root 解決付きに変更。bash snippet 例:

   ```bash
   MAIN_ROOT="$(pwd)"
   if [ -f .jj/repo ]; then
     STORE="$(cat .jj/repo)"; case "$STORE" in /*|[A-Za-z]:*) ;; *) STORE=".jj/$STORE";; esac
     MAIN_ROOT="$(dirname "$(dirname "$STORE")")"
   fi
   # 書込先: $MAIN_ROOT/.claude/weekly-review-last-run.json
   ```

   frontmatter の SessionStart hook 経路説明も更新し、skill-sync で `~/.claude/skills/` へ deploy する。
4. **ADR 追記**:
   - ADR-031: 状態ファイルの workspace-locality 盲点と canonical 化の決定 (§ トリガー方式と reminder 付近)。
   - ADR-045: 「gitignore 済み untracked 状態ファイルの workspace 分裂」を silent bug class として追記
     (mtime リセット (CR #233) と対になる実例。2026-07-19 に weekly-review で実観測)。

### 運用ステップ (コード外、land 時に 1 回)

`claude-code-hook-test-improve/.claude/weekly-review-last-run.json` (2026-07-01 実行記録) を
メイン workspace の `.claude/` にコピーし、実行履歴を救済する。

### テスト・検証

- `cargo test` (resolve_main_workspace_root の fixture / E2E、weekly_review の main-root 読込)
- deploy 後、**ccht-improve workspace からセッション起動** → メイン側 last-run が読まれ、
  systemMessage / additionalContext の経過日数が実日数になることを確認 (削除条件 3)。

---

## PR-N3: session-start nudge の telemetry 統合

観測層のみの小 PR。表示ノイズゼロのため systemMessage と違い **全 nudge 一括** で記録する。

### 変更内容

1. **`src/hooks-session-start/Cargo.toml`**: `lib-telemetry` 依存を追加。
2. **`src/hooks-session-start/src/main.rs`** (emit 経路): 各 nudge の発火時に
   `lib_telemetry::record(Firing { hook: "hooks-session-start", kind: FiringKind::Hook, id: "<nudge名>", decision: Decision::Warn, session_id: Some(...) })` を呼ぶ。
   id は 5 種: `weekly_review_reminder` / `pr_monitor_catchup` / `reaper` / `staleness` / `workspace_stale`。
   fail-open (記録失敗は握りつぶし、ADR-055 の設計原則どおり)。
3. **`docs/adr/adr-055-firing-telemetry-collection.md`**: スコープ表に session-start nudge 群を追記。

### テスト・検証

- 新セッション起動 → `.claude/telemetry/firings-*.jsonl` に `hooks-session-start` 行が append されること (削除条件 4)。

---

## PR 外の即時運用アクション (本計画とは独立、忘れず実施)

1. **`/weekly-review` の実行**: 前回 2026-07-01 から 18 日超経過 (調査時点)。
2. **PR #297 の post-merge-feedback failed marker 対応**:
   `.claude/feedback-reports/297.md.failed` を Read して復旧手順に従う (2026-07-19 の
   UserPromptSubmit nudge で検出。これも「行動要求系 nudge が見えない」実例で、
   ADR-059 段階展開の第 2 弾対象)。

## 最終目標

PR-N1 〜 PR-N3 の land と削除条件 1〜4 の確認が完了したら、**本ファイル
(`docs/weekly-review-notification-plan.md`) を削除する**。計画の履歴は git log と
ADR-059 / ADR-031 / ADR-045 / ADR-055 追記に残るため、本ファイルを残す必要はない
(ADR-031 の ephemeral handoff doc retire と同じ運用)。
