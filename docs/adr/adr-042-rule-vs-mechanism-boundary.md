# ADR-042: ルール vs 仕組み化の境界基準

## ステータス

試験運用 (2026-05-25)

> ADR-039 (Experimental feature 標準パターン) に準拠: config opt-in なし (本 ADR は decision criteria であり実装機構ではないため該当しない) / kill-switch = 本 ADR を supersede する後続 ADR で停止可能 / bounded lifetime = 採用判定 6 ヶ月 (2026-11-25) を目安に dogfood 結果から本採用 / 修正 / 却下を判定。

## コンテキスト

### 問題

本プロジェクトでは知見の codify 先として **3 種類** の choice が存在する:

1. **rule docs** (`~/.claude/rules/common/*.md` / `CLAUDE.md` / ADR) — 人間 / AI が session 起動時に読み込む
2. **mechanism** (custom lint rule / PreToolUse hook / CI step / cargo test 等) — runtime に機械強制
3. **memory** (`memory/feedback_*.md`) — session 固有の補足

新規知見を codify する際、どの choice を選ぶかの判断が個別に行われており、systemic な判断基準が不在だった。具体的には PR #172 (順位 144 = `jj-message-required` preset) で「post-merge-feedback analyzer の docs 化原案を、ユーザー判断で hook 化に切り替え」というケースが発生し、後続 PR でも同型の判断 (6 件: 順位 44/61/146-151) が連続した。

### 既存事例

本 ADR 起案前にも、本 criteria に従う実装が散見される (= 暗黙裡の運用が systemic に存在していた):

