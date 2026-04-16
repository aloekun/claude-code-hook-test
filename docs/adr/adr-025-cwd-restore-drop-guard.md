# ADR-025 (仮): CwdRestore Drop guard パターン

## ステータス

試験運用 (観察開始: 2026-04-17)

> 本 ADR は試験運用ステータス。正式採用は他パッケージの統合テストでの使用例出現による。

## コンテキスト

### 問題

PR #44 の統合テスト (`src/cli-pr-monitor/src/stages/repush.rs::tests::integration_noop_takt_does_not_trigger_push_and_preserves_description`) は以下のような構造だった:

```rust
let original_cwd = env::current_dir().expect("cwd 取得失敗");
env::set_current_dir(repo_dir).expect("cd 失敗");
// ... アサート群 ...
env::set_current_dir(&original_cwd).ok();  // panic すると実行されない
```

CodeRabbit Minor 指摘: `assert!` が途中で失敗すると cwd が `repo_dir` のまま残り、**同プロセスの後続テストに影響**する。`#[ignore]` + `--test-threads=1` で他テストと分離していても、複数の `#[ignore]` テストを追加した場合に問題が顕在化する。

### 修正: RAII Drop guard

PR #44 で以下の Drop guard を導入:

```rust
struct CwdRestore {
    original: std::path::PathBuf,
}

impl Drop for CwdRestore {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.original);
    }
}

// 使用箇所
let original_cwd = env::current_dir().expect("cwd 取得失敗");
env::set_current_dir(repo_dir).expect("cd 失敗");
let _cwd_guard = CwdRestore { original: original_cwd };
// アサート群 (panic しても Drop で cwd 復元)
```

### 他パッケージへの適用可能性

現時点で使用箇所は cli-pr-monitor の 1 箇所のみ。将来以下のような場面で再利用可能性あり:

- cli-merge-pipeline の統合テスト (merge flow を dummy repo で検証する際)
- cli-push-runner の統合テスト (push flow を dummy repo で検証する際)
- 他 Rust パッケージで cwd 依存テストが必要になったとき

## 決定 (試験運用方針)

### 観察期間

2026-04-17 ～ 2026-07-31 (約 3.5 ヶ月、ADR-023 / ADR-024 と同期)。

### 観察対象

- cli-pr-monitor 以外のパッケージで cwd 依存テストが発生するか
- 発生時に「コピペで同じ struct を書く」「共通モジュール化する」のどれが自然か

### 正式採用条件 (2026-07-31 再評価)

| 他パッケージでの使用例 | アクション |
|----------------------|----------|
| 2 つ目の使用例が出現 | 正式採用 → `src/lib-test-helpers/` 新設 (dev-deps として共有) |
| 1 つだけ (cli-pr-monitor のみ) | ADR 廃止 (repush.rs 内に留める) |
| 使用例なしだが明確な計画あり | 延長 (半年) |

### 正式採用時の候補構成

```text
src/lib-test-helpers/
  ├── Cargo.toml       (特に dev-dep 用の設定)
  └── src/
      └── lib.rs       ← CwdRestore / TempJjRepo (仮) / その他 RAII ガード
```

使用側は以下のように依存:

```toml
[dev-dependencies]
lib-test-helpers = { path = "../lib-test-helpers" }
```

ADR-026 (予定) の Cargo workspace 化 を前提とする。

## 影響

### 試験運用中の運用

- `CwdRestore` は `src/cli-pr-monitor/src/stages/repush.rs` の test module に留める (現状維持)
- 他パッケージで同様のガードが必要になったら:
  1. まずコピペで実装 (観察のため)
  2. 2 箇所目が出たタイミングで本 ADR の正式採用を検討
  3. 正式採用なら `src/lib-test-helpers/` に集約

### 参照する他 ADR

- ADR-012 (src/ ディレクトリの命名規約): `lib-*` prefix に従う
- ADR-026 (予定): Cargo workspace 化により cross-package dev-dep が自然になる

## 次ステップ (試験運用中に確認すること)

- cli-merge-pipeline の post_steps 実装時、テストに cwd 操作が必要か確認
- integration テストのテンプレート (dummy jj repo, mock takt, etc.) が出現するか観察

## 観察終了条件

- 2026-07-31 時点で使用例を再評価
- 正式採用 / 延長 / 廃止 のいずれかを選択し、本 ADR の status を更新
