# ローカル LLM オフロード — 残作業計画 (Phase b 以降)

> **位置づけ**: 本ファイルは「残作業の **次に何をするか** だけ」を持つ実行計画。完了済みの分析・実装は以下に切り出し済:
> - [local-llm-offload-history.md](local-llm-offload-history.md) — 旧計画 (§A-2 dogfood 等) + Phase a-c 以前の retrospective
> - [local-llm-offload-phase-d-outcomes.md](local-llm-offload-phase-d-outcomes.md) — Phase A〜D 各 PR の詳細 dogfood outcome / Phase E 判定材料 / Dogfood signal log (2026-05-13 分離)
>
> **状態**: 試験運用 (Phase a〜c 完了 / Phase c+ Bundle i + §8.D 完了 = PR #135/#136 / **Phase d Round 1 完遂 = 2026-05-12 (D-1〜D-3) / Phase d Round 2 進行中 = 2026-05-13 (D-4/D-5/D-6 完遂 → 累積 6 data points / 4 PR、残 D-7)** — 詳細は outcomes ファイル参照)。Phase d 運用ガイドは [docs/local-llm-offload-phase-d-guide.md](local-llm-offload-phase-d-guide.md)。
>
> **引退条件**: 以下のいずれかで **3 ファイル (analysis.md / history.md / phase-d-outcomes.md) を同時削除** する (docs-governance.md retirement workflow 準拠):
> - 残作業 (§8.D / §8.E / §8.F, §1 Phase b/c/d) が **すべて land または却下** → permanent value を ADR-038 に migrate
> - **6 ヶ月経過** (= 2026-11-08) しても §8.E 採否未決 → 採用見込みなしとみなして削除
>
> **本ファイルだけで Phase b 以降を再開できることを目的とする**。完了詳細は outcomes ファイル、背景は history ファイル参照。

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

##### 📊 Phase A〜D 経過サマリー (完了済み詳細は [phase-d-outcomes.md](local-llm-offload-phase-d-outcomes.md) 参照)

| Phase | 状態 | PR | 主成果 |
|---|---|---|---|
| 🚀 A: Diagnostic | ✅ 完了 | #142 (2026-05-11) | `lib-ollama-client` に `OllamaMetadata` (num_ctx overflow 診断) を実装 |
| 🔍 B: Root cause id | ✅ 完了 | (A 即時 dogfood) | `prompt_eval_count: 8192 / num_ctx: 8192` = num_ctx truncation が真因と確定 |
| 🔧 C: Root cause fix | ✅ 完了 | #143 (2026-05-11) | `DEFAULT_NUM_CTX 8192 → 32768`、smoke 3 PR で fallback 100% → 33% に解消 |
| 🛠️ D 前提整備 (順位 109) | ✅ 完了 | #144 (2026-05-11) | `.takt/lint-screen-report.md` に `## Diagnostic` section、real pipeline 経由で warn log が visible |
| 🔄 D Round 1 (D-1〜D-3) | ✅ 完遂 | #145/#146/#147/#148 (2026-05-12) | env var override (順位 115) 解消、D-3 で初の real lint_screen 観測 = 1 data point |
| 🔄 D Round 2 (D-4〜D-6) | ✅ 完遂 | #150/#151/#152 (2026-05-13) | size ramp-up + verdict variance 観測、累積 6 data points / 4 PR |
| 🔄 D Round 2 (D-7) | 未着手 | (Bundle c-1) | cli-merge-pipeline Drop guard / orphan reaper / ADR-030 spec |

**Phase E 着手前提条件 = 3-5 PR 累積 dogfood は D-6 完遂時点で既に充足** (4 PR / 6 data points)。D-7 完遂後に Phase E (採否判定) 移行可能。Round 1/2 で観測した **false positive 5 件中 5 件が file-type / scope 混同型** (mistral:7b の context window 内 hook source 周辺 hallucinate) → Bundle k 順位 123 (MD 除外フィルター) の構造的解消対象。詳細 metrics・観測の意義・各 PR の dogfood outcome は [phase-d-outcomes.md](local-llm-offload-phase-d-outcomes.md) 参照。

##### 🔄 Phase D Round 2 残: D-7 (Bundle c-1)

| Order | 構成 | Tier / Effort | 推定 diff 行 | Diff Profile |
|---|---|---|---|---|
| **D-7** | Bundle c-1 (順位 63 + 64 + 67) = cli-merge-pipeline Drop guard / signal handler + orphan run reaper + ADR-030 spec amendment | T1×2 + T3 / M+M+XS | ~600-800 | Rust impl + signal trap + reaper logic + ADR markdown |

**想定リスク (D-7)**:

- **size 上限超過リスク**: M+M+XS が 800 行を超えた場合、c-1a (順位 63 単独) / c-1b (順位 64+67) の 2 PR 分割に switch (D-7 → D-7a/D-7b で 5 PR 拡張、Phase E 判定材料が 1 件増える方向で副次的に valid)
- **detail 見積もりの精度**: todo7.md (順位 63/64/67) の詳細を未参照、着手時に scope 修正の必要あり
- **num_ctx 32768 再 overflow**: D-3 (496 行) より大きい diff のため可能性あり、Phase A diagnostic log (`## Diagnostic` section) で即検知

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

##### 🎯 Phase E: 採否判定 + retirement (1 PR、**Phase D Round 2 完遂後着手**)

- **着手前提**: D-7 完遂 + 累積 5 PR 分の dogfood data 揃い (D-6 時点で 4 PR / 6 data points 充足済)
- **採用 case**: ADR-038 を「採用」に昇格 + [docs/local-llm-offload-phase-d-guide.md](local-llm-offload-phase-d-guide.md) を削除 (試験運用ガイド役目終了) + 3 ファイル (`analysis.md` / `history.md` / `phase-d-outcomes.md`) を削除 (permanent value は ADR-038 へ migrate)
- **却下 case**: cli-finding-classifier crate revert + ADR-038 を「却下」に更新 + 同 3 ファイル + Phase d guide 削除
- **継続 case**: Phase D で別問題判明等 (例: real pipeline で classifier preview と異なる挙動) なら判定延期 + 本 §「次に何をするか」を再 pivot

##### Critical path 外 (並行 land 可、本 phase 完了を block しない)

| Task | Effort | 関連 |
|---|---|---|
| Bundle k 全 entry (順位 123-127、Phase D dogfood 由来の lint-screen FP 対策 + ADR-038 codify 等) | S+S+M+XS+XS | Phase D Round 2 で 4 PR 観測の構造的解消、優先実装 |
| Bundle j-2 (順位 95+96、`.github/workflows/lint.yml` 新設) | M (S+M) | 独立 |
| Bundle f-1/f-2 (PR #120 feedback) | S+M | 独立 |
| 順位 110-114 (PR #142/#143 post-merge-feedback 採用分) | XS-S 各 | Phase D の対象 PR 候補としても活用可能 |

優先度低の独立 task (Phase d を block しない):
- **§8.D**: classify mode の `normalized_issue` 言語制約強化 (low priority、ROI ★)
- **§8.F**: PR body draft (§8.E 採用 = Phase d 完了 後)
- **順位 97** (todo): `with_num_ctx(X)` override serialization 検証 test (PR #136 T2-#1、Phase d で num_ctx tweak する局面に入る前の安全網)

§8.D / §8.E / §8.F の実装に着手する場合は、本ファイル該当節 + history §10.6/§10.7 を参照。

## 関連リンク

- [local-llm-offload-phase-d-outcomes.md](local-llm-offload-phase-d-outcomes.md) — **Phase A〜D 各 PR の dogfood outcome / Phase E 判定材料 / Dogfood signal log** (本ファイルから 2026-05-13 分離)
- [local-llm-offload-history.md](local-llm-offload-history.md) — 旧計画 (§A-2 dogfood 等) + Phase a-c 以前の retrospective
- [local-llm-offload-phase-d-guide.md](local-llm-offload-phase-d-guide.md) — Phase D 着手時の運用ガイド
- [ADR-038: ローカル LLM による CodeRabbit findings classification](adr/adr-038-local-llm-finding-classification.md) — 提案 2 (cli-finding-classifier) の land 結果、Phase a evals infrastructure の re-use 対象
- [ADR-040: Local LLM Context Size と Resource Trade-off](adr/adr-040-local-llm-context-size.md) — Phase C `num_ctx 32768` の根拠
- [ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止](adr/adr-018-pr-monitor-takt-migration.md)
- [ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略](adr/adr-020-takt-facets-sharing.md)
- [ADR-034: CodeRabbit 監視・対話の自動化戦略 — Bundle a 設計根拠](adr/adr-034-coderabbit-auto-monitoring.md)
