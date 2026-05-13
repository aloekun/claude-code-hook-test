# ローカル LLM オフロード — 残作業計画 (Phase b 以降)

> **位置づけ**: 本ファイルは「残作業の **次に何をするか** だけ」を持つ実行計画。完了済みの分析・実装・dogfood 計測・retrospective は [local-llm-offload-history.md](local-llm-offload-history.md) に切り出した。
>
> **状態**: 試験運用 (Phase a 完了 = PR #130 land / Phase b 完了 = conditional GO 2026-05-08, PR #131 / Phase c MVP 完了 = PR #132 land 2026-05-08 / **Phase c+ Bundle i 完了 = PR #135 land 2026-05-09 / §8.D 完了 = PR #136 land 2026-05-09 (num_ctx 8192、agreement 86.7% / verdict GO) / Phase d kickoff prep 完了 = 2026-05-10 / Phase d Round 1 完遂 = 2026-05-12 (D-1〜D-3、実 dogfood は D-3 単独 1 data point) / Phase d Round 2 進行中 = 2026-05-13 (D-4 完遂 → PR #150 merged、累積 3 data points / 5 PR 計画、残 D-5/D-6/D-7)** ([docs/local-llm-offload-phase-d-guide.md](local-llm-offload-phase-d-guide.md) 参照))。
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

#### 次に何をするか (analysis.md 削除条件への critical path、2026-05-11 更新)

> **本ファイル削除条件**: Phase d 採否判定完了 (採用 or 却下) → ADR-038 を「採用」or「却下」に昇格 → follow-up タスクを permanent artifact (ADR / global rule / todo) に移管 → retirement workflow (`~/.claude/rules/common/docs-governance.md` §Retirement Workflow を参照、global config のため URL link なし) を実行 → 本ファイル削除。
>
> **進行方針 (2026-05-11、kill-switch 100% trend を踏まえた pivot)**: dogfood を一度止めて broken signal の repair (順位 98 = `num_ctx` overflow detection 診断) を最優先。診断 → root cause 特定 → fix → clean dogfood の順で進む。Bundle c-1 (旧 P-4) や Bundle c の他項目は **本 critical path 外** として通常 Tier 1 優先度で別途処理。

##### 🚀 Phase A: Diagnostic ✅ **完了 (PR #142、2026-05-11)**

**順位 98 実装完了** = `lib-ollama-client` の `generate_json` に `OllamaMetadata` (`prompt_eval_count` / `eval_count` / `num_ctx`) を組み込み、serde parse error 時に stderr へ warn log を emit する診断層を追加。`OllamaApi` trait の `generate_with_metadata` (default fallback あり、StubOllama は変更不要)、`emit_overflow_diagnostic` 関数で 90% 以上時に「num_ctx を増やす hint」を含める。16 unit test pass、cli-finding-classifier 経由でも warn log が stderr に出ることを smoke 確認。

##### 🔍 Phase B: Root cause identification ✅ **完了 (Phase A 即時 dogfood、2026-05-11)**

Phase A 実装後、PR #141 (P-3 = 187 行 mixed diff) を replay → **`prompt_eval_count: 8192 (vs num_ctx: 8192)` = 100% 到達を実機確認**。**真因 = num_ctx truncation で確定**。mistral の prompt が完全に context cap で truncate されて JSON output が完成せず `screen_decision` field 欠落の症状を引き起こしていた。仮説 2 候補 (num_ctx truncation / mistral 出力崩壊) のうち前者が真因と decisive 判定。

##### 🔧 Phase C: Root cause fix ✅ **完了 (PR #143、2026-05-11)**

`DEFAULT_NUM_CTX = 8192 → 16384` (initial) → 16384 でも 100% overflow を再観測 → `DEFAULT_NUM_CTX = 16384 → 32768` (mistral:7b theoretical max) に再増加。`lib_ollama_client` の lint test 17 件 pass、`cli-finding-classifier` evals 20 件 pass。副次的に `push-runner-config.toml` の `step_timeout = 180 → 600` に拡大 (num_ctx 増加で `cargo test -- --ignored` の 12 件 mistral invoke が long-running 化)。

**Phase C smoke dogfood** (32768 で 3 PRs replay):

| PR | Lines | Latency | Old (8192) | **New (32768)** |
|---|---|---|---|---|
| P-1 (#139) | 414 | 48s | fallback (truncation) | ✅ `auto_fix` (real classification) |
| P-2 (#140) | 275 | 50s | fallback (truncation) | ⚠️ fallback (`invalid severity: error` = contract violation、num_ctx ではなく mistral semantic 精度の別問題) |
| P-3 (#141) | 487 | 55s | fallback (truncation) | ✅ `auto_fix` (real classification) |

- **num_ctx truncation 起因 fallback: 3/3 → 0/3 (100% 解消)** ← Phase C 主目的を達成
- **総合 fallback rate: 3/3 (100%) → 1/3 (33%)** ← **Phase D 基準 (<50%) を classifier preview で達成**
- 残り 1/3 は mistral 出力の contract violation (Phase b' agreement 75% で説明可能な semantic 精度問題、別 phase で対応 / Phase D scope 外)

##### 🛠️ Phase D 前提整備 (順位 109) ✅ **完了 (PR #144、2026-05-11)**

`src/cli-push-runner/src/stages/lint_screen.rs` 改修: graceful fallback (exit 0) 時にも classifier stderr を `.takt/lint-screen-report.md` の `## Diagnostic` section に取込。Phase A 診断 warn log が **real pipeline 経由で visible** になる状態を確保。新 struct `ClassifierOutput { stdout, stderr }`、新 helper `render_diagnostic`、新規 smoke test 4 件 (TP / FP / edge case / parse-error path) で contract を seal。lint_screen tests 14/14 pass + workspace 全 cargo test pass。

##### 🔄 Phase D: Clean dogfood validation (real pipeline 経由、Round 1 完遂 2026-05-12 / Round 2 進行中 = D-4 完遂 2026-05-13 / 残 D-5/D-6/D-7)

Phase C fix + Phase D 前提整備 (順位 109) 完了で **real pipeline 経由 dogfood の必要十分条件が揃った**。D-1 着手時に session-only opt-in workflow が jj auto-snapshot と本質的に衝突する gap が判明したが、**順位 115 (`LINT_SCREEN_ENABLED` env var override) land で解消**。env var 経路 (`$env:LINT_SCREEN_ENABLED = "true"`) で `push-runner-config.toml` を編集せずに lint_screen を有効化できるため、D-3 で初の実 dogfood が成立し、計画 3 PR + prereq 1 PR がすべて land 完了。

**Phase D Round 1 対象 PR 構成 (D-1〜D-3、2026-05-12 完遂)**:

| Order | 構成 (todo-summary.md priority list より) | Effort | 実 diff 行 | Diff Profile | 状態 |
|---|---|---|---|---|---|
| **D-1** ✅ | 順位 112 + 113 + 114 = ADR amendments bundle (ADR-038 eprintln scope / ADR-027 metrics override / 新規 ADR Local LLM context size) + 順位 115 backlog 化 | S+ | 298 (insert 228 / delete 70) | docs + 1 Rust comment | **PR #145 land 済 (2026-05-12)**、lint_screen dogfood は skip (workflow gap) |
| **D-2** ✅ | 順位 101 + 106 + 103 = lint rule code touch (rule⑧ edge case test / self-exclusion assertion / lint runner field comment) | S+S+S | 172 (insert 84 / delete 88) | Rust test/comment mix | **PR #146 land 済 (2026-05-12)**、lint_screen dogfood は skip (順位 115 未 land 時点) |
| **115** ✅ | `LINT_SCREEN_ENABLED` env var override (D-1 で発見した workflow gap 解消) | S | 325 (insert 268 / delete 57) | Rust impl + 10 tests + Phase D guide rewrite | **PR #147 land 済 (2026-05-12)**、D-3 着手 unblock |
| **D-3** ✅ | 順位 102 = `paths` filter を lint runner に実装 (impl + test、既存 rule⑧ migration は 順位 118 で trade-off 検討に保留) | M | 496 (insert 375 / delete 121) | Rust impl + 7 unit tests + globset 依存追加 + glob filter helper | **PR #148 land 済 (2026-05-12)、初の real lint_screen dogfood 観測** |

**size ramp-up 設計**: small → mid → mid-large の漸増で、small PR 単体での fallback 観測と large PR で num_ctx 限界に近づく挙動を両方カバー。**D-1 / D-2 は workflow gap により lint_screen dogfood をスキップ、実質 metrics 観測は D-3 のみ**。3 PR 観測予定だったが kill-switch 基準 (3/5 で停止) を踏まえて D-3 単独で 1 dogfood data point を取得済。Phase E 採否判定は次回以降の通常 PR 累積で確定する設計に変更。

**D-1 / D-2 dogfood outcome (skip 理由 + 副産物)**:

- lint_screen dogfood は実施せず (D-1 着手時の workflow gap が両 PR で持続)
- 副産物 (D-1): **workflow gap を systemic に発見 + 順位 115 を Tier 1 backlog 登録 + post-merge-feedback Tier 1 #1 で再 validate**
- 副産物 (D-1): ADR-040 内部不整合 (3.33x label vs `(num_ctx/8192)*180s` formula = 4x) は takt review 1 iter で検出 → fix で解消、post-merge-feedback Tier 3 #1 で sublinear clarification 採用 (順位 116)
- 副産物 (D-1): lib.rs L128-139 → ADR-040 移管 edit order を post-merge-feedback Tier 3 #3 で codify 採用 (順位 117)
- 副産物 (D-2): clean merge (post-merge-feedback 0 件採用)、feedback loop 正常動作を再確認

**D-3 dogfood outcome (Phase D 初の real lint_screen 観測、PR #148)**:

| Metric | 観測値 |
|---|---|
| screen_decision | `auto_fix` |
| findings 件数 | 1 (minor severity) |
| finding 内容 | `unused-import` rule、`src/hooks-post-tool-linter/Cargo.toml:12` で `globset` を誤検出 |
| finding accuracy | **false positive** (takt reviewer が main.rs での import 使用済を diff verify で dismiss) |
| fallback_reason | なし (clean run、JSON parse error 無し) |
| `## Diagnostic` section | 不在 = num_ctx 32768 で overflow 発生せず (Phase A 診断 log emit せず) |
| lint_screen latency | 推定 ~80-120s (pipeline 総 628s − takt 248s − その他) |
| kill-switch (fallback > 50%) | fallback 0/1 = 0% → 基準内 |

**D-3 観測の意義**:

1. **env override 経路の実証**: PR #147 で実装した `LINT_SCREEN_ENABLED` env var で `[lint_screen] enabled = false` (TOML default) を override し、commit-free な session opt-in が成立
2. **num_ctx 32768 の容量実証**: ~270 line Rust diff (Cargo.toml + main.rs + Cargo.lock + docs) を overflow せず完走、Phase A 診断 log も emit せず
3. **lint_screen が takt reviewer の context として活用**: reviewer 出力に「Lint-screen finding: false positive」と明示的に評価あり = advisory consumption が成立
4. **1 false positive は Phase b' agreement 75% (= 25% disagreement) と整合**: 想定範囲内、複数 PR 累積評価が前提
5. **副産物 (D-3 post-merge-feedback)**: `MAX_CUSTOM_VIOLATIONS` outer/inner loop break scope の explicit test 必要性を発見 (Tier 2-1 採用、順位 119)、rule⑧ への paths filter 適用範囲検討を順位 118 として backlog 化

**Phase D Round 2 対象 PR 構成 (D-4〜D-7、2026-05-13 追加)**:

Round 1 で実 dogfood data point が **1 件のみ** (D-3) に留まり、ADR-038 採用条件「5 PR 以上」+ analysis.md「3-5 PR 累積」前提との乖離が判明。D-1 反省 (docs-only は lint_screen findings 0 件で metrics 価値低) + workflow gap 解消済 (順位 115 = `LINT_SCREEN_ENABLED` env var override land、PR #147) を踏まえ、**残 4 PR で Rust code 中心 + size ramp-up + 累積 5 PR (D-3 + D-4〜D-7) 達成** を狙う延長計画を策定。

| Order | 構成 (todo-summary.md priority list より) | Tier / Effort | 推定 diff 行 | Diff Profile | 状態 |
|---|---|---|---|---|---|
| **D-4** ✅ | 順位 39 単独 = takt workflow `model` 必須化 lint rule + 副次作業 (`.takt/workflows/*.yaml` の `persona:` 行 3 件に `model:` 明示追加で clean baseline 確保) + CR Major fix で 4 fields 追加 | T1 / S | 実 ~340 行 (commit 0c2cc07d + 1ec15686) | Rust lint rule (yaml multi-line regex、enumeration 方式) + 6+1 unit tests + custom-lint-rules.toml entry + 3 yaml site touch + CR Major fix | **PR #150 merged 2026-05-13、初 real lint_screen 観測 2 data points (initial + CR fix push)** |
| **D-5** ✅ | 順位 56 + 119 bundle = comment-lint hook test 拡充 + `MAX_CUSTOM_VIOLATIONS` outer/inner loop break scope explicit test + 副産物として `byte_offset_to_line` の char-boundary panic bug fix | T2+T2 / S+S | 実 ~120 行 (test 12 件追加 + production fix 1 行) | hooks-post-tool-comment-lint-rust + hooks-post-tool-linter test infra (UTF-8 multi-byte 5 + block boundary 6 + multi-rule MAX cap 2 + direct unit test 1) | **着手済** |
| **D-6** | 順位 51 単独 = `.takt/review-diff.txt` を fix→review iteration 間で refresh (案 A takt hook 不可と判明 → 案 C fix.md instruction-level refresh に pivot) | T1 / M→S | 実 ~80 行 (fix.md 追記 + todo7.md / analysis.md 更新) | takt facet instruction (markdown) + design docs | **着手済 (本 PR、impl 完了 / dogfood pending)** |
| **D-7** | Bundle c-1 (順位 63 + 64 + 67) = cli-merge-pipeline Drop guard / signal handler + orphan run reaper + ADR-030 spec amendment | T1×2 + T3 / M+M+XS | ~600-800 | Rust impl + signal trap + reaper logic + ADR markdown | 未着手 |

**size ramp-up 設計 (Round 2)**: small → small-mid → mid → mid-large で num_ctx 32768 容量限界に向け漸増、各 size 帯で fallback 発生率 / Phase A diagnostic warn log 出力有無を観測。D-3 (mid, 496 行) と組合せて 5 size 帯をカバー。

**D-1 反省の適用 (Round 2)**:

- ❌ **docs-only PR を回避**: D-4〜D-7 すべて Rust impl/test 中心 (D-7 で ADR markdown 1 つを bundle 内で同梱するのみ、主成分は Rust)
- ✅ **workflow gap 解消済**: 順位 115 (`LINT_SCREEN_ENABLED` env var) で session-only opt-in が成立、`push-runner-config.toml` 編集不要
- ✅ **dogfood 実施可否を事前確認**: 各 PR 着手時に (a) env var set 確認 / (b) Ollama 起動確認 / (c) PR が code change を含むこと を check

**想定リスク (Round 2)**:

- **D-7 (Bundle c-1) size 上限超過リスク**: M+M+XS が 800 行を超えた場合、c-1a (順位 63 単独) / c-1b (順位 64+67) の 2 PR 分割に switch。その場合は D-7 → D-7a/D-7b で 5 PR に拡張 (Phase E 判定材料が 1 件増える方向で副次的に valid)
- **D-4 size の下振れリスク**: 順位 39 単独 + 3 yaml site touch + tests は ~150-280 行を見込むが、focused single-purpose PR として 250 lower bound 未達も許容
- **detail 見積もりの精度**: 各 todo`N`.md の詳細 (実装方針 / acceptance criteria) を未参照、着手時に scope 修正の必要あり
- **D-4 の re-pivot 経緯 (2026-05-13)**: 当初 D-4 = 順位 47 (`>` vs `>=` boundary lint) を予定していたが、着手直前 (memory rule `feedback_verify_task_not_already_done.md` 適用) で **PR #126 (commit `b677b9d4f54d`) で既に land 済 (`no-time-field-strict-greater` rule、custom-lint-rules.toml line 208-243)** を発見。D-5 から 順位 39 を D-4 に繰上げ、D-5 を 順位 56 + 119 bundle に再構成。stale todo7.md 順位 47 entry は同 PR の docs commit で削除

**D-4 dogfood outcome (Phase D Round 2 初の real lint_screen 観測、PR #150)**:

PR #150 は同一 PR 内で **2 push event** が発生し、それぞれ独立した lint_screen dogfood data point を生成した:

| Push event | commit | screen_decision | findings | fallback | num_ctx overflow | latency 推定 |
|---|---|---|---|---|---|---|
| 初回 push (D-4 impl) | `0c2cc07d` | **`informational`** | 0 | なし | なし | ~10-15s (pipeline 総 645s) |
| CR Major fix re-push | `1ec15686` | **`auto_fix`** | 1 (FP: TOML に Rust `unused-import` 誤検出) | なし | なし | ~10-15s (pipeline 総 583s) |

**D-4 観測の意義**:

1. **`informational` verdict の初観測**: D-3 (`auto_fix` + 1 FP) と異なる「指摘なし」経路を実証。lint_screen の判定空間 2 経路 (auto_fix / informational) を D-3 + D-4 で本セッション内にカバー、特定 verdict への偏りバイアスが無いことを確認
2. **same-PR 2 push の independent dogfood**: CR Major fix re-push でも pipeline が独立に走り、新 data point を生成。Phase E 累積カウントへの直接寄与は 1 PR = 1 と数えるが、verdict variance 観測材料としては 2 data points として有効
3. **CR Major auto-fix の構造的成功**: persona 直後の field 列挙不足 (`output_contracts` / `pass_previous_response` / `required_permission_mode` / `parallel`) を CR が指摘 → memory rule `feedback_review_severity_auto_fix.md` 適用で auto-fix → regression test 同梱 land。CR Minor は `resolved:` reply で auto-resolve (memory `project_coderabbit_auto_resolve.md`)
4. **post-merge-feedback 採用 3 件**: 順位 120 (rule comment + ADR-007 case study、Tier 1→3 reclassify) / 順位 121 (3 fields の individual fixture test 追加、Tier 2) / 順位 122 (`development-workflow.md` Step 0 への stale-entry 確認 step 追加、Tier 3) を `docs/todo8.md` に登録 + `docs/todo-summary.md` table 反映済 (commit 0c2cc07d-merged → 39ae2cd1)
5. **副産物 (D-4 セッション知見)**: post-merge-feedback analyzer の Tier 分類が誤りやすい構造を発見 (rule コメント追記を Tier 1 と分類) → memory `feedback_tier_classification.md` に正しい Tier 定義 (mechanical enforcement = T1 / docs 修正 = T3) を codify

**D-5 dogfood outcome (Phase D Round 2 二件目の real lint_screen 観測、PR #TBD)**:

D-4 と同様、D-5 も同一 PR 内で **2 push event** が発生し、それぞれ独立した lint_screen dogfood data point を生成:

| Push event | commit | screen_decision | findings | fallback | num_ctx overflow | lint_screen latency | pipeline 総時間 |
|---|---|---|---|---|---|---|---|
| 初回 push (D-5 impl、~649 行 Rust diff) | `5cbed3c3` | **`auto_fix`** | 1 (FP: comment-lint-rust line 1 を `use std::io::Write;` 誤認、実 line 1 は `//!` doc comment) | なし | なし | 54s | 679s (takt review 5m 23s) |
| 2 回目 push (docs-only outcome record、~67 行 analysis.md 更新) | `9458660b` | **`auto_fix`** | 1 (同 FP 再現: docs-only diff にも関わらず Rust file hallucinate、構造的 root cause) | なし | なし | ~推定 30-50s | 522s (takt review 2m 44s、docs-only scope で executable criteria waived) |

**D-5 観測の意義**:

1. **`auto_fix` verdict 4 件目**: D-3 + D-4 CR fix + D-5 (2 push events) と同じ verdict 経路、false positive pattern も file/scope 混同で共通 (mistral:7b の構造的特徴)
2. **reviewer による cross-check の構造的成功**: simplicity-review が "Lint Screen Cross-Check" section で初回 finding を **明示的に false positive と判定** + 根拠 (line 1 直接読取 + use 文の実 location 特定 = `hooks-post-tool-linter` 側 line 1131/1163) を report に記載。Phase b' agreement 75% 設計通りに reviewer が独立判断する advisory consumption が functional
3. **docs-only diff でも同 FP 再現**: 2 回目 push は analysis.md ~67 行のみで Rust file の変更ゼロにも関わらず、mistral:7b が同じ FP を出力。**lint_screen の FP は diff 内容ではなく hook のソース全文を見て hallucinate している強い証拠** → mistral:7b の context window 内に hook source の周辺コードが含まれ、過去 commit の `use std::io::Write;` (test fn 内) を unused と推論。Phase b' scale-aware fixture では捕捉できない failure mode
4. **副産物 (D-5 セッション知見、production bug fix)**: UTF-8 漢字単独 test 着手時に `byte_offset_to_line` の char-boundary panic bug を発見 → 1-line fix で resolve、direct unit test も追加。AI 編集で multi-byte 文字で終わる `new_string` (例: 「漢字のみのコメント」末尾改行なし) を渡すと従来 hook が panic していたのを構造的に修正。test 拡充が production fault detection に直結した事例
5. **lint_screen agreement の累積観測 (5 観測中 4 FP)**: D-3 / D-4 CR fix / D-5 (×2) はいずれも file-type / scope 混同型 FP で同 root cause 推定。Phase b' agreement 75% 想定からは過大な FP 率 (~80%) だが、severity が全て `minor` で reviewer cross-check による blocking なし → 運用 viable
6. **副産物 (D-5 push workflow 知見)**: 初回 push 後の outcome record 追記を「local @ in-place edit → split」で扱うと force-push 必要になる jj workflow gap を発見。代替策として `jj new <bookmark>@origin` で remote bookmark の child commit を直接作成 → FF push で advance する手順を実証。本 PR の 2 push 構成自体がこの workflow validation の dogfood

**Phase D Round 1 完遂 + Round 2 D-4 + D-5 完遂後の Phase E 判定材料**:

- ✅ pipeline integration works end-to-end (D-1 #144 smoke test + D-3 #148 + D-4 #150 + D-5 ×2 で計 5 real diff 完走)
- ✅ num_ctx 32768 で 67-649 行 diff overflow なし (Phase C reference values と整合、D-5 docs-only 67 行 〜 D-5 impl 649 行で size 帯拡大)
- ✅ fallback rate < 50% (D-3 0/1、D-4 initial 0/1、D-4 CR fix 0/1、D-5 ×2 0/2 = 累積 0/5 = 0%)
- ⚠️ agreement: 累積 false positive 4 件観測 (D-3 / D-4 CR fix / D-5 ×2) — いずれも `minor` severity で reviewer cross-check 通過、blocking なし。D-5 観測で「diff 外 context から hallucinate する failure mode」を新発見
- ✅ verdict variance: `auto_fix` (D-3 + D-4 CR fix + D-5 ×2) と `informational` (D-4 initial) の 2 経路を観測
- 🔄 **累積 PR data 充足中**: Round 1 (D-3) + Round 2 (D-4 + D-5) で **5 data points (3 PR)** 取得済、累積 5 PR (= ADR-038 採用条件) は **3 PR 段階で先行充足**。残 D-6/D-7 で更に観測予定

Phase E 着手の前提条件は **3-5 PR 累積 dogfood**。D-3 (1) + D-4 (1) + 残 D-5/D-6/D-7 (3 PR 計画) で計画上 **累積 5 PR** に到達する見込み。D-4 で「2 push event = 2 data points」が同 PR で発生する pattern を実証したため、Round 2 の残 PR でも CR fix 経由の追加観測が見込まれる。各 PR push 時に `$env:LINT_SCREEN_ENABLED=true` を opt-in で set し、`.takt/lint-screen-report.md` を post-push で記録する運用を継続。

**Phase D 計測手順** (各 PR 共通):

> **D-1 着手時に判明した workflow gap (2026-05-12)**: Phase D guide §1 の「session-only opt-in」 (`[lint_screen] enabled = true` を commit せず runtime のみ反映) は jj の auto-snapshot 性質と本質的に衝突する。cli-push-runner は `push-runner-config.toml` を TOML 経由でのみ読み取り、env var / CLI flag による override path は未実装。よって config 変更を @ に持たせるとそのまま push commit に乗ってしまい、「local enable / remote disable」が成立しない。**暫定方針**: D-1 (docs-only、ADR markdown は lint_screen 分析価値が低い) は `enabled = false` のまま push して dogfood をスキップ。D-2 着手前に **env var override (`LINT_SCREEN_ENABLED`) を cli-push-runner に追加** (todo8.md に backlog 登録予定) してから D-2 / D-3 の dogfood を実施する。

1. **PR 着手前** (D-2 以降): env var `LINT_SCREEN_ENABLED=true` を session に export (`push-runner-config.toml` の `enabled = false` は維持)
2. **push 前**: env var の有効性を `echo $LINT_SCREEN_ENABLED` (Unix) or `echo $env:LINT_SCREEN_ENABLED` (PowerShell) で確認、config TOML 側の `enabled = false` は変更しない
3. **pnpm push 実行**: lint_screen stage → takt review iteration → jj git push の流れ
4. **report 確認**: `.takt/lint-screen-report.md` を読み、以下 metrics を記録
   - (a) screen_decision (auto_fix / human_review / informational)
   - (b) findings 件数 + severity 分布
   - (c) fallback_reason (あれば)
   - (d) `## Diagnostic` section の有無 (Phase A warn log の visible 化検証)
   - (e) latency (`.takt/runs/<latest>/logs/` から抽出)
5. **post-push cleanup**: env var を unset (session 終了で自動消滅、commit には影響なし)、PR 作成 → CR review → merge
6. **3 PR 完了後**: 累積 fallback rate を集計 (num_ctx truncation / contract violation / 別問題 / 成功 で分類)、Phase D 基準 (<50% fallback) 達成判定、本 § Phase D row に dogfood outcome table 追加、Phase E 移行判断

**想定リスク**:

- **D-1 dogfood 不実施**: D-1 は ADR markdown のみで lint_screen が code lint findings を検出しない予測 (informational 0 件)。Phase D 前提整備 PR #144 で pipeline integration は smoke test 4 件で seal 済のため、D-1 skip による metrics ロスは限定的
- **env var override 実装の D-2 への前出し**: D-2 (Rust test/comment mix) の scope に env var override (~30-50 行 Rust) が加わる。D-2 effort が S+S+S+S → S+S+S+M 程度に増加するが、PR sizing rule (250-800 行) 内に収まる予測
- **D-3 (順位 102) のサイズ**: 250-350 行を超える可能性。L effort 化しても PR sizing rule (250-800 行) 内
- **contract violation の再観測**: Phase C P-2 で `invalid severity: "error"` を観測した型崩壊系が Rust diff (D-2/D-3) で再発する可能性、Phase D scope 外として metrics 記録のみ
- **num_ctx 32768 再 overflow**: D-3 は P-3 (487 行) より小さいため発生しないはずだが、prompt の token 効率次第。Phase A diagnostic log (`## Diagnostic` section) で即検知

**別案 (棚上げ)**: D-1 を順位 110+111+104 (testing.md + docs-governance routing rule + ADR-007 amendment、mixed) に変更する案もあったが、ADR codify 優先で 112+113+114 を採用。

##### 🎯 Phase E: 採否判定 + retirement (1 PR、analysis.md 削除を含む、**Phase D Round 2 完遂後着手**)

- **着手前提**: Phase D Round 2 (D-4〜D-7) 完遂 + 累積 5 PR 分の dogfood data 揃い + 各 PR の metrics (latency p50/p95 / fallback rate / classification 妥当性) 集計済
- **採用 case**: ADR-038 を「採用」に昇格 + [docs/local-llm-offload-phase-d-guide.md](local-llm-offload-phase-d-guide.md) を削除 (試験運用ガイド役目終了) + 本 analysis.md を削除 + history.md は permanent record として保持判断
- **却下 case**: cli-finding-classifier crate revert + ADR-038 を「却下」に更新 + Phase d guide 削除 + 本 analysis.md 削除
- **継続 case**: Phase D で別問題判明等 (例: real pipeline で classifier preview と異なる挙動) なら判定延期 + 本 §「次に何をするか」を再 pivot

##### Critical path 外 (並行 land 可、本 phase 完了を block しない)

| Task | Effort | 関連 |
|---|---|---|
| 順位 100-108 docs PR (8 entries の todo registration、bundle 1 PR で消化) | S | Phase A〜C と並行 land 可、commit chain 整理 |
| Bundle j-2 (順位 95+96、`.github/workflows/lint.yml` 新設) | M (S+M) | 独立 |
| Bundle f-1/f-2 (PR #120 feedback) | S+M | 独立 |
| 順位 110-114 (PR #142/#143 post-merge-feedback 採用分) | XS-S 各 | Phase D の対象 PR 候補としても活用可能 |

##### Dogfood signal log (旧 PR roster の preview 結果、Phase B/D の比較対象として保持)

| PR | 構成 | Diff 行 | Latency | findings | fallback_reason | Cumulative fallback |
|---|---|---|---|---|---|---|
| #139 (旧 P-1) | Bundle h + g-2 (docs-only) | 337 | 23s | 0 | `JSON parse error: missing field 'screen_decision'` (line 94) | 1/1 = 100% |
| #140 (旧 P-2) | Bundle j-1 (TOML + Rust regex) | 203 | 46s | 0 | 同 (line 94 column 1) | 2/2 = 100% |
| #141 (旧 P-3) | Bundle d (Rust test only) | 187 | 11s | 0 | 同 (line 1 column 692) | **3/3 = 100%** |

**観測**: (a) fallback rate 100% が 3 PR 連続 = 既に kill-switch 60% 超過、(b) latency variance (23s/46s/11s) は input size と弱相関 = mistral 内部状態 (cold/warm context) が支配的要因の仮説、(c) すべて同一 fallback_reason = 単一 root cause の可能性大。

**Phase d guide §3 kill-switch との関係**: ガイドは「real pipeline 経由で 3/5 fallback 観測 = 停止」と規定。本 preview は cli-finding-classifier 直叩きで pipeline 経由ではないが、3 連続 100% fallback は **厳密 kill-switch 超過に相当する severity**。Phase A〜C で repair しない限り Phase D に進めない判断。

**Out-of-roster dropouts**: 旧 P-3 = Bundle g-1 (順位 85+86) は PR #125 で land 済を P-3 着手時に発見 → roster 除外 + stale todo 削除。経緯詳細は `feedback_verify_task_not_already_done.md` 参照。

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