| 機械化済 | 元の rule docs section |
|---|---|
| `no-ephemeral-todo-reference` lint rule | `coding-style.md` § Cross-File Reference Lifecycle |
| `no-mutable-anchor` lint rule | `coding-style.md` § 日付入り見出しアンカー |
| `hooks-post-tool-comment-lint-rust` 関数長 50 行 ratchet | `coding-style.md` § Long Functions |
| `polling-anti-pattern` preset | `development-workflow.md` § 背景タスクの待機方針 |
| `exe-help-block` preset | `development-workflow.md` § 長時間 subprocess pipe truncate 禁止 |
| `find_powershell_rules_missing_case_insensitive_flag` cargo test | `code-review.md` § `(?i)` フラグ必須 |
| `rule_test_coverage_check` cargo test | `testing.md` § Custom Lint Rule Test Coverage |
| `jj-message-required` preset (PR #172) | (rule 化前に hook 化方針確定) |

合計 **11 custom lint rule + 10 preset + cargo test contracts** が既に機械化済。残る rule docs 内に仕組み化候補が存在する。

### 既存 memory rules (本 ADR の哲学的基盤)

3 件の memory rule が本 ADR の核心哲学を session-level で codify している:

- **`feedback_no_unenforced_rules.md`**: 強制力のないルール追加は即却下、機械検知不可なら何もしない方がマシ。ルール乱立は重要ルール埋没の害悪。
- **`feedback_pipeline_over_rules.md`**: 動作の不確実さはパイプラインで吸収。ルール codify ではなくパイプライン設計で機械的に解決、Claude 判断介入を新規導入する anti-pattern を避ける。
- **`feedback_dogfood_evals_two_phase.md`**: 動作不確実な検証は evals + dogfood の 2 段階。妥当性 evals → 実運用 dogfood の順、PR-based 一気進行は阻害要因。

本 ADR ではこれら 3 件を ADR レベルに昇格し、派生プロジェクト transferability を確保する (memory は session-specific 補足として継続)。

## 検討した選択肢

### 選択肢 A: 既存 ADR-022 (自動化責務分離) に拡張

ADR-022 は automated actor の副作用範囲 (生成 vs 確定 / 既存 artifact 上書き禁止 等) を扱う。本 ADR の scope (rule vs mechanism の meta-decision) とは **直交**しており、拡張すると ADR-022 の主旨が霞む。**却下**。

### 選択肢 B: 新規 ADR で独立 codify (採用)

本 ADR は具体的 architecture pattern ではなく **上位の meta-decision criteria** であり、後続 ADR (ADR-022 / ADR-036 / ADR-039 等) が「本 ADR criteria に従って...」と参照する構造になる。独立 ADR が ADR 階層上 clean。**採用**。

### 選択肢 C: 記述せず memory rules のみで運用継続

memory は session-specific 補足の位置付けで、派生プロジェクトに伝播しない。本 criteria は派生プロジェクト (techbook-ledger / auto-review-fix-vc) でも適用される meta-principle のため codify が必要。**却下**。

## 決定

新規知見の codify 先を選択する際、以下の **3 step 判定** と **decision matrix** を適用する。

### Decision framework (3 step 判定)

#### Step 1: Mechanizable analysis (機械検知可能性)

以下の問いに Yes/No で答える:

- 検知方法が **regex / structural / AST / runtime check** で表現可能か?
- 検知失敗時の **graceful degradation** が可能か? (fail-soft で work 続行)

両者 Yes なら mechanizable。一方でも No なら mechanism 化困難 = rule docs 維持。

#### Step 2: False positive 緩和分析

mechanizable な場合、以下を評価:

- FP 率が許容範囲 (経験則 `< ~10%`) か?
- FP が出る場合、以下のいずれかで緩和可能か:
  - **paths filter** (test / config / docs フォルダ除外)
  - **opt-in design** (default fallback に含めない、明示有効化必要)
  - **severity warning** (block ではなく judgment 補助に格下げ)

緩和可能なら「限定 scope で仕組み化」、緩和不可能なら rule docs に倒す。

#### Step 3: Cost-benefit + Frequency 評価

- **実装工数**: S (~半日) / M (~1-2 日) / L (~3 日以上)
- **維持コスト**: rule = session 毎の read コスト × 期間、mechanism = FP 修正 / pattern 更新 コスト
- **観測頻度**: Frequency Low (1 観測) / Medium (2-3 観測) / High (4+ 観測)
- **Adoption Risk**: 既存 workflow 阻害 / breaking change の程度

Frequency Low の場合は **観測継続** (3 観測で再評価) を default とし、Medium+ で実装着手。

### Decision matrix

| Mechanizable | FP 緩和可 | Frequency | 判定 | 代表例 |
|---|---|---|---|---|
| ✅ Yes | ✅ Yes | Medium+ | **仕組み化** (hook / lint / CI / cargo test contract) | 順位 144 (jj-message-required) / 順位 146 (secret detection) |
| ✅ Yes | ⚠️ scope 限定で可 | Medium+ | **限定 scope で仕組み化** (paths filter / opt-in / warning) | 順位 150 (magic number, source folder 限定) / 順位 151 (PR diff, 条件付き block 3 段階) |
| ✅ Yes | ❌ No (FP 過多) | * | **Rule docs** (機械化 FP 過多、judgment が必要) | 順位 100 (同一 file multi-edit anti-pattern) |
| ❌ No (semantic) | * | * | **Rule docs** (intent / NLP 必要、機械化不可) | 順位 117 (ephemeral → permanent edit order) / 順位 128 (retirement clause consistency) |
| ✅ Yes | * | Low (1 観測) | **観測継続** (3 観測で Frequency Medium 昇格 → 再評価) | 順位 81 (cli-pr-monitor CR 投稿エラー auto-retry、defer 中) |

### 関連 design 原則

#### 伝播経路の違い

- **機械化** (custom lint rule / hooks-config.toml / push-runner-config.toml): exe deploy で派生プロジェクトに伝播。各プロジェクトで明示的に有効化 / config 設定が必要
- **Rule docs** (`~/.claude/rules/common/*.md`, `CLAUDE.md`): global location で派生プロジェクトに自動波及。`~/.claude/` の編集が直接的に派生プロジェクトの session 起動時 context に乗る

#### Mechanism graveyard prevention

機械化したものは継続的維持コストがかかる (FP 修正 / pattern 更新 / 廃止判定)。Frequency が下がっても放置されると tech debt 化する。本 ADR 採用後は以下を運用 default とする:

- 機械化機構は ADR-039 (Experimental feature 標準パターン) に従い試験運用期間を設定
- 採用判定で本採用 / 修正 / 却下を明示
- 却下時は機械化機構を物理削除 (test を含む全 artifact)

#### Rule docs 縮小効果

機械化された機構は対応する rule docs section を hook block message / CI gate output に集約することで rule docs を縮小可能。session 起動時 context 消費の削減 + AI 解釈ブレ排除の効果がある。

本セッション (2026-05-25) で 6 件 (順位 146-151) の仕組み化を採用した場合、`~/.claude/rules/common/*.md` の総量を **~30-50% 縮小** 見込み。

## 帰結

### 採用効果

- 新規知見の codify 先選択が systemic に判定可能になる (meta-decision の judgment 揺らぎ削減)
- 既存 rule の review 時にも適用 (本セッションで 6 件採用 + 3 件保留判定の実例)
- memory rules (`feedback_no_unenforced_rules` / `feedback_pipeline_over_rules` / `feedback_dogfood_evals_two_phase`) を ADR レベルに昇格、派生プロジェクト transferability 確保
- rule docs 縮小 → session 起動時 context 消費削減

### 既存運用との整合

- ADR-022 (自動化責務分離): runtime 責務、本 ADR と直交。両者を併用
- ADR-036 (Bundle Z 3 層): 本 ADR criteria に従い設計された pattern の一例 (= 仕組み化判定後の specific architecture)
- ADR-039 (Experimental feature 標準パターン): 仕組み化採用後の rollout phase で適用 (= 本 ADR criteria の下流)
- memory rules: ADR 昇格後も session-specific 補足として継続。新規判定時に memory を確認する運用は変更なし

### 欠点 / 留意点

- **判定の揺らぎ**: 「FP 率 `< ~10%`」「Frequency Medium」等の閾値は経験則であり、厳密な定義ではない。具体ケースでの判定は AskUserQuestion 等でユーザー判断を仰ぐ運用継続が望ましい
- **既存 rule の retroactive review**: 本 ADR 採用後、既存の全 rule を review して仕組み化候補を洗い出すべきか? 本セッションで 6 件採用したが、全件 audit は scope creep。**実装着手時の opportunistic review** で十分とする
- **mechanism graveyard 防止コスト**: 機械化機構の維持・廃止判定が本 ADR 採用で増える。ADR-039 (Experimental feature 標準パターン) 適用で軽減するが、長期的な technical debt 監視は別途必要
- **既存 ADR / docs への遡及参照**: 本 ADR を採用する時点で ADR-022 / ADR-036 / ADR-039 の冒頭に「本 ADR criteria に従う」blockquote を追加するか? **本 ADR 起案時は追加しない** (ADR-039 起案時と同方針、後続 PR で個別追補)

### 採用判定基準 (2026-11-25 目安)

以下のいずれかが満たされた時点で本採用に昇格:

- 本 ADR criteria を適用した新規 rule / mechanism 判定が 5+ ケースで適切に機能 (dogfood 期間中の AskUserQuestion 介入が 30% 以下)
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) で本 ADR を reference として独自 rule / mechanism 判定が行われた事例 1+

