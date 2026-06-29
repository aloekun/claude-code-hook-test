# ADR-045: jj workspace による並列セッション運用 — メイン作業と細粒度改善の分離

## ステータス

試験運用 (2026-06-29)

> ADR-039 (Experimental feature 標準パターン) に準拠: config opt-in なし (本 ADR は workflow 運用ポリシーであり実装機構ではないため該当しない) / kill-switch = 本 ADR を supersede する後続 ADR で停止可能、運用上は単一 workspace へ戻すだけで無効化 / bounded lifetime = 採用判定 3 ヶ月 (2026-09-29) を目安に dogfood 結果から本採用 / 修正 / 却下を判定。

## コンテキスト

### 問題

メインセッションは `docs/file-length-enforcement-plan.md` の W3 (cli-merge-pipeline 分割) / W4 (cli-push-runner 分割) という大規模 src リファクタを進める。一方で post-merge-feedback / 週次レビュー / CodeRabbit 対応から、`docs/todo*.md` 系列に細粒度の不具合修正・改善タスク (lint rule / takt facet / test / docs 系、順位 225-232 等) が継続的に蓄積している。

これらを単一セッションで直列処理すると:

1. 大規模 src 分割の最中に細かい改善タスクが割り込み、context が錯綜する
2. 改善タスクが大規模作業の land 待ちでブロックされる
3. 1 つの working copy (`@`) を共有するため、別領域の作業を並行できない

git には worktree (同一リポジトリを汚染せず複数 working tree で並列作業) があるが、本プロジェクトは jj を採用している (CLAUDE.md / 各 hook config)。jj で同等の並列運用が可能か、かつ本プロジェクトの hook / pipeline 構成で実用できるかを検証する必要があった。

### 検証結果 (jj 0.42.0、colocated repo)

`jj workspace add <path>` で git worktree 相当の並列 workspace を作成できることを実機確認:

- 同一リポジトリ (commit store / operation log / bookmark) を共有しつつ、各 workspace が独立した working-copy commit (`@`) を持つ
- 検証で `default` と追加 workspace が別々の `@` を保持し、互いの working copy を汚染しないことを確認
- `jj workspace forget <name>` + ディレクトリ削除でクリーンに撤去可能

本プロジェクト固有の caveat も実機確認:

| 項目 | 状態 | 含意 |
|---|---|---|
| `src/` `docs/` `.takt/` 等 tracked file | 新 workspace に checkout される | 問題なし |
| `.claude/*.exe` (hooks / push-runner / monitor 等) | **gitignore** (`.gitignore` line 9、ビルド成果物) → 未コピー (検証で 0 個) | **新 workspace で `pnpm build:all` 必須** |
| `.claude/settings.local.json` | gitignore (PROJECT_DIR テンプレ生成) → 未コピー | `build:all` 内の `build:hooks-settings` がそのパス用に生成 |
| `.claude/pr-monitor-state.json` / lock / `.session-id` / `feedback-reports/` | gitignore + per-checkout | **workspace ごとに独立 → 並列 monitor が state を衝突させない** |
| `/target/` (Cargo workspace) | gitignore | workspace ごとに独立ビルド (disk コストのみ) |

## 検討した選択肢

### 選択肢 A: 単一セッションで直列処理

- 追加の setup 不要だが、大規模作業と細粒度改善が同 context / 同 `@` で錯綜
- 改善タスクが大規模 land 待ちでブロック
- **却下**

### 選択肢 B: 別 clone で並列

- 完全に独立するが commit store が重複 (disk 2 倍) し、bookmark / PR 状態の同期が手動
- master 進行の取り込みが clone 間で都度 fetch + merge 必要
- jj の op log 共有メリットを失う
- **却下**

### 選択肢 C: jj workspace で並列 (採用)

- 同一リポジトリを共有しつつ独立した `@` で並列作業 = git worktree 相当
- commit store / op log / bookmark を共有するため、別 clone より効率的かつ状態同期が容易
- state / lock / target は gitignore + per-checkout で自動的に workspace 分離される
- 初回 setup (`pnpm build:all` で exe / settings 用意) を払えば、各 workspace で `pnpm push` / `pnpm merge-pr` が通常通り機能
- **採用**

## 決定 (試験運用)

メイン作業 (大規模 src 分割) と細粒度改善を、jj workspace で並列セッション分離する。

### セットアップ手順 (新 workspace)

```sh
jj workspace add ../ccht-improve         # master@origin 上に独立 @ を持つ workspace を作成
cd ../ccht-improve
pnpm install                             # node_modules (gitignore)
pnpm build:all                           # 全 exe ビルド + settings.local.json をこのパス用に生成
# → 新しい Claude Code セッションを ../ccht-improve で起動 (独自の .claude/settings.local.json を持つ)
```

撤去は `jj workspace forget <name>` + ディレクトリ削除 (`rm -rf` は PreToolUse guard で block されるため、単一ファイルずつ or `Remove-Item -Recurse`)。

### タスク領域の分割方針 (論理衝突回避)

