# ADR-009: Post-PR Monitor — push/PR作成後の CI・CodeRabbit 自動監視

## ステータス

承認済み (2026-04-01)

## コンテキスト

public リポジトリでは PR 作成や push 後に CodeRabbit による自動レビューが行われるが、
レビュー完了までに 3〜5 分程度かかるため、手動で待機・確認するのは摩擦が大きい。
同様に CI (GitHub Actions) の成否も push 後にしか確認できない。

これらを自動的に監視し、結果を Claude 経由で報告する仕組みが必要。

### 参考にした既存実装

- shomatan/cc-knowledge の `post-push-monitor.sh` + `check-ci-coderabbit.sh`
  - PostToolUse hook (bash) で `gh pr create` / `git push` を検出
  - CronCreate で定期ポーリング、gh API で CI・CodeRabbit 状態を取得
  - 構造化 JSON で判定結果を返す

ただし、シェルスクリプトは Windows 環境で直接実行できず、テスト可能性も低いため、
本プロジェクトの方針（ADR-001: Rust 実装）に従い Rust exe で再実装する。

### 検討した選択肢

1. **シェルスクリプトをそのまま採用し、Git Bash / WSL で実行**
   - Windows 環境依存が増える。テスト可能性が低い。

2. **Skill 内で Claude が gh コマンドを直接実行**
   - テスト不要だが、判定ロジックが AI 依存になり安定性に欠ける。

3. **Rust exe × 2 (hook トリガー + スタンドアロン checker) + Skill + CronCreate**
   - 判定ロジックが確定的。unit test で網羅可能。Windows ネイティブ。
   - CronCreate で定期ポーリング、Claude はスクリプトの出力を解釈するだけ。

## 決定

**選択肢 3 を採用する。**

### アーキテクチャ

```text
Claude が "gh pr create" / "git push" を実行
       │
       ▼
PostToolUse hook (hooks-post-pr-monitor.exe)
  ├─ コマンド検出 (regex)
  ├─ PR番号・リポジトリ情報を gh CLI で取得
  ├─ PUSH_TIME を記録 (UTC ISO 8601)
  └─ additionalContext で CronCreate 指示を返す
       │
       ▼
Claude が CronCreate で定期ジョブ作成 (30秒間隔)
       │
       ▼ (定期実行)
check-ci-coderabbit.exe --push-time <T> --repo <R> --pr <N>
  ├─ CI 状態チェック (gh run list)
  ├─ CodeRabbit 状態チェック (gh api .../statuses)
  ├─ 新規コメント取得 (gh api .../comments, created_at > push_time)
  ├─ レビュー本文クロスチェック (Actionable comments posted: N)
  ├─ 未解決スレッド (gh api graphql)
  └─ 判定 JSON を stdout 出力
       │
       ▼
Claude が action フィールドに従い行動
```

### コンポーネント

| コンポーネント | 種別 | 役割 |
|---|---|---|
| `hooks-post-pr-monitor.exe` | PostToolUse hook (Rust) | コマンド検出 → CronCreate 指示 |
| `check-ci-coderabbit.exe` | スタンドアロン CLI (Rust) | CI・CodeRabbit 状態チェック → JSON 出力 |
| `post-pr-create-review-check` SKILL.md | Claude Skill | 監視結果の解釈・報告手順 |
| `hooks-config.toml [post_pr_monitor]` | 設定 | ポーリング間隔・監視対象の設定 |

### 判定ロジック

| CI 状態 | CodeRabbit 状態 | 指摘/未解決 | action |
|---------|----------------|------------|--------|
| pending | * | * | `continue_monitoring` |
| * | pending/not_found | * | `continue_monitoring` |
| failure | * | * | `stop_monitoring_failure` |
| * | failure/error | * | `stop_monitoring_failure` |
| success | success | あり | `action_required` |
| success | success | なし | `stop_monitoring_success` |

### 新規指摘の判定

**`created_at > PUSH_TIME` でフィルタ。`commit_id == HEAD` は使わない。**

理由: fix コミット push で HEAD が変わるが、CodeRabbit コメントは前コミットに紐づいたまま。
`commit_id` フィルタでは前コミットの指摘が全て見落とされる。

### 設定 (hooks-config.toml)

```toml
[post_pr_monitor]
enabled = true
poll_interval_secs = 30
max_duration_secs = 600
check_ci = true
check_coderabbit = true
```

## 影響

### Positive

- push/PR 作成後の CI・CodeRabbit 確認を自動化し、開発フローの摩擦を削減
- 判定ロジックが Rust の純粋関数に分離されており、unit test で網羅可能
- 既存のビルド・配布フロー (`pnpm build:hooks`, `pnpm deploy:hooks`) にそのまま乗る
- `hooks-config.toml` でプロジェクトごとにポーリング間隔・監視対象をカスタマイズ可能

### Negative

- PostToolUse hook が全 Bash コマンドで発火するため、非対象コマンドでの性能影響に注意が必要
  （stdin パース + regex チェックのみで即座に exit するよう設計で対処）
- CronCreate は Claude のセッション内機能であり、セッション終了時にジョブも消える
- `check-ci-coderabbit` は gh CLI に依存するため、gh 未インストール環境では動作しない

## 参考

- ADR-001 — hooks の実装言語として Rust を採用
- ADR-003 — hooks の配置規則とビルド戦略
- ADR-006 — hooks の設定駆動型アーキテクチャ
- ADR-008 — Push Pipeline ハーネスの実装
- [shomatan/cc-knowledge](https://github.com/shomatan/cc-knowledge) — 参考実装 (shell script)
