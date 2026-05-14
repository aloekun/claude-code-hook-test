# ローカル LLM オフロード — Phase A〜D 完了済み詳細記録

> **位置づけ**: [local-llm-offload-analysis.md](local-llm-offload-analysis.md) から完了済みの Phase A〜D の詳細 (Diagnostic / Root cause / Fix / Phase D 各 PR の dogfood outcome / Phase E 判定材料 / Dogfood signal log) を切り出した記録専用ファイル。`analysis.md` 軽量化 (50KB 接近回避) の一環として 2026-05-13 に分離。
>
> **状態**: 試験運用 (analysis.md と同じ lifecycle、Phase D 採否判定 = ADR-038 採用/却下 確定で同時 retire)
>
> **引退条件**: 以下のいずれか (docs-governance.md retirement workflow 準拠、`analysis.md` と同時判断):
>
> - 残作業 (§8.D / §8.E / §8.F, §1 Phase b/c/d) が **すべて land または却下** → permanent value を ADR-038 に migrate → 3 ファイル (`analysis.md` / `history.md` / 本ファイル) 削除
> - **6 ヶ月経過** (= 2026-11-08) しても §8.E 採否未決 → 3 ファイル削除
>
> **参照元 (active)**: [local-llm-offload-analysis.md](local-llm-offload-analysis.md) の Phase A〜D 経過サマリーから本ファイルへリンク
>
> **本ファイルは記録専用**。次に何をするかは analysis.md 側を参照。

## 🚀 Phase A: Diagnostic ✅ **完了 (PR #142、2026-05-11)**

**順位 98 実装完了** = `lib-ollama-client` の `generate_json` に `OllamaMetadata` (`prompt_eval_count` / `eval_count` / `num_ctx`) を組み込み、serde parse error 時に stderr へ warn log を emit する診断層を追加。`OllamaApi` trait の `generate_with_metadata` (default fallback あり、StubOllama は変更不要)、`emit_overflow_diagnostic` 関数で 90% 以上時に「num_ctx を増やす hint」を含める。16 unit test pass、cli-finding-classifier 経由でも warn log が stderr に出ることを smoke 確認。

## 🔍 Phase B: Root cause identification ✅ **完了 (Phase A 即時 dogfood、2026-05-11)**

Phase A 実装後、PR #141 (P-3 = 187 行 mixed diff) を replay → **`prompt_eval_count: 8192 (vs num_ctx: 8192)` = 100% 到達を実機確認**。**真因 = num_ctx truncation で確定**。mistral の prompt が完全に context cap で truncate されて JSON output が完成せず `screen_decision` field 欠落の症状を引き起こしていた。仮説 2 候補 (num_ctx truncation / mistral 出力崩壊) のうち前者が真因と decisive 判定。

## 🔧 Phase C: Root cause fix ✅ **完了 (PR #143、2026-05-11)**

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

## 🛠️ Phase D 前提整備 (順位 109) ✅ **完了 (PR #144、2026-05-11)**

`src/cli-push-runner/src/stages/lint_screen.rs` 改修: graceful fallback (exit 0) 時にも classifier stderr を `.takt/lint-screen-report.md` の `## Diagnostic` section に取込。Phase A 診断 warn log が **real pipeline 経由で visible** になる状態を確保。新 struct `ClassifierOutput { stdout, stderr }`、新 helper `render_diagnostic`、新規 smoke test 4 件 (TP / FP / edge case / parse-error path) で contract を seal。lint_screen tests 14/14 pass + workspace 全 cargo test pass。

## 🔄 Phase D Round 1 完遂 (D-1〜D-3、2026-05-12)

Phase C fix + Phase D 前提整備 (順位 109) 完了で **real pipeline 経由 dogfood の必要十分条件が揃った**。D-1 着手時に session-only opt-in workflow が jj auto-snapshot と本質的に衝突する gap が判明したが、**順位 115 (`LINT_SCREEN_ENABLED` env var override) land で解消**。

### Round 1 対象 PR 構成

