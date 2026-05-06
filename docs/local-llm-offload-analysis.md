# ローカル LLM オフロード可能性調査

> **状態**: 試験運用 (本ドキュメントは「調査レポート」であり、提案が ADR 化または却下された時点で役割を終える)
>
> **引退条件**: 以下のいずれかを満たした時点で本ファイルを削除する。
> - ADR 化された (例: "ADR-038: ローカル LLM オフロード戦略") 場合 → ADR にエッセンスを移し、本ファイルを削除
> - 提案が却下された場合 → 却下理由を ADR または commit message に残し、本ファイルを削除
> - 6 ヶ月経過しても採否が決まらない場合 → 採用見込みなしとみなして削除
>
> **由来**: セッション ID `5ca01479-6d71-4328-91d0-861343120c3f` (2026-05-05〜2026-05-06, Bundle b 関連) の作業ログ分析

## 1. 背景・目的

Claude Code のトークン消費・レートリミットを抑える目的で、現開発環境のどの工程をローカル LLM (Ollama) に置き換えられるかを評価する。

### 環境前提

- GPU: NVIDIA GeForce GTX 3070 (VRAM 8GB)
- ランタイム: Ollama
- インストール済みモデル: `llama2:13b`, `mistral:7b`
- 既存の自動化基盤: takt (push-runner) + Rust hooks + cli-pr-monitor + 各種 skill

### 設計指針 (3 層構造)

| 層 | 役割 | 担当 |
|---|---|---|
| ① 思考層 | バグ原因特定、アーキテクチャ設計、不確実性の高い判断、要件解釈 | Claude Code |
| ② 実行層 | 設計済みコード生成、定型リファクタリング、整形、diff 要約、指摘分類、コメント構造化、lint 的レビュー | ローカル LLM |
| ③ 制御層 | 振り分けロジック、I/O 整形 | scripts (takt facet, Rust hooks, cli-pr-monitor 拡張) |

本リポジトリでは ③ が既に整備されているため、新規アーキテクチャを書かずに「facet 単位で model を差し替える」だけで段階導入可能。

## 2. セッション分析サマリ

### 対象セッション

- 期間: 2026-05-05 04:35 UTC 〜 2026-05-06 08:49 UTC (約 28 時間)
- 内容: CodeRabbit rate-limit 自動回復機能 (Bundle b) の Rust 実装、PR #113〜#118 の land サイクル
- Tool calls: 535 回

### Tool 使用ボリューム

| Tool | 呼び出し回数 | 比率 | 主用途 |
|---|---|---|---|
| Bash | 218 | 40.7% | うち `cd` が 199 回 (LLM 仕事ではない)、cargo / gh / jj / grep が実質 19 回 |
| Edit | 127 | 23.7% | `poll.rs` / `state.rs` / `config.rs` の段階修正、`todo*.md` 更新 |
| Read | 85 | 15.9% | 動的コンテキスト取得 |
| TodoWrite | 55 | 10.3% | タスク追跡 |
| Grep | 26 | 4.9% | パターン検索 |
| その他 | 24 | 4.5% | CronCreate, Write, ToolSearch, ScheduleWakeup, Glob |

> 注: Bash 218 回のうち 91% は単なる `cd` で LLM 推論ではないため、「オフロード可能率」の分母には含めない。実質オフロード対象は **Edit / Grep / 一部の assistant message 生成** に絞るのが妥当。

## 3. オフロード候補

### ② 実行層 → mistral:7b 推奨 (常駐)

| 工程 | セッション内の発生箇所 | 統合先 (案) |
|---|---|---|
| **A. CodeRabbit findings の severity 分類 / resolved 判定** | 各 PR review サイクル (#113〜#118) で Major/Minor 振り分け | `cli-pr-monitor` の post-pr-monitor フェーズに subcommand 追加 |
| **B. diff 要約 / commit message draft** | `jj describe` 前の文章生成、PR body draft | `prepare-pr` skill / takt facet |
| **C. 定型 lint 的指摘の一次フィルタ** | unused import, style 指摘, 軽微な命名 | takt の新 facet `ollama-lint-screen` |
| **D. 長い tool_result の要点抽出** | `gh pr view --json` の生データ要約、log sift | cli-pr-monitor の前処理層 |

### ② 実行層 → llama2:13b 推奨 (on-demand)

| 工程 | 発生箇所 |
|---|---|
| **E. Rust docstring / コメント生成** | `poll.rs` / `state.rs` の関数コメント追加 |
| **F. ADR / test skeleton 生成** | 新規 ADR の boilerplate, 関数 signature → 空実装 |

### ① 思考層 → Claude を残すべき箇所

- **Bundle 分割の判断**: 例) Bb-1 / Bb-2 を 3 PR に分ける、依存関係の解釈 (多角的 cost-benefit が必要)
- **CronCreate API の設計選定**: 例) durable vs session-only, ADR-018 整合性 (既存設計との conflict 検出が必要)
- **conflict resolution 戦略**: 例) jj abandon vs marker 手動編集 (規模判定 + リカバリ戦略)
- **root cause 分析**: 例) CI failure 解釈, security scan signature 推断

