# ローカル LLM オフロード可能性調査

> **状態**: 試験運用 (調査 → 提案 2 (cli-finding-classifier) は ADR-038 として land 済 / PR #119, 2026-05-06)。提案 1 (lint screen facet) と提案 3 (PR body draft) は未着手。フォローアップ作業の判断材料として保持。
>
> **引退条件**: 以下のいずれかを満たした時点で本ファイルを削除する (docs-governance.md retirement workflow 準拠)。
> - 提案 1〜3 すべてが land または却下された場合 → ADR-038 等にエッセンスを移し、本ファイルを削除
> - 提案 1 / 提案 3 が却下された場合 → 却下理由を本ファイルに記録した上で、ADR-038 関連の learnings を ADR-038 に migrate して本ファイルを削除
> - 6 ヶ月経過しても提案 1 / 3 の採否が決まらない場合 → 採用見込みなしとみなして削除
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

## 7. 実装進捗ログ

### 2026-05-06: 提案 2 (cli-finding-classifier) land — PR #119

#### 完了範囲

- **新 crate `lib-ollama-client`** (8 unit tests pass、ureq blocking + dyn-compatible trait + thiserror)
- **新 crate `cli-finding-classifier`** (10 lib + 6 bin + 1 後追い回帰テスト = 17 unit tests pass、stdin/stdout pipe 可能 CLI)
- **ADR-038 (試験運用)** 起草・land 済
- **package.json `build:all` 統合**、`.claude/cli-finding-classifier.exe` (2.2MB) 配置
- **PR #119**: feat → CR review 3 round → squash merge `9f368a25` (2026-05-06T14:17:29Z, master)

#### 実 Ollama dogfood (commit 前検証)

- 5 件サンプルを mistral:7b で classify
- JSON parse 100%、3.6s/件、全件妥当な classification (Critical state-bug → human_review、stale comment → auto_fix 等)
- `normalized_issue` が一部英語混じり (mistral:7b の Japanese 指示違反、実害小)

#### CR review 経過 (3 round)

| Round | Commit | Findings | 対応 |
|---|---|---|---|
| 1 | `e9a422e7` | Nitpick 3 件 (ADR Cons, fallback_reason, send_json) | 全 3 件適用、`6f12963c` で push |
| 2 | `6f12963c` | Actionable 3 件 (ADR ureq 統一, build_prompt 単一スキャン化, normalized_issue 検証) | A 手動、B/C は takt auto-fix 適用、`dac2e15c` で push |
| 3 | `dac2e15c` | Actionable 1 件 (Duplicate=C strict 化) + 新 1 件 (ADR `confidence` → `action_confidence` 名称統一) | 両方 `resolved:` reply で却下、merge へ |

#### 検証で見えた予期せぬ知見

- **prompt injection リスク**: `{issue}` / `{suggestion}` 連続 `.replace()` で再展開される問題を CR と takt pre-push reviewer が独立に指摘。CR round 2 を契機に single-pass scanner に書き換え (B fix)
- **OllamaError::Parse 誤マッピング bug**: `serde_json::to_value(&body)?` が**リクエスト構築失敗**を**レスポンス解析エラー**に分類していた問題。CR round 1 で発見、`send_json(&body)` 直接渡しで意味論修正
- **takt fix-trust shortcut (ADR-037) が機能**: round 2 push で convergence_verdict による Iter 3 短絡が動作、2 iter 7m54s で APPROVE
- **monitor edge case**: `review_state: "not_found"` (CR 未処理) → findings 空 → takt approved → park スキップで終了する経路がある (post-pr-monitor 改善 follow-up 候補)

#### 2026-05-07: Phase 5 land (PR #120) — cli-pr-monitor / classifier 統合 + Finding C strict 化

##### 完了範囲

- **Phase 5 (A)**: `cli-pr-monitor` poll stage への classifier subprocess 統合 (`ClassifierConfig` / `classifier_runner.rs` / `state.classified_findings` / `enrich_with_classifier`)。default OFF (試験運用)
- **Finding C strict (B)**: `from_llm_output` で `normalized_issue` の改行・80 chars 超を検出して fallback。`NORMALIZED_ISSUE_MAX_CHARS=80` const 化。回帰テスト 3 件追加
- **PR #120**: feat → CR review 1 round (Major 2 件) → takt auto-fix → squash merge `3b6a847a` (2026-05-07T04:04:38 JST、master)
- **CR Major 2 件対応**: try_wait deadlock 修正 (thread + mpsc::channel に書き換え) + stale guard 削除確認 (CR round 2 で resolution 承認)

##### Phase 5 dogfood で観測した cli-pr-monitor robustness 課題 → Bundle f として登録

PR #120 の dogfood で post-pr-monitor の wakeup state 遷移と auto-retry path に複数の edge case を発見。本 doc の retirement 直接条件ではないが、ローカル LLM dogfood の副産物として monitor 堅牢化を進める Bundle f を [docs/todo.md](todo.md) priority table に登録 (順位 80-84):

- **順位 80** (Tier 1, Bundle f): rate-limit auto-retry wakeup 予約ロジックの整理 — `auto_retry_enabled=true` でも park 未予約で exit する事象を観測
- **順位 81** (Tier 1, Bundle f): CR 投稿エラー (`Failed to post review comments`) の auto-retry 拡張 — 既存 rate-limit detection をバイパスして retry 未発火
- **順位 82** (Tier 3, Bundle f): ADR-018 update — rate-limit 以外の transient failure auto-retry 設計の明文化
- **順位 83** (Tier 2, 独立): 複合 AND guard の各条件を独立テストで検証
- **順位 84** (Tier 3, 独立): `code-review.md` に「early-return guard テスト分離」チェックリスト追記

詳細エントリは [docs/todo5.md](todo5.md) 末尾。Bundle f land は本 doc の retirement に必須ではない。本セッション 1 回限りの観測で頻度が未確定のため、**優先度は再観測を経て判断**する (新規フィードバックは頻度が確認できるまで優先しない方針)。

### 効果実測の現状

未測定。次回 Claude Code session で:

1. cli-finding-classifier を post-pr-review フローに統合した後の token 消費を比較
2. CodeRabbit triage タスクが Claude を経由しなくなった分の体感計測

統合 (Phase 5) 完了までは「効果見込み 15-25%」は推定値のまま。

## 8. 次の作業候補 (Phase 5 + 残作業)

優先度順に列挙。各項目はそれぞれ独立 PR を想定。

### A. ✅ Phase 5: cli-pr-monitor / takt facet への classifier 統合 (LANDED in PR #120, 2026-05-07)

`cli-pr-monitor` poll stage に `cli-finding-classifier.exe` を subprocess で統合。`ClassifierConfig` (default OFF / 試験運用) + `state.classified_findings` field + `enrich_with_classifier` step を追加。詳細は §7 実装進捗ログ参照。

- **prompt injection サニタイズ**: 本 PR では未対応 (`auto_fix` execution 経路がまだ無いため)。提案 1 (lint screen facet) で auto_fix を実行する経路を作るタイミングで導入予定

### B. ✅ Finding C strict 化 (LANDED in PR #120, 2026-05-07)

`from_llm_output` で `normalized_issue` の改行・80 chars 超を検出して fallback。`NORMALIZED_ISSUE_MAX_CHARS=80` const 化。回帰テスト 3 件追加。

### C. Finding D: ADR-038 line 61 textual fix (low priority)

- **目的**: ADR-038 line 61 の `confidence=0.0` を `action_confidence=0.0` に統一 (実装の schema 名称と整合)
- **作業**: 1 単語修正
- **依存**: なし
- **見積**: 5 分 (他の Phase 5 PR に bundle 可)
- **ROI**: ★ (永続 doc の整合は重要だが単独 PR を立てるほどではない)

### D. プロンプト v2: `normalized_issue` 言語制約強化 (low priority)

- **目的**: dogfood で観測された「mistral:7b が日本語指示でも英語混じりで返す」問題を改善
- **作業**: `prompts/classify.txt` でより強い言語固定指示 + few-shot examples を追加
- **依存**: なし
- **見積**: 半日 (prompt 変更 + 簡易ベンチで安定性検証)
- **ROI**: ★ (実害は小、UX 微改善)

### E. 提案 1 (lint screen facet) — 実効果見極め後

- **目的**: takt の新 facet `ollama-lint-screen` で pre-push 時に diff の lint 一次フィルタを mistral:7b に逃す
- **依存**: Phase 5 で classifier の実効果が確認できた後
- **見積**: 1〜2 日
- **ROI**: 提案 1 として中程度。Phase 5 の効果次第で優先度が変動

### F. 提案 3 (PR body draft) — 提案 1 採用後

- **目的**: `prepare-pr` skill で `jj diff` 要約を mistral:7b で前処理し、Claude への入力 token を圧縮
- **依存**: 提案 1 が land して `lib-ollama-client` の運用知見が貯まった後
- **見積**: 半日
- **ROI**: input token 削減への寄与は最大ライン (cache 倍率の影響が大きい領域)

## 9. 過去判断のサマリ (引き継ぎ用)

- **scope decision (2026-05-06)**: 初版 PR は提案 2 のみ。Phase 5 / 提案 1 / 提案 3 は別 PR
- **action category (Finding C 関連)**: KISS で `trim + non-empty filter` のみ採用、改行/長さ check は Phase 5 で実装するという保留判断
- **architecture decision (Finding 3 関連)**: `OllamaApi` trait は dyn-compatible にするため `generate_raw_json -> Result<String, _>` シグネチャ。型付き convert は trait 外の自由関数 `generate_json::<T>` で提供
- **input sanitization (PR #119 WARN 関連)**: prompt injection 対策は本 PR では skip (`auto_fix` を実行する経路がない)。Phase 5 で auto_fix execution を実装するタイミングで input sanitize / プレースホルダのブラケット化を導入予定

## 関連リンク

- [ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止](adr/adr-018-pr-monitor-takt-migration.md)
- [ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略](adr/adr-020-takt-facets-sharing.md)
- [ADR-034: CodeRabbit 監視・対話の自動化戦略 — Bundle a 設計根拠](adr/adr-034-coderabbit-auto-monitoring.md)
- [ADR-038: ローカル LLM による CodeRabbit findings classification](adr/adr-038-local-llm-finding-classification.md) — 本ファイル提案 2 の land 結果