| Order | 構成 | Effort | 実 diff 行 | Diff Profile | 状態 |
|---|---|---|---|---|---|
| **D-1** ✅ | 順位 112 + 113 + 114 = ADR amendments bundle (ADR-038 eprintln scope / ADR-027 metrics override / 新規 ADR Local LLM context size) + 順位 115 backlog 化 | S+ | 298 (insert 228 / delete 70) | docs + 1 Rust comment | **PR #145 land 済 (2026-05-12)**、lint_screen dogfood は skip (workflow gap) |
| **D-2** ✅ | 順位 101 + 106 + 103 = lint rule code touch (rule⑧ edge case test / self-exclusion assertion / lint runner field comment) | S+S+S | 172 (insert 84 / delete 88) | Rust test/comment mix | **PR #146 land 済 (2026-05-12)**、lint_screen dogfood は skip (順位 115 未 land 時点) |
| **115** ✅ | `LINT_SCREEN_ENABLED` env var override (D-1 で発見した workflow gap 解消) | S | 325 (insert 268 / delete 57) | Rust impl + 10 tests + Phase D guide rewrite | **PR #147 land 済 (2026-05-12)**、D-3 着手 unblock |
| **D-3** ✅ | 順位 102 = `paths` filter を lint runner に実装 (impl + test、既存 rule⑧ migration は 順位 118 で trade-off 検討に保留) | M | 496 (insert 375 / delete 121) | Rust impl + 7 unit tests + globset 依存追加 + glob filter helper | **PR #148 land 済 (2026-05-12)、初の real lint_screen dogfood 観測** |

**size ramp-up 設計**: small → mid → mid-large の漸増で、small PR 単体での fallback 観測と large PR で num_ctx 限界に近づく挙動を両方カバー。**D-1 / D-2 は workflow gap により lint_screen dogfood をスキップ、実質 metrics 観測は D-3 のみ**。

### D-1 / D-2 dogfood outcome (skip 理由 + 副産物)

- lint_screen dogfood は実施せず (D-1 着手時の workflow gap が両 PR で持続)
- 副産物 (D-1): **workflow gap を systemic に発見 + 順位 115 を Tier 1 backlog 登録 + post-merge-feedback Tier 1 #1 で再 validate**
- 副産物 (D-1): ADR-040 内部不整合 (3.33x label vs `(num_ctx/8192)*180s` formula = 4x) は takt review 1 iter で検出 → fix で解消、post-merge-feedback Tier 3 #1 で sublinear clarification 採用 (順位 116)
- 副産物 (D-1): lib.rs L128-139 → ADR-040 移管 edit order を post-merge-feedback Tier 3 #3 で codify 採用 (順位 117)
- 副産物 (D-2): clean merge (post-merge-feedback 0 件採用)、feedback loop 正常動作を再確認

### D-3 dogfood outcome (Phase D 初の real lint_screen 観測、PR #148)

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

## ✅ Phase D Round 2 完遂 (D-4〜D-7、2026-05-13〜2026-05-14)

Round 1 で実 dogfood data point が **1 件のみ** (D-3) に留まり、ADR-038 採用条件「5 PR 以上」+ analysis.md「3-5 PR 累積」前提との乖離が判明。**残 4 PR で Rust code 中心 + size ramp-up + 累積 5 PR (D-3 + D-4〜D-7) 達成** を狙う延長計画を策定 (D-1 反省 = docs-only 回避 / workflow gap 解消済確認)。**全 4 PR (D-4〜D-7) が land し、累積 7 data points / 5 PR で ADR-038 採用条件を完全充足**。

### Round 2 対象 PR 構成

| Order | 構成 | Tier / Effort | 実 diff 行 | Diff Profile | 状態 |
|---|---|---|---|---|---|
| **D-4** ✅ | 順位 39 単独 = takt workflow `model` 必須化 lint rule + 副次作業 + CR Major fix で 4 fields 追加 | T1 / S | 実 ~340 行 (commit 0c2cc07d + 1ec15686) | Rust lint rule (yaml multi-line regex) + 6+1 unit tests + custom-lint-rules.toml entry + 3 yaml site touch | **PR #150 merged 2026-05-13、初 real lint_screen 観測 2 data points** |
| **D-5** ✅ | 順位 56 + 119 bundle = comment-lint hook test 拡充 + `MAX_CUSTOM_VIOLATIONS` test + 副産物 `byte_offset_to_line` char-boundary panic bug fix | T2+T2 / S+S | 実 ~120 行 | comment-lint-rust + post-tool-linter test infra (UTF-8 5 + block boundary 6 + multi-rule MAX cap 2 + direct unit 1) | **PR #151 merged 2026-05-13、2 push events で 2 data points** |
| **D-6** ✅ | 順位 51 単独 = `.takt/review-diff.txt` を fix→review iteration 間で refresh (案 A takt hook 不可と判明 → 案 C fix.md instruction-level refresh に pivot) + Bundle k 順位 123-127 entry 登録 combined PR | T1 / M→S | 実 ~80 行 D-6 + Bundle k entry ~130 行 | takt facet instruction (markdown) + design docs (docs-only PR) | **PR #152 merged 2026-05-13、real lint_screen 1 data point** |
| **D-7** ✅ | Bundle c-1 (順位 63 + 64 + 67) = cli-merge-pipeline pre-emptive marker + RAII Drop guard + orphan run reaper + ADR-030 §L1/L2 spec amendment | T1×2 + T3 / M+M+XS | 実 845 ins / 175 del | Rust impl (cli-merge-pipeline pre-emptive marker + FailedMarkerGuard + hooks-session-start ISO 8601 parser + orphan reaper + meta.json mutator) + ADR markdown amendment | **PR #154 merged 2026-05-14、self-dogfood で L1 recovery 機構が実証 + 新 failure mode (Ollama timeout) 観測** |

