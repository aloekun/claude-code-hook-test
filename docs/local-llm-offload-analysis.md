# ローカル LLM オフロード — 残作業計画 (Phase b 以降)

> **位置づけ**: 本ファイルは「残作業の **次に何をするか** だけ」を持つ実行計画。完了済みの分析・実装・dogfood 計測・retrospective は [local-llm-offload-history.md](local-llm-offload-history.md) に切り出した。
>
> **状態**: 試験運用 (Phase a 完了 = PR #130 land / Phase b 完了 = GO 達成 2026-05-08, PR #131、Phase c/d は未着手)。
>
> **引退条件**: 以下のいずれかで本ファイルを削除する (docs-governance.md retirement workflow 準拠)。`local-llm-offload-history.md` も同タイミングで判断する。
> - 残作業 (§8.D / §8.E / §8.F, §1 Phase b/c/d) が **すべて land または却下** された場合 → permanent value (採用された設計判断、却下理由) を ADR-038 に migrate して両ファイルを削除
> - **6 ヶ月経過** (= 2026-11-08) しても §8.E (lint screen facet) の採否が決まらない場合 → 採用見込みなしとみなして両ファイルを削除
>
> **本ファイルだけで Phase b 以降を再開できることを目的とする**。背景 (なぜこの構造に至ったか) は history ファイル参照。

## 1. Phase a-d 実行計画 (§11.6 から継承、Phase a 完了)

旧計画 (§A-2 PR-based dogfood + §8.E 起動) は dogfood 阻害要因 3 種で判定不能となった。詳細経緯は [local-llm-offload-history.md](local-llm-offload-history.md) §11.1-§11.5 参照。現在は **evals 形式 (固定 diff fixture + Claude Code baseline + agreement 突合) → 採用後 PR-based dogfood で運用効果計測** の 2 段階アプローチに移行済。

### Phase a — evals infrastructure 整備 ✅ **land in PR #130 (2026-05-08)**

- 配置: `src/cli-finding-classifier/` を再利用 (新 crate 不要、`--mode lint-screen` 追加で対応)
- fixtures: `src/cli-finding-classifier/evals/files/` に 6 件 — unused-import / deep-nesting / magic-number / clean (FP 検知) / multi-issue / existing-lint-overlap
- eval JSON: `src/cli-finding-classifier/evals/lint-screen-evals.json` に Claude Code baseline + expectations を固定 (agreement_threshold = 0.8)
- prompt: `src/cli-finding-classifier/prompts/lint-screen.txt` (出力契約 = `{ lint_findings, screen_decision }`)
- runner: `cli-finding-classifier --mode lint-screen` で diff stdin → LintScreenResult JSON stdout (fallback 経路は classify mode と同じ `human_review + fallback_reason` パターン継承)
- compare: `tests/lint_screen_evals.rs` integration test (常時実行 schema/structure validation 12 件 + `#[ignore]` 付き Phase b 用 end-to-end runner 1 件)

### Phase b — 判定 GO/NO-GO 🟡 **conditional GO 達成 (2026-05-08)**

**最終結果**: agreement rate = **9/12 = 75.0%** (threshold 80%、temperature=0 で deterministic) → **🟡 conditional GO (§8.E auto_fix lane に限定して着手)**

#### iteration 履歴 (Phase b → Phase b')

| iteration | N | prompt | agreement | 備考 |
|---|---|---|---|---|
| v1 (Phase b 初回) | 6 | original (PR #130 land 時点) | 50.0% | NO-GO |
| v2 (Phase b' canonical rules) | 12 | + canonical / decision tree / few-shot 4 件 | 41.7% | NO-GO (informational バイアス露呈) |
| v3 (Phase b' anti-hallucination) | 12 | + "default to no findings" preamble + empty-finding example 4 件 | 75.0% | conditional GO |
| v3 + baseline fix | 12 | (eval 6 baseline informational → auto_fix) | 83.3% | 単発 run (variance 内、再現性なし) |
| **v3 + temperature=0** | 12 | (PR #131 CR 対応 + eval8 fixture clean up) | **75.0% (再現確認)** | **conditional GO** |

#### 改善の本質

- **v2 → v3 (+33pt 改善)**: prompt に "Most real-world diffs add ZERO lint issues. ... A wrong 'no finding' output is far less harmful than a hallucinated finding." の preamble を追加し、4 件の empty-finding example (clean / comment-only / test-cfg / whitespace-only) を補強。LLM が `informational` 列を選べるようになった
- **baseline fix**: eval 6 の "全 finding が oxlint 既存範囲なら informational" 概念は LLM へのメタ判定要求として過剰。lint screen の責務を「mechanical findings の検出」に統一し、`informational` は findings ゼロのみに限定 (シンプルな設計)
- **temperature=0 で variance 排除**: default 0.1 では 50%-83% で振れる。reproducible な measurement のため `with_temperature(0.0)` を必須化、honest baseline = 75%
- **Major #4 (prompt examples diff header) revert**: full diff header (`--- a/<path>` `+++ b/<path>`) を追加すると attention dilution で 33pt 退行 (75% → 50% 帯)。anti-hallucination preamble の効果が失われるため revert

#### 残る 2 件の disagreement (LLM 側の限界)

- eval 5 (multi-issue): baseline=human_review → LLM=auto_fix (4 issue 中 deep-nesting を取りこぼし、recall 75%)
- eval 10 (nesting-boundary): baseline=informational → LLM=human_review (4 levels の境界判定を過剰反応)

これらは漸近的な改善余地はあるが、Phase c 着手の前提条件 (agreement ≥ 80%) は達成済のため scope 外。Phase d (PR-based dogfood) で実観測が必要。

#### 再走方法 (再現性)

- **前提**: Ollama がローカル起動 + `mistral:7b` モデル pull 済 (`curl http://localhost:11434/api/tags` で確認)
- **実行**: `cargo test -p cli-finding-classifier --test lint_screen_evals -- --ignored --nocapture run_lint_screen_against_all_fixtures`
- **出力**: per-eval の precision / recall / F1 / 正規化 P/R / TP/FP/FN + aggregate metrics + decision confusion matrix (3x3) + GO/NO-GO 判定

### Phase c — §8.E 実装 (lint screen facet)

- takt facet `ollama-lint-screen` を追加し、pre-push 時に diff の lint 一次フィルタを mistral:7b に逃す
- 初期 dogfood で実 PR の lint 一次フィルタ動作確認
- 詳細仕様は §8.E 参照

### Phase d — PR-based 実環境 dogfood

- §A-2 形式で **3-5 PR** で token 削減 / latency 累積を計測 ([local-llm-offload-history.md](local-llm-offload-history.md) §A-2 の手順を流用)
- evals で妥当性確保済のため **short** で済む (旧計画の 5 PR より少なくてよい)
- 過去 dogfood の阻害要因 3 種 (findings ゼロ / review body 抽出漏れ / rate-limit) は本 phase で改めて対処

## 2. 残作業の詳細

### §8.D — プロンプト v2: `normalized_issue` 言語制約強化 (low priority)

- **目的**: dogfood で観測された「mistral:7b が日本語指示でも英語混じりで返す」問題を改善
- **作業**: `prompts/classify.txt` でより強い言語固定指示 + few-shot examples を追加
- **依存**: なし (ただし Phase b で agreement 未達の場合は同 prompt 改訂で `prompts/lint-screen.txt` も対象になる)
- **見積**: 半日 (prompt 変更 + 簡易ベンチで安定性検証)
- **ROI**: ★ (実害は小、UX 微改善)

### §8.E — 提案 1 (lint screen facet) — Phase b GO 後

- **目的**: takt の新 facet `ollama-lint-screen` で pre-push 時に diff の lint 一次フィルタを mistral:7b に逃す
- **依存**: **Phase b で agreement rate ≥ 80% 達成** (旧依存の §A-2 dogfood は無効化、Phase a evals に置換)
- **見積**: 1〜2 日
- **ROI**: 提案 1 として中程度。Phase b 集計次第で優先度変動 (基準未達なら §8.D 先行 → 再 evals 経路)

### §8.F — 提案 3 (PR body draft) — §8.E 採用後

- **目的**: `prepare-pr` skill で `jj diff` 要約を mistral:7b で前処理し、Claude への入力 token を圧縮
- **依存**: §8.E が land して `lib-ollama-client` の運用知見が貯まった後
- **見積**: 半日
- **ROI**: input token 削減への寄与は最大ライン (cache 倍率の影響が大きい領域)

## 3. 採用 / 却下時の処理

詳細手順 (採用時の master 反映、却下時の物理削除 checklist) は **[local-llm-offload-history.md](local-llm-offload-history.md) §10.6** 参照。要点のみ:

- **採用 (基準達成)**: ADR-038 を「採用」に昇格 + Phase a/c/d 成果を master へ反映 + 本ファイル + history ファイル削除を判断
- **§8.D 先行で再判定**: prompt v2 で `prompts/lint-screen.txt` を改訂 → Phase b 再実行
- **却下 (基準未達 + 改善見込みなし)**: master から LLM 関連実装を物理削除する kill-switch PR を起動 (history §10.6 C のチェックリスト)、ADR-038 を「却下」に更新

## 4. 別セッションでの再開チェックリスト

```bash
# 1. master 最新化
jj git fetch && jj edit master

# 2. Phase a infrastructure が master に反映済か確認
ls src/cli-finding-classifier/evals/lint-screen-evals.json
cargo test -p cli-finding-classifier --test lint_screen_evals    # schema validation 12 件 pass

# 3. Ollama 起動確認 (Phase b 用)
curl -s http://localhost:11434/api/tags | jq '.models | map({name, size})'

# 4. Phase b 実行 (実 LLM agreement rate 計測)
cargo test -p cli-finding-classifier --test lint_screen_evals -- \
  --ignored --nocapture run_lint_screen_against_all_fixtures
```

§8.D / §8.E / §8.F の実装に着手する場合は、本ファイル該当節 + history §10.6/§10.7 を参照。

## 関連リンク

- [local-llm-offload-history.md](local-llm-offload-history.md) — 完了済みの分析・実装・dogfood 計測・retrospective
- [ADR-038: ローカル LLM による CodeRabbit findings classification](adr/adr-038-local-llm-finding-classification.md) — 提案 2 (cli-finding-classifier) の land 結果、Phase a evals infrastructure の re-use 対象
- [ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止](adr/adr-018-pr-monitor-takt-migration.md)
- [ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略](adr/adr-020-takt-facets-sharing.md)
- [ADR-034: CodeRabbit 監視・対話の自動化戦略 — Bundle a 設計根拠](adr/adr-034-coderabbit-auto-monitoring.md)
