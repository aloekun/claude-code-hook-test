# ローカル LLM オフロード可能性調査

> **状態**: 試験運用 (調査 → 提案 2 (cli-finding-classifier) は ADR-038 として land 済 / PR #119, 2026-05-06)。提案 1 (lint screen facet) と提案 3 (PR body draft) は未着手。フォローアップ作業の判断材料として保持。
>
> **引退条件**: 以下のいずれかを満たした時点で本ファイルを削除する (docs-governance.md retirement workflow 準拠)。
> - 提案 1〜3 すべてが land または却下された場合 → ADR-038 等にエッセンスを移し、本ファイルを削除
> - 提案 1 / 提案 3 が却下された場合 → 却下理由を本ファイルに記録した上で、ADR-038 関連の learnings を ADR-038 に migrate して本ファイルを削除
> - 6 ヶ月経過しても提案 1 / 3 の採否が決まらない場合 → 採用見込みなしとみなして削除
>
> **由来**: セッション ID `5ca01479-6d71-4328-91d0-861343120c3f` (2026-05-05〜2026-05-06, Bundle b 関連) の作業ログ分析
>
> **検証ブランチ運用**: §8.D / §8.E / §8.F の **追加実装作業**は **jj bookmark `feature/local-llm-dogfood`** で隔離 (新規 LLM コードは検証完了まで master に流さない)。一方 §A-2 Phase 5 dogfood の **P-0 (config opt-in) と P-1〜P-5 は通常 master PR フロー** (個別 PR で land、classifier は master 上で起動)。詳細は §10 ブランチ分離運用。

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

#### 2026-05-07: §8.C land (PR #121) — ADR-038 textual fix + Bundle f task registration

- **§8.C 実施**: ADR-038 line 61 の `confidence=0.0` → `action_confidence=0.0` に統一 (実装 schema 名称との整合)
- **Bundle f 登録**: 順位 80-84 を `docs/todo.md` priority table + `docs/todo5.md` 詳細エントリに記録 (頻度確認後に優先度判断する方針を明記)
- **PR #121**: docs-only PR → CR 「No actionable comments」 → squash merge `6640dc7b` (2026-05-07T04:47:56Z、master)
- **post-merge-feedback (PR #121)** で Bundle g (順位 85-88) を新規追加: `(review_state, findings) → verdict` 評価ロジックの edge case を 3 PR (#119/#120/#121) 連続観測で頻度確認 → Tier 1 妥当性確定。詳細は [docs/todo5.md](todo5.md) 順位 85-88、bundle commentary は [docs/todo.md](todo.md) Bundle g 段落

### 効果実測の現状

**未測定** (PR #120 で統合は完了したが、`ClassifierConfig::default().enabled = false` のため classifier は実 review サイクルで一度も起動していない)。

計測計画は **§8.A-2 (Phase 5 dogfood 計測)** に集約。P-0 (config opt-in) → P-1〜P-5 (5 PR dogfood) の流れで以下を計測予定:

1. classifier の classification 妥当性 (agreement rate)
2. Claude session の入力 token 削減効果
3. classifier latency / fallback rate
4. `normalized_issue` 言語制約違反率 (§8.D 着手判断材料)

dogfood 完了までは「効果見込み 15-25%」は推定値のまま。

## 8. 次の作業候補 (Phase 5 + 残作業)

優先度順に列挙。各項目はそれぞれ独立 PR を想定。

### A. ✅ Phase 5: cli-pr-monitor / takt facet への classifier 統合 (LANDED in PR #120, 2026-05-07)

`cli-pr-monitor` poll stage に `cli-finding-classifier.exe` を subprocess で統合。`ClassifierConfig` (default OFF / 試験運用) + `state.classified_findings` field + `enrich_with_classifier` step を追加。詳細は §7 実装進捗ログ参照。

- **prompt injection サニタイズ**: 本 PR では未対応 (`auto_fix` execution 経路がまだ無いため)。提案 1 (lint screen facet) で auto_fix を実行する経路を作るタイミングで導入予定

### B. ✅ Finding C strict 化 (LANDED in PR #120, 2026-05-07)

`from_llm_output` で `normalized_issue` の改行・80 chars 超を検出して fallback。`NORMALIZED_ISSUE_MAX_CHARS=80` const 化。回帰テスト 3 件追加。

### C. ✅ Finding D: ADR-038 line 61 textual fix (LANDED in PR #121, 2026-05-07)

ADR-038 line 61 の `confidence=0.0` を `action_confidence=0.0` に修正、実装の `ClassifiedFinding.action_confidence` schema 名称と整合。同 PR で Bundle f task registration (順位 80-84) も land。詳細は §7 実装進捗ログ参照。

### A-2. ★ Phase 5 dogfood 計測 (E/F 着手前の必須前提)

> **状態**: **未着手** (本計画は 2026-05-07 セッションで策定、別セッションで実施想定)
>
> **位置づけ**: §8.A (Phase 5 統合) は完了したが、ADR-038 §試験運用→本採用の昇格条件のうち **条件 1 (5 PR 以上 dogfood)** と **条件 3 (token 削減効果体感)** は未達。§8.E (lint screen facet) の「実効果見極め後」依存を満たすため、本計画で実 dogfood を行う。

#### 目的

ADR-038 試験運用 → 本採用の **未達 2 条件** を充足:

1. **条件 1**: 5 PR 以上の実 review サイクルで dogfood、classification 妥当性目視確認
2. **条件 3**: Claude session の入力 token 削減効果が体感で確認できる

#### 構成: P-0 (前提セットアップ) + 5 PR

##### P-0: classifier opt-in (config 1 行変更、独立 PR)

**変更内容**: `pr-monitor-config.toml` に classifier section を追加 (現状ファイルに section 不在の場合は新規追記、存在する場合は `enabled = true` に変更)

```toml
[classifier]
enabled = true
# 以下は default 値、明示しなくてもよいが、config の意図を残すなら明示推奨
model = "mistral:7b"
endpoint = "http://localhost:11434"
timeout_secs = 30
```

**確認方法**:

```bash
grep -A5 "^\[classifier\]" pr-monitor-config.toml
```

**意義**: P-0 land 後、後続 5 PR で `cli-pr-monitor` の poll stage が自動的に `cli-finding-classifier.exe` を invoke する。

##### P-1〜P-5: dogfood 本体 (頻度確認済 Tier 1 を優先、findings 多様性確保)

| PR | タスク | 順位 | Effort | 期待 findings | 選定理由 |
|---|---|---|---|---|---|
| P-1 | Bundle g-1 (monitor state guard + test) | 85 + 86 | S+S | 中 (3-5 件) | 頻度確認済 Tier 1 (3 PR 観測)、ユーザー方針 (頻度確認後優先) と整合、Rust 実装層 |
| P-2 | `> vs >=` boundary inconsistency lint rule | 47 | S | 中 (3-5 件) | Tier 1、独立、Rust + lint rule (異種 PR で多様性) |
| P-3 | PowerShell `(?i)` フラグ自動検証 lint rule | 7 | S | 中 (3-5 件) | Tier 1、独立、別言語 lint で多様性 |
| P-4 | overflow 統合テスト + 境界値 matrix (Bb-3) | 76 + 77 | M+S | 中 (3-5 件) | Tier 2、Rust test 集中、test カバレッジ系の findings |
| P-5 | Bundle f-1 (retry logic + ADR) | 80+81+82 | M+M+S | 高 (5-10 件) | P-1〜P-4 中の dogfood で順位 80 が再観測されれば頻度確認達成、最終 PR で大規模 finding 負荷確認 |

**Bundle g-2** (順位 87+88、global rule codify、XS+XS) は docs-only で classifier dogfood 対象外。Phase 5 dogfood と並列で別 PR (例: P-2.5 として挟む) に land 可。

#### Setup 手順 (別セッション開始時の確認チェックリスト)

> **前提**: §A-2 P-0 (config opt-in) が master に land 済であること。P-1〜P-5 は **master ベースの個別 PR** で進行 (詳細は §10.4)。`feature/local-llm-dogfood` 枝への切替は §A-2 dogfood では不要 (枝は §8.D / §8.E / §8.F の追加実装専用)。

```bash
# 1. Ollama 起動確認
curl -s http://localhost:11434/api/tags | jq '.models | map({name, size})'
# 期待: mistral:7b が含まれる

# 2. classifier exe deploy 確認
ls -la .claude/cli-finding-classifier.exe
# 期待: ファイル存在、~2.2MB

# 3. config opt-in 確認
grep -A5 "^\[classifier\]" pr-monitor-config.toml
# 期待: enabled = true

# 4. 過去 dogfood 実行 (PR #119) 動作確認
echo '[{"severity":"Major","file":"f.rs","line":"1","issue":"test","suggestion":"fix","source":"CodeRabbit"}]' | \
  .claude/cli-finding-classifier.exe --timeout-secs 30
# 期待: action / action_confidence / normalized_issue を含む JSON
```

#### 計測手順 (各 PR で実施)

**1. classifier 出力の取得** (post-pr-monitor 実行後):

```bash
jq '.classified_findings // []' .claude/pr-monitor-state.json
```

各 finding は以下の field を持つ:

```json
{
  "severity": "Major",
  "file": "src/...",
  "line": "42",
  "issue": "...",
  "suggestion": "...",
  "source": "CodeRabbit",
  "action": "auto_fix | human_review | false_positive_likely | informational",
  "action_confidence": 0.0-1.0,
  "normalized_issue": "..." | null,
  "fallback_reason": "..." | null
}
```

**2. classification 妥当性 (agreement rate) 評価**:

各 finding に対して、私 (Claude) が action/confidence を独立評価し一致率を測定。記録形式:

```text
P-1: 4/5 agreement (80%)、disagree 1 件は <理由>
```

**3. session token 集計**:

Claude Code 側で session statistics を `/cost` 等で取得可能なら、各 PR session 開始 → merge までの input token を記録。比較対象は本セッションの PR #119/#120/#121 (classifier OFF)。

**4. classifier latency**:

`cli-pr-monitor` の log で「classifier: N findings を分類完了」の前後で経過秒数を測定。本 doc §1 dogfood 実測 (PR #119) では **3.6s/件**。

**5. fallback rate**:

```bash
jq '[.classified_findings[] | select(.fallback_reason != null)] | length' .claude/pr-monitor-state.json
# /
jq '.classified_findings | length' .claude/pr-monitor-state.json
```

**6. `normalized_issue` 言語制約違反率** (§8.D 着手判断材料):

```bash
jq '[.classified_findings[] | select(.normalized_issue) | .normalized_issue | test("[a-zA-Z]{8,}"))]' .claude/pr-monitor-state.json
```

英単語が 8 文字以上連続する箇所を検知 (簡易ベンチ、誤検出許容)。

#### 判定基準

各 PR 完了後に追記、5 PR 終了で集計。本採用昇格条件への対応:

| 指標 | 目標 | ADR-038 昇格条件 |
|---|---|---|
| **agreement rate** | ≥80% (action 一致) | 条件 1 (5 PR dogfood + 妥当性確認) |
| **session token 削減** | ≥10% (見込み 15-25%) | 条件 3 (体感確認) |
| **classifier latency** | ≤5s/件 | (補助指標、運用許容ライン) |
| **fallback rate** | ≤20% | (Ollama 安定性、運用許容ライン) |
| **言語制約違反率** | ≤10% | §8.D 着手判断 (>10% なら D 先行) |

#### 既知の注意事項 (本セッションで観測した dogfood 阻害要因)

1. **CR rate-limit 再発リスク**: 本セッション PR #120 / #121 で観測 (1 hour あたり commits 制限)。5 PR 連続作成は無料枠を圧迫。Bundle g-2 (XS docs) を間に挟む等で stretch すれば緩和。`comment_created_at` から 41 分待つと clear
2. **post-pr-monitor wakeup edge case** (Bundle f #80 / Bundle g #85): 本 dogfood 中に再観測された場合、Bundle f / g の頻度カウントを進める副次目的にもなる。手動 `@coderabbitai review` 投入や CronCreate 手動予約で迂回可
3. **Ollama 不安定**: 落ちた場合 fallback で human_review に倒れて block しないが計測 noise になる。各 PR 着手前に `curl /api/tags` で確認推奨
4. **Windows shell の CP932 漏れ**: 日本語 finding が含まれる場合、cli-pr-monitor → classifier の subprocess は Rust 実装で UTF-8 安全 (PR #119 で検証済)。bash 経由のデバッグでは `--data-binary @file.json` 形式必須 (§6 注意点参照)
5. **takt 600s timeout**: 本セッション PR #120 で observed、auto-fix iteration 中に timeout。dogfood 中に再観測される可能性あり、その場合 takt の fix step が不完全終了するため `state.classified_findings` の最終形を確認

#### session 跨ぎ運用ガイド

**dogfood 中断・再開時**:
- `pr-monitor-config.toml` の `[classifier] enabled` 状態は repo に commit されているため再 clone でも引き継がれる
- 各 PR 完了後に `state.classified_findings` を export してテキスト保存しておくと session 跨ぎでも参照可:
  ```bash
  jq '.classified_findings' .claude/pr-monitor-state.json > .takt/dogfood-pr-NNN-classified.json
  ```
- 計測 log は本 doc の §A-2.x に追記して history 化 (例: §A-2.measurements として後続セッションで埋める)

**dogfood 完了 → 本採用判断時**:
- §7 § 効果実測の現状 を「測定済」に更新 (具体値を記載)
- ADR-038 §試験運用→本採用の昇格条件 1 / 3 を達成した旨を ADR 本体に追記 (試験運用 flag を外す)
- §8.A-2 を「✅ LANDED」にマーク、§8.E (lint screen facet) の依存条件解除

**dogfood 完了 → 却下判断時** (基準未達):
- agreement rate <80% → §8.D (prompt v2) 先行で再 dogfood
- session token 削減 <10% → 提案 1 / 3 の ROI 再評価、本 doc retirement の引退条件「却下」経路を検討
- 却下時は ADR-038 を **「却下」ステータス** に更新、`lib-ollama-client` / `cli-finding-classifier` 両 crate の削除可否判断

#### 完了基準

- [ ] P-0 land (config opt-in)
- [ ] P-1〜P-5 land (5 PR で classifier 実起動経験)
- [ ] 各 PR で `state.classified_findings` を保存
- [ ] 5 PR 集計で agreement rate / token / latency / fallback / 言語制約違反率 を本 §A-2 内に記録
- [ ] 判定基準 4/5 以上達成 → §7 効果実測の現状を「測定済」に更新、ADR-038 昇格条件 1/3 達成を明記
- [ ] §8.A-2 を ✅ LANDED にマーク、§8.E の dependency 解除

#### 計測ログ (実施時に追記)

(各 PR 完了ごとに以下を埋める。全て master ベース個別 PR 記法)

```text
P-0 (config opt-in): PR #123, merged 2026-05-07 ✅ (本ファイル §10 governance + [classifier] enabled=true 同梱)
  - smoke test: mistral:7b で `unused import` finding → action=auto_fix / confidence=1.0
  - cli-pr-monitor は [classifier] section を読み込み可、compile 通過
P-1 (Bundle g-1):    PR #125, merged 2026-05-07, findings: 0 (CR APPROVE no comments), classifier 未起動 (input data なし), 計測 N/A — dogfood 不発
P-2 (順位 47):       PR #126, merged 2026-05-07, findings: 1 (Nitpick, CR review body 内 `<details>` block), agreement: 1/1 (100%, 私評価=human_review と一致), latency: 6.4s/件 (>5s 目標), fallback: 1/1 (normalized_issue length 100>80)
  - 既知 gap: check-ci-coderabbit が review body の `<details>` block 内 Nitpick を抽出しない (post-pr-monitor が classifier に渡せず、手動で synthetic finding 構築して classifier 実行)
P-3 (順位 7):        PR #127, merged 2026-05-08, findings: rate-limit blocked (CR が 27 min wait の rate-limit overlay で formal review 投稿不可)、classifier 未起動、計測 N/A — dogfood 不発 (但し CR rate-limit 経路が dogfood の阻害要因として観測された、§A-2 §6 注意事項 #1 を実証)
P-4 (順位 76+77):    PR #128, merged 2026-05-08, findings: 1 (CR Nitpick: cross_module_* 命名 misleading)、手動 synthetic finding で classifier 実行 → action=human_review / action_confidence=**0.9** (P-2 の 0.0 から大幅改善、length contract pass)、latency: 6.6s/件、fallback: **0/1** (P-2 の 1/1 から改善、normalized_issue 50 chars Japanese で contract pass)、agreement: 1/1 (100%、私評価=human_review と一致)
P-5 (Bundle f-1):    PR #___, merged ___, findings: __, agreement: __/__, token Δ: __, latency: __s/件, fallback: __/__

集計:
- 総 findings: __
- 総 agreement rate: __/__ = __%
- 平均 session token Δ: __%
- 平均 classifier latency: __s/件
- 総 fallback rate: __/__ = __%
- 言語制約違反率: __/__ = __%
- 判定: ✅ 本採用 / 🔄 §8.D 先行 / ❌ 却下
```

### D. プロンプト v2: `normalized_issue` 言語制約強化 (low priority)

- **目的**: dogfood で観測された「mistral:7b が日本語指示でも英語混じりで返す」問題を改善
- **作業**: `prompts/classify.txt` でより強い言語固定指示 + few-shot examples を追加
- **依存**: なし
- **見積**: 半日 (prompt 変更 + 簡易ベンチで安定性検証)
- **ROI**: ★ (実害は小、UX 微改善)

### E. 提案 1 (lint screen facet) — §8.A-2 dogfood 完了後

- **目的**: takt の新 facet `ollama-lint-screen` で pre-push 時に diff の lint 一次フィルタを mistral:7b に逃す
- **依存**: **§8.A-2 (Phase 5 dogfood 計測) 完了 + 判定基準達成** (agreement rate ≥80% かつ session token 削減 ≥10%)
- **見積**: 1〜2 日
- **ROI**: 提案 1 として中程度。§8.A-2 の集計結果次第で優先度が変動 (基準未達なら §8.D prompt v2 先行 → 再 dogfood の経路あり)

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

## 10. 検証作業のブランチ分離運用

> **状態**: 試験運用 (本 §10 は 2026-05-07 セッションで追加。§A-2 dogfood の前提運用ガイド)
>
> **目的**: 本 §10 だけ参照すれば、ローカル LLM 検証 (Phase 5 dogfood + §8.D / §8.E / §8.F の追加実装) を別セッションで再開・完了できる状態にする。

### 10.1 方針: Option B (隔離ベース)

ローカル LLM 統合の実装 (`lib-ollama-client` / `cli-finding-classifier` / `cli-pr-monitor` 統合 / ADR-038 / `pr-monitor-config.toml` の `[classifier]` section) は **既に master に land 済 (PR #119 / #120 / #121 / #122)**。これを引き抜く (revert) のではなく、master 上で動かしながら検証する。隔離スコープは **「未実装かつ動作の不確実性が高い新コード」のみ**:

- **master で進める** — §A-2 P-0 (config opt-in) / P-1〜P-5 (master ベース個別 PR で dogfood)。既存実装の有効化と試験運用
- **feature/local-llm-dogfood 枝で進める** — §8.D prompt v2 / §8.E lint screen facet / §8.F PR body draft。新規コードであり、検証完了 (採用判定) まで master に流さない

判断の根拠:

1. master 上の classifier は smoke test で動作確認済 (P-0 完了時、§A-2 計測ログ参照)。enabled = true 後の dogfood で問題が出れば `[classifier] enabled = false` に戻す revert PR (§10.6 C の kill-switch 簡易版) が即時 fallback 可
2. revert は workspace `Cargo.toml` / `package.json` の `build:all` / 配備 exe / ADR-038 / 関連 todo 構造まで波及し、Option A (revert) と Option B (隔離) で touchpoint コストが非対称
3. グローバル設定 (`~/.claude/CLAUDE.md` / `~/.claude/skills/` 等) への既往変更は「戻せないことを許容」というユーザー stance と整合 (= 戻せないものは戻さない、本当に危険な拡張のみ枝で隔離)

### 10.2 ブランチ構成

| 名前 | 種別 | 役割 |
|---|---|---|
| `master` | jj bookmark (push 対象) | 既存の LLM 実装 + P-0 で classifier 有効化後の運用枝。非 LLM 機能 (Bundle f / g / 単発 lint rule) と §A-2 P-0 / P-1〜P-5 の PR はすべて master に land |
| `feature/local-llm-dogfood` | jj bookmark (push 可、PR は base = master) | §8.D / §8.E / §8.F の追加実装作業枝。base = master tip。寿命は §10.6 判定処理で確定 |

### 10.3 master と feature 枝の責務分担

| 種類 | 行先 | 理由 |
|---|---|---|
| 非 LLM 機能 (hook / takt / rule / docs) | **master** (個別 PR) | LLM 検証に依存しない通常開発 |
| §A-2 P-0 (`pr-monitor-config.toml` で classifier `enabled = true`) | **master** (個別 PR) | classifier を master 上で起動可能にする。問題発生時は revert PR で即 fallback (§10.6 C 簡易版) |
| §A-2 P-1〜P-5 (dogfood の試験台 PR: Bundle g-1 / 順位 47 等) | **master** (個別 PR) | 通常 PR フローで classifier が自動起動。計測ログは §A-2 にマージ後追記 |
| §8.D prompt v2 (`prompts/classify.txt` 改訂) | **feature/local-llm-dogfood 枝のみ** | 検証で agreement rate が基準未達だった場合の改善ループ。新規コード |
| §8.E lint screen facet (takt の新 facet `ollama-lint-screen`) | **feature/local-llm-dogfood 枝のみ** | 検証成功後に master へ抜き出し PR。新規 facet |
| §8.F PR body draft (`prepare-pr` skill の Ollama 前処理) | **feature/local-llm-dogfood 枝のみ** | 同上 |
| LLM crate の bug fix (検証で判明したもの) | **master** (個別 PR) | 既に master 上にある機能のバグ修正経路 |

### 10.4 dogfood 実行 (Phase 5 用)

§A-2 P-0 が master に land すると `[classifier] enabled = true` が master の working tree に存在し、以降の master ベース PR の post-pr-monitor で classifier が自動起動する。**ブランチ切替は不要** (P-1〜P-5 は通常 PR フロー)。

```bash
# P-N 着手 (master ベース、通常 PR フロー):
jj edit master
jj new -m "<P-N の機能内容>"
# ... 実装 ...
pnpm push                                                 # cli-push-runner
pnpm create-pr                                            # PR 作成

# post-pr-monitor が自動起動し、classifier が走る
# 完了後、findings + classified_findings を取得:
jq '.classified_findings' .claude/pr-monitor-state.json > .takt/dogfood-pr-NNN-classified.json

# 本ファイル §A-2 計測ログに追記 (master の master 系 docs PR か、対象 PR の最終 commit に同梱)
```

#### 注意点

- §A-2 P-0 land 前の PR で classifier は起動しない (config が default OFF)。P-0 を最初に必ず land する
- classifier 起動が予期せぬ問題を起こした場合は **revert PR で `[classifier] enabled = false` に戻す** のが kill-switch (§10.6 C の最小版)。crate 削除等の物理削除は dogfood 失敗判定後にまとめて実施
- §8.D / §8.E / §8.F の追加実装は feature/local-llm-dogfood 枝で進める。詳細手順は §10.7 (再開チェックリスト) 後半

### 10.5 グローバル設定バックアップ規約

ユーザー方針: **「グローバルスキル・CLAUDE.md に影響する変更は、バックアップを取ってから実施する」** (本 §10 追加と同セッションで提示された運用ルール)。

#### 対象

- `~/.claude/CLAUDE.md`
- `~/.claude/skills/`
- `~/.claude/rules/`
- `~/.claude/agents/`
- `~/.claude/settings.json`

#### バックアップ手順

リポジトリ内に `__backup-claude-config/` を置く (`__` prefix で `.gitignore` 対象、scratch 命名規約準拠)。スナップショットは `__backup-claude-config/<YYYYMMDD-HHMMSS>/` に格納。

```powershell
# PowerShell 例
$ts = Get-Date -Format "yyyyMMdd-HHmmss"
$dst = "__backup-claude-config/$ts"
New-Item -ItemType Directory -Force -Path $dst | Out-Null
Copy-Item -Recurse "$env:USERPROFILE\.claude\CLAUDE.md" $dst
Copy-Item -Recurse "$env:USERPROFILE\.claude\skills"   "$dst/skills"
Copy-Item -Recurse "$env:USERPROFILE\.claude\rules"    "$dst/rules"
Copy-Item -Recurse "$env:USERPROFILE\.claude\agents"   "$dst/agents"
Copy-Item -Recurse "$env:USERPROFILE\.claude\settings.json" $dst
```

#### 規約

1. 検証作業中にグローバル設定を変更する **直前に** snapshot を取る
2. 変更内容を本 §10.5 配下に追記 (どのファイルに何を加えたか / rollback 不能なら明示)
3. 検証完了後 (採用 / 却下) に snapshot 処理:
   - 採用 → snapshot は不要 (現状が正)
   - 却下 → snapshot から rollback を試行 (best-effort、不能ケースは記録のみ)

#### 強制力なし (意図的)

本規約は **手動運用** であり hook 等で強制しない。mechanical 検知不能なルールを hook 化しない方針 (CLAUDE.md feedback "強制力のないルール追加は却下" / "ドキュメント lint を hook で強制しない" と整合)。

#### 変更履歴 (検証作業中に埋める)

(実際にグローバル設定を触ったタイミングで追記)

```text
- YYYY-MM-DD HH:MM: <ファイル> に <変更内容>。snapshot: __backup-claude-config/<ts>/
```

### 10.6 採用 / 却下判断後の処理

§A-2 dogfood 完了後、判定別に以下を実施。

#### A. 採用 (基準達成)

1. feature 枝の追加分 (§8.D / §8.E / §8.F の実装) を整理し、master へ PR 化
2. `pr-monitor-config.toml` の `[classifier] enabled = true` を master へ反映する PR
3. ADR-038 を「試験運用」→「採用」に昇格 (条件 1, 3 達成を ADR 本体に明記)
4. §7 効果実測の現状を「測定済」に更新、具体値を記載
5. §A-2 を ✅ LANDED にマーク
6. feature 枝を `jj bookmark forget feature/local-llm-dogfood` で破棄
7. 本ファイル retirement 条件 1 (提案 1〜3 すべて land) の達成度を再評価し、必要なら本ファイル削除 (`docs-governance.md` retirement workflow 準拠)

#### B. §8.D 先行で再 dogfood

1. feature 枝で `prompts/classify.txt` を改訂 (言語固定指示 + few-shot)
2. P-1〜P-5 を再実行 (枝寿命延長)

#### C. 却下 (基準未達 + 改善見込みなし) — kill-switch

master から LLM 関連実装を物理削除する PR を起こす。touchpoint チェックリスト:

- [ ] `Cargo.toml` workspace member から `src/lib-ollama-client` と `src/cli-finding-classifier` を削除
- [ ] `src/cli-pr-monitor/` から `classifier_runner.rs` / `ClassifierConfig` / `enrich_with_classifier` step / `state.classified_findings` field を削除
- [ ] `src/cli-pr-monitor/` のテストから classifier 関連テストを削除
- [ ] `package.json` の `build:cli-finding-classifier` 等の script を削除
- [ ] `pnpm deploy:hooks` 経路から `.claude/cli-finding-classifier.exe` 配備を外す
- [ ] `.claude/cli-finding-classifier.exe` を削除 (gitignore 済なので git 管理対象外、配備先の物理削除のみ)
- [ ] `pr-monitor-config.toml` の `[classifier]` section を削除
- [ ] `docs/adr/adr-038-local-llm-finding-classification.md` を「却下」ステータスに更新 (ADR 本体は履歴として残す)
- [ ] プロジェクトルートの `CLAUDE.md` の ADR-038 リンクに「却下」アノテーションを追加
- [ ] feature 枝を `jj bookmark forget` で破棄
- [ ] 本ファイル retirement 条件「却下」を発動 (`docs-governance.md` retirement workflow に従って permanent value 移管 → 削除)

#### D. 6 ヶ月経過判断未達

本ファイル冒頭 retirement 条件「採用見込みなし」を発動。

### 10.7 検証作業の再開チェックリスト (別セッション開始時)

#### 共通: 環境前提の確認

```bash
# 1. master 最新化
jj git fetch
jj edit master  # working tree を master 最新に揃える

# 2. P-0 land 済か確認 (master の pr-monitor-config.toml に [classifier] enabled=true)
grep -A5 "^\[classifier\]" pr-monitor-config.toml

# 3. Ollama 起動確認
curl -s http://localhost:11434/api/tags | jq '.models | map({name, size})'

# 4. classifier exe 配備確認
ls -la .claude/cli-finding-classifier.exe

# 5. 過去計測ログ確認
ls .takt/dogfood-pr-*.json 2>/dev/null
grep -A20 "計測ログ" docs/local-llm-offload-analysis.md
```

#### A. P-N (P-1〜P-5) 着手の場合 — master ベース個別 PR

```bash
# master 上で新規変更を作成 (jj new で空 commit、その後実装)
jj new master -m "<P-N feature 説明>"
# ... 実装 ...
pnpm push
pnpm create-pr
# post-pr-monitor が classifier を自動起動 (P-0 の enabled=true 効果)
```

#### B. §8.D / §8.E / §8.F 追加実装の場合 — feature/local-llm-dogfood 枝

```bash
# 1. ブランチ存在確認 (push 済なら origin にもある)
jj bookmark list -r feature/local-llm-dogfood
jj log -r 'bookmarks(feature/local-llm-dogfood)' --no-graph

# 2. 検証枝に切替 (なければ jj new master でベースから派生)
jj edit feature/local-llm-dogfood
# 必要なら: jj rebase -d master -r 'bookmarks(feature/local-llm-dogfood)..'

# 3. 実装 → push → 採用判定後 master へ抜き出し PR (§10.6 A 経路)
```

### 10.8 関連 ADR / 規約

- ADR-022 (自動化コンポーネントの責務分離) — 本ファイル提案 1〜3 が依存する 3 層構造の根拠
- ADR-038 (ローカル LLM による CodeRabbit findings classification) — 本ファイル提案 2 の land 結果。試験運用 → 採用 / 却下 の判定は本 §10.6 経由
- `docs-governance.md` (グローバル rule) — 本ファイル retirement workflow の準拠先

## 11. §A-2 dogfood retrospective + evals 形式への検証方式切替 (2026-05-08)

> **状態**: 試験運用 (本 §11 は §A-2 5 PR シリーズ完了後の振り返りで策定、2026-05-08)
>
> **目的**: §A-2 PR-based dogfood で実証された阻害要因を踏まえ、§8.D / §8.E / §8.F の妥当性検証を **evals 形式** (固定 diff fixture + 期待出力との突合) で進められるよう方針整備。今後の検証作業 (LLM / hook / lint rule 等) で再利用可能な検証パターンとして codify。

### 11.1 §A-2 PR-based dogfood の振り返り

5 PR (P-1〜P-5、PR #125〜#129) で classifier を実 review サイクルにかける dogfood を実施。集計結果 (§A-2 計測ログ詳細):

| 項目 | 値 |
|---|---|
| classifier 起動率 | 2/5 = **40%** |
| 阻害要因観測数 | **3 種** (findings ゼロ / review body 抽出漏れ / rate-limit) |
| agreement rate | 2/2 = 100% (但し N=2 で statistically limited) |
| 平均 latency | 6.5s/件 (目標 5s 超過 30%) |
| dogfood 結論 | classifier 妥当性検証は不十分、阻害要因の発見が主成果 |

### 11.2 PR-based dogfood の構造的限界

実 PR review サイクルに依存する dogfood は以下を制御できない:

| 阻害要因 | 影響 PR | 構造的原因 |
|---|---|---|
| **findings ゼロ** | P-1 (#125) | CR が APPROVE で終了 → classifier 入力なし。設計通りだが dogfood では noise |
| **review body 抽出漏れ** | P-2 (#126), P-4 (#128) | `check-ci-coderabbit` が CR review body の `<details>` block 内 Nitpick を inline comment として抽出しない (parser の scope 限界)。手動 synthetic finding 構築で迂回したが本来は post-pr-monitor が自動でやるべき |
| **CR rate-limit** | P-3 (#127), P-5 (#129) | per-hour commit quota 超過で review が blocked、classifier は CR 出力に依存するため連動 |

これらは「実環境で実走させる」ことの benefit として一見良いが、**classifier の妥当性検証** という主目的に対しては noise / blocker として機能した。N=5 で実 classification データが取れたのは N=2 のみ。

### 11.3 evals 形式の検証方式 (新提案)

ユーザー提案 (本セッション 2026-05-08): `E:\work\claude-code-skills\analyze-pr\evals\evals.json` の構造を参考に、**固定 diff fixture + 期待出力 + 突合**で検証する。

#### 参照: `analyze-pr` skill の evals 構造

```text
analyze-pr/
├── SKILL.md
├── evals/
│   ├── evals.json           # eval ケース定義
│   ├── trigger_eval.json    # skill 起動条件 (positive/negative)
│   └── files/               # 固定 fixture
│       ├── eval1-good-pr.diff
│       ├── eval1-review-comments.json
│       ├── eval1-reviews.json
│       ├── eval2-clean-pr.diff
│       └── ...
```

各 eval の構造:

- **id**: ID
- **prompt**: 入力 prompt (fixture file 参照を含む)
- **expected_output**: prose 形式の期待出力サマリ
- **files**: 補助 fixture files
- **expectations**: 個別検証可能な assertion list (例: "Markdown レポートが '## PR Analysis Report' ヘッダーで始まる")

### 11.4 §8.E (lint screen facet) への適用設計

#### 目的

takt の新 facet `ollama-lint-screen` で pre-push 時に diff の lint 一次フィルタを mistral:7b に逃す前に、**mistral:7b の lint 判定が Claude Code (gold standard) と同等の結果を安定して出すか**を検証。

#### evals 構造案

```text
src/cli-finding-classifier/  (or 新 crate cli-lint-screener)
└── evals/
    ├── lint-screen-evals.json
    └── files/
        ├── eval1-unused-import.diff       # Rust unused import の典型
        ├── eval2-deep-nesting.diff        # nesting > 4 levels
        ├── eval3-magic-number.diff        # 未定数化数値
        ├── eval4-clean.diff               # 問題なし (false positive 検知用)
        ├── eval5-multi-issue.diff         # 複数 issue 混在
        ├── eval6-existing-lint-overlap.diff # 既存 oxlint/biome が拾える系 (overlap 率測定用)
        ├── eval7-style-only.diff          # style のみ (lint screen の対象外想定)
        └── eval8-large-refactor.diff      # 大規模変更 (LLM の文脈長限界テスト)
```

#### eval 1 件の構造案

```json
{
  "id": 1,
  "name": "unused-import-detection",
  "input_diff": "evals/files/eval1-unused-import.diff",
  "claude_code_baseline": {
    "model": "claude-sonnet-4-7",
    "captured_at": "2026-05-08T...",
    "lint_findings": [
      {"severity": "minor", "rule": "unused-import", "file": "src/foo.rs", "line": 3,
       "issue": "use std::collections::HashMap; が未使用", "suggestion": "削除"}
    ],
    "screen_decision": "auto_fix"
  },
  "expectations": [
    "mistral:7b 出力の lint_findings 配列に unused-import 系 finding が 1 件含まれる",
    "screen_decision が 'auto_fix' (Claude baseline と一致)",
    "false positive (実在しない issue) の出力なし",
    "latency ≤ 10s/件",
    "JSON parse 成功 (fallback rate 0)"
  ]
}
```

#### 検証手順

1. **claude_code_baseline 収集 (人間 + Claude Code)**: 各 diff を Claude Code 自身に読ませて lint findings を生成し、人間が確認して固定保存 (eval JSON 内に永続化)
2. **mistral:7b runner 構築**: 既存 cli-finding-classifier を再利用 or 拡張 (`--mode lint-screen` を追加して diff 入力 + lint 出力 prompt を実装)
3. **mistral:7b run**: 全 eval に対して runner 実行、output 取得
4. **突合**: structured field (severity / rule / line) で agreement rate 計算、prose field (issue / suggestion) で string similarity / contains 判定
5. **判定**: agreement rate ≥ 閾値 (例 80%) で「§8.E 着手 GO」、未達なら §8.D prompt v2 先行で再 evals

### 11.5 evals 形式の利点 (PR-based との比較)

| 観点 | PR-based dogfood (§A-2) | evals 形式 (§11) |
|---|---|---|
| 入力制御 | ❌ CR / GitHub / rate-limit に依存 | ✅ 完全制御 (固定 fixture) |
| 再現性 | ❌ 同じ PR でも CR 出力が変動 | ✅ 同じ diff で同じ expected_output |
| 速度 | ❌ wakeup ループで PR あたり 5-30 分 | ✅ 1 eval 数秒、5-10 件で 1 分以内 |
| 統計的有意性 | ❌ 5 PR で実データ N=2 | ✅ 任意の N を確保可能 (10-30 件) |
| 阻害要因 | ❌ 3 種の noise が混入 | ✅ noise なし、classifier 妥当性に focus |
| 失敗 mode の分離 | ❌ 「classifier 不妥当」と「投入経路 broken」が混ざる | ✅ classifier 妥当性のみ純粋検証 |
| 実環境 fidelity | ✅ 本番 review サイクルそのもの | ❌ 実 CR 挙動を 100% 模倣はできない |
| 品質定義 | ⚠ post-hoc (実走後に評価) | ✅ ex-ante (expected_output で品質を定義) |
| 範囲制御 | ❌ 全部入りで時間がかかる | ✅ 必要な範囲に絞れる |

→ **classifier 妥当性検証 phase は evals 形式が圧倒的に優位**。実環境 fidelity が必要な phase (採用後の運用試験) は別途 PR-based dogfood で補う **2 段階アプローチ** が適切。

### 11.6 §8.E 着手の進め方 (見直し版)

旧計画 (§8.E 元の依存): 「§A-2 dogfood 完了 + 判定基準達成」 → §A-2 で判定不能となったため block。

#### 新計画 (§A-2 retrospective を反映)

1. **Phase a — evals infrastructure 整備** ✅ **本セッション (2026-05-08) で land**:
   - 配置: 既存 `src/cli-finding-classifier/` を再利用 (新 crate は不要、`--mode lint-screen` 追加で対応)
   - fixtures: `src/cli-finding-classifier/evals/files/` に 6 件 (initial scope) — unused-import / deep-nesting / magic-number / clean (FP 検知) / multi-issue / existing-lint-overlap
   - eval JSON: `src/cli-finding-classifier/evals/lint-screen-evals.json` に Claude Code baseline + expectations を固定
   - prompt: `src/cli-finding-classifier/prompts/lint-screen.txt` (出力契約 = `{ lint_findings, screen_decision }`)
   - runner: `cli-finding-classifier --mode lint-screen` で diff stdin → LintScreenResult JSON stdout (fallback 経路は classify mode と同じ `human_review + fallback_reason`)
   - compare: `tests/lint_screen_evals.rs` integration test (常時実行 schema/structure validation 12 件 + `#[ignore]` 付き Phase b 用 end-to-end runner)
   - **追加サブタスクの先送り**: style-only / large-refactor 系 fixture (§11.4 の eval7-8) は Phase b 結果を見てから追加判断
2. **Phase b — 判定 GO/NO-GO**:
   - 実行: `cargo test -p cli-finding-classifier --test lint_screen_evals -- --ignored --nocapture run_lint_screen_against_all_fixtures`
   - agreement ≥ 80% → §8.E 着手 GO
   - 未達 → §8.D prompt v2 先行で再 evals → 改善後再判定
3. **Phase c — §8.E 実装**: takt facet `ollama-lint-screen` 追加、初期 dogfood で実 PR の lint 一次フィルタ動作確認
4. **Phase d — PR-based 実環境 dogfood**: §A-2 形式で 3-5 PR で token 削減 / latency 累積を計測 (この phase は evals で妥当性確保済のため short)

### 11.7 進め方の総括 — 「いっぺんに進めすぎず、検証→計測→拡張のサイクル」

#### §A-2 dogfood の反省 (本セッションでユーザーから指摘)

> 「いっぺんに進めすぎて、結果を制御できていないように見えます」

1 セッションで 5 PR を連続 land する負荷を取ってしまったため、阻害要因 3 種に振り回されて主目的 (classifier 妥当性検証 → §8.E 着手 GO/NO-GO 判定) を見失った。

#### 新しい検証作業の運用原則

1. **目的に最短 reach する検証手段を選ぶ**: 検証目的によって手段を分ける
   - **「妥当性確認」**: evals 形式 (固定入力 + 期待出力 + 突合)
   - **「実運用効果計測」**: PR-based dogfood (token / latency / 累積影響)
2. **小さく早いサイクルから**: evals 5-10 件 → 結果評価 → 改善 or 拡張
3. **阻害要因は副産物として記録**: §A-2 で発見した「review body 抽出漏れ」「rate-limit」「findings ゼロ」は §8.E の運用 phase で改めて対処、検証 phase では noise として除外
4. **evals は組織資産**: §8.D / §8.F でも再利用可能 (各 facet/skill 用の eval セットを段階的に整備)
5. **dogfood と evals は補完関係**: どちらか一方ではなく目的に応じた使い分け、検証は evals 先行 + 運用は dogfood 補完

#### 結論: §8.E 着手の前提条件 (改訂版)

| 旧 | 新 |
|---|---|
| §A-2 dogfood 完了 + 判定基準達成 | §11 evals (Phase a) で agreement rate ≥80% 達成 |
| classifier 起動率の制約あり | classifier 起動率 100% (固定入力で必ず起動) |
| 期間: 数日 (5 PR の land サイクル) | 期間: 半日〜1 日 (evals + 突合) |

### 11.8 関連リンク

- `analyze-pr` skill evals (参照モデル): `E:\work\claude-code-skills\analyze-pr\evals\evals.json` + `trigger_eval.json` + `files/eval*.diff`
- `cli-finding-classifier` (ADR-038): mistral:7b runner として再利用可能
- §A-2 計測ログ: PR-based dogfood の実測値、本 §11 の retrospective ベース
- §8.E (本ファイル §8): lint screen facet の元計画、本 §11 で着手 phasing を改訂

## 関連リンク

- [ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止](adr/adr-018-pr-monitor-takt-migration.md)
- [ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略](adr/adr-020-takt-facets-sharing.md)
- [ADR-034: CodeRabbit 監視・対話の自動化戦略 — Bundle a 設計根拠](adr/adr-034-coderabbit-auto-monitoring.md)
- [ADR-038: ローカル LLM による CodeRabbit findings classification](adr/adr-038-local-llm-finding-classification.md) — 本ファイル提案 2 の land 結果
