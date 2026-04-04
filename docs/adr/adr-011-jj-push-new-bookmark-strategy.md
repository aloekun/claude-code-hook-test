# ADR-011: jj の新規ブックマーク push 戦略

## ステータス

承認済み (2026-04-04)

## コンテキスト

Jujutsu (jj) は安全策として、ローカルにしか存在しないブックマークをリモートに新規作成する際に明示的な許可を要求する。
新規ブランチの初回 push 時に以下のエラーが発生し、push パイプライン（テスト → レビュー → push）の再実行が必要になっていた。

```text
Warning: Refusing to create new remote bookmark <name>@origin
Hint: Run `jj bookmark track <name> --remote=origin` and try again.
```

### 問題の発生フロー

```text
pnpm push
  ├─ test → PASS
  ├─ AI review → PASS
  └─ jj git push → ❌ "Refusing to create new remote bookmark"
                    （テスト・レビューは通過済みなのに push だけ失敗）

手動で jj bookmark track ... を実行

pnpm push（2回目 — テスト・レビューを再実行する無駄が発生）
  ├─ test → PASS（2回目）
  ├─ AI review → PASS（2回目）
  └─ jj git push → ✅ 成功
```

### 検討した選択肢

jj が提供する新規ブックマーク push の許可方法は3段階の deprecation 経路がある:

| 方式 | 設定方法 | deprecation 状況 (jj 0.37.0) |
|------|---------|-----|
| `--allow-new` | CLI フラグ | deprecated（警告あり） |
| `git.push-new-bookmarks` | 設定値 | deprecated（警告あり） |
| `remotes.<name>.auto-track-bookmarks` | 設定値 | **現行推奨** |

適用方法の選択肢:

1. **`push_cmd` にフラグ/設定をインライン指定** — `hooks-config.toml` 内で完結するが、deprecated 警告が出る
2. **リポジトリレベル設定** — `jj config set --repo` で設定。`push_cmd` を素の `"jj git push"` に保てる
3. **ユーザーレベル設定** — `jj config set --user` で設定。全リポジトリに適用される

## 決定

**リポジトリレベルで `remotes.origin.auto-track-bookmarks` を設定し、`push_cmd` は素の `"jj git push"` を維持する。**

```bash
# 各リポジトリで初回セットアップ時に実行
jj config set --repo remotes.origin.auto-track-bookmarks '*'
```

```toml
# hooks-config.toml — push_cmd にフラグ不要
[push_pipeline]
push_cmd = "jj git push"
```

選定理由:

- **deprecation 警告ゼロ**: jj 0.37.0 の現行推奨方式に準拠
- **`push_cmd` がクリーン**: フラグやインライン設定が不要で、将来の jj バージョンアップ時の追従が容易
- **push guard との整合性**: PreToolUse の `jj-push-guard` により直接 push はブロックされたままなので、auto-track が悪用されるリスクはない

## 影響

### Positive

- 新規ブランチの初回 push がパイプライン1回で完了する（テスト・レビューの再実行が不要）
- deprecated 警告が出ない
- `hooks-config.toml` の `push_cmd` がシンプルに保たれる

### Negative

- `.jj/repo/config.toml` は git 管理対象外のため、リポジトリの clone 直後に `jj config set --repo` を別途実行する必要がある
- 派生プロジェクトごとに同じ設定が必要（`deploy:hooks` では配布されない）

### 派生プロジェクトでの適用手順

```bash
# 1. hooks をデプロイ（既存フロー）
pnpm deploy:hooks

# 2. jj の新規ブックマーク auto-track を有効化
jj config set --repo remotes.origin.auto-track-bookmarks '*'
```

### 将来の検討事項

- ユーザーレベル設定 (`jj config set --user`) への移行により、リポジトリごとの設定を不要にできる
- `deploy:hooks` スクリプトに `jj config set --repo` の自動実行を組み込む案

## 参考

- [jj ドキュメント — Automatic tracking of bookmarks](https://docs.jj-vcs.dev/latest/config/#automatic-tracking-of-bookmarks)
- ADR-008 — Push Pipeline ハーネスの実装
- ADR-006 — hooks の設定駆動型アーキテクチャ
