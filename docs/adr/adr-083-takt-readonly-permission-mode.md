# ADR-083: takt の readonly step を permission mode で強制する

## ステータス

試験運用 (2026-09-26)

> takt の `edit: false` は、それだけでは書き込みを止めていなかった。takt 0.35.3 は Claude の
> 既定 permission mode を組み込みで `edit` にしており、readonly のつもりの step が
> `acceptEdits` で動いていた。プロジェクト設定 `.takt/config.yaml` で既定を `readonly` に
> 上書きし、書き込みが要る step だけを `required_permission_mode: edit` で引き上げる。

## コンテキスト

2026-09-25 の insights は「編集禁止の step で Write が使われ、報告だけを返す再プロンプトが要る」と指摘した。実ログで確かめると、編集禁止と明示された step の 73 セッション中 15 セッションで Write が**成功**していた。書き込み先は Report Directory のほか、`/tmp` などリポジトリの外にも及んでいた。`allowed_tools` を Read / Glob / Grep に絞った step でも、Bash / PowerShell / Write が実行されていた。

takt 0.35.3 のソースを読むと、原因は 3 つ重なっている:

1. **既定 permission mode が `edit`**: `DEFAULT_PROVIDER_PERMISSION_PROFILES` (`core/piece/permission-profile-resolution.js`) が `claude: { defaultPermissionMode: 'edit' }` を持つ。step が `edit: false` でも、プロジェクト / グローバルのプロファイルが無ければこの既定が効き、Claude の `acceptEdits` になる。実セッションのログにも `"permissionMode":"acceptEdits"` が記録されていた (PR #520 の post-merge feedback の 3 step すべて)
2. **`allowed_tools` は許可の一覧で、禁止の一覧ではない**: takt は `allowed_tools` を Claude Agent SDK の `allowedTools` (確認なしで許可するツール) に渡す。`disallowedTools` は渡さない。一覧に無いツールの扱いは permission mode が決めるので、`acceptEdits` の下では Write が一覧に無くても通る
3. **`edit: false` が効くのはプロンプトの文面と自動許可の一覧だけ**: 「Editing is DISABLED」の文言を足し、`allowedTools` から `Write` を除く。どちらも `acceptEdits` の下では書き込みを止めない

プロンプトの文面を直す対処は PR #519 で行ったが、PR #520 の実走でも文面を変えていない facet (analyze-prepush-reports) が Write を実行した。文面は発生源を減らす層であって保証ではない ([ADR-042](adr-042-rule-vs-mechanism-boundary.md))。

## 決定

### 1. プロジェクト設定で既定を `readonly` にする

`.takt/config.yaml` に次を置く:

```yaml
provider_profiles:
  claude:
    default_permission_mode: readonly
```

`readonly` は Claude の `default` モードに対応する。確認なしで許可されるのは `allowed_tools` のツールと、Claude Code が読み取り専用と判定するコマンド (`ls` 等) だけになり、それ以外は拒否される。

### 2. 書き込みが要る step は `required_permission_mode: edit` を宣言する

takt は `required_permission_mode` を下限として扱う (`applyRequiredPermissionFloor`)。既定を `readonly` にしても、宣言した step は `edit` (Claude の `acceptEdits`) で動く。2026-09-26 時点で書き込みが要る step は pre-push-review と post-pr-review の `fix` / `fix_supervisor` の 4 つで、4 つとも宣言済み。

**新しく書き込みが要る step を足すときは、`edit: true` と `required_permission_mode: edit` の両方を書く。** `edit: true` だけでは `readonly` で動き、Write が拒否される。

### 3. 検証 (spike、2026-09-26)

scratchpad に最小の takt プロジェクトを作り、本 ADR の config で haiku を実走させた。拒否は即座に返り、確認待ちで止まることはなかった (各 20 秒台で完了)。

| step | 記録された permission mode | Write | Bash の `>` 書き込み | `gh --version` | `ls` |
|---|---|---|---|---|---|
| `edit: false`、Bash を `allowed_tools` に**含まない** | `default` | — | 拒否 | 拒否 | 成功 |
| `edit: false`、Bash を `allowed_tools` に**含む** | `default` | 拒否 | **成功** | — | 成功 |
| `required_permission_mode: edit` | `acceptEdits` | 成功 | — | — | — |

既存の readonly step 18 個のうち、Bash を `allowed_tools` に含まない 11 個の facet は、`gh` / `jj` / `cargo` 等の実行を前提にしていない (出てくるのは「`jj diff` を自分で実行しない」といった禁止文だけ)。本決定で動かなくなる step は無い。

### 4. readonly step の Bash はコマンド単位で許可する

表の 2 行目のとおり、`allowed_tools` に素の `Bash` を書くと、readonly step でも Bash 経由の書き込み (`echo x > file` 等) が通る。そこで readonly step には、facet が実行するコマンドだけを `Bash(<prefix>:*)` の形で許可する。読み取り専用と判定されるコマンド (`ls` / `grep` / `wc` 等) は許可しなくても通る。

| step | 許可 |
|---|---|
| post-merge-feedback の analyze-pr | `Bash(gh pr diff:*)` / `Bash(gh pr view:*)` / `Bash(gh api:*)` |
| post-pr-review の analyze | なし (facet が Bash のコマンドを使わない) |
| weekly-review の ledger-candidates | `Bash(pnpm ledger-candidates)` |
| weekly-review の review-todo-whole / review-jj-robustness-whole | `Bash(jj log:*)` |
| weekly-review の file-length-watchlist / workspace-hygiene-scan | 素の `Bash` を残す (下記) |

リポジトリを cwd にした spike (2026-09-26、haiku) で確かめた:

| step の許可 | 実行 | 結果 |
|---|---|---|
| `gh` の 3 パターン | `gh pr view` / `gh api --jq` / `gh pr diff \| head` | 成功 |
| 同上 | `echo x > <file>` | 拒否 |
| `Bash(jj log:*)` + `Bash(pnpm ledger-candidates)` | `jj log` / `pnpm ledger-candidates` / `wc -l` | 成功 |
| 同上 | `jj new -m ...` | 拒否 ("This command requires approval") |
| `Bash(find:*)` + `Bash(jj file list:*)` + `Bash(du:*)` | `find ... -exec wc -l {} +` を含むパイプ | 拒否 ("find with '-exec' ... cannot be auto-allowed by a Bash(find:*) prefix rule") |
| 同上 | `if FILES="$(jj file list -r @)"; then ...; fi; for ...` | 拒否 ("Contains shell syntax ... that cannot be statically analyzed") |

最後の 2 行が、file-length-watchlist と workspace-hygiene-scan に素の `Bash` を残す理由である。両 step の facet は固定スクリプト (`find -exec`、`if` / `for` / `$(...)`) を実行し、コマンド単位の許可では拒否される。**readonly step に Bash のコマンドを足すときは、`Bash(<prefix>:*)` を足し、同じ構文で拒否されないかを実走で確かめる。**

## 却下した案

- **PreToolUse hook で readonly step の Write を止める**: hook からは、呼び出し元が takt のどの step かを判別できない。permission mode は takt が step ごとに決めて SDK に渡しているので、そこで止める方が正確
- **permission mode を変えずに `allowed_tools` だけを絞る**: `allowed_tools` は禁止リストではないので、`acceptEdits` の下では絞っても書き込みは止まらない。絞ることが効くのは決定 1 で既定を `readonly` にした後である (決定 4)

## 結果

- readonly step の Write は拒否される。PR #519 の文面修正は、拒否される呼び出しを減らす層として残る
- [ADR-031](adr-031-weekly-review-pipeline.md) § ADR-022 との整合性の「takt facets は副作用なし」は、本 ADR より前は成り立っていなかった (同 ADR に注記した)

### 既知の限界

- **素の `Bash` を残した 2 step は、Bash 経由で書き込める** (決定 4)。weekly-review の file-length-watchlist と workspace-hygiene-scan が該当する。どちらも LLM の判断を挟まない固定スクリプトの実行なので、スクリプトを決定論 exe (`cli-*`) へ移し、step は出力を読むだけにするのが根本対処になる
- **takt の組み込み既定値に依存する**: 本 ADR は 0.35.3 の `DEFAULT_PROVIDER_PERMISSION_PROFILES` と `applyRequiredPermissionFloor` の挙動を前提にする。takt を更新するときは ([ADR-017](adr-017-takt-version-pinning.md))、この 2 つが変わっていないかを確認する。確認は、readonly step のセッションログに `"permissionMode":"default"` が記録されるかで行える
- **派生プロジェクトには配布していない**: `.takt/config.yaml` は本リポジトリにだけ置いた。takt を使う派生プロジェクトは同じ状態のまま
