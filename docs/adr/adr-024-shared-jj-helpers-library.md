# ADR-024 (仮): 共通 jj ヘルパーライブラリ

## ステータス

試験運用 (観察開始: 2026-04-17)

> 本 ADR は試験運用ステータス。正式採用は他パッケージでの使用例出現による。

## コンテキスト

### 問題

PR #44 で `src/cli-pr-monitor/src/runner.rs` に以下の jj ヘルパーを追加した:

```rust
pub(crate) fn capture_commit_id() -> Option<String> { ... }
pub(crate) fn diff_is_empty(from: &str, to: &str) -> bool { ... }
```

現状は `pub(crate)` (cli-pr-monitor 内からのみ呼び出し)。正式採用時に package を跨いで共有するとなった段階で `pub` に引き上げる。

これらは ADR-021 の「jj 変更検出の二段構え判定」を実装する基盤ヘルパーで、cli-pr-monitor 固有のロジックではない。将来以下のような場面で同じ関数が必要になる可能性がある:

- **cli-merge-pipeline の post_steps 実装** (ADR-013 + ADR-014): merge 後の AI ステップで「merge 後に変化があったか」を検出
- **cli-push-runner の bookmark 自動化** (feat/push-runner-auto-bookmark): push 前に @ が変わったか検出
- **その他の jj 連携 CLI**: 将来の拡張

### 早期の共通化はリスク

現時点で `src/lib-jj-helpers/` を新設して 2 関数を移すのは YAGNI に反する:

- 1 つの呼び出し元しか存在しない
- API 設計 (関数シグネチャ、エラー型、timeout 値、log 出力方法) の安定性が読めない
- 2 つ目の呼び出し例が出て初めて「共通部分」と「固有部分」を分離できる

## 決定 (試験運用方針)

### 観察期間

2026-04-17 ～ 2026-07-31 (約 3.5 ヶ月、ADR-023 と同期)。

### 観察対象

- cli-pr-monitor 以外の Rust パッケージで `capture_commit_id` / `diff_is_empty` 相当の関数を使いたい場面が出現するか
- 出現時に「cli-pr-monitor の関数をそのまま呼ぶ」「コピペで再実装」「共通ライブラリ化」のどれが自然か

### 正式採用条件 (2026-07-31 再評価)

| 他パッケージでの使用例 | アクション |
|----------------------|----------|
| 2 つ目の使用例が出現 | 正式採用 → `src/lib-jj-helpers/` 新設 |
| 1 つだけ (cli-pr-monitor のみ) | ADR 廃止 (cli-pr-monitor 内に留める) |
| 使用例なしだが明確な計画あり | 延長 (半年) |

### 正式採用時の候補構成

```text
src/lib-jj-helpers/
  ├── Cargo.toml
  └── src/
      └── lib.rs  ← capture_commit_id / diff_is_empty / その他共通 jj ラッパー
```

- 既存 `src/lib-report-formatter/` と同階層 (ADR-012 の命名規約 `lib-*`)
- 依存元パッケージは workspace の member として参照 (ADR-026 (予定) の Cargo workspace 化を前提)

## 影響

### 試験運用中の運用

- cli-pr-monitor の `runner.rs` に `capture_commit_id` / `diff_is_empty` を持つ (現状維持)
- 他パッケージで同機能が必要になったら:
  1. まず「cli-pr-monitor の関数を pub 化して参照」を試す
  2. Cargo workspace 化 (ADR-026 (予定)) 後なら cross-package dependency で呼び出せる
  3. 2 箇所以上で必要になったらライブラリ化を本 ADR の正式採用として検討

### 参照する他 ADR

- ADR-012 (src/ ディレクトリの命名規約): `lib-*` prefix に従う
- ADR-021 (jj 変更検出): 本 ADR のヘルパーが実装する原則
- ADR-026 (予定): Cargo workspace 化が先行する前提

## 次ステップ (試験運用中に確認すること)

- cli-merge-pipeline の post_steps 実装 (現 docs/todo.md #3) が始まったときに使用を検討
- cli-push-runner の bookmark 自動化再開時も同様
- 使用パターンを 2 件観察してから共通化すれば、過度に汎用化せずに済む

## 観察終了条件

- 2026-07-31 時点で使用例を再評価
- 正式採用 / 延長 / 廃止 のいずれかを選択し、本 ADR の status を更新
