# File Length Watchlist (週次 800 行超 scan)

決定論的 scan で 800 行超 file を全件列挙する。LLM が判断する余地はなく、shell command 出力を markdown table に整形するだけの mechanical task。

## 背景

順位 147 (file_length lint、PR #202) は `hooks-post-tool-comment-lint-rust` の PostToolUse hook として実装されており、**触られた file の編集時のみ** `additionalContext` で 800 行超を警告する設計 (soft-nag、touch-trigger ratchet)。

このため:

- 未触り state の violation は警告されない
- AI / 人が警告を無視して進められる
- 結果として PR-3a (#217) 時点で 7 件の 800 行超 file が累積した経緯あり (PR #218 で計画書 `docs/file-length-enforcement-plan.md` が land、PR-W0 として本 step を追加)

本 facet は週次 1 回 master HEAD の `src/` 全体を deterministic に scan し、800 行超 file を全件列挙して watchlist として report 化する。これにより ratchet が未発火の violation も可視化でき、aggregate-weekly が weekly report の "file_length watchlist" section として記載する。

## Phase 1: scan 実行

以下の shell command を実行 (Bash tool):

```bash
find src -name '*.rs' -not -path '*/target/*' -exec wc -l {} + 2>/dev/null \
  | awk '$1 > 800 && $2 != "total" { print $0 }' \
  | sort -rn
```

出力例 (PR-3a #217 land 直後の master state):

```text
1606 src/hooks-post-tool-comment-lint-rust/src/main.rs
1432 src/cli-merge-pipeline/src/feedback.rs
1404 src/cli-pr-monitor/src/stages/poll/mod.rs
 982 src/cli-push-runner/src/stages/lint_screen.rs
 972 src/cli-pr-monitor/src/fix_commit.rs
 946 src/cli-push-runner/src/config.rs
 890 src/cli-merge-pipeline/src/main.rs
```

0 件のとき: command が空出力。

## Phase 2: markdown 整形

`file-length-watchlist.md` を以下の format で Report Directory に出力する。

### 800 行超 file が 1 件以上ある場合

```markdown
# File Length Watchlist (週次 800 行超 scan)

- scan 日時: <ISO 8601 UTC、本 step の wall clock>
- scan 対象: `src/**/*.rs` (`target/` 除外)
- 閾値: 800 行 (coding-style.md File Organization)
- 件数: N 件

## 800 行超 file 一覧 (上限 800 行を超過、N 件)

| 行数 | ファイル |
|---|---|
| 1606 | `src/hooks-post-tool-comment-lint-rust/src/main.rs` |
| 1432 | `src/cli-merge-pipeline/src/feedback.rs` |
| ... | ... |

## 進捗参照

`docs/file-length-enforcement-plan.md` の Phase 1 (PR-W1 〜 W4) で各 file の分割計画が capture されている。本 watchlist は分割 PR の land 状況を週次で可視化する役割。

完了条件 (本 watchlist の 0 件到達) を満たすと、計画書の削除条件 1/3 を満たす。
```

### 0 件 (clean state) の場合

```markdown
# File Length Watchlist (週次 800 行超 scan)

- scan 日時: <ISO 8601 UTC>
- scan 対象: `src/**/*.rs` (`target/` 除外)
- 閾値: 800 行 (coding-style.md File Organization)
- 件数: **0 件 (clean state)**

現時点で 800 行超 file は存在しない。Phase 1 (file-length-enforcement-plan.md PR-W1〜W4) は完了状態にあるか、もしくは新規 file が制約内に収まっている。
```

## Output contract

- File: `file-length-watchlist.md` (Report Directory)
- Format identifier: `file-length-watchlist`
- 0 件 case でも file を生成 (clean state 確認のため。aggregate-weekly が常に Read 可能)

## Completion criteria

scan 完了 + markdown 出力で `analysis complete` を articulate (他の facet と同じ条件文字列を使用、step-level rule `all("analysis complete")` と整合)。

## 重要な原則

- **読み取り専用 (`edit: false`)**。コード修正は行わない (= watchlist 報告のみ)
- **LLM 判断の余地なし**: 命令通りに Bash を実行し、出力を整形するだけ
- **件数 0 でも file を生成**: aggregate-weekly が常に Read 可能な前提を満たすため
