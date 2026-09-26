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

次のコマンドを **1 回だけ**実行する (Bash tool)。

```bash
pnpm weekly-scan:workspace-hygiene
```

出力は `### scan-status` / `### root-unexpected (allowlist 突合)` / `### scratch-pattern (__* / _tmp_*)` / `### ignored-size (報告のみ)` の 4 section。scan の中身は [scripts/weekly-scan.mjs](../../../scripts/weekly-scan.mjs) が持ち、次の設計要件を守る:

- **scan 失敗を 0 件として扱わない**: `jj file list -r @` を**一度だけ**実行して成否を `scan-status` に出し、失敗時は一覧を使う 2 検査 (root-unexpected / scratch-pattern) を `(未実施: ...)` と出す。ignored-size は一覧を使わないので、失敗時も続けて出る
- **サイズ取得の失敗を握り潰さない**: `(取得失敗: <dir> — <理由>)` と出す。サイズはファイルの見かけの合計で、`du` とは 1〜2 割ずれる (報告専用)
- Windows の `jj file list` が出す `\` 区切りは `/` にそろえてから判定する

コマンド自体が 0 以外で終了したときは、3 検査とも「**未実施** (理由: stderr の文言)」と書く。

**本 step が許可されている Bash はこのコマンドだけ** (ADR-083 決定 4)。`jj` / `grep` / `du` などで自前に調べ直さない。

> **allowlist の保守**: root 直下に正当なファイルを追加した PR では、`scripts/weekly-scan.mjs` の
> `ROOT_ALLOWLIST` にも同じ PR で追加する。突合は完全一致であり、パターン解釈による誤除外は起きない。

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
- **レポート本文は日本語で書く。** コード識別子・ファイルパス・ADR 番号・コマンド (Bash の出力を含む) はもちろん、**完了条件の `analysis complete` も訳さない** (`weekly-review.yaml` の `rules.condition` と step-level rule `all("analysis complete")` が英語リテラルで照合する)

## Completion criteria

scan 完了 + markdown 出力で `analysis complete` を articulate (他 facet と同じ条件文字列、step-level rule `all("analysis complete")` と整合)。

## 重要な原則

- **読み取り専用 (`edit: false`)**。ファイルの削除・移動・`.gitignore` 編集は行わない (= 列挙報告のみ)
- **LLM 判断の余地なし**: 命令通りに `pnpm weekly-scan:workspace-hygiene` を実行し、出力を転記するだけ。ファイルの中身を解釈しない。「これは消してよさそう」という推測を書かない
- **3 検査とも件数 0 でも section を生成**: aggregate-weekly が常に Read 可能な前提を満たすため
