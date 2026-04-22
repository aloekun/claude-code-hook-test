# ADR-029: Post-Merge Feedback の自動起動 — pending file + 現セッション起動

## ステータス

試験運用 (2026-04-23)

## コンテキスト

### 問題

ADR-014 で導入した `/post-merge-feedback` skill は、ユーザーが明示的に呼び出す前提である。実運用では呼び忘れが発生し、得られるはずのフィードバックが失われていた。ADR-013 の merge pipeline は `[[merge_pipeline.post_steps]]` に `type = "ai"` のスロットを持つが、現状は [src/cli-merge-pipeline/src/main.rs:313-322](../../src/cli-merge-pipeline/src/main.rs#L313-L322) で SKIP 実装のまま残っている。

`pnpm merge-pr` 後に自動でフィードバックループを起動する仕組みを追加したい。ただし以下の設計制約を破ってはならない:

1. **セッション知見の維持 (ADR-014)**: skill はメイン会話内で実行される必要がある。新規 Claude Code session を spawn すると、ADR-014 が選択肢 3 で回避した「会話履歴にアクセスできない」問題 (選択肢 1 の欠点) が再発する
2. **意図表現の不可侵 (ADR-022 原則 1)**: automated actor は commit description / bookmark 名 / PR title/body 等の既存 artifact を書き換えない
3. **外部可視成果物の生成ゲート (ADR-028)**: PR 作成/マージ等は既存の二層ゲートで管理済み。本 ADR で新たな外部成果物を生成しない

### 検討した選択肢

#### 1. exe からの AI spawn (`claude -p` など)

`cli-merge-pipeline` が直接 `claude -p "/post-merge-feedback"` を起動する案。

- 新規セッションになりセッション知見 (現在のメイン会話履歴) が失われる
- ADR-014 選択肢 1 と同じ欠点が再発する
- **却下**

#### 2. Stop hook が無条件で `/post-merge-feedback` を呼ぶ

Stop のたびに skill を呼ぶ案。

- マージ直後以外でもノイズとして発火する
- 「マージ直後だけ呼ぶ」ための状態受け渡しがどのみち必要 → 案 3 のベースとなる
- 単独では **却下**

#### 3. state file + 現セッション起動 (採用)

`cli-merge-pipeline` が `.claude/post-merge-feedback-pending.json` を書き込み、新規 Stop hook (`hooks-stop-feedback-dispatch`) が検出して `additionalContext` で Claude に skill 起動を指示する。

- 新規 session を spawn しないのでセッション知見が維持される (ADR-014 選択肢 3 の強みを維持)
- pending file が決定論的 artifact として 3 コンポーネント (CLI / Stop hook / skill) を疎結合に協調させる
- state は `cat` で確認でき可観測性が高い

## 決定

**選択肢 3 を採用する。**

### アーキテクチャ

```text
pnpm merge-pr (cli-merge-pipeline, ADR-013)
  ├─ ... (マージ本体 + ローカル同期)
  ├─ post_steps: type="ai" 分岐が pending file を atomic 書き込み
  └─ exit 0
       │
       ▼
Claude Code が Stop に向かう
       │
       ▼
Stop hooks (ADR-004 + 本 ADR, 責務分離は ADR-022 に準拠)
  1. hooks-stop-quality (既存: lint / test / build)
  2. hooks-stop-feedback-dispatch (新規)
       ├─ stop_hook_active == true → silent exit (無限ループ防止)
       ├─ pending 不在 / 破損 / stale → 削除 + silent exit
       ├─ status == "dispatched" → silent exit (二重通知しない)
       ├─ status == "consumed" → 削除 + silent exit (後片付け)
       └─ status == "pending" → additionalContext 出力 + atomic で status="dispatched" に更新
       │
       ▼
Claude がメイン会話内で /post-merge-feedback を起動
  ├─ Phase 0: pending file 先読み (本 ADR 追加)
  ├─ Phase 1-5: ADR-014 のフローを踏襲
  └─ skill 完了時に status="consumed" → ファイル削除
```

### Pending file JSON スキーマ (v1)

**配置パス**: `.claude/post-merge-feedback-pending.json` (本プロジェクト・派生プロジェクトで統一)

```json
{
  "schema_version": 1,
  "pr_number": 123,
  "owner_repo": "aloekun/claude-code-hook-test",
  "prompt": "post-merge-feedback",
  "status": "pending",
  "created_at": "2026-04-23T10:00:00Z",
  "dispatched_at": null,
  "consumed_at": null
}
```

**フィールド定義**:

| キー | 型 | 必須 | 説明 |
|---|---|---|---|
| `schema_version` | u32 | yes | スキーマ互換性管理。非一致で削除して silent exit |
| `pr_number` | u64 | yes | 対象 PR 番号 |
| `owner_repo` | string | yes | `{owner}/{repo}` 形式 |
| `prompt` | string | yes | skill 名または prompt key (今は `"post-merge-feedback"` 固定) |
| `status` | enum | yes | `"pending" \| "dispatched" \| "consumed"` |
| `created_at` | ISO 8601 UTC string | yes | cli-merge-pipeline が書き込んだ時刻 |
| `dispatched_at` | ISO 8601 UTC string | nullable | hooks-stop-feedback-dispatch が additionalContext を出した時刻 |
| `consumed_at` | ISO 8601 UTC string | nullable | skill が完了処理を行った時刻 |

### 状態遷移

```text
                    ┌──────────────────────────────┐
                    │ stale TTL (24h) → 強制削除    │
                    └───────────┬──────────────────┘
                                │
(書き込み) → pending ──→ dispatched ──→ consumed ──→ (削除)
                │            │
                └─ (破損)  ──┤
                             └─ 削除 + silent exit
```

- `pending`: cli-merge-pipeline が書き込んだ直後
- `dispatched`: hooks-stop-feedback-dispatch が additionalContext を出した (二重発火防止のマーカー)
- `consumed`: skill が処理完了した直後 (削除する前の最終状態。論理的には極短時間で遷移)
- ファイル不在: 初期状態 or consumed 後の正常終端

**stale TTL**: `now - created_at > 24h` の pending は hooks-stop-feedback-dispatch が削除して silent exit。skill 呼び忘れに対する自動回復機構として働く。TTL 24h はマージ直後の dogfood セッションが途中終了しても翌日には復帰できる幅を狙った初期値。

### 競合ポリシー

cli-merge-pipeline が pending file を書き込もうとしたときの既存ファイル別の挙動:

| 既存 status | 挙動 |
|---|---|
| 不在 | 新規書き込み (通常経路) |
| `consumed` (削除忘れ) | 上書き |
| `pending` / `dispatched` | **書き込み skip + WARN** (ステップ自体は PASS 扱いで merge-pr を中断しない) |
| 破損 (size 0 / JSON parse 失敗 / schema_version 不一致) | 削除してから書き込み |

同一セッション内で短時間に複数 PR をマージした場合 (現実には稀)、最初の pending が consume されるまで後続は取りこぼす。取りこぼしは WARN ログで可観測性を残すことで後追い対応可能とする。

**将来拡張**: 取りこぼしが問題化したらディレクトリベースのキュー (`.claude/post-merge-feedback/<pr>.json`) への移行を検討。現段階では YAGNI で単一ファイルを採用する。

### 破損耐性

hooks-stop-feedback-dispatch が pending file を読み取る際の分岐表:

| 状態 | 挙動 |
|---|---|
| `stop_hook_active == true` | silent exit (無限ループ防止。pending は読まない) |
| ファイル不在 | silent exit (通常経路) |
| size 0 | ファイル削除 + silent exit |
| JSON parse 失敗 | ファイル削除 + silent exit |
| `schema_version` 不一致 | ファイル削除 + silent exit |
| stale (`created_at + 24h < now`) | ファイル削除 + silent exit |
| `status == "pending"` | additionalContext 出力 + `status="dispatched"` に atomic 更新 |
| `status == "dispatched"` | silent exit (二重通知しない) |
| `status == "consumed"` | ファイル削除 + silent exit |

**書き込み方式**: 「一時ファイルに write → `fs::rename` で atomic rename」の 2 段階を常に使う。ロックファイルは不要 (POSIX / Windows の `rename` semantics に依存)。

### additionalContext 構造化フォーマット

hooks-stop-feedback-dispatch が stdout に出力する JSON:

```json
{
  "hookSpecificOutput": {
    "hookEventName": "Stop",
    "additionalContext": "[POST_MERGE_FEEDBACK_TRIGGER]\nschema_version: 1\npr_number: 123\nowner_repo: aloekun/claude-code-hook-test\naction: invoke_skill\ncommand: /post-merge-feedback 123\nreason: cli-merge-pipeline wrote pending artifact"
  }
}
```

`additionalContext` の内部は行区切りの `key: value` 形式。先頭行の `[POST_MERGE_FEEDBACK_TRIGGER]` タグが他の additionalContext (例: `hooks-session-start` の `CLAUDE_CODE_SESSION_ID=...`) との識別子になる。

**固定キー順序** (パース容易性と unit test の比較を単純化するため):

1. `schema_version`
2. `pr_number`
3. `owner_repo`
4. `action` (現状 `invoke_skill` 固定)
5. `command` (Claude が実行すべき slash command 文字列)
6. `reason` (観察用のコンテキスト)

### ADR-022 原則 1 との整合性

本 ADR が導入する全副作用は原則 1 の許可側に収まる:

| 副作用 | 分類 | 整合性 |
|---|---|---|
| `.claude/post-merge-feedback-pending.json` の新規書き込み | **新規 artifact への自己記述** | 許可 (緩和条項の適用も不要) |
| `status` の `pending → dispatched → consumed` 更新 | **自身が作成した artifact の自己更新** (意図表現ではない内部状態) | 許可 |
| 破損/stale pending の削除 | **自身が管理する artifact の破棄** | 許可 |
| `additionalContext` 出力 | **現セッション内 Claude への指示** (ファイル成果物を生成しない) | 副作用なし (草案生成に類する) |

commit description / bookmark 名 / PR title/body への介入は一切発生しない。skill 側が pending を `consumed` に更新してから削除するのも同じ枠内。

### ADR-013 / ADR-014 / ADR-016 / ADR-022 / ADR-028 との関係

- **ADR-013**: `[[merge_pipeline.post_steps]]` の `type = "ai"` ステップを本 ADR の仕様で実装する。ADR-013 の「将来実装」プレースホルダを具体化する
- **ADR-014**: 選択肢 3 (skill はメイン会話内で実行) を維持。本 ADR は「明示呼び出し → 自動発火」の橋渡しのみを担い、skill 本体フローは ADR-014 を踏襲
- **ADR-016**: `pnpm merge-pr` は 10-30 秒で完了するため長時間コマンド戦略の対象外。pending file 書き込みは追加のブロッキングを生まない
- **ADR-022**: 原則 1 (新規 artifact への自己記述) と原則 3 (amend ≠ describe、意図表現不変) の枠内で完結する。既存 commit / bookmark / PR には触れない
- **ADR-028**: 本 ADR は外部可視成果物 (PR / tag 等) を生成しない。したがって ADR-028 の `permissions.ask` ゲートの対象外

## 実装タスク

詳細な実装手順は `docs/todo.md` の「マージ後フィードバックの定常化」セクションを参照。本 ADR は仕様のみを規定する。

- **1-B**: cli-merge-pipeline の `"ai"` 分岐を pending file 書き込みに置き換え
- **1-C**: `hooks-stop-feedback-dispatch` 新規 exe の追加 + Stop hook 登録
- **1-D**: `.claude/hooks-config.toml` の post_steps 有効化 + dogfood 開始
- **1-E**: post-merge-feedback skill の Phase 0 「pending file 先読み」追加

## 影響

### Positive

- マージ直後にフィードバックループが自動で起動し、skill 呼び忘れによるロスがなくなる
- 新規 session を spawn しないためセッション知見が維持される (ADR-014 選択肢 3 の強みを継承)
- pending file が単一の決定論的 artifact なので、CLI / Stop hook / skill の 3 者が疎結合に協調できる
- state の可観測性が高い (ファイル内容を `cat` するだけで現状把握)
- 責務分離 (ADR-022) を維持したまま自動化を実現

### Negative

- Stop hook が 2 つになる (hooks-stop-quality + hooks-stop-feedback-dispatch)。実行順序は 1 → 2 固定で、品質ゲート失敗時は pending dispatch に進まない
- pending file の schema 変更時は `schema_version` を bump して互換性管理が必要
- 同一セッションで短時間に複数 PR をマージした場合、最初の 1 件しか自動発火しない (後続は WARN で可観測。将来拡張でキュー化可能)

### 将来の展望

- 取りこぼしが問題化したらディレクトリベースのキュー (`.claude/post-merge-feedback/<pr>.json`) へ移行 (schema_version bump を伴う)
- dogfood で問題なければ ADR-014 の試験運用ステータスを本採用化 (docs/todo.md の 1-F タスク)
- 派生プロジェクト (takt-test-vc 等) へバックポート

## References

- [ADR-004: Stop フックによる品質ゲート](adr-004-stop-hook-quality-gate.md) — `stop_hook_active` 無限ループ防止パターンの先行例
- [ADR-013: Merge Pipeline](adr-013-merge-pipeline.md) — `post_steps` の `type = "ai"` スロットの提供元
- [ADR-014: Post-Merge Feedback](adr-014-post-merge-feedback.md) — skill 本体のフロー定義 (選択肢 3 採用の根拠)
- [ADR-016: 長時間コマンド実行戦略](adr-016-long-running-command-strategy.md) — `pnpm merge-pr` の実行時間特性
- [ADR-022: 自動化コンポーネントの責務分離原則](adr-022-automation-responsibility-separation.md) — 原則 1 (新規 artifact への自己記述) の適用根拠
- [ADR-026: Cargo workspace](adr-026-cargo-workspace.md) — `hooks-stop-feedback-dispatch` 新規 crate の追加手順
- [ADR-028: pnpm create-pr ゲート](adr-028-pnpm-create-pr-gate.md) — 外部可視成果物ゲートとの軸別境界 (本 ADR の射程外)
