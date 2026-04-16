# ADR-026: Cargo workspace による Rust パッケージ統合

## ステータス

承認済み (2026-04-17)

## コンテキスト

### 問題

本プロジェクトには 10 の Rust パッケージ (`src/check-ci-coderabbit/`, `src/cli-*/`, `src/hooks-*/`, `src/lib-report-formatter/`) が存在するが、各パッケージが独立した `Cargo.toml` を持ち、workspace 化されていなかった。この構成では以下の不便が発生していた:

1. **`cargo test` が repo ルートで動かない**: 各パッケージで `cargo test --manifest-path src/<package>/Cargo.toml` のように個別指定が必要。PR #44 で push pipeline に rust-test group を追加したとき、この個別指定が必要になった
2. **target/ ディレクトリが package ごとに分散**: 10 個の `src/<pkg>/target/` が独立して生成され、compile cache が共有されない。依存する lib-report-formatter のコンパイル結果も再利用されない
3. **profile.release が各パッケージで重複定義**: 9 個のパッケージに同じ `[profile.release]` ブロック (opt-level=3, lto=true, strip=true) が書かれていた

### Rust workspace 化の効果

Cargo workspace を導入すると:

- `cargo test` を repo ルート 1 行で全パッケージ実行
- `cargo test -- --ignored --test-threads=1` で統合テストも同様に 1 行
- `target/` が workspace 共通化され、compile cache 再利用でビルド時間短縮 (特に lib-report-formatter に依存する複数パッケージ)
- `[profile.release]` を workspace root に集約でき、重複排除
- 将来の `[workspace.dependencies]` (共通依存バージョン管理) への拡張余地

## 決定

### workspace 構成 (最小構成)

`Cargo.toml` (repo ルート、新規) に以下を定義:

```toml
[workspace]
resolver = "2"
members = [
    "src/check-ci-coderabbit",
    "src/cli-merge-pipeline",
    "src/cli-pr-monitor",
    "src/cli-push-pipeline",
    "src/cli-push-runner",
    "src/hooks-post-tool-linter",
    "src/hooks-pre-tool-validate",
    "src/hooks-session-start",
    "src/hooks-stop-quality",
    "src/lib-report-formatter",
]

[profile.release]
opt-level = 3
lto = true
strip = true
```

### 設計原則

1. **minimal workspace を先に**: `[workspace.dependencies]` (共通依存バージョン管理) や `[workspace.package]` (共通 metadata) の導入は**本 ADR のスコープ外**。実利が見えてから別 PR で対応 (YAGNI)
2. **`[profile.release]` は root に集約**: workspace では member の profile 設定が ignore されるため、root で一元管理する必要がある (Cargo の仕様)
3. **target/ は workspace 共通化**: `.gitignore` で `/target/` (root) と `src/*/target/` (legacy 互換) の両方を ignore
4. **package.json の build スクリプト変更**: `cargo build --release -p <name>` に統一。target path は `target/release/<name>.exe` (workspace 直下)
5. **push pipeline の rust-test group を workspace ベースに**: `cargo test --manifest-path ...` の個別指定を廃止し、`cargo test` 1 行に

### 依存の変更

既存 package 間の依存 (例: cli-pr-monitor → lib-report-formatter) は引き続き `path = "../lib-report-formatter"` で記述する。workspace 化により path dependency が暗黙的に解決されるため、実質的な変更はない。

## 影響

### 採用される構成要素

- `Cargo.toml` (repo ルート): `[workspace]` + members + `[profile.release]`
- `.gitignore`: `/target/` を追加
- `package.json`: 8 個の `build:<name>` スクリプトを `cargo build --release -p <name>` 形式に統一
- `push-runner-config.toml`: rust-test group の command を `cargo test` / `cargo test -- --ignored --test-threads=1` に簡素化
- `templates/push-runner-config.toml`: rust-test group のコメントアウト済みテンプレートを追加 (Rust を使う派生プロジェクトで有効化可能に)

### 避けるべきアンチパターン

- **Member の `[profile.release]` 併存**: workspace では ignore されるため、警告の原因になる。root に集約する
- **各 package で個別 `--manifest-path` 指定**: workspace 化後は不要。誤って個別指定すると target/ の共有が無効化される可能性
- **`cargo build --release` を member dir で実行する旧スタイル**: target path が `src/<pkg>/target/` ではなく `target/` (workspace root) に変わっているため、build 後の `cp` でパスを更新する必要がある

### 削除された構成要素

- 各 member Cargo.toml の `[profile.release]` ブロック (9 箇所): コメントで「workspace root に集約」と記録

### package.json の変更例

Before:
```json
"build:hooks-pre-tool-validate": "cd src/hooks-pre-tool-validate && cargo build --release && cp target/release/hooks-pre-tool-validate.exe ../../.claude/hooks-pre-tool-validate.exe"
```

After:
```json
"build:hooks-pre-tool-validate": "cargo build --release -p hooks-pre-tool-validate && cp target/release/hooks-pre-tool-validate.exe .claude/hooks-pre-tool-validate.exe"
```

`cd` が不要になり、target path が workspace root 基準に統一される。

## 次ステップ (スコープ外)

- **`[workspace.dependencies]` への集約**: serde / toml / regex などの重複依存を集約できる。実利が見えたら別 PR で実施
- **`[workspace.package]` への version / edition 集約**: 現状すべて `version = "0.1.0"` / `edition = "2021"` で統一されているので、集約しても見た目のみの変化。YAGNI で保留
- **cli-push-pipeline の deprecation**: ADR-015 で cli-push-runner (takt ベース) に移行済み。cli-push-pipeline は dead code だが本 ADR の scope 外。削除は別 PR
- **ADR-024 (仮) の正式採用**: 2 つ目の jj ヘルパー使用例が出たら `src/lib-jj-helpers/` を workspace member として新設 (workspace 化により追加が容易に)
- **ADR-025 (仮) の正式採用**: 2 つ目の cwd 依存テストが出たら `src/lib-test-helpers/` を workspace member として新設 (同上)