**size ramp-up 設計 (Round 2)**: small → small-mid → mid → mid-large で num_ctx 32768 容量限界に向け漸増、各 size 帯で fallback 発生率 / Phase A diagnostic warn log 出力有無を観測。D-3 (mid, 496 行) と組合せて 5 size 帯をカバー。

**D-4 の re-pivot 経緯 (2026-05-13)**: 当初 D-4 = 順位 47 (`>` vs `>=` boundary lint) を予定していたが、着手直前 (memory rule `feedback_verify_task_not_already_done.md` 適用) で **PR #126 で既に land 済** を発見。D-5 から 順位 39 を D-4 に繰上げ、D-5 を 順位 56 + 119 bundle に再構成。stale todo7.md 順位 47 entry は同 PR の docs commit で削除。

### D-4 dogfood outcome (PR #150)

PR #150 は同一 PR 内で **2 push event** が発生し、それぞれ独立した lint_screen dogfood data point を生成:

| Push event | commit | screen_decision | findings | fallback | num_ctx overflow | latency 推定 |
|---|---|---|---|---|---|---|
| 初回 push (D-4 impl) | `0c2cc07d` | **`informational`** | 0 | なし | なし | ~10-15s (pipeline 総 645s) |
| CR Major fix re-push | `1ec15686` | **`auto_fix`** | 1 (FP: TOML に Rust `unused-import` 誤検出) | なし | なし | ~10-15s (pipeline 総 583s) |

**D-4 観測の意義**:

1. **`informational` verdict の初観測**: D-3 (`auto_fix` + 1 FP) と異なる「指摘なし」経路を実証。lint_screen の判定空間 2 経路 (auto_fix / informational) を D-3 + D-4 で本セッション内にカバー
2. **same-PR 2 push の independent dogfood**: CR Major fix re-push でも pipeline が独立に走り、新 data point を生成。Phase E 累積カウントへの直接寄与は 1 PR = 1 と数えるが、verdict variance 観測材料としては 2 data points として有効
3. **CR Major auto-fix の構造的成功**: persona 直後の field 列挙不足を CR が指摘 → memory rule `feedback_review_severity_auto_fix.md` 適用で auto-fix → regression test 同梱 land
4. **post-merge-feedback 採用 3 件**: 順位 120 / 121 / 122 を `docs/todo8.md` に登録
5. **副産物**: post-merge-feedback analyzer の Tier 分類が誤りやすい構造を発見 → memory `feedback_tier_classification.md` に正しい Tier 定義 (mechanical enforcement = T1 / docs 修正 = T3) を codify

### D-5 dogfood outcome (PR #151)

D-4 と同様、D-5 も同一 PR 内で **2 push event** が発生:

| Push event | commit | screen_decision | findings | fallback | num_ctx overflow | lint_screen latency | pipeline 総時間 |
|---|---|---|---|---|---|---|---|
| 初回 push (D-5 impl、~649 行 Rust diff) | `5cbed3c3` | **`auto_fix`** | 1 (FP: comment-lint-rust line 1 を `use std::io::Write;` 誤認、実 line 1 は `//!` doc comment) | なし | なし | 54s | 679s (takt review 5m 23s) |
| 2 回目 push (docs-only outcome record、~67 行 analysis.md 更新) | `9458660b` | **`auto_fix`** | 1 (同 FP 再現: docs-only diff にも関わらず Rust file hallucinate) | なし | なし | ~30-50s | 522s (takt review 2m 44s) |

**D-5 観測の意義**:

