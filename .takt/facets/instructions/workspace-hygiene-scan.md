# Workspace Hygiene Scan (週次 機械 scan: 迷い込みファイル + scratch pattern + ignored 堆積)

決定論的 scan で **リポジトリに残るべきでないファイルの候補を全件列挙**する。3 検査を実行する:

- **root 直下の想定外ファイル**: `@` の tree の root 直下ファイルを allowlist と突合。差分 = 一時スクリプト等の迷い込み候補
- **scratch pattern の whole-tree 走査**: basename が `__*` / `_tmp_*` に合致するファイル (push-runner の scratch 検査と同一 pattern の週次補完)
- **ignored 資産の堆積**: `.gitignore` 済み主要ディレクトリのサイズ報告 (報告のみ、削除提案はしない)

LLM が判断する余地はなく、shell command 出力を markdown に整形するだけの mechanical task。

> **決定論性と persona について**: 本 step は純機械 (LLM 判断ゼロ) だが、takt は **全 step に
> persona (agent) を必須**とし persona-less な shell step 型を持たない。よって workflow 上の
> `persona:` 指定は **takt の構造的要件**であり、データに対する LLM 判断を意味しない
> (file-length-watchlist と同じ整理、WR-2026-07-01-C01)。ADR-031 の 3 層分離のうち**機械層**に属する。

## 背景

2026-08-14 に post-merge-feedback workflow の分析 agent が一時スクリプト `analyze_transcript.py` をリポジトリ root へ残し、jj auto-snapshot が working copy commit へ取り込んだ (人間のレビューで偶然発見)。既存の検出層はどれも捕まえられない:

- **push-runner の scratch stage** (`src/cli-push-runner/src/stages/scratch_file_warning.rs`): push 時にしか走らず、ファイル名も `__*` / `_tmp_*` に合致しなかった
- **custom lint rule**: text 内容の編集時検査であり、ファイルの存在は対象外

jj は auto-snapshot で新規ファイルを即 commit に取り込むため、「バージョン管理対象外のまま残る」のではなく「**気づかないうちに commit へ混入する**」のが実際の失敗モード。本 step は週次 1 回、混入済み・混入しかけのファイルを棚卸しする回収網 (backstop)。生成元 facet の書き込み先制約 (上流修正) は別タスクが扱う。

## Phase 1: scan 実行

以下の shell command を実行 (Bash tool)。**git コマンドは使わない** (jj リポジトリ規約により pre-tool hook が遮断する)。

**scan 失敗を 0 件として扱わない**のが本 script の設計要件: `jj file list` の一覧を**一度だけ**取得して成否を `scan-status` に明示し、失敗時は各検査を `(未実施: ...)` と出力する。`grep` の exit 1 (= 一致なし) だけを正常 (0 件) として扱い、それ以外の失敗は `(未実施: ...)` に落とす。`du` の失敗も握り潰さない。

```bash
echo "### scan-status"
if FILES="$(jj file list -r @)"; then JJ_STATUS=OK; else JJ_STATUS=FAILED; fi
echo "jj_file_list: $JJ_STATUS"
NORM="$(printf '%s\n' "$FILES" | tr '\134' '/')"
echo "### root-unexpected (allowlist 突合)"
if [ "$JJ_STATUS" = OK ]; then
  OUT="$(printf '%s\n' "$NORM" | grep -v / | grep -vxF \
    -e .coderabbit.yaml \
    -e .gitignore \
    -e .markdownlint-cli2.jsonc \
    -e CLAUDE.md \
    -e Cargo.lock \
    -e Cargo.toml \
    -e README.md \
    -e autonomy-config.toml \
    -e package.json \
    -e pnpm-lock.yaml \
    -e pr-monitor-config.toml \
    -e push-runner-config.toml \
    -e tsconfig.json)"; RC=$?
  if [ "$RC" -le 1 ]; then [ -n "$OUT" ] && printf '%s\n' "$OUT" || echo "(0 件)"; else echo "(未実施: grep 失敗 rc=$RC)"; fi
else
  echo "(未実施: jj file list 失敗)"
fi
echo "### scratch-pattern (__* / _tmp_*)"
if [ "$JJ_STATUS" = OK ]; then
  OUT2="$(printf '%s\n' "$NORM" | grep -E '(^|/)__[^/]*$|(^|/)_tmp_[^/]*$')"; RC2=$?
  if [ "$RC2" -le 1 ]; then [ -n "$OUT2" ] && printf '%s\n' "$OUT2" || echo "(0 件)"; else echo "(未実施: grep 失敗 rc=$RC2)"; fi
else
  echo "(未実施: jj file list 失敗)"
fi
echo "### ignored-size (報告のみ)"
for d in .takt/runs target; do
  if [ -d "$d" ]; then du -sh "$d" || echo "(取得失敗: $d)"; else echo "(不在: $d — cloud 実行では正常)"; fi
done
```