| workspace | 担当領域 |
|---|---|
| メイン (`default`) | `docs/file-length-enforcement-plan.md` の W3 / W4 = `src/cli-merge-pipeline` / `src/cli-push-runner` の大規模分割 |
| 改善 (`../ccht-improve`) | 順位 225-232 等の細粒度改善 = custom lint rule / takt facet (`.takt/facets/`) / test / docs 系 |

src の大規模分割 (メイン) と lint/facet/docs (改善) は編集領域が直交し、同一ファイルの同時編集を避けられる。

### 並列運用の調整ポイント

1. **bookmark 名は workspace 間で共有 namespace**。タスクごとに別名 (メイン = `pr-w3-...` / `pr-w4-...`、改善 = `fix-<task>` / `lint-<rule>` 等) を付け、別 PR として独立させる。同名 bookmark を両 workspace で作らない。
2. **ローカル `master` bookmark は共有**。片方が `pnpm merge-pr` で land すると `master@origin` が進む。もう片方は `jj git fetch` + `jj rebase -d master@origin` で取り込んでから push する (`docs/file-length-enforcement-plan.md` § Cargo.lock 競合の rebase 手順と同型)。
3. **state / lock / monitor は workspace ごとに独立** (gitignore + per-checkout)。並列の post-PR monitor / CronCreate が互いの state を壊さない。
4. **op log は共有**だが、並行操作は jj が安全にマージする (concurrent operation を自動解決)。

### マージ方法 (各 workspace で独立)

- 各 workspace で通常どおり `jj describe` → `jj bookmark create <name>` → `pnpm push` → `pnpm create-pr` → `pnpm merge-pr` を実行する。
- `pnpm merge-pr` は `@-` の bookmark から PR を自動検出する (ADR-013) ため、各 workspace は自分の bookmark に対応する PR のみをマージする。
- マージ後の `jj new master@origin` 同期は実行した workspace のみに効く。もう片方は調整ポイント 2 の手順で master を取り込む。
- push は `cli-push-runner` (ADR-015)、merge は `cli-merge-pipeline` (ADR-013) を各 workspace の `.claude/*.exe` 経由で使う (= 初回 `pnpm build:all` が前提)。

## 影響

### 良い影響

- メインの大規模リファクタと細粒度改善を並列に進められ、改善タスクの land 待ちブロックが解消
- working copy (`@`) が workspace ごとに独立し、別領域の作業が互いを汚染しない
- state / lock / monitor が自動分離されるため、並列 PR の監視フローが衝突しない
- 別 clone と違い commit store / op log / bookmark を共有するため、master 進行の取り込みが軽量

### 注意点

- 新 workspace は初回 `pnpm build:all` (全 exe ビルド) のコストと時間が必要。これを払わないと hook / pipeline が動かない (`.claude/*.exe` が gitignore のため)
- disk コスト: full checkout (tracked file ~262 個) + workspace ごとの `target/` (Rust ビルド) で数百 MB 規模
- ローカル `master` が共有のため、両 workspace が master 起点で作業すると land 順に応じた rebase が都度必要 (調整ポイント 2)
- 同一ファイルを両 workspace で同時編集すると working copy は独立だが、同一箇所を両方 land すると master で論理衝突 → rebase 解決。タスク領域の分割 (上表) で構造的に回避する
- harness の Agent `isolation: "worktree"` や EnterWorktree は本セッション内の sub-agent 用であり、別の対話セッションには使わない。並列の対話セッションには本 ADR の jj workspace を使う

## 再評価 trigger

以下のいずれかが発生したら本 ADR を見直す:

1. タスク領域分割が機能せず、master での論理衝突 (同一箇所の二重編集) が複数回観測される
2. 新 workspace 作成頻度が高く `pnpm build:all` の初回コストが運用ボトルネック化する (事前ビルド共有 / deploy 機構の検討トリガー)
3. jj workspace 間の op log 競合や bookmark 衝突が dogfood で顕在化する
4. 採用判定期限 (2026-09-29) で本採用 / 修正 / 却下を判定

## 関連 ADR

- ADR-011: jj の新規ブックマーク push 戦略 — 各 workspace の push 戦略の前提
- ADR-013: Merge Pipeline — 各 workspace が `@-` bookmark から PR 自動検出してマージ
- ADR-015: Push Pipeline を takt ベースの push-runner に移行 — 各 workspace の push 経路
- ADR-026: Cargo workspace — `target/` が gitignore で workspace ごとに独立する前提
- ADR-039: Experimental feature 標準パターン — 本 ADR のステータス管理形式

## 由来

- PR #224 (PR-W2 = cli-pr-monitor 分割) / PR #225 (改善タスク 8 件登録) のセッション (2026-06-29) で、メイン (file-length plan の W3/W4) と並行して細粒度改善 (順位 225-232) を別セッションで進めたいユーザー要望から起案
- jj の git-worktree 相当機能の可否を実機検証: jj 0.42.0 で `jj workspace add` により独立 `@` を確認、`.claude/*.exe` が gitignore で新 workspace に未コピー (要 `pnpm build:all`)、state / lock が per-checkout で自動分離されることを確認 → 選択肢 C を採用