1. **`auto_fix` verdict 4 件目**: D-3 + D-4 CR fix + D-5 (2 push events) と同じ verdict 経路、FP pattern も file/scope 混同で共通
2. **reviewer による cross-check の構造的成功**: simplicity-review が "Lint Screen Cross-Check" section で finding を **明示的に false positive と判定** + 根拠を report に記載
3. **docs-only diff でも同 FP 再現**: 2 回目 push は analysis.md ~67 行のみで Rust file の変更ゼロにも関わらず、mistral:7b が同じ FP を出力。**lint_screen の FP は diff 内容ではなく hook のソース全文を見て hallucinate している強い証拠**
4. **副産物 (production bug fix)**: UTF-8 漢字単独 test 着手時に `byte_offset_to_line` の char-boundary panic bug を発見 → 1-line fix で resolve、direct unit test も追加
5. **lint_screen agreement の累積観測 (5 観測中 4 FP)**: いずれも file-type / scope 混同型 FP で同 root cause 推定、severity 全て `minor` で reviewer cross-check による blocking なし → 運用 viable
6. **副産物 (push workflow 知見)**: `jj new <bookmark>@origin` で remote bookmark の child commit を直接作成 → FF push で advance する手順を実証

### D-6 dogfood outcome (PR #152)

D-6 は当初想定 (案 A = takt workflow hook) から **案 C = fix.md instruction-level refresh** への pivot を経て、docs-only PR として land。同時に Bundle k 順位 123-127 の entry 登録も含めた combined PR。push event は 1 回のみ (CR review が Nitpick のみで user 判断で auto-merge):

| Push event | commit | screen_decision | findings | fallback | num_ctx overflow | lint_screen latency | pipeline 総時間 |
|---|---|---|---|---|---|---|---|
| 初回 push (D-6 impl + Bundle k entry、~210 行 docs-only diff) | `520ac0bb` | **`auto_fix`** | 1 (FP: `docs/local-llm-offload-analysis.md` line 1 を `use std::io::Write;` 誤認、D-5 と同 root cause) | なし | なし | ~30-50s 推定 | 510s (takt review 2m 48s、1 iter で APPROVE) |

**D-6 観測の意義**:

1. **`auto_fix` verdict 5 件目**: D-3 + D-4 CR fix + D-5 ×2 + D-6 で計 5 観測。docs-only diff (本 PR は markdown のみ) でも `auto_fix` 判定 + 同型 `unused-import` FP が一貫して再現 — mistral:7b の context window 内に hook source が含まれて hallucinate する pattern を **5 観測目** として裏付け
2. **本 PR の改修内容の direct dogfood は未達成**: takt review が 1 iter で APPROVE のため、fix step の `jj diff -r @ > .takt/review-diff.txt` instruction が実行される機会なし。本改修の execution rate / 効果検証は後続 PR (fix iteration が発生する PR) に持ち越し
3. **reviewer cross-check の構造的成功 (D-5 と同パターン)**: simplicity-review が "Lint Screen Cross-Check" で lint_screen finding を **明示的に false positive と判定** (line 1 が冒頭 markdown 見出しであることを直接読取 + use 文の不在を確認)
4. **post-merge-feedback 4 件採用 → すべて Bundle k 既存エントリと完全重複**: analyzer 自身が "Bundle k 優先度 123 で既に roadmap 済" と明記 → 新規順位を追加せず既存 4 entries (順位 123/124/126/127) に PR #152 を追加観測として追記 (Frequency 観測: 3 PR → 4 PR に更新)
5. **設計判断 pivot (案 A→案 C) のメタ知見**: takt v0.35.3 schema を直接参照 (`piece-types.d.ts` / `runtime-environment.js`) して案 A 不可と確定 → advisor 相談で案 C 採用 (既存 Bundle Z #B-β `fix-metrics-check.ps1` invocation と同形 precedent)。framework capability の不確実性は **実装前の schema/source 直接確認** で解消可能
6. **副産物 (push workflow 知見)**: 2 つの独立タスク (Bundle k entry 登録 + D-6 impl) が working copy に混在した状態から `jj split` で 2 commit に分離 + 1 PR で push という pattern を実証 (PR 分割せず commit のみ分離)

### D-7 dogfood outcome (PR #154)

D-7 は Bundle c-1 = post-merge-feedback workflow の abrupt termination 対策 3 件 (順位 63 pre-emptive marker + Drop guard / 順位 64 orphan reaper / 順位 67 ADR-030 §L1/L2 spec) を集約した Rust impl + ADR PR。pre-push-review APPROVE で 1 iter 完了、push event は 1 回のみ。**Round 2 最終 PR + 累積 5 PR 達成**:

| Push event | commit | screen_decision | findings | fallback | num_ctx overflow | lint_screen latency | pipeline 総時間 |
|---|---|---|---|---|---|---|---|
| 初回 push (Bundle c-1 impl + ADR、~845 ins / 175 del = 実 net +710 行) | `da5d8ae2` | **`human_review`** (fallback path) | 0 | **あり (新 failure mode)** | なし (mistral:7b 到達前) | 測定不可 (HTTP timeout) | 757s (pre-push-review 6m 19s、1 iter で APPROVE) |

**Fallback 詳細**:

- `fallback_reason`: `ollama error: http: HTTP error: http://localhost:11434/api/generate: Network Error: ... (os error 10060)`
- 失敗層: HTTP 接続 (mistral:7b inference に到達せず)
- `## Diagnostic` section: 不在 (Phase A 診断 metadata は Ollama 応答から抽出するため、HTTP 失敗時は emit 不可)
- 設計通りの soft-fail: lint_screen が `human_review` で fallback、push pipeline は block せず完走

**D-7 観測の意義**:

1. **新 failure mode 観測 (Ollama サーバ可用性)**: D-3〜D-6 で観測した 5 件はいずれも mistral:7b context window 内 hallucinate (file/scope 混同 FP) で同 root cause だったが、D-7 は **mistral:7b 到達前の HTTP 層 timeout** で異なる軸。Phase E 採否判定で「Ollama 可用性」という新軸を考慮する必要が顕在化
2. **fallback path の運用 viability 実証**: pipeline がブロックされず completes (757s = D-6 と同等の所要時間)、reviewer (simplicity-review) も lint_screen 不在で独立に APPROVE 判定。soft-fail 設計が機能
3. **Bundle c-1 self-dogfood 成功**: 本 PR がマージされた直後の post-merge-feedback workflow で **L1 pre-emptive marker が `.failed` として ~13 分間ディスクに visible**、UserPromptSubmit hook が正しく検出。workflow 完了 (`Workflow completed (2 iterations, 13m 22s)`) で `cleanup_failed_marker` により marker 削除 → Bundle c-1 の L1 floor が単体 test では捕捉できない full lifecycle で動作することを実証
4. **post-merge-feedback 採用ゼロ + 5 件様子見 + 2 件却下**: aggregate-feedback agent が「PR #154 は L1 Drop guard + L2 orphan reaper の多層 recovery architecture を高品質な実装と十分なテストカバレッジを伴って land」と総評、提案項目 (panic unwind test / Ollama timeout test / ADR-024 amendment / global rule mirror / e2e integration test) はすべて即時実装義務なし
5. **累積 verdict variance の 3 経路化**: D-3〜D-6 で `auto_fix` (5) + `informational` (1) の 2 経路だったが、D-7 で `human_review` (via fallback) を追加観測 = lint_screen の判定空間 3 経路すべてカバー
6. **副産物 (workflow 知見)**: `pnpm merge-pr` を bash `&` で background 化したとき、bash subshell が即 exit 0 を返すため Claude Code 側の task 完了通知は merge 終了より早く来る。長時間 subprocess の正確な完了検知には Monitor + tail -f + meta.json status 監視を併用する pattern が有効

## 📊 Phase D Round 1 + Round 2 (D-4〜D-7) 完遂後の Phase E 判定材料

- ✅ pipeline integration works end-to-end (D-1 #144 smoke test + D-3 #148 + D-4 #150 + D-5 ×2 + D-6 #152 + **D-7 #154** で計 7 real diff 完走)
- ✅ num_ctx 32768 で 67-649 行 diff overflow なし (Phase C reference values と整合、D-5 docs-only 67 行 〜 D-5 impl 649 行で size 帯拡大、D-6 docs-only 210 行で再確認、D-7 は HTTP 層失敗で mistral:7b 未到達のため num_ctx 軸の観測対象外)
- ⚠️ fallback rate: D-3 0/1、D-4 initial 0/1、D-4 CR fix 0/1、D-5 ×2 0/2、D-6 0/1、**D-7 1/1 (新 failure mode = Ollama HTTP timeout)** = 累積 **1/7 ≈ 14%**。kill-switch 50% 閾値との距離は十分確保、ただし新軸 (サーバ可用性) を Phase E で評価する必要あり
- ⚠️ agreement: 累積 false positive **5 件観測** (D-3 / D-4 CR fix / D-5 ×2 / D-6) — いずれも `minor` severity で reviewer cross-check 通過、blocking なし。D-5 / D-6 観測で「diff 外 context から hallucinate する failure mode」が docs-only diff でも reproducible と確定 = Bundle k 順位 123 (MD 除外フィルター) の構造的解消対象。D-7 は fallback path のため FP 観測機会なし (findings 0)
- ✅ verdict variance: `auto_fix` (D-3 + D-4 CR fix + D-5 ×2 + D-6) + `informational` (D-4 initial) + **`human_review` via fallback (D-7)** の **3 経路すべて**を観測、判定空間カバレッジ完成
- ✅ **累積 PR data 充足完了**: Round 1 (D-3) + Round 2 (D-4 + D-5 + D-6 + **D-7**) で **7 data points (5 PR)** 取得済、累積 5 PR (= ADR-038 採用条件) を **完全充足**
- ✅ **Bundle c-1 self-dogfood 成功**: D-7 自身のマージ過程で post-merge-feedback workflow の L1 pre-emptive marker + Drop guard が機能 (`Workflow completed (2 iterations, 13m 22s)` + 正常 path で marker cleanup) → 単体 test では捕捉できない full lifecycle で recovery 機構を実証

Phase E 着手の前提条件 **3-5 PR 累積 dogfood** は **完全充足** (5 PR / 7 data points)。Phase E (採否判定) に移行可能な状態。判定時の主要観察軸: (a) pipeline 機能性 ✅、(b) FP rate / 構造的解消可能性 ⚠️ (Bundle k 順位 123 で対処予定)、(c) verdict variance / fallback 設計 ✅、(d) **新軸 Ollama 可用性** ⚠️ (D-7 で 14% 観測、運用上の許容範囲評価が必要)。

## 📝 Dogfood signal log (旧 PR roster の preview 結果、Phase B/D 比較対象)

cli-finding-classifier 直叩き (pipeline 経由ではない) の preview 結果。Phase A〜C を triggerした 100% kill-switch 観測の原典:

| PR | 構成 | Diff 行 | Latency | findings | fallback_reason | Cumulative fallback |
|---|---|---|---|---|---|---|
| #139 (旧 P-1) | Bundle h + g-2 (docs-only) | 337 | 23s | 0 | `JSON parse error: missing field 'screen_decision'` (line 94) | 1/1 = 100% |
| #140 (旧 P-2) | Bundle j-1 (TOML + Rust regex) | 203 | 46s | 0 | 同 (line 94 column 1) | 2/2 = 100% |
| #141 (旧 P-3) | Bundle d (Rust test only) | 187 | 11s | 0 | 同 (line 1 column 692) | **3/3 = 100%** |

**観測**: (a) fallback rate 100% が 3 PR 連続 = 既に kill-switch 60% 超過、(b) latency variance (23s/46s/11s) は input size と弱相関 = mistral 内部状態 (cold/warm context) が支配的要因の仮説、(c) すべて同一 fallback_reason = 単一 root cause の可能性大 → Phase B で確定 (num_ctx truncation)。

**Phase d guide §3 kill-switch との関係**: ガイドは「real pipeline 経由で 3/5 fallback 観測 = 停止」と規定。本 preview は cli-finding-classifier 直叩きで pipeline 経由ではないが、3 連続 100% fallback は **厳密 kill-switch 超過に相当する severity**。Phase A〜C で repair しない限り Phase D に進めない判断 → Phase C fix 後の real pipeline dogfood (D-3〜D-6) で fallback 0/6 = 0% を実証して解消。

**Out-of-roster dropouts**: 旧 P-3 = Bundle g-1 (順位 85+86) は PR #125 で land 済を P-3 着手時に発見 → roster 除外 + stale todo 削除。経緯詳細は `feedback_verify_task_not_already_done.md` 参照。

## 関連リンク

- [local-llm-offload-analysis.md](local-llm-offload-analysis.md) — 現在の active 実行計画 (D-7 plan / Phase E 判定 / 残作業)
- [local-llm-offload-history.md](local-llm-offload-history.md) — Phase a-c 以前の analysis / 旧計画 retrospective
- [local-llm-offload-phase-d-guide.md](local-llm-offload-phase-d-guide.md) — Phase D 着手時の運用ガイド
- [ADR-038: ローカル LLM による CodeRabbit findings classification](adr/adr-038-local-llm-finding-classification.md)
- [ADR-040: Local LLM Context Size と Resource Trade-off](adr/adr-040-local-llm-context-size.md)
