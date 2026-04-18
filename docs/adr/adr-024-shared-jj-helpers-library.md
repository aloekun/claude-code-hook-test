# ADR-024: 共通 jj ヘルパーライブラリ

## ステータス

本採用 (2026-04-19、試験運用期間: 2026-04-17 ～ 2026-04-19)

> 観察期間中 (2026-04-17 起点、想定 3.5 ヶ月) の早い段階で正式採用条件 (3 箇所の port 完了) を達成したため繰上げ本採用。実抽出作業は PR-C (`docs/todo.md` #8) で `src/lib-jj-helpers/` を新設して実施する。

## コンテキスト

### 問題

PR #44 で `src/cli-pr-monitor/src/runner.rs` に以下の jj ヘルパーを追加した:

```rust
pub(crate) fn capture_commit_id() -> Option<String> { ... }
pub(crate) fn diff_is_empty(from: &str, to: &str) -> bool { ... }
```

これらは ADR-021 の「jj 変更検出の二段構え判定」を実装する基盤ヘルパーで、cli-pr-monitor 固有のロジックではない。

本 ADR 試験運用時点では 1 箇所のみの使用で、早期の共通化は YAGNI 違反と判断して観察ステータスに留めていた。

### 早期の共通化はリスク

- 1 つの呼び出し元しか存在しない
- API 設計 (関数シグネチャ、エラー型、timeout 値、log 出力方法) の安定性が読めない
- 2 つ目の呼び出し例が出て初めて「共通部分」と「固有部分」を分離できる

## 決定 (本採用)

### 正式採用条件の達成実績

観察期間 2026-04-17 ～ 2026-07-31 を想定していたが、2026-04-19 時点で既に 3 箇所で同パターンが port 済みとなった:

| クレート | ファイル | 導入 PR |
|---|---|---|
| cli-pr-monitor | `src/cli-pr-monitor/src/util.rs` | PR #55 (2026-04-19) |
| cli-merge-pipeline | `src/cli-merge-pipeline/src/main.rs` | PR #54 (2026-04-19) |
| cli-push-runner | `src/cli-push-runner/src/stages/push_jj_bookmark.rs` | PR #50 (2026-04-18) |

試験運用方針で定めた「2 つ目の使用例出現で正式採用」を超えており、さらに ADR-021 原則 5 (bookmark 検出の優先度付き revset + trunk filter) で定数・関数が機械的に追加される見通しが立った。このままだと 4 箇所目以降も重複コピペが増える。

### 採用する構成

```text
src/lib-jj-helpers/
  ├── Cargo.toml
  └── src/
      └── lib.rs
```

公開する API の初期セット:

- ADR-021 原則 1-4 系 (変更検出):
  - `capture_commit_id() -> Option<String>`
  - `diff_is_empty(from: &str, to: &str) -> bool`
- ADR-021 原則 5 系 (bookmark 検出):
  - 定数 `BOOKMARK_SEARCH_REVSETS = ["@", "@-", "@--"]`
  - 定数 `TRUNK_BOOKMARKS = ["main", "master", "trunk", "develop"]`
  - `is_trunk_bookmark(name: &str) -> bool`
  - `parse_bookmark_list_output(stdout: &str) -> Vec<String>`
  - `select_from_revsets(...)` (クロージャ注入型 pure function)
  - `query_bookmarks_at(revset: &str) -> Vec<String>`
  - `get_jj_bookmarks(stderr_mode: StderrMode) -> Vec<String>`

配置は ADR-012 の命名規約 `lib-*` に従い、ADR-026 の Cargo workspace の member として登録する。

### API 設計方針 (PR-C で確定)

呼び出し側 3 クレートで `stderr` ハンドリングと log prefix が異なるため、以下で吸収する:

- **`stderr` ハンドリングは引数化**: `enum StderrMode { Silent, Piped(LogFn) }` で `Stdio::null` 派 (cli-pr-monitor) と `Stdio::piped` + logging 派 (cli-merge-pipeline) を両立
- **`log_info` 注入**: `fn(&str)` クロージャを引数で受ける設計。各クレート固有 prefix (`[post-pr-monitor]` / `[merge-pipeline]` 等) を崩さない
- **fallback 方針**: log 注入設計で詰まった場合は「各クレート固有の薄いラッパー関数を残す」方針で進める (PR-C 段階で判断)

### 移行方針

PR-C (`docs/todo.md` #8) で以下を実施:

1. `src/lib-jj-helpers/` 新設、workspace member 登録
2. 共通定数・関数を移動し `pub` 公開
3. 呼び出し側 3 クレートを差し替え、各 `Cargo.toml` に依存追加
4. unit テストを `lib-jj-helpers` 側に集約、3 クレートの重複テスト削除
5. `cargo test --workspace` / `pnpm build:all` でグリーン確認

## 影響

### 採用される構成要素

- `src/lib-jj-helpers/` (PR-C で新設予定)
- 3 呼び出し側クレート (`cli-pr-monitor` / `cli-merge-pipeline` / `cli-push-runner`) の `Cargo.toml` への依存追加

### 避けるべきアンチパターン

- **4 箇所目のクレートが出現しても個別コピペで対応**: ADR-021 原則 5 の定数・関数が広がる機械的パターンでは保守性が崩壊する
- **`pub(crate)` のまま他クレートから呼び出そうとする**: workspace 依存で解決できるが、所在が不透明になり循環依存の温床になる
- **共通化で過度に汎用化する**: 既存 3 箇所の使い方を超える抽象化は YAGNI 違反。今必要な API だけを公開

### 参照する他 ADR

- ADR-012 (src/ ディレクトリの命名規約): `lib-*` prefix に従う
- ADR-021 (jj 変更検出): 本 ADR のヘルパーが実装する原則 (原則 1-5 すべて)
- ADR-026 (Cargo workspace): workspace member として参照する前提

## 次ステップ (スコープ外、PR-C で実施)

- **PR-C (`docs/todo.md` #8)**: `src/lib-jj-helpers/` 新設と 3 クレート差し替え
- **将来の新規クレート**: jj 連携が必要になったらまず `lib-jj-helpers` を依存に追加することから始める
- **API 拡張**: `jj new` / `jj describe` / `jj bookmark` 系のラッパーは都度検討 (早期汎用化を避ける)
