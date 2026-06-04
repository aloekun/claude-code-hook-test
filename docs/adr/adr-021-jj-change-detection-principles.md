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

### 原則 5: bookmark 検出は優先度付き revset + trunk filter を標準とする (2026-04-19 追加)

PR #54 / PR #55 で cli-merge-pipeline / cli-pr-monitor の bookmark 検出を標準化した。以下をプロジェクト共通の既定値とする:

```rust
const BOOKMARK_SEARCH_REVSETS: &[&str] = &["@", "@-", "@--"];
const TRUNK_BOOKMARKS: &[&str] = &["main", "master", "trunk", "develop"];
```

#### 問題の背景

`jj new` 直後は「`@` = 空コミット / bookmark は `@-` 上」という構成になる (PR #53 実測)。`@` だけを見る実装では bookmark 検出が空振りする。

単純に `@-` まで広げると、fresh checkout 直後に `@-` が trunk bookmark (`master` 等) を指してしまい、PR の head として trunk を誤検出する。

#### 検討した選択肢

| option | revset | 評価 | 結果 |
|---|---|---|---|
| A | `@` のみ | 空コミット状況で空振り (PR #53 実測の症状) | 不採用 |
| **B** | **`@`, `@-`, `@--` の近い順** | 最大 2 階層までカバー。trunk filter 併用で false hit 回避可 | **採用** |
| C | `ancestors(@, N)` 等の広い revset | N の決定が恣意的、遠い祖先の bookmark を誤検出するリスク | 不採用 |

option B + trunk filter で、以下の両方を成立させる:

- `@` 空 + bookmark が `@-` / `@--` 上にある一般的ケースをカバー
- fresh checkout で `@-` = `master` の場合に false hit しない

#### 検出ロジックの 3 層構造

```text
parse   : jj bookmark list 出力のテキスト解析 (pure function)
query   : revset を受けて bookmark 名リストを返す (副作用: jj プロセス)
select  : BOOKMARK_SEARCH_REVSETS を近い順に走査し、
          最初に非空かつ trunk filter 通過する revset の結果を返す
```

`select_from_revsets(revsets, query_fn)` はクロージャを受け取る pure function 設計にし、unit test で jj プロセスなしに network / revset priority / trunk filter を検証できる (ADR-021 原則 3 と整合)。

#### 実装箇所 (PR #54 / PR #55 時点)

| クレート | ファイル | 用途 |
|---|---|---|
| cli-merge-pipeline | `src/cli-merge-pipeline/src/main.rs` | `pnpm merge-pr` の PR detection |
| cli-pr-monitor | `src/cli-pr-monitor/src/util.rs` | `pnpm create-pr` の bookmark 検出 + `--head` 自動補完 |
| cli-push-runner | `src/cli-push-runner/src/stages/push_jj_bookmark.rs` | `pnpm push` の bookmark fallback |

3 クレートで定数・関数が重複していた状態は PR-C (ADR-024 本採用) で `src/lib-jj-helpers/` に集約済。以降の新規クレートは `lib-jj-helpers` を依存に追加して共通 API を呼び出す。

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
- **lib-jj-helpers の利用徹底**: 新規 jj 連携クレートでは `src/lib-jj-helpers/` を依存に追加し、本 ADR 原則 5 の共通 API (`get_jj_bookmarks` 等) を利用する。`capture_commit_id` / `diff_is_empty` は 2 つ目の使用例出現時に段階的移設予定 (ADR-024 本採用)
- **他の jj コマンド差異の文書化**: `jj bookmark` / `jj new` / `jj describe` も git と意味が違う箇所が多い。必要に応じて追記

## Revset Composability (PR #194 T3-#3 追記、2026-06-04)

PR #194 で `sweep_empty_commits_in_pr_range` の初版が「全 empty commit を Rust 側で取得 → for ループで `description` を check」という generate-then-filter 設計だったが、`description(substring:"fix(review):")` を **revset 側で filter** することで output が最初から絞られる方が efficient + reviewer cost 低 + scope 明確という改善が takt-fix iteration で観測された。本 section は jj revset の composability 原則と設計チェックリストを codify する。

### 原則: revset で「何を取得するか」を最小化

jj の revset は AND / OR / 範囲 / metadata filter を表現できる小さな DSL。**「Rust に返す前に revset で絞れるものは全て絞る」** ことで、以下が同時に実現する:

- **token / IO 効率**: 出力量が最小化され、後続 processing コストが下がる
- **review cost 低**: revset 自体が「何を対象にするか」の宣言になり、Rust 側の filter ロジックを読まなくても scope が読み取れる
- **scope drift 防止**: filter を Rust の for ループに書くと、後の修正で条件追加が漏れる risk があるが、revset で 1 箇所にまとまっていれば変更追跡が容易

### 典型 filter 関数

| revset 関数 | 用途 |
|---|---|
| `empty()` | file change なし (`jj diff --stat` 空相当) |
| `description(substring:"...")` | description 部分一致 (parens / 記号を含む文字列も safe) |
| `description(exact:"...")` | description 完全一致 |
| `description(regex:"...")` | description 正規表現 |
| `(branch..@)` | branch を除いた `@` までの範囲 |
| `author(...)` / `mine()` | author 絞り込み |
| `bookmarks()` | bookmark がついている commit のみ |
| `<expr1> & <expr2>` | AND (両方満たす) |
| <code>&lt;expr1&gt; \| &lt;expr2&gt;</code> | OR (どちらかを満たす) |
| `~<expr>` | NOT |

### 設計チェックリスト (新規 jj 操作コードを書く前に)

- [ ] **取得目的は明示**: revset が何を表現しているかコメント / 関数名で書く (例: "fix(review): empty commits in PR range")
- [ ] **各条件を revset で表現できないか検討**: Rust の for ループに filter を書く前に revset operators で代替できないか確認
- [ ] **`&` / `|` で組合せ可能か**: 複数条件は revset 内で AND/OR 結合する
- [ ] **description マッチは `substring:` 修飾子を必須化**: parens / 記号を含む文字列で default `exact:` が 0 hit する bug を防ぐ (例: `description("fix(review):")` は完全一致になり失敗、`description(substring:"fix(review):")` が正解)
- [ ] **fail-open に注意**: jj log 失敗時の警告ログのみで継続するか、副作用処理を block するかは ADR-043 § 適用範囲 で判断 (sweep 系は fail-open、gate 系は fail-closed)
- [ ] **integration test の不変式は description ベース**: `~/.claude/rules/common/testing.md` § "jj 操作コードの integration test pattern" の `assert_descriptions_absent_in_pr_range` パターンを使う (count NG / description OK)

### Anti-pattern: generate-then-filter

```rust
// BAD: 全 empty を取得して Rust で filter
let all_empty_change_ids = jj_log("empty() & (master..@)");
for cid in all_empty_change_ids {
    let desc = jj_log_description(&cid);
    if desc.starts_with("fix(review):") {
        jj_abandon(&cid);
    }
}

// GOOD: revset 側で filter 済
let target_change_ids = jj_log("empty() & description(substring:\"fix(review):\") & (master..@)");
for cid in target_change_ids {
    jj_abandon(&cid);
}
```

### 実装事例

PR #194 の `sweep_empty_commits_in_pr_range` (`src/cli-pr-monitor/src/fix_commit.rs:217-218`) で本原則を適用済。

### 試験運用判断基準

本 section は試験運用とする。今後の jj 操作コード PR で本チェックリストが reviewer / Claude の判断 anchor として参照されるかを観測。3 PR 以上で「revset filter で書き直し」の review iteration が発生しなければ stable 昇格、再発があれば原則に不足がないか分析。

### 参照

- `~/.claude/rules/common/testing.md` § "jj 操作コードの integration test pattern": 本 section と相補関係 (revset 設計 + test 設計の対)
- ADR-043 (Security/Quality Gate Fail-Closed 原則): gate 系の fail-closed と sweep 系の fail-open の使い分け
- PR #194 commit (`src/cli-pr-monitor/src/fix_commit.rs:217-218`): 実装事例
