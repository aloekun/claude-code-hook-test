# ADR-008: Push Pipeline ハーネスの実装

## ステータス

承認済み (2026-03-30)

## コンテキスト

VCS に Jujutsu (jj) を採用しているプロジェクトで、push 前に「テスト通過」と「ローカルレビュー」を強制したい。
Git には `pre-push` フックがあり、Lefthook のような OSS でパイプライン的に管理できるが、Claude Code hooks には push イベントが存在しない。

### 検討した選択肢

1. **Claude Code の PreToolUse/PostToolUse/Stop のいずれかに push パイプラインを組み込む**
   - Stop hook に push を組み込む案: Stop は「応答終了時」に発火するため、push のタイミングと合わない
   - PostToolUse に組み込む案: PostToolUse は個別のツール実行後に発火するため、パイプライン全体の制御に不向き
   - → いずれも Claude Code hooks のイベントモデルと push パイプラインの性質が合致しない

2. **PreToolUse でブロック + スタンドアロン exe で パイプラインを実行**
   - PreToolUse で `jj git push` をブロックし、「`pnpm push` を使え」と誘導
   - `pnpm push` がスタンドアロン exe (hooks-push-pipeline) を呼び出し、テスト → レビュー → push を順次実行
   - → Claude Code hooks の制約内で push パイプラインを実現可能

3. **Skill (`/push`) として実装**
   - Claude Code の Skill 機構を使い、`/push` コマンドとして実装
   - AI ステップ（レビュー、コミット整理）との親和性が高い
   - → パイプラインのハーネス部分とは独立に検討可能。将来的に Skill が exe を呼び出す形での統合もありうる

## 決定

**選択肢 2 を採用する。** PreToolUse の `jj-push-guard` プリセットで直接の push をブロックし、`hooks-push-pipeline` (スタンドアロン Rust exe) で push 前パイプラインを実行する。

### アーキテクチャ

```text
Claude が "jj git push" を実行しようとする
       │
       ▼
PreToolUse (hooks-pre-tool-validate)
  ├─ "jj-push-guard" プリセットでブロック
  └─ エラーメッセージ: 「pnpm push を使用してください」
       │
       ▼
Claude が "pnpm push" を実行する
       │
       ▼
hooks-push-pipeline.exe (スタンドアロン)
  ├─ hooks-config.toml [push_pipeline] を読み込み
  ├─ command 型ステップを順次実行
  ├─ ai 型ステップは現在スキップ (将来実装)
  ├─ 全 command ステップ成功 → push_cmd を実行
  └─ 失敗 → エラー出力 (exit code 1)
```

### Claude Code hooks プロトコルとの違い

| | Claude Code hooks (Pre/Post/Stop) | hooks-push-pipeline |
|---|---|---|
| 起動方法 | Claude Code が自動的に呼び出す | `pnpm push` から手動/Claude 経由で呼び出す |
| 入力 | stdin に JSON | なし (hooks-config.toml から設定読み込み) |
| 出力 | stdout に JSON (`decision`, `reason`) | stderr にログ出力 |
| 終了コード | hooks により意味が異なる | 0 = 成功, 1 = 失敗, 2 = 設定エラー |

### ステップタイプ

- `type = "command"`: シェルコマンドを実行。exit code 0 で成功判定。失敗時はパイプライン中断。
- `type = "ai"`: AI 処理が必要なステップ。現在は placeholder としてスキップ。将来的に Skill 統合や Claude API 呼び出しで実装予定。

### 前提条件

- push は Claude 経由でのみ行う（ユーザーが手動でターミナルから push することは想定しない）
- ハードブロック: パイプラインを通さない push は技術的に不可能にする

## 影響

### Positive

- Claude Code hooks に push イベントがない制約下で、事実上の push hook を実現できる
- 既存のビルド・配布フロー (`pnpm build:hooks`, `pnpm deploy:hooks`) にそのまま乗る
- `hooks-config.toml` の `[push_pipeline]` セクションで、プロジェクトごとにステップをカスタマイズできる
- `type = "ai"` ステップの導入により、将来の AI レビュー・コミット整理統合への拡張ポイントが確保されている

### Negative

- PreToolUse ブロックは Claude 経由の push にのみ有効。ユーザーが直接ターミナルから `jj git push` を叩いた場合はバイパスされる（前提条件により許容）
- `hooks-push-pipeline` は Claude Code hooks プロトコルに準拠しないスタンドアロン exe であり、hooks 群の中で唯一の例外的な存在になる
- `run_step()` / `drain_pipe()` ロジックが `hooks-stop-quality` と重複する（ADR-003 の独立 Cargo プロジェクト方針に従い、共通クレート化は見送り）

### 将来の検討事項

- **AI ステップの実装**: Skill `/push` との統合、または exe 内から Claude API を呼び出す方式
- **グローバル設定**: `~/.claude/push_pipeline.toml` のような共通設定と、プロジェクトローカル設定のマージ機構
- **共通クレート化**: hooks 間で重複するユーティリティ (`run_step`, `drain_pipe`, `config_path` 等) の共通化

## 参考

- ADR-001 — hooks の実装言語として Rust を採用
- ADR-003 — hooks の配置規則とビルド戦略
- ADR-006 — hooks の設定駆動型アーキテクチャ
- [Lefthook](https://github.com/evilmartians/lefthook) — Git hooks マネージャー（今回の設計の着想元）