> **パス区切りの正規化に `tr '\134' '/'` (8 進表記) を使う理由**: Windows の `jj file list` は
> `\` 区切りを出力するが、`\\` リテラルを含む awk/sed 正規表現は quoting 層 (agent への
> コマンド転送) を跨ぐ際に `\` へ潰れて壊れることを 2026-08-14 に実測した。8 進表記なら
> バックスラッシュの連続がそもそも現れない。正規化後は `/` だけを扱えばよく、cloud (Linux)
> 実行でもそのまま動く。
>
> **allowlist の保守**: root 直下に正当なファイルを追加した PR では、本 allowlist にも同じ PR で
> 追加する。突合は完全一致 (`grep -vxF`) であり、パターン解釈による誤除外は起きない。

## Phase 2: markdown 整形

`workspace-hygiene-scan.md` を以下の format で Report Directory に出力する。3 検査とも常に section を出す (0 件でも「clean state」と明示、aggregate が常に Read 可能)。

> **下記ブロックは形式例であり、`<...>` はすべて placeholder。実 report には Phase 1 の shell 出力に
> 実際に現れた値だけを転記する。例の値をコピーしない。** 0 件の検査はデータ行を出力せず件数行の
> 「0 件 (clean state)」だけを書く。shell 出力に `(未実施: ...)` が現れた検査は、件数を書かず
> 「**未実施** (理由: shell 出力の文言を転記)」と書く — 未実施を 0 件と報告してはならない。

```markdown
# Workspace Hygiene Scan (週次 機械 scan)

- scan 日時: <ISO 8601 UTC、本 step の wall clock>
- scan-status: <shell 出力の scan-status section を転記 (例: jj_file_list: OK)>
- 対象: root 直下 allowlist 突合 / basename `__*` `_tmp_*` whole-tree / ignored 主要 dir サイズ

## root 直下の想定外ファイル

- 件数: <N> 件  (0 件のときは「**0 件 (clean state)**」、未実施のときは「**未実施** (理由)」)

| ファイル | 備考 |
|---|---|
| `<shell 出力に現れたファイル名>` | allowlist 外 |

## scratch pattern 合致 (whole-tree)

- pattern: `__*` / `_tmp_*` (push-runner `[scratch_file_warning]` と同一)
- 件数: <N> 件  (0 件のときは「**0 件 (clean state)**」、未実施のときは「**未実施** (理由)」)

| ファイル |
|---|
| `<shell 出力に現れたパス>` |

## ignored 資産の堆積 (報告のみ)

| サイズ | ディレクトリ |
|---|---|
| <du 出力のサイズ> | `<du 出力のディレクトリ>` |

(「不在: ...」「取得失敗: ...」の行は table に入れず、そのまま文として転記する)

保持ポリシーの判断は既存タスク「`check_concurrent_run_guard` の `.takt/runs` 全走査コストと保持ポリシー」の管轄。本 section は観測値の週次記録のみ。
```

severity の目安 (aggregate-weekly の統合用): root 直下の想定外ファイル = `medium` (commit 混入の実績があるクラス)、scratch pattern 合致 = `low`〜`medium`、ignored 堆積 = 情報提供 (finding にしない)。

## Output contract

- File: `workspace-hygiene-scan.md` (Report Directory)
- Format identifier: `workspace-hygiene-scan`
- 3 検査とも 0 件でも section を生成 (「未実施」と「0 件」を区別するため。aggregate-weekly が常に Read 可能)
- **未実施は 0 件と書かない**: shell 出力が `(未実施: ...)` の検査は「未実施 + 理由」で報告する。aggregate-weekly はこれを finding にせず warning として weekly report に転記する
- **削除は提案止まり**: 検出ファイルの削除・`.gitignore` 追記は `/weekly-review` skill の Phase 3 でユーザーが決める (ADR-022)。本 step は列挙のみ

## Completion criteria

scan 完了 + markdown 出力で `analysis complete` を articulate (他 facet と同じ条件文字列、step-level rule `all("analysis complete")` と整合)。

## 重要な原則

- **読み取り専用 (`edit: false`)**。ファイルの削除・移動・`.gitignore` 編集は行わない (= 列挙報告のみ)
- **LLM 判断の余地なし**: 命令通りに Bash を実行し、出力を転記するだけ。ファイルの中身を解釈しない。「これは消してよさそう」という推測を書かない
- **git コマンド禁止**: jj リポジトリ規約 (pre-tool hook が遮断)。列挙は `jj file list`、サイズは `du` で完結させる
- **3 検査とも件数 0 でも section を生成**: aggregate-weekly が常に Read 可能な前提を満たすため
