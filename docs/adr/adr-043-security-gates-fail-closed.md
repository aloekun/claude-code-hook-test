# ADR-043: Security/Quality Gate での Fail-Closed 原則

## ステータス

試験運用 (2026-06-04)

> 本 ADR は PR #194 で観測した `behind?` (Option<usize>) を使った fail-open bug の根因を一般化し、security/quality gate 関数で Rust の `?` 演算子と早期 return の意味的衝突 (semantic mismatch) を構造的に避けるための設計原則を codify する。`~/.claude/rules/common/security.md` の Mandatory Security Checks の補完層として、判定不能時の挙動を明示する。

## コンテキスト

PR #194 で `src/hooks-pre-tool-validate/src/main.rs` の `check_todo_staleness` 経路に以下のような書き方が含まれていた:

```rust
fn build_todo_staleness_message(
    file_path: &str,
    behind: Option<usize>,
    ...,
) -> Option<String> {
    let stale = behind? > 0;  // ← BAD: None で関数全体が早期 return
    ...
}
```

CodeRabbit Major #5 が指摘した問題:

- `behind` は `count_commits_branch_ahead(branch)` の戻り値で、jj log 失敗 / branch 未取得 / fetch エラー等で `None` になる
- `Option::?` は `None` のとき関数全体を早期 return する Rust の便利 syntax だが、本関数はその時点で **gate を bypass** することになる
- 直感的には「判定不能なら **念のため block (stale=true 扱い)**」がセキュアな選択 (fail-closed)
- しかし `?` で書くと「判定不能なら **OK 扱いで通過**」になり、本来 stale な docs/todo*.md edit が untracked のまま通ってしまう (fail-open)

takt-fix で以下のイディオムに修正:

```rust
let stale = behind.is_none_or(|n| n > 0);  // ← GOOD: None で stale=true (fail-closed)
```

`is_none_or` (Rust 1.82+) は `None` の場合 `true` を返し、`Some(v)` の場合は closure 適用結果を返す。これにより「判定不能 → block」が semantic に揃う。

## 決定

security / quality gate 関数では、以下の **Fail-Closed 原則** を遵守する。

### 原則 1: 判定不能 (None / Err / timeout) はデフォルト blocking

gate 関数の戻り値 (block すべきか / 通過してよいか) を計算する際、入力データが `None` / `Err` / timeout 等で確定不能な場合は、**block 側にデフォルトする**:

| 判定対象 | 入力が確定 | 入力が不確定 |
|---|---|---|
| 「stale か?」 | `Some(n) > 0` で判定 | **stale=true** で扱う |
| 「safe か?」 | 検証 pass / fail で判定 | **safe=false** で扱う |
| 「許可済か?」 | allow-list lookup | **不許可** で扱う |

### 原則 2: Rust idiom — `Option::?` は gate 関数で禁止

`Option<T>::?` は `None` で関数全体を早期 return する。これは gate 関数の semantics と衝突する (None = bypass = fail-open):

```rust
// BAD: fail-open
let stale = behind? > 0;  // None → 関数全体 return → gate bypass

// GOOD: fail-closed
let stale = behind.is_none_or(|n| n > 0);

// GOOD: fail-closed (代替)
let stale = behind.map_or(true, |n| n > 0);
```

`is_none_or` は Rust 1.82 で stabilize (`std::option`)。1.82 未満の MSRV では `map_or(true, ...)` を使う。`unwrap_or(0)` 系は「`None` を `0` と扱う」= 「不確定を OK と扱う」ため gate には不適 (PR #194 同型の fail-open)。

### 原則 3: 反例 — gate 関数で禁止される pattern

以下は全て fail-open になるため、gate 関数では使ってはいけない:

```rust
// BAD 1: ? early-return
fn is_stale(behind: Option<usize>) -> Option<bool> {
    Some(behind? > 0)  // None で None 返却 = caller は gate を skip
}

// BAD 2: unwrap_or(0) で確定値化
fn is_stale(behind: Option<usize>) -> bool {
    behind.unwrap_or(0) > 0  // None で 0 扱い = fail-open
}

// BAD 3: if let Some/else { false }
fn is_stale(behind: Option<usize>) -> bool {
    if let Some(b) = behind { b > 0 } else { false }
    // None で false = fail-open
}
```

正しい代替:

```rust
// GOOD
fn is_stale(behind: Option<usize>) -> bool {
    behind.is_none_or(|n| n > 0)
}
```

### 原則 4: 適用範囲

本原則は以下のような gate 関数群に適用される:

- `hooks-pre-tool-validate` の各 staleness / matching / safety check
- `hooks-stop-quality` の test 結果集約
- `cli-push-runner` の各 stage gate (lint / clippy / test 結果判定)
- `cli-pr-monitor` の retry / circuit breaker 判断
- 一般に「block / allow を決める」関数全般

ただし **non-gate な計算関数** (純粋に数値を計算 / 表示用文字列を作る等) は本原則の対象外。`?` は通常通り使ってよい。

## 反例の判別ヒント

関数が gate 関数か non-gate 関数かは、以下の質問で判別する:

1. 戻り値が「block / allow」「stale / fresh」「safe / unsafe」等の二値判断か?
2. 戻り値が `Some(message)` のときに caller が action を取る (block 表示など) か?
3. 戻り値が `None` だと caller は「何もせず通過」するか?

3 が yes なら gate 関数 → 本原則を適用。

## 実装事例

PR #194 commit `dfad56ff` で `build_todo_staleness_message` 内の `let stale = behind.is_none_or(|n| n > 0);` 修正が実装。test は PR #194 T2-#1 (`build_todo_staleness_message_returns_some_when_behind_is_none` / `build_todo_staleness_message_behind_none_with_matches_includes_both_sections`) で fail-closed contract を検証。

## 試験運用判断基準

本 ADR は試験運用とする。3 つ以上の独立 gate 関数で本原則を適用し、同型 fail-open bug が再発しないか観測。

- 観測点: `hooks-pre-tool-validate` / `hooks-stop-quality` / `cli-push-runner` stages の各 gate 関数
- 期間: 2026-06-04 から最低 3 PR の review
- 本採用判断: 3 PR の review で fail-open 指摘が CR / reviewer から再発しなければ stable 昇格、再発があれば本 ADR の不足を分析して原則追加

## 参照

- PR #194 (`feat(hooks): merge 前 mechanical gate 強化 (clippy + 空 commit sweep)`) commit `dfad56ff`: `behind?` → `is_none_or` 修正
- CodeRabbit Major #5 (PR #194 review): 「security gate は判定不能時 fail-closed であるべき」
- ADR-021 (`jj 変更検出ロジックの設計原則`) § Revset Composability: jj 操作の fail-safe 方向との対比
- `~/.claude/rules/common/security.md` § Mandatory Security Checks: 本 ADR が補完する global checklist
- Rust 公式 doc: [`Option::is_none_or`](https://doc.rust-lang.org/std/option/enum.Option.html#method.is_none_or) (1.82+ stable)
