# Phase d (lint_screen 実環境 dogfood) 運用ガイド

> **位置づけ**: ADR-038 試験運用配下の §8.E (lint screen facet) を 3-5 通常 PR で dogfood し、**本採用 / 却下** を判定するための operational guide。
>
> **対象**: `cli-push-runner` の `lint_screen` stage (PR #132 / #135 / #136 で land 済)。`cli-finding-classifier` の dogfood (`local-llm-offload-history.md` §A-2) とは **別 feature**、CR 非依存。
>
> **状態**: 試験運用 (kickoff prep land 後、実 dogfood は次回以降の通常 PR で実施)。
>
> **引退条件**: Phase d 完了 (3-5 PR で実観測) → §8.E 採否判定 → ADR-038 を「採用」or「却下」に昇格 → 本ファイル削除 + analysis.md / history.md も同タイミングで再評価。

## 1. Setup (session-only opt-in)

`push-runner-config.toml` は default OFF のまま。dogfood する session で **手動で `enabled = true` に切り替え (commit しない)**。

```bash
# 1. Ollama 起動確認
curl -s http://localhost:11434/api/tags | jq '.models | map({name, size})'
# 期待: mistral:7b が含まれる

# 2. config 切替 (commit しない、session 内のみ)
# push-runner-config.toml の [lint_screen] section で
#   enabled = false → enabled = true
# 編集後、jj diff で確認 (push 時に意図せず commit に乗らないよう注意)

# 3. cli-finding-classifier.exe deploy 確認
ls -la .claude/cli-finding-classifier.exe
# 期待: ファイル存在、~2.2MB

# 4. dogfood 完了後、必ず enabled = false に戻す (revert)
jj diff push-runner-config.toml  # 確認
# 編集して enabled = false に戻す、または jj restore push-runner-config.toml
```

**意義**: kill-switch が即可能、他人 / 派生プロジェクトの push に影響なし、設計 (default OFF, 試験運用 opt-in) との整合性。

## 2. 計測 (各 dogfood PR で実施)

ユーザー判断 (Phase d kickoff、2026-05-10) で以下 3 metrics を採用:

### 2-1. lint_screen latency p50/p95

```bash
# push 後、push-runner log から抽出
grep "lint-screen.*出力:" .takt/runs/<latest>/logs/*.jsonl
# 期待 format: "出力: .takt/lint-screen-report.md (Ns)"
```

3-5 PR 集計後に p50 / p95 を算出。**baseline**: Bundle i evals dogfood で p50=4.5s / p95=8.4s (eval fixtures、num_ctx=8192)。

### 2-2. fallback rate

```bash
# .takt/lint-screen-report.md の冒頭に "fallback_reason" が記録されていれば fallback 経路
grep -l "fallback_reason" .takt/lint-screen-report.md
# 5 PR 中の fallback 件数を数える
```

**threshold**: kill-switch = fallback > 50% (3 / 5 PR で fallback 発生で停止)。

### 2-3. Claude session input token 削減効果

**現状実装 caveat**: lint_screen は `.takt/lint-screen-report.md` 出力のみで、Claude には自動転送されない。token 削減効果を計測するには以下のいずれか:

- (a) **手動転送**: dogfood 中に Claude が `.takt/lint-screen-report.md` を Read して advisory として参照する (実 dogfood で運用試行)
- (b) **将来実装**: takt facet `review-simplicity.md` instruction で advisory 読み込みを自動化 (PR #132 で部分的に実装済、Phase d 効果次第で深化判断)
- (c) **計測のみ**: Claude session の `/cost` 出力を dogfood 前後で記録 (lint_screen 利用 vs 非利用で比較、ただし他要因の混入あり)

Phase d 期間中は **(a) 手動転送 + (c) /cost 計測** の組合せで **質的傾向**を観察。定量化は (b) 実装後に再計測。

## 3. Kill-switch criteria (fallback rate > 50%)

3 / 5 PR (= 60%) で fallback が発生したら **即停止**:

1. config を `enabled = false` に戻す (jj restore push-runner-config.toml で確実に revert)
2. fallback 原因を `.takt/lint-screen-report.md` の `fallback_reason` から特定:
   - `ollama error: ...` → Ollama 側問題 (down / model unloaded / 通信)
   - `JSON parse error: missing field ...` → mistral 出力崩壊 (順位 98 = num_ctx overflow detection で診断強化予定)
   - `diff over limit` → `max_diff_lines` 設定不足 (config tweak で再 dogfood 可)
   - `timeout` → `timeout_secs` 設定不足 (latency 増を許容するか re-tune)
3. ADR-038 §試験運用→採用条件の **未達** として retrospective を `local-llm-offload-history.md` に追記
4. §8.D 改善 (prompt v2) または別技術 (llama2:13b 等) への switch 判断

## 4. 過去 dogfood 阻害要因の scope 再確認

**`local-llm-offload-history.md` §11.2 の 3 obstacles は本 Phase d では scope 外**:

| 旧 obstacle | 旧 scope (classifier) | 本 Phase d (lint_screen) との関係 |
|---|---|---|
| findings ゼロ | CR が APPROVE で classifier 入力なし | ❌ scope 外: lint_screen は CR 非依存、`jj diff` を直接読む |
| review body 抽出漏れ | check-ci-coderabbit の parser scope 限界 | ❌ scope 外: lint_screen は CR review を読まない |
| CR rate-limit | per-hour commit quota で review blocked | ❌ scope 外: lint_screen は pre-push、CR より前に走る |

代わりに **lint_screen 固有の懸念**:

- **Ollama 起動状態**: dogfood session 開始時に確認 (Setup §1)
- **mistral:7b 出力安定性**: num_ctx 8192 で agreement 86.7% (eval fixtures)、実 PR diff で再計測する (本 Phase d の主目的)
- **push pipeline UX impact**: latency p95 監視 (現状 ~8s で許容範囲)

## 5. Phase d 完了 → 結果集約 → §8.E 採否判定

3-5 PR 完了後:

1. metrics 集計 (latency / fallback / token 質的傾向) を `local-llm-offload-history.md` の dogfood 計測ログ (§A-2 と同形式) に追記
2. ADR-038 §試験運用→採用条件 (5 PR 以上 / token 削減 / classification 妥当性) との突合
3. 採用判定:
   - **採用**: ADR-038 を「採用」に昇格 + 派生プロジェクト deploy 計画 + 本ガイド削除
   - **却下**: ADR-038 を「却下」に更新 + lint_screen stage 物理削除 PR + 本ガイド削除
   - **継続**: 課題 ID 化 + 改善 task として todo 系列に追加 + 次 dogfood 計画策定

## 関連リンク

- [docs/local-llm-offload-analysis.md](local-llm-offload-analysis.md) — Phase d を含む実行計画 (ephemeral)
- [docs/local-llm-offload-history.md](local-llm-offload-history.md) — §A-2 (classifier dogfood retrospective) / §11 (evals 形式への切替)
- [ADR-038](adr/adr-038-local-llm-finding-classification.md) — 提案 1 / 2 / 3 の元設計、本採用判定の base