### ③ 制御層 → 既存基盤の拡張

- 既存の takt facets / cli-pr-monitor / Rust hooks に「Ollama 呼び出し facet」を追加するだけで振り分け層が完成
- 新規アーキテクチャは不要

## 4. 統合提案 (最小工数順)

### 提案 1: takt の新 facet `ollama-lint-screen` 追加

- 工数見積: 1〜2 日
- 内容: pre-push 時に diff を mistral:7b に流し、`unused / import / style` の一次フィルタを行う
- 既存の Rust hooks (oxlint/biome) は決定論的、ollama は「ニュアンス的指摘」担当という棲み分け
- 効果: Edit の繰り返しを Claude に投げる前に圧縮できる

### 提案 2: `cli-pr-monitor` に CodeRabbit findings 分類サブコマンド

- 工数見積: 1〜2 日
- 入力: `gh api .../comments --jq` の生 finding
- 出力: `{severity, resolved_likely, suggested_action}` の構造化 JSON
- mistral:7b の structured output モードを利用
- 効果: 本セッションで反復した「Major/Minor 振り分け」を Claude から外せる
- **ROI 最大候補** — 反復工程そのものが対象、scope も単一サブコマンドで閉じる

### 提案 3: `prepare-pr` skill に commit/PR body draft の Ollama 前処理

- 工数見積: 半日
- 内容: `jj diff` を mistral:7b で要約 → Claude には「要約 + 必要部分」だけ渡す
- 効果: PR description 生成時の input token を圧縮

## 5. 期待効果

### 削減見込み (控えめ評価)

| 領域 | 削減見込み |
|---|---|
| Edit 関連 (lint screen + 軽微修正) | 127 回中 15〜20 回程度の token を 7b に逃せる |
| CodeRabbit triage | PR あたり 5〜10 件 → 1 セッション 30〜50 件相当を 7b で処理 |
| diff/log 圧縮 | 大きな tool_result が context に乗る前に 1/5〜1/10 に要約可能 (cache 倍率 9x で見えづらいが最大効果) |

体感ベースでは **Claude session の入力 token 15〜25% 削減** が現実ライン。

### 効果の本質

「コスト削減」ではなく **「Claude をボトルネック工程に限定することでレートリミットから解放される」** こと。Claude 80% / ローカル 20% ではなく、**ローカル 80% / Claude 20%** が理想形。

## 6. 注意点・現実ライン

### 技術的制約

- mistral:7b の structured output は llama.cpp / Ollama の最新版でないと不安定 → JSON schema 強制を要確認
- ローカル LLM の幻覚は「失敗してもリトライ可能 / Claude が後段で検証」前提なら許容範囲
- VRAM 8GB なので、`mistral:7b` (4bit 量子化) 常駐 + `llama2:13b` on-demand swap が安全策

### 適用しない領域

- 永続的な意思決定 (ADR 起案、設計選定)
- セキュリティ判定 (false negative が事故になる領域)
- 不確実性の解釈が必要な分析 (root cause, conflict resolution の戦略決定)

## 7. Next Steps

提案を採用する場合の推奨着手順序:

1. **提案 2 (CodeRabbit findings 分類) から着手** — ROI 最大、scope が閉じている、効果実測しやすい
2. 効果が確認できれば **提案 1 (lint screen facet)** を追加
3. 最後に **提案 3 (PR body draft)** を導入

却下する場合: 本ファイルを削除し、却下理由を commit message または専用 ADR に残す。

## 関連リンク

- [ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止](adr/adr-018-pr-monitor-takt-migration.md)
- [ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略](adr/adr-020-takt-facets-sharing.md)
- [ADR-034: CodeRabbit 監視・対話の自動化戦略 — Bundle a 設計根拠](adr/adr-034-coderabbit-auto-monitoring.md)
