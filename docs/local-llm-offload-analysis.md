# ローカル LLM オフロード — 残作業計画 (Phase b 以降)

> **位置づけ**: 本ファイルは「残作業の **次に何をするか** だけ」を持つ実行計画。完了済みの分析・実装・dogfood 計測・retrospective は [local-llm-offload-history.md](local-llm-offload-history.md) に切り出した。
>
> **状態**: 試験運用 (Phase a 完了 = PR #130 land / Phase b 完了 = conditional GO 2026-05-08, PR #131 / Phase c MVP 完了 = PR #132 land 2026-05-08 / **Phase c+ Bundle i 完了 = PR #135 land 2026-05-09 / §8.D 完了 = PR #136 land 2026-05-09 (num_ctx 8192、agreement 86.7% / verdict GO)**、Phase d 着手前提条件は全て充足済、kickoff 待機)。
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

### Phase c — §8.E 実装 (lint screen facet) ✅ **MVP land in PR #132 (2026-05-08)**

最終的に「takt facet」ではなく **`cli-push-runner` の Rust stage** として実装 (理由: takt facet は Sonnet で動くため Claude tokens 節約という主目的と矛盾、push-runner step なら mistral:7b 直接呼出で目的達成)。

#### 実装したもの (PR #132)

- 配置: `src/cli-push-runner/src/stages/lint_screen.rs` (新 stage)
- 起動: `cli-finding-classifier.exe --mode lint-screen` を subprocess で呼び出し、stdin に diff、stdout に LintScreenResult JSON
- 出力: `.takt/lint-screen-report.md` (markdown table 形式、`severity / rule / file / line / issue / suggestion` を pipe escape 付きで出力)
- パイプライン位置: `quality_gate → diff → lint_screen → takt → push` (gating なし、report のみ)
- config: `[lint_screen]` section を `push-runner-config.toml` に追加 (default `enabled = false`、試験運用 opt-in)
- fallback: exe 不在 / diff 空 / diff 過大 (`max_diff_lines = 5000` default) / timeout (`timeout_secs = 60` default) / Ollama down / JSON parse 失敗 → すべて skip + warn (push を block しない)
- reviewer 連携: `.takt/facets/instructions/review-simplicity.md` に「lint-screen-report.md があれば advisory として読む」instruction 追加

#### Phase c MVP の意図的 scope 制限 (Phase b' conditional GO 反映)

- **gating しない**: lint-screen の決定 (`screen_decision`) を根拠に既存 reviewers をスキップしたり model を切替えたりしない (Phase b' 75% agreement なので誤指摘リスク回避)
- **auto-fix 経路なし**: `auto_fix` 判定でも実コードに自動 patch を当てない。reviewer の context 補強のみ
- **default OFF**: 手動 opt-in を踏まないと起動しない (ADR-038 試験運用配下)

#### Phase c MVP smoke で観測した重要事象

868 行の現実 PR diff (Phase c 自身) を流したところ、mistral:7b の JSON 出力が壊れた:

```json
{"lint_findings":[],"screen_decision":"human_review","fallback_reason":"ollama error: parse: JSON parse error: missing field `screen_decision`"}
```

= mistral:7b は大規模 diff で **structured output schema を欠落** させがち。Phase b' eval fixtures (10-30 行/件) では出ない failure mode。fallback path が graceful に処理し push pipeline をブロックしない設計が機能した一方、**Phase d 投入前に scale-aware fixture (200+ 行) で改善ループを回す必要** が判明。

#### Phase c+ (Phase d 着手前の必須 follow-up、Bundle i) ✅ **land in PR #135 (2026-05-09)**

PR #132 post-merge-feedback で採用された 3 件をすべて land:

- **順位 91 (`[lint_screen]` config parse テスト)** ✅ — `src/cli-push-runner/src/config.rs` に 5 tests 追加 (full fields / minimal only enabled / absent yields None / numeric defaults / string defaults)。silent field rename 時に compile/test 段階で気付ける構造に
- **順位 92 (scale-aware eval fixtures 200+ 行)** ✅ — eval13 (5 file / 280 行) / eval14 (3 file / 153 行) / eval15 (1 file / 208 行) の 3 fixture を追加。`lint-screen-evals.json` に baseline (auto_fix lane × 13 findings 計) を登録、count test を rename + 上限緩和 (`>=15` floor)、Bundle i 実体スモーク test を追加
- **順位 93 (coding-style.md partial fix 例追記)** ✅ — `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に「変更差分外への partial fix 再発」anti-pattern を追加 (PR #94 / #111 / #132 を inline cite)

##### Bundle i dogfood 結果 (mistral:7b / temperature=0)

| 指標 | 値 |
|---|---|
| **decision agreement rate** | **11/15 = 73.3%** (Phase b' 75% から marginal 劣化、ただし fixture が設計通り failure mode を再現) |
| aggregate precision / recall | 76.2% / 51.6% |
| latency p50 / p95 | 4591ms / 8370ms |
| verdict | CONDITIONAL-GO (§8.E auto_fix lane に限定) |

**Bundle i fixtures が捕捉した failure mode** (Phase d 投入前に reproducible 化できた点が本 bundle の中核成果):

| eval | scale | observed failure | 設計意図との一致 |
|---|---|---|---|
| 13 | 5 file / 280 行 | `missing field 'screen_decision'` → fallback (human_review) | ✅ PR #132 smoke (868 行 diff) と同型の top-level field omission を再現 |
| 15 | 1 file / 208 行 | `missing field 'severity' at line 38` → fallback | ✅ 単 file 長尺での nested field omission を新規捕捉 |
| 14 | 3 file / 153 行 | JSON 完全だが recall 33% (1/3 TP) | mid-scale で findings 取りこぼし顕在化 |

**73.3% < 75% 解釈**: regression ではなく fixture が設計した stress test の成功。todo6.md L164 「未達理由が文書化される」branch を満たす形で land。Phase d 投入前の data 確保により「mistral:7b は scale で JSON schema を壊す」という事実が定量化された。

##### CodeRabbit Major fix (PR #135 review)

- `eval_set_loads_and_has_at_least_phase_b_prime_baseline_count` の floor を `>= 12` → `>= 15` に変更 (Bundle i baseline = 15 fixtures を下限固定、既存 fixture 削除を regression として検出)

##### Phase d 着手前提条件の充足状況

Bundle i land で以下が揃った:

- (a) `[lint_screen]` config silent failure 防止 (順位 91)
- (b) scale-aware fixtures による failure mode の reproducible measurement (順位 92)
- (c) cross-file partial fix anti-pattern の global rule 化 (順位 93)

**残る最終 gate** (PR #136 で land 完了): JSON 完全性問題への一次対策は当初 §8.D 'v4 prompt 改訂ループ' と命名されていたが、root cause 検証 (raw Ollama output dump) で prompt 設計ではなく `num_ctx` default (4096) 超過と確定し、`lib-ollama-client` の `num_ctx` を 8192 に拡張する形で解決。dogfood で agreement 73.3% → 86.7% (verdict CONDITIONAL-GO → GO) に昇格。詳細は [docs/todo6.md](todo6.md) Bundle i 参照。

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

### §8.E — 提案 1 (lint screen facet) ✅ MVP land 済 (PR #132、2026-05-08) + Phase c+ Bundle i land (PR #135、2026-05-09)

- **当初目的**: takt の新 facet `ollama-lint-screen` で pre-push 時に diff の lint 一次フィルタを mistral:7b に逃す
- **実装方針の変更**: takt facet (Sonnet 動作) ではなく **`cli-push-runner` の Rust stage** として実装 (Claude tokens 節約という主目的との整合)。詳細は §1 Phase c 参照
- **MVP scope**: report 出力のみ (gating なし、auto-fix なし、default OFF)。conditional GO (75%) を反映した安全 scope
- **Phase c+ (Bundle i)** ✅: scale-aware fixture (順位 92 / eval13/14/15) / config parse test (順位 91 / 5 tests) / coding-style anti-pattern 追記 (順位 93) を land 済。dogfood で 73.3% agreement / fallback 2/15 観測 (eval13: `missing field 'screen_decision'` / eval15: `missing field 'severity'`)
- **Phase d 着手前提**: 大規模 diff の JSON 不完全問題への一次対策 ✅ PR #136 で land 完了 (当初 'v4 prompt 改訂' と命名されていたが root cause 検証で `num_ctx` default 超過と確定、`lib-ollama-client` 側の修正に pivot)。Bundle i fixtures は改善ループの reference point として固定済み

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
# 1. master 最新化 (Phase a/b/c MVP / Phase c+ Bundle i まで land 済)
jj git fetch && jj edit master

# 2. Phase a/c/c+ infrastructure が master に反映済か確認
ls src/cli-finding-classifier/evals/lint-screen-evals.json
ls src/cli-push-runner/src/stages/lint_screen.rs
ls src/cli-finding-classifier/evals/files/eval13-large-refactor-real.diff  # Bundle i
ls src/cli-finding-classifier/evals/files/eval14-mid-mixed.diff            # Bundle i
ls src/cli-finding-classifier/evals/files/eval15-syntax-stress.diff        # Bundle i
cargo test -p cli-finding-classifier --test lint_screen_evals    # schema validation 20 件 pass (Bundle i で 12→20)
cargo test -p cli-push-runner                                    # 53+ 件 pass (Bundle i で +5 lint_screen config tests)

# 3. Ollama 起動確認 (Phase b 再走 / Phase c smoke / Phase d dogfood で必要)
curl -s http://localhost:11434/api/tags | jq '.models | map({name, size})'

# 4. agreement 再現確認 (Bundle i 後の baseline = 73.3% deterministic)
cargo test -p cli-finding-classifier --test lint_screen_evals -- \
  --ignored --nocapture run_lint_screen_against_all_fixtures
# 期待: agreement 11/15 = 73.3% / fallback 2/15 (eval13 + eval15) / verdict CONDITIONAL-GO

# 5. Phase c MVP smoke (lint_screen step を一時的に enabled=true で起動)
# push-runner-config.toml [lint_screen] enabled = true に設定 (commit しない)
# 任意の小さい diff で pnpm push して .takt/lint-screen-report.md が生成されるか確認
```

#### 次に何をするか (優先度順)

1. **Phase d kickoff prep** (session 内で完結可能): `push-runner-config.toml [lint_screen] enabled = true` への切替判断、metrics 計測スクリプト整備、dogfood 運用ガイド作成、過去 dogfood 阻害要因 3 種 (findings ゼロ / review body 抽出漏れ / rate-limit) への対処設計。**前提条件 (a)-(d) は PR #135 + PR #136 で全充足済**
2. **Phase d 実 dogfood** (long-running、数日〜数週間): kickoff 後の通常 PR 3-5 件で lint_screen の token 削減 / latency / 大規模 diff JSON 完全性を実観測。`feedback_dogfood_evals_two_phase` (evals → dogfood の 2 段階) の dogfood 段階に該当
3. **Phase d 結果集約**: 計測結果から §8.E 採用 / §8.F 着手 / kill-switch を判定。dogfood 完了後

優先度低の独立 task (Phase d を block しない):
- **§8.D**: classify mode の `normalized_issue` 言語制約強化 (low priority、ROI ★)
- **§8.F**: PR body draft (§8.E 採用 = Phase d 完了 後)
- **順位 97** (todo): `with_num_ctx(X)` override serialization 検証 test (PR #136 T2-#1、Phase d で num_ctx tweak する局面に入る前の安全網)

§8.D / §8.E / §8.F の実装に着手する場合は、本ファイル該当節 + history §10.6/§10.7 を参照。

## 関連リンク

- [local-llm-offload-history.md](local-llm-offload-history.md) — 完了済みの分析・実装・dogfood 計測・retrospective
- [ADR-038: ローカル LLM による CodeRabbit findings classification](adr/adr-038-local-llm-finding-classification.md) — 提案 2 (cli-finding-classifier) の land 結果、Phase a evals infrastructure の re-use 対象
- [ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止](adr/adr-018-pr-monitor-takt-migration.md)
- [ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略](adr/adr-020-takt-facets-sharing.md)
- [ADR-034: CodeRabbit 監視・対話の自動化戦略 — Bundle a 設計根拠](adr/adr-034-coderabbit-auto-monitoring.md)
