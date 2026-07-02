# File Size Watchlist (週次 機械 scan: `.rs` 800 行 + `todo*.md` 50KB)

決定論的 scan で **サイズ閾値を超えた/接近した file を全件列挙**する。2 次元を計測する:

- `src/**/*.rs` の **行数** (800 行閾値、観点⑦・順位 147 file length と整合)
- `docs/todo*.md` の **バイト数** (50KB=51200B 閾値 / 48KB=49152B で接近警告、観点⑦ Todo 分割 trigger)

LLM が判断する余地はなく、shell command 出力を markdown に整形するだけの mechanical task。

> **決定論性と persona について (WR-2026-07-01-C01 解消)**: 本 step は純機械 (LLM 判断ゼロ) だが、
> takt は **全 step に persona (agent) を必須**とし persona-less な shell step 型を持たない。
> よって workflow 上の `persona:` 指定は **takt の構造的要件**であり、データに対する LLM 判断を
> 意味しない。矛盾を避けるため本 step は「Bash が最終 markdown まで生成 → agent はそれを Report
> Directory へ書き出すだけ (整形も判断もしない)」形とし、`weekly-review.yaml` 側にも同旨コメントを
> 付す。ADR-031 の 3 層分離 (Rust/shell 機械 / takt AI / skill ask) のうち **機械層**に属する。

## 背景

順位 147 (file_length lint、PR #202) と PR-W5 (`[file_length_gate]` Stop gate、#234) は **触られた file / PR 範囲の file** を対象とする edit-time / push-time の強制で、**未触り state の全件棚卸し**は対象外。`docs/todo*.md` の 50KB 分割 trigger (PR #88 / #96 / #101 / #123 / #172 で実証) も同様に自動計測経路が無い。

本 step は週次 1 回 master HEAD を deterministic に scan し、両次元の閾値超過/接近を watchlist として report 化する。aggregate-weekly が weekly report の "file size watchlist" section として転載する。

## Phase 1: scan 実行

以下の shell command を実行 (Bash tool)。出力は整形済みなので、そのまま次 Phase で転記する。

```bash
echo "### rs-lines (>800)"
find src -name '*.rs' -not -path '*/target/*' -exec wc -l {} + 2>/dev/null \
  | awk '$1 > 800 && $2 != "total" { print $0 }' \
  | sort -rn
echo "### todo-bytes (>=49152 = 48KB, 閾値 50KB=51200)"
find docs -maxdepth 1 -name 'todo*.md' -exec wc -c {} + 2>/dev/null \
  | awk '$1 >= 49152 && $2 != "total" { print $1, $2 }' \
  | sort -rn
```

各 section が空出力のとき: その次元は 0 件 (clean)。

## Phase 2: markdown 整形

`file-length-watchlist.md` を以下の format で Report Directory に出力する。両次元とも常に section を出す (0 件でも「clean state」と明示、aggregate が常に Read 可能)。

```markdown
# File Size Watchlist (週次 機械 scan)

- scan 日時: <ISO 8601 UTC、本 step の wall clock>
- 対象/閾値: `src/**/*.rs` 行数 > 800 (`target/` 除外) / `docs/todo*.md` バイト数 >= 50KB (48KB で接近)

## `.rs` 行数 watchlist (> 800 行)

- 件数: N 件  (0 件のときは「**0 件 (clean state)**」)

| 行数 | ファイル |
|---|---|
| 921 | `src/cli-merge-pipeline/src/main.rs` |
| ... | ... |

## `docs/todo*.md` サイズ watchlist (>= 48KB)

- 閾値: 50KB=51200B 到達で分割推奨、48KB=49152B で接近警告
- 件数: N 件  (0 件のときは「**0 件 (clean state)**」)

| バイト数 | ファイル | 状態 |
|---|---|---|
| 49915 | `docs/todo-summary.md` | 接近 (48KB超) |
| ... | ... | 分割推奨 (50KB超) / 接近 (48KB超) |

## 進捗参照

`.rs` 側は `docs/file-length-enforcement-plan.md` / PR-W5 `[file_length_gate]`、`todo` 側は
`docs/todo.md` preamble の分割ルーティング (新規は次 file へ) と対応。
```

## Output contract

- File: `file-length-watchlist.md` (Report Directory)
- Format identifier: `file-length-watchlist`
- 両次元とも 0 件でも section を生成 (clean state 確認のため。aggregate-weekly が常に Read 可能)

## Completion criteria

scan 完了 + markdown 出力で `analysis complete` を articulate (他 facet と同じ条件文字列、step-level rule `all("analysis complete")` と整合)。

## 重要な原則

- **読み取り専用 (`edit: false`)**。コード / todo の修正は行わない (= watchlist 報告のみ)
- **LLM 判断の余地なし**: 命令通りに Bash を実行し、出力を転記するだけ。file の中身を解釈しない
- **両次元とも件数 0 でも section を生成**: aggregate-weekly が常に Read 可能な前提を満たすため
