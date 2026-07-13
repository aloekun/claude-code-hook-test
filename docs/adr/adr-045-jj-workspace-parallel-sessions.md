# ADR-045: jj workspace による並列セッション運用 — メイン作業と細粒度改善の分離

## ステータス

試験運用 (2026-06-29) / 改訂 (2026-06-30: 初 PR 運用ケースで判明した secondary workspace の `.git` 不在 → gh ベースコマンドの `GIT_DIR` 必須、および merge-pipeline の bookmark 誤検出を「§ PR 運用時の追加設定」として追記) / 改訂 (2026-07-03: 恒久対策候補 1 = `GIT_DIR` 自動注入を実装。cli-* exe は手動 `GIT_DIR` 前置なしで動作するようになり、手動前置は直接 gh 呼び出し時の fallback に格下げ。PR #238 で `GH_REPO` による場当たり対処が部分故障を招いた実観測を受け `gh-repo-env-guard` preset も追加) / 改訂 (2026-07-13: **再評価 trigger #3 が発火** — 並列セッションの concurrent 操作と重なる時間帯に 2 コミット分の作業が消失する incident が発生 (PR #265 セッション、手動再構築で復旧)。「並行操作は jj が安全にマージする」という調整ポイント 4 の記述を jj 公式の並行モデルに即して是正し、「§ Known operational risks」「§ 並列運用の運用ルール」「§ Operation Verification Checklist」を新設)

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
4. **op log は共有**であり、並行操作の扱いは jj 公式の並行モデルに従う (2026-07-13 是正。当初の「並行操作は jj が安全にマージする」という記述は不正確だった)。公式モデル: jj は lock-free 設計で、並行操作は op log の分岐 (divergent operation heads) として記録され、次のコマンドが自動 3-way マージする。干渉は「stale working copy」(エラーで停止 → `jj workspace update-stale` で recovery commit が作られ変更は保全される) と「bookmark 競合」(競合状態として可視化) の 2 形態で表面化する。ただし **colocated リポジトリの同時編集は upstream が「十分にテストされていない」と明記する領域**であり (本リポジトリの default workspace は colocated)、公式保証の外側がある。「§ Known operational risks」を参照。

### Known operational risks (2026-07-13 新設)

並列 workspace 運用で実際に遭遇しうるリスクと対処。出典: jj 公式 [Concurrency 設計文書](https://docs.jj-vcs.dev/latest/technical/concurrency/) / [Working copy 文書](https://docs.jj-vcs.dev/latest/working-copy/) と本プロジェクトの実観測。

| リスク | 内容 | 対処 |
|---|---|---|
| **stale working copy** | 別 workspace がリポジトリを変更 (この workspace の wc commit の書き換え・abandon 等) すると、こちらの working copy が stale になり jj コマンドがエラーで停止する | `jj workspace update-stale` を実行する。recovery commit が作られ、working copy 上の変更は失われない (公式の設計保証)。エラーは異常ではなく、公式が想定する正常な干渉の表面化 |
| **bookmark 競合** | 同じ bookmark が並行して別方向に動くと競合状態として記録される。特に **`jj git push --all` は他 workspace の作業中 bookmark を巻き込んで push する** | push は必ず bookmark 明示 (`jj git push -b <name>`)。`--all` は並列運用中は使わない |
| **colocated repository の同時編集** | upstream が「十分にテストされていない」と明記する領域。default workspace は colocated (`.git` 併設) で、fetch は git refs を共有 store に取り込む | repo 境界操作 (fetch / push / merge) の同時多発を避ける (下記運用ルール)。挙動不審時は op log で確認 |
| **parallel terminal output corruption** | 2026-07-12/13 に実観測: 2 セッション並行稼働中、ツール出力の混線 (偽の成功表示・別セッションのテキスト混入) と同時間帯に、2 コミット分の操作が **op log に痕跡なく消失** (手動再構築で復旧)。op が記録されない事象は jj 公式モデルでは説明できず、有力仮説は「コマンドが実際には実行されず、成功表示は出力混線による偽物」(ハーネス側の問題)。ただしコマンド送信失敗 / tool 呼び出しキャンセル / terminal multiplex 不具合 / jj 未発見バグ / colocated 特有問題も排除できていない (未確定) | 下記運用ルール + Operation Verification Checklist で検出・封じ込め。混線を発見したら直ちに両セッションを停止しログを保存する |

### 並列運用の運用ルール (2026-07-13 新設)

1. **1 terminal = 1 Claude Code session**。terminal multiplex は使わない
2. workspace ごとに terminal ウィンドウを分離する
3. repo 境界操作 (`jj git fetch` / `jj git push` / `pnpm merge-pr`) は共有資源への書き込みであり、並列セッション稼働中はどちらか一方のセッションに寄せる
4. push は bookmark 明示 (`jj git push -b <name>`) のみ。`--all` 禁止
5. 変更系 jj コマンドは 1 つずつ実行し、並行投入しない (1 メッセージに複数の変更系 jj を混ぜない)
6. マージ等の repo 境界操作の前に、background task (monitor / lint 等) の完了を確認する
7. 出力混線 (重複・欠落・身に覚えのないテキスト) を発見したら、直ちに両セッションを停止し、`jj op log` と会話ログを保存してから再開する

### Operation Verification Checklist (2026-07-13 新設、暫定手順)

変更系 jj 操作 (`new` / `abandon` / `describe` / `rebase` / `squash` / `git fetch` / `git push`) の直後に、operation が記録されたことを確認する:

```sh
jj op log --limit 1 --no-graph
```

- 直前の操作に対応する op (description が操作内容と一致) が先頭にあること
- 無い場合は「operation not recorded」= 上記 output corruption リスクの兆候。作業を止めて状態を確認する
- `jj op log` は working copy を snapshot しない (副作用なし) ため、確認自体は安全
- 本手順は PostToolUse hook による自動化 (todo 順位 275-278 と同経緯の feedback 採用分) が実装されるまでの暫定。hook 実装後は自動検証に置き換わる

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
| `pnpm create-pr` / `pnpm merge-pr` | `gh pr create` / `gh pr merge` | ✅ 動く (2026-07-03〜、exe が `GIT_DIR` 自動注入) |
| `cli-pr-monitor --monitor-only` / `check-ci-coderabbit` | `gh api` / `gh pr checks` / `gh repo view` | ✅ 動く (2026-07-03〜、同上) |
| `gh api` / `gh pr checks` 直接呼び出し | gh | ❌ 要 `GIT_DIR` 前置 (fallback、後述) |

#### 解決 (2026-07-03 改訂): cli-* exe が `GIT_DIR` を自動注入

**恒久対策候補 1 を実装済み**: `cli-pr-monitor` / `cli-merge-pipeline` / `check-ci-coderabbit` は main() 冒頭で `lib_jj_helpers::inject_git_dir_for_gh()` を呼び、`.git` 不在 + `GIT_DIR` 未設定のとき `.jj/repo` (secondary では main store への相対パスを格納したファイル) → `store/git_target` の順に辿って main の `.git` を導出し、プロセス env に `GIT_DIR` を設定する (子プロセスの gh 全体へ伝播)。**これにより `pnpm create-pr` / `pnpm merge-pr` / `cli-pr-monitor --monitor-only` は secondary workspace でも素のコマンドで動作する**。注入時は `[env] GIT_DIR 自動注入: <path>` ログが出る。既存の `GIT_DIR` env は尊重、導出失敗は warning + 続行 (fail-soft)。

以下の手動 `GIT_DIR` 前置は、**exe を経由しない直接の gh 呼び出し時の fallback** として残す。**jj は `GIT_DIR` を無視する** (独自に `.jj/repo` を解決) ため、`GIT_DIR` は gh だけを制御し jj 操作 (bookmark 自動補完 / push / rebase) には影響しない。monitor state は `<exe>` パス基準で解決されるため、`GIT_DIR` 設定下でもこの workspace に保たれる (並列 monitor の分離を壊さない)。

なお **`GH_REPO` 環境変数による代替は不可**: `GH_REPO` は gh の pr / issue / api 系にしか効かず、引数なし `gh repo view` (exe 群の repo 検出) には無効なため、「PR 作成・マージは成功するが repo 検出依存の機能 (監視 checker / post-merge feedback) だけ silent に失敗する」部分故障を招く (PR #238 実観測)。`gh-repo-env-guard` preset (hooks-pre-tool-validate) が `GH_REPO=` の使用を block して本節へ誘導する。

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

1. **cli-* exe 側で `GIT_DIR` 自動注入** — ✅ **実装済み (2026-07-03)**: `lib_jj_helpers::inject_git_dir_for_gh()` を `cli-pr-monitor` / `cli-merge-pipeline` / `check-ci-coderabbit` の main() で呼ぶ。導出ロジックと fail-soft 方針は上記「§ 解決」参照
2. **direnv `.envrc`** — workspace ルートで `GIT_DIR` を自動エクスポート (direnv 導入が前提)。候補 1 実装により優先度低下
3. **workspace の colocated 化** — `jj workspace add` 後に当該 workspace を `.git` 付きにできるか jj の機能を調査。候補 1 実装により優先度低下

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
3. jj workspace 間の op log 競合や bookmark 衝突が dogfood で顕在化する — **発火 (2026-07-12/13)**: 並列セッションの concurrent 操作 (`jj git fetch` / `jj new` / `jj abandon` / `jj git push --all`) と重なる時間帯に 2 コミット分の操作が op log に痕跡なく消失。対応 = 本改訂 (§ Known operational risks / § 運用ルール / § Operation Verification Checklist) + push の bookmark 明示必須化 + stale 検知 nudge + operation 検証 hook (同一 PR で実装)。ADR 自体は却下せず運用ルール + 決定論ガードで継続 (公式・コミュニティとも workspace 並列は標準的な使い方であり、workspace 誤用の証拠はないため)
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
