# ADR-045: jj workspace による並列セッション運用 — メイン作業と細粒度改善の分離

## ステータス

試験運用 (2026-06-29) / 改訂 (2026-06-30: 初 PR 運用ケースで判明した secondary workspace の `.git` 不在 → gh ベースコマンドの `GIT_DIR` 必須、および merge-pipeline の bookmark 誤検出を「§ PR 運用時の追加設定」として追記)

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
| `.git` (colocated git ref) | **secondary workspace には存在しない** (`jj workspace add` は colocated 化しない、`.jj` のみ) | **gh ベースの `pnpm create-pr` / `pnpm merge-pr` / `cli-pr-monitor --monitor-only` は `GIT_DIR` 必須** (後述「§ PR 運用時の追加設定」)。`pnpm push` は `jj git push` backend のため不要 — この非対称が初回検証で見落とされた |

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
- 初回 setup (`pnpm build:all` で exe / settings 用意) を払えば、各 workspace で `pnpm push` が通常通り機能 (`pnpm create-pr` / `pnpm merge-pr` は gh ベースのため `GIT_DIR` の追加設定が必要 — 後述「§ PR 運用時の追加設定」)
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

- 各 workspace で `jj describe` → `jj bookmark create <name>` → `pnpm push` → `pnpm create-pr` → `pnpm merge-pr` を実行する。**ただし `pnpm create-pr` / `pnpm merge-pr` / `cli-pr-monitor --monitor-only` は secondary workspace では `GIT_DIR` 必須** (後述「§ PR 運用時の追加設定」)。
- `pnpm merge-pr` は `@-` の bookmark から PR を自動検出する (ADR-013) ため、各 workspace は自分の bookmark に対応する PR のみをマージする。
- マージ後の `jj new master@origin` 同期は実行した workspace のみに効く。もう片方は調整ポイント 2 の手順で master を取り込む。
- push は `cli-push-runner` (ADR-015)、merge は `cli-merge-pipeline` (ADR-013) を各 workspace の `.claude/*.exe` 経由で使う (= 初回 `pnpm build:all` が前提)。

### PR 運用時の追加設定 (2026-06-30 追記 — 初 PR 運用ケースで判明)

#### 問題: secondary workspace は `.git` を持たない

`jj workspace add` で作った secondary workspace は **colocated 化されず `.git` を持たない** (`.jj` のみ)。メインリポジトリ (`default` workspace、`.jj/repo` が指す先) は colocated で `.git` を持つ。

このため、内部で `gh` (= git リポジトリコンテキスト必須) を呼ぶコマンドが secondary workspace で `fatal: not a git repository` で失敗する。**`pnpm push` は `jj git push` backend なので動くが、PR 操作は全滅する** — この非対称が初回検証 (push 中心) で見落とされた。

| コマンド | 内部実装 | secondary workspace 単独 |
|---|---|:---:|
| `pnpm push` / `jj` 系すべて | jj backend (`.jj/repo/store/git`) | ✅ 動く |
| `pnpm create-pr` / `pnpm merge-pr` | `gh pr create` / `gh pr merge` | ❌ 要 `GIT_DIR` |
| `cli-pr-monitor --monitor-only` | `gh api` / `gh pr checks` | ❌ 要 `GIT_DIR` |
| `gh api` / `gh pr checks` 直接呼び出し | gh | ❌ 要 `GIT_DIR` |

#### 解決: `GIT_DIR` でメインリポジトリの `.git` を参照

gh ベースのコマンド実行時に環境変数 `GIT_DIR` でメインリポジトリ (`.jj/repo` が指す先、本プロジェクトでは `~/work/claude-code-hook-test`) の `.git` を指す。**jj は `GIT_DIR` を無視する** (独自に `.jj/repo` を解決) ため、`GIT_DIR` は gh だけを制御し jj 操作 (bookmark 自動補完 / push / rebase) には影響しない。monitor state は `<exe>` パス基準で解決されるため、`GIT_DIR` 設定下でもこの workspace に保たれる (並列 monitor の分離を壊さない)。

```sh
# Bash — $HOME=/c/Users/<user> (Unix 形式) なのでフォワードスラッシュ。フルパスは $HOME で隠蔽
GIT_DIR="$HOME/work/claude-code-hook-test/.git" pnpm create-pr -- --title "..." --body-file __pr-body.md
GIT_DIR="$HOME/work/claude-code-hook-test/.git" pnpm merge-pr
GIT_DIR="$HOME/work/claude-code-hook-test/.git" .claude/cli-pr-monitor.exe --monitor-only
```

```powershell
# PowerShell — $HOME=C:\Users\<user> (Windows 形式) なのでバックスラッシュ
$env:GIT_DIR = "$HOME\work\claude-code-hook-test\.git"
pnpm create-pr -- --title "..." --body-file __pr-body.md
pnpm merge-pr
& .\.claude\cli-pr-monitor.exe --monitor-only
```

注意:
- **シングルクォート不可** — `GIT_DIR='$HOME\...'` は `$HOME` が展開されずリテラル文字列になり失敗する。必ずダブルクォートで囲む。
- **`work/claude-code-hook-test` の構成は固定** — 他 PC で配置が違えば調整が必要 (`.jj/repo` からの動的導出は jj 内部の `.jj/` 基準相対パスが絡み fragile なため非採用)。
- **Bash で exe を直接呼ぶ際はフォワードスラッシュ** (`.claude/cli-pr-monitor.exe`)。`.\.claude\...` (バックスラッシュ) は Bash がエスケープ解釈して `command not found` になる。

#### merge-pipeline / monitor の bookmark 誤検出に注意

`cli-merge-pipeline` / `cli-pr-monitor` は `lib-jj-helpers` の `BOOKMARK_SEARCH_REVSETS = ["@", "@-", "@--"]` を優先順に探索し、**最初に bookmark を持つ commit のものを PR として採用**する (trunk 系は除外)。

PR head 以外の non-trunk bookmark が `@` / `@-` / `@--` の近接 3 世代に存在すると、それを誤って PR 候補に選ぶ (初 PR 運用で、別タスクの todo commit に付けた bookmark を `@` に置いていたため merge-pipeline が PR head ではなくそちらを選び 2 回失敗した)。

- PR 操作 (push / create-pr / merge-pr) 時は、近接 3 世代に **対象 PR の bookmark だけ**が来るようにする
- 「PR の作業」と「別タスクのローカル commit」を同 workspace に同時に持つ場合、別タスクの bookmark は近接 revset から外す (PR head を `@` か `@-` に置く、または別タスク commit の bookmark を一時的に外す)
- これは「§ タスク領域の分割方針」がファイル衝突のみを論じ、**bookmark/revset の近接性衝突**を盲点としていた点の補足

#### 恒久対策の候補 (follow-up)

`GIT_DIR` の手動前置は忘れると失敗するため、構造的解決を推奨 (投資対効果順):

1. **cli-* exe 側で `GIT_DIR` 自動注入** — `cli-pr-monitor` / `cli-merge-pipeline` が `.git` 不在を検出したら `.jj/repo` からメインリポジトリを導出し、子プロセスの gh に `GIT_DIR` を自動設定する。ユーザーが `GIT_DIR` を意識せず `pnpm create-pr` がそのまま動く。最も投資対効果が高い
2. **direnv `.envrc`** — workspace ルートで `GIT_DIR` を自動エクスポート (direnv 導入が前提)
3. **workspace の colocated 化** — `jj workspace add` 後に当該 workspace を `.git` 付きにできるか jj の機能を調査。可能なら `GIT_DIR` 自体が不要

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