不採用判定 (= 修正 or 却下) は以下で発火:

- judgment 揺らぎが収まらず criteria が機能不全 (= AskUserQuestion 介入が 70%+)
- decision matrix の現実適合性が低いことが dogfood で明確 (例: Frequency Low でも仕組み化推奨ケースが頻発)

## 派生プロジェクトへの展開

- `~/.claude/rules/common/` への直接的な追記は本 ADR では行わない。本 ADR は `docs/adr/` 内に閉じ、派生プロジェクトは本 ADR を reference として参照する
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) が独自 rule / mechanism 提案する際、本 ADR criteria を参照する想定
- memory rules (`feedback_no_unenforced_rules.md` 等) は global location (`~/.claude/.../memory/`) で派生プロジェクトに自動波及するため、ADR と memory の両方が transferability 経路として機能

## 関連 ADR

- [ADR-022 (自動化責務分離)](adr-022-automation-responsibility-separation.md) — runtime 自動化の責務境界、本 ADR と直交 scope
- [ADR-036 (Bundle Z 3 層 review)](adr-036-bundle-z-three-layer-review.md) — 本 ADR criteria に従い設計された pattern の一例 (= 仕組み化判定後の specific architecture)
- [ADR-039 (Experimental feature 標準パターン)](adr-039-experimental-feature-standard-pattern.md) — 仕組み化採用後の rollout phase pattern、本 ADR criteria の下流
- [ADR-035 (docs-only PR 評価ポリシー)](adr-035-doc-evaluation-policy.md) — rule docs 系 PR の review 基準、本 ADR の「rule docs 維持」判定後の運用層

## References

- memory: `feedback_no_unenforced_rules.md` (機械検知不可ルール却下)
- memory: `feedback_pipeline_over_rules.md` (パイプライン化優先原則)
- memory: `feedback_dogfood_evals_two_phase.md` (動作不確実な検証の 2 段階アプローチ)
- 本セッション (2026-05-25) 6 件採用 (順位 44 / 61 / 122→136 統合 / 146 / 147 / 148 / 149 / 150 / 151) + 3 件 rule 維持 (順位 100 / 117 / 128+133)
- PR #172 (順位 144 = `jj-message-required` preset) — 本 ADR 起案の直接 trigger となった hook 化 dogfood 成功事例
