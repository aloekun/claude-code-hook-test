# ADR-021: jj 変更検出ロジックの設計原則

## ステータス

承認済み (2026-04-17)

## コンテキスト

### 問題

PR #43 で `pnpm create-pr` 実行後、`[post-pr-monitor] takt fix による変更を検出` のログと共に意図しない auto re-push が発動し、commit description が元の `docs(todo): ...` から `fix(cli-pr-monitor): CodeRabbit 指摘を自動修正` に上書きされた。

根本原因は `src/cli-pr-monitor/src/stages/monitor.rs` (当時) の以下のコードにあった:

```rust
let (ok, diff_output) = run_cmd_direct("jj", &["diff", "--stat"], &[], 30);
```

この `jj diff --stat` は `@` vs parent の差分を返す。jj の working-copy-is-a-commit モデルでは `@` が PR の content commit そのものであるため、**PR 全体の diff が常に「takt fix 後の変更」として誤報告**される。

### git との差異

git では working copy と HEAD が別で、`git diff` は staged/unstaged の概念を持つ。一方 jj では `@` 自体が commit であり、`jj diff` は文脈によって意味が異なる:

| コマンド | 意味 |
|---------|------|
| `jj diff --stat` (引数なし) | `@` vs parent (PR 全体の差分) |
| `jj diff --from <X>` | `X` vs `@` (特定状態との差分) |
| `jj diff --from <X> --to <Y>` | `X` vs `Y` の差分 |

git の `git diff` (staged) 感覚で移植すると、意図しない範囲の差分を取得してしまう。

### commit_id 単独比較の限界

commit_id を pre/post で比較する案も検討したが、jj は auto-snapshot で working copy を定期的に取り込むため、以下のケースで commit_id が変化する可能性がある:

- takt 実行中の `jj auto-snapshot` による timestamp 更新
- jj metadata のみの更新 (外部同期等)
- 実ファイル内容は変わっていないのに ID だけ変化

commit_id 単独では「実質的な変更があったか」の判定として不十分。

## 決定

### 原則 1: 二段構え判定

「takt/AI 実行前後の変更」を判定する際は、commit_id と diff の両方を確認する:

```text
pre_cid = capture_commit_id()       // 実行前
run_takt_or_ai()
post_cid = capture_commit_id()      // 実行後

決定ロジック:
  pre_cid == post_cid                        → NoChange
  pre_cid != post_cid && diff_is_empty       → NoChange  (metadata 変更のみ)
  pre_cid != post_cid && diff has content    → HasChange  (実質変更あり)
  いずれかの cid 取得失敗                     → IdCaptureFailed (fail-safe)
```

diff 確認は `jj diff --from pre_cid --to post_cid --stat` で行う (PR 全体ではなく差分区間のみ)。

### 原則 2: `jj diff --stat` 単独を変更検出に使わない

`jj diff --stat` は「PR 全体の差分」を返すため、「直前の操作で変化した部分」の検出には使用禁止。
必ず `--from X --to Y` で比較区間を明示する。

### 原則 3: 判定は pure function、副作用は注入

判定関数は `Option<&str> + FnOnce(&str, &str) -> bool` のシグネチャで書き、jj 呼び出しを外部から注入する:

```rust
pub fn decide_repush(
    pre_cid: Option<&str>,
    post_cid: Option<&str>,
    diff_empty_fn: impl FnOnce(&str, &str) -> bool,
) -> RepushDecision { ... }
```

これにより unit test が容易になり、外部 jj プロセス無しで 4 分岐すべてを網羅できる。

### 原則 4: fail-safe デフォルト

commit_id 取得失敗時 (`IdCaptureFailed`) は「変更なし」と同じ扱いにし、push を発動しない。「判定できないから念のため push」は致命的副作用 (元 description 上書き等) を招く。

## 影響

### 採用される構成要素

- `src/cli-pr-monitor/src/runner.rs` の `capture_commit_id()` / `diff_is_empty()`
- `src/cli-pr-monitor/src/stages/repush.rs` の `decide_repush()` pure function
- `src/cli-pr-monitor/src/stages/repush.rs::RepushDecision` 3 状態 enum (`NoChange` / `HasChange` / `IdCaptureFailed`)
- unit test 6 本 (4 分岐 + pre/post いずれか欠損の 2 ケース)
- 統合テスト 1 本 (`#[ignore]` 付き、実 jj で no-op シナリオを検証)

### 避けるべきアンチパターン

- **`jj diff --stat` 単独を「直前の変更」判定に使う**: @ vs parent なので PR 全体を拾う (PR #43 で誤発火の原因)
- **commit_id 単独比較**: jj の metadata 更新で ID が変わるケースを誤判定
- **判定関数に jj 呼び出しを直接埋め込む**: unit test で現実の jj プロセスが必要になり、テストコストが膨らむ
- **取得失敗時に push を試みる**: fail-open は致命的副作用を招く

## 次ステップ (スコープ外)

- **cli-merge-pipeline の post_steps 実装時に流用**: ADR-013 の merge 後 AI ステップで、merge の副作用を検出する際も同パターン
- **共通ライブラリ化の検討**: 2 つ目の使用例が出たら `src/lib-jj-helpers/` への切り出しを検討 (ADR-024 (仮) で観察中)
- **他の jj コマンド差異の文書化**: `jj bookmark` / `jj new` / `jj describe` も git と意味が違う箇所が多い。必要に応じて追記
