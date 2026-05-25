# Bundle 履歴 — post-merge-feedback 反映の累積記録

> **本ファイルの位置付け**: `docs/todo-summary.md` から「完了済 Bundle / post-merge-feedback 反映」の長文 paragraph を切り出した history 専用ファイル (2026-05-25 分離、todo-summary.md 50KB 超過解消)。サマリーは index 専用責務に集中、本ファイルは Bundle 単位の経緯 / 採用判定 / Sub-PR 構成等を時系列で蓄積する。
>
> **更新方針**: 新規 Bundle の post-merge-feedback 反映時に **本ファイル末尾に追記**。Bundle 単位の paragraph は完了後も残し、reference value として保持する (削除しない)。新規 task entry は引き続き `docs/todoN.md` 系列に登録、本ファイルは「採用判定の経緯」のみ codify する。
>
> **関連**: [docs/todo-summary.md](todo-summary.md) (現役 task の優先度 index) / [docs/todo*.md](todo.md) (現役 task 詳細) / `~/.claude/memory/feedback_*.md` (session-specific 補足)

---

## Bundle 1 完了 + post-merge-feedback 反映 (2026-04-29)

PR #91 (Bundle 1: PowerShell + Markdown anchor lint rules) merge 後の post-merge-feedback で **4 件の新規 task を追加** (PowerShell `(?i)` 自動検証 / `.claude/` filter + ADR-030 制約 / cli-pr-monitor 通知 Recovery 経路 / takt REJECT-ESCALATE)。**前 2 件は本 PR で実証された「fix iteration の根因」に対する決定論的防止策で最優先候補**。**日付ベース見出しアンカーのグローバル明文化 task は決定論的防止 (no-mutable-anchor rule) との二重防衛として継続有効**。

**reviewer facet 改善 (Bundle T で land 済)** + **post-pr-review fix loop の `.claude/` filter (Bundle T で land 済)** + **cli-pr-monitor ポーリング延長 + 重複起動ロック (Phase 3 で land 済)** + **rate-limit 自動検出 + 再トリガー (Phase 4 で land 済)** が完了し、reviewer 精度向上 + convergence cost 削減 + ポーリング頻度削減 + rate-limit 自動 recovery の四段構えが成立。残る Tier 2 では takt REJECT-ESCALATE が最優先候補。

**rate-limit 抑制の 3 層**: (1) Polling anti-pattern 検出 (PR #86 T1-1、完了済) = Claude 側の polling 禁止 (preventive)、(2) cli-pr-monitor ポーリング延長 + 重複起動ロック (PR #88 T2-4 / #96、完了済) = tool 側のポーリング頻度削減 (corrective)、(3) post-pr-review rate-limit 自動検出 + 再トリガー (Phase 4 / 完了済) = review 単位の自動 recovery。

**cli-pr-monitor 通知 Recovery 経路 (SessionStart hook 拡張) は PR #91 の直接観測知見**。SessionStart hook で再起動跨ぎの通知ロスト防止。post-pr-review fix loop の `.claude/` filter (path-based 解決) は Bundle T で land 済。

**Stop hook の `pnpm lint:md` 統合 task は Markdown linter hook 統合 (PR #88 で merged) の gap closure**。**AI 生成一時スクリプト pattern 検出は push 前 untracked `__*` hook (PR #85 T1-4) と関連** (実装前に擦り合わせ要)。

**`.failed` marker 自己文書化 task は ADR-030 soft-fail 機構の運用負荷削減** (PR #89 セッションで recovery が機能した実証から派生、Effort S)。

**takt REJECT-ESCALATE は post-pr-review fix loop の `.claude/` filter (Bundle T で land 済) の verdict-based 一般解**。path-based 解決が完了したので、本 task 着手で補完関係を完成させる。

**T3 グローバルルール 4 件 (日付ベース見出しアンカー / jj conflict リカバリ / `__` prefix scratch / post-pr-monitor polling 禁止) は `~/.claude/` 配下への XS 追記なので並列実施推奨**。

---

## Bundle U / V / e 完了 (2026-05-05)

**Bundle U の 順位 29 = PR #110、順位 30 = Bundle e、Bundle V の 順位 31/32 = PR #109、順位 33 = Bundle e で全消化**。**Bundle e (convention 明文化 long-tail、2026-05-05)**: 順位 23/24/25/26 (PR #85/#86 由来) + 順位 30 (Bundle U 残) + 順位 33 (Bundle V 残) + 順位 70 (Bundle d 残) を 1 PR で集約 land、`~/.claude/rules/common/{coding-style,git-workflow,development-workflow,code-review}.md` + `~/.claude/CLAUDE.md` の global rules に convention 7 項目を codify。XS×7 で long-tail 一掃。

---

## Bundle W / X (PBT + 型強化 + cargo-mutants)

**Bundle W (PBT + 型強化) は PR #96 で実証された flaky 実装防御の最上層**。Finding D (`saturating_sub` の silent semantic mismatch) と E (concurrency test の guard 即 drop) はどちらも「Rust 的に正しいコードがドメイン的に間違う」典型例で、advisor + takt-fix の 2 layer も貫通した。**仕様を proptest properties で明文化 + `PastTime` 等の型で invalid state を unrepresentable に** することで、ルール (ask-based) では塞げない bug class を構造的に排除する。**rate-limit 自動検出 (Phase 4 で land 済) / takt REJECT-ESCALATE を先行**し、その後 Bundle W に着手する流れがユーザー指示。

**Bundle X (cargo-mutants + stress runner) は Bundle W の後付け検証層**。L2 post-PR で変更 crate + 1-hop 依存に cargo-mutants を走らせ test の弱さを直接測定、L1 pre-push で concurrency stress N=100 を回し scheduling race を sampling。Bundle W で書いた spec / 型を後段で機械的に検証する補完関係。**L3 weekly cargo-mutants workspace 全体 + stress N=1000 は ADR-031 Phase B 週次レビューと bundle 化** することで long-tail flake と coverage 全体監査を week 単位で audit する layer に統合。

---

## PR #98 (Bundle Y2) post-merge-feedback 反映 (2026-05-01)

3 件の follow-up task を追加。**takt workflow `model` フィールド必須化 lint rule** と **Bundle Y2 効果の定量計測** は Bundle Y2 完全性確保 + ROI 検証で同系列 (lint rule 着手時に post-pr-review.yaml supervise step への `model: sonnet` 明示追加を同 PR に含める想定)。**prepare-pr skill Step 1 bookmark 存在チェック強化** は本セッション運用痛 (bookmark 未作成 push 失敗) から派生した独立 task で skill repository 側の更新となる。

---

## Bundle a (PR #99 post-merge-feedback 反映、2026-05-02 拡張)

4 component を **2 Sub-PR で分割** land 推奨 (設計根拠は ADR-034)。共通テーマは「PR #99 で複数回発生した手動 `@coderabbitai review` 投稿の自動化」 + 「CR review query の token bloat 削減」。Bundle Y2 効果でパイプラインが加速した結果として CR rate-limit 発生頻度が増えた逆説的副作用への対策。effort 合計 M+S+XS+M (= 2 Sub-PR で M、M+S 程度に分散)。

- **Sub-PR 1 (token 削減層、先行)**: **gh CLI 使用規則** (`git-workflow.md` 追記) + **`check-ci-coderabbit --list-findings`** (Rust モード、cli-pr-monitor 連携 API 提供)。旧 Bundle Z2 の `#D-1` + `#D-3` を本 Bundle に統合 (旧 `#D-2` は `#D-3` で代替のため取り下げ、旧 `#D-4` は思考連続性懸念で保留、ADR-034 参照)
- **Sub-PR 2 (rate-limit 自動化層、主軸)**: **cli-pr-monitor の rate-limit auto-retry** (Sub-PR 1 の `--list-findings` API を消費) + **ADR-018 / ADR-009 の rate-limit retry ポリシー明文化** + **integration test 追加** (rate-limit 検出 → backoff → retry サイクルの regression 防止、PR #100 post-merge-feedback T2-1 採用) + **`parse_findings` 系 error-path test infra** (順位 49、PR #101 T2-1、`unwrap_or_else(\|_\| empty)` silent fallback の test 検証)。session 超え recovery / walkthrough overlay 検出 / 解除 + 1 分マージン投稿の設計詳細は ADR-034

---

## PR #101 (Bundle a Sub-PR 1) post-merge-feedback 反映 (2026-05-03)

9 件の finding を頻度評価 (過去 report 横断 + 同一 PR latent 件数) して **3 件を採用**。**順位 47 (`>` vs `>=` boundary lint)** は同一ファイル内 3 関数 (parse_listed_findings / parse_new_comments / parse_findings) で同 drift が実証済 = latent 高頻度。**順位 48 (関数長 oxlint)** は #96 / #101 で繰り返し言及 = explicit 高頻度。両者とも Bundle Z #B-α と同じ「決定論的防止層」哲学で、Bundle Z Phase 1 (Rust comment lint) の land 後に並列 deploy 可能。**順位 49 (error-path test infra)** は #99 / #101 で同型 silent fallback anti-pattern が再発、Bundle a Sub-PR 2 (順位 42 / 43 / 46) と **同一 PR で land** 推奨 (cli-pr-monitor の mock infrastructure を再利用、test 二重投資なし)。残り 6 件 (Tier 1 #1, #3, #5、Tier 2 #2、Tier 3 #1, #2) は session 1 回限りの low-frequency events として不採用。

---

## Bundle h (PR #123 post-merge-feedback、experimental feature 標準パターン + ephemeral lifecycle 強化、2026-05-07) ✅ 完了

順位 89 (Experimental feature 標準パターン = ADR-039 として codify) + 順位 90 (ephemeral 大規模 content の ADR 昇格基準 + config コメント lifecycle anti-pattern を `~/.claude/rules/common/{docs-governance,coding-style}.md` に追加) で land。

**却下** (4 件): T1 #3 (`enabled = true` 検出 lint、誤検出確実) / T1 #4 (見出し参照誤り検出 hook、NLP 必要) / T2 #2 (env var override、ROI 不成立) / T3 #4 (ADR-039 config hardcode policy、ADR-038 でカバー済)。**様子見** (4 件): T1 #1 (ephemeral 計画書参照 lint、命名規則 codify 先行) / T1 #2 (jq 括弧不均衡 lint、再発頻度低) / T2 #1 (classifier endpoint fallback integration test、takt test infra 調査依存) / T3 #3 (config コメント ADR 参照修正、XS opportunistic)。

---

## Bundle g (PR #121 post-merge-feedback、monitor verdict logic + session pattern codify、2026-05-07) ✅ 完了

4 件採用 (Tier 1 #85、Tier 2 #86、Tier 3 #87/#88) を 2 PR で land。**g-1 (順位 85 + 86、Rust 実装 + verdict transition matrix tests) は PR #125 で land 済** (`compute_verdict` の review_state == "not_found" / "pending" guard + 12 verdict transition tests in `src/cli-pr-monitor/src/stages/monitor.rs`)。**g-2 (順位 87 + 88、global rule 追記)** は Bundle h と同 PR (#139) で land 済。Bundle f との関係: Bundle f は retry logic (rate-limit + 投稿エラー)、Bundle g は verdict logic (review_state 評価) で別軸、両者 land で post-pr-monitor の robustness が retry/verdict/state 全方向で堅牢化された状態。

---

## Bundle f (PR #120 post-merge-feedback、cli-pr-monitor robustness、2026-05-07)

PR #120 (ADR-038 Phase 5: cli-finding-classifier 統合) の dogfood で post-pr-monitor の wakeup state 遷移に複数の edge case を観測。**5 件採用** (Tier 1 #80/#81、Tier 3 #82、Tier 2 #83、Tier 3 #84) で **3 層対策**: (1) 実装層 = 順位 80 / 81 (rate-limit + CR 投稿エラーの auto-retry path 整理) + 順位 82 (ADR-018 設計明文化、同 PR 推奨)、(2) test 層 = 順位 83 (複合 guard の独立 variant test)、(3) ガイド層 = 順位 84 (code-review.md checklist 追記、独立並列可)。**Sub-PR 分割推奨**: f-1 (順位 80 + 81 + 82、cli-pr-monitor + ADR、Effort M+M+S、Bundle f コア) / **f-2 (順位 83、test 拡充) + f-3 (順位 84、global rule) は 2026-05-21 land 済 (Bundle B)**。Bundle f はローカル LLM dogfood (ADR-038 採用、2026-05-15) の副産物として cli-pr-monitor の堅牢化を進める位置づけ。

---

## Bundle c (PR #109 post-merge-feedback 堅牢化、2026-05-04)

PR #109 で post-merge-feedback workflow が SIGPIPE で silent 中断され `.failed` marker 未生成という ADR-030 仕様違反が実証された。5 件採用 (Tier 1 #63/#64/#65 + Tier 3 #66/#67) で **3 層防御** を構築: (1) 事前防止 = 順位 65 (exe + `--help` を PreToolUse block) + 順位 66 (グローバルルールの subprocess pipe truncate 禁止)、(2) in-process recovery = 順位 63 (Drop guard / signal trap で abrupt 終了時の `.failed` marker 保証)、(3) out-of-process backstop = 順位 64 (`meta.json status=running` 5-15 分放置 reaper)。順位 67 (ADR-030 spec 拡張) は実装と同 PR で仕様/実装の整合性確保。**Sub-PR 分割推奨**: c-1 (順位 63 + 64 + 67、Rust 実装 + ADR、Effort M+M+XS、コア層) / c-2 (順位 65 + 66、hook + global rule、Effort S+XS、trigger 防止層)。c-1 と c-2 は独立に land 可能だが c-1 land 後の dogfood で recovery 機構を実証してから c-2 を入れると順位の合理性が見える順序になる。

---

## Bundle d (PR #110 post-merge-feedback、2026-05-04)

PR #110 (Bundle "docs quality pre-write") merge 後の post-merge-feedback で 6 findings 中 3 件採用。共通テーマは「PR #110 で導入した `no-ephemeral-todo-reference` rule (順位 29 採用分) の robustness 強化 + 設計 doc / 実装の乖離 ガード」。**順位 68 (T2 self-exclusion test)** は **本 PR (Phase d P-3 繰上げ) で land 済** = `hooks-post-tool-linter` の 6 件 unit test (TP / FP / Edge / 大文字無視 / 拡張子限定 / deployed TOML self-exclusion invariant)。**順位 69 (T3 yaml/yml コメント)** は OBS-2 (spec-impl 乖離) 対策で別 PR で対応予定。順位 70 (code-review checklist) は Bundle e で land 済。

---

## Bundle f + retirement (PR #111 post-merge-feedback + 計画書 retire、2026-05-05) ✅ 完了

PR #111 (Bundle e) merge 後の post-merge-feedback で 10 findings 中 4 件採用 (順位 71/72/73/74 = Tier 3 XS×4) + 順位 62 (Document Governance) + `docs/docs-pr-iteration-efficiency.md` retirement を **1 PR で集約 land**。Sub-PR 分割推奨ルール (順位 73 自身が codify する内容) を本 PR で **dogfood**: 順位 73 が land する PR 自身が「分割 vs 統合」判断対象 → ファイル削除 + 順位 62 + Bundle f を統合した結果、scope は 5 ファイル touch (global rules 2 + ADR 2 + 削除 1) + cleanup で clean、Bundle 分割で得られる review 容易性より統合 PR の atomic な lifecycle 完結性が勝った。共通テーマ: 「PR #111 自己違反事例 → self-application 強化」 + 「Document Governance を global rule に codify」 + 「計画書 retirement を実例化」の 3 layer 同時 land。

---

## PR #113 (Bb-1 = Bundle b PR-1) post-merge-feedback (2026-05-05)

9 findings に対して **1 件のみ採用** (順位 75 = T2-2 の `finalize_parked` write_state 失敗時 fail-safe 回帰テスト)。T1 #1/#2 (lint rule 案) は NLP 必要 / FP リスクで却下、T2-1 (Windows path test) / T2-3 (state cycle integration) / T2-4 (CronCreate format lint) は ROI 不見合いで不採用、**T3-1 / T3-2 (`~/.claude/rules/common/coding-style.md` への ルール追記)** は **ユーザー判断で却下** — 「強制力のないルール追加は却下: 機械検知できなければ何もしない方がマシ。ルール乱立は重要ルール埋没の害悪」(memory: feedback_no_unenforced_rules.md として codify 済)、T3-3 (PARK signal 設計 ADR) は premature で 🤔 様子見保留。**本 PR 含意**: Bb-1 の sibling parity invariant (`finalize_*` 群の error path 対称性) は Bb-2 / Bb-3 で同種関数を追加する際に再発確度が高いため、**test レベルで machine-enforceable に保護** することを Bb-2 着手前の前提条件とする。

---

## PR #114 (Bb-2 + 順位 75 = Bundle b PR-2 + T2-2) post-merge-feedback (2026-05-05) ✅ 完了

9 findings に対して **2 件採用 / 5 件様子見 / 3 件却下**。**Bb-3 (順位 55) で fold-in する採用提案**: T2-2 (Parity test coverage 拡張 = `finalize_park_siblings_have_symmetric_write_state_handling` テストに `finalize_initial_review_park` を追加、self-violation 解消、Effort S)。T2-1 (Legacy JSON deserialize test) は **PR #114 で既に実装済** (`state_legacy_json_without_new_fields_deserializes_with_defaults`、Bb-3 以降の新フィールド追加時に同 pattern を継続するための reference として保存)。

**様子見 (5 件)**: T1-1 (finalize_* parity lint、Effort M + NLP 必要、簡易プロキシで再評価) / T1-2 (polling block lint、FP リスクで dogfood 後判断) / T2-3 (env override コメント強化、XS Low) / T2-4 (CI parallel race 確認、preventive only) / T3-1 (Wakeup Resume Invariant ADR) / T3-2 (DI 戦略 ADR、Bb-3 着手時に再検討)。

**却下 (3 件)**: T1-3 (Serde schema lint、ROI 低、T2-1 で代替) / T3-3 (parity invariant の global rules 追加) / T3-4 (test-only env var prefix rule) — 後 2 件は **memory: feedback_no_unenforced_rules.md** を直接引用してアナライザが正しく即却下判定。Bb-2 land 時点で Bundle b の核 (CronCreate park モデル) 完成、残る Bb-3 は config 整理 + SessionStart catch-up + T2-2 follow-up を bundled。

---

## Bundle j (PR #133 post-merge-feedback、docs/ 整合性多層検証、2026-05-09)

PR #133 (todo.md / todo5.md 50KB 分割) merge 後の post-merge-feedback で 9 findings 中 **3 件採用** (Tier 1 #1 / Tier 2 #3, #4) で **3 層対策**: (1) **規約層** = 順位 94 (`(?i)\]\(\.\./docs/` regex 1 行で `docs/` 配下からの逆戻り参照を block) は CodeRabbit が PR #133 で実検出した broken link を決定論的に防止、(2) **CI 検証層** = 順位 95 (preamble file count 自動照合) は todo*.md 分割が今後反復する pattern (todo3 → 4 → 5 → 6 → 7) のため Frequency Medium で採用、(3) **包括的 link 検証層** = 順位 96 (Markdown cross-reference validator、directory-aware resolution) は順位 10 (ADR-032 PR-broken-link) と方向性近接で fold-in 検討余地あり。

**Sub-PR 構成**: **j-1 (順位 94、`.claude/custom-lint-rules.toml` 規約追加) は land 済 (Phase d P-2)** / j-2 (順位 95 + 96、`.github/workflows/lint.yml` 新設 = workflows 未存在 repo の最初の workflow 整備、Effort S+M、まとめて land が効率的)。

**却下** (2 件): T1 #2 (prose 数詞 lint、NLP 必要 + FP 確実) / T3 #5/#6 (機械検知不可ルール追加、`feedback_no_unenforced_rules.md` 適用)。**様子見** (3 件): T3 #7 (ADR-035 not_applicable と GitHub thread state の乖離明文化、1 観測のみ) / T3 #8 (ADR-030 AFK wakeup 時 PR body intent ルール化、1 観測のみ) / T3 #9 (50KB 分割原則 CLAUDE.md 明文化、機械検知不可)。

**本 PR 含意**: 順位 94 = 「決定論的防止層」哲学 (Bundle Z #B-α 系譜)、順位 95-96 = 「規約だけでは塞げない構造的検証は CI で」(ADR-031 週次レビューと相補)。`.github/workflows/` 未存在 repo に最初の workflow を追加する転換点でもあり、scope 慎重判断が必要。

---

## Bundle k (PR #151 post-merge-feedback、Phase D dogfood 観測由来の lint-screen FP 対策、2026-05-13)

PR #151 (Phase D D-5 = comment-lint test 拡充 + MAX cap test) merge 後の post-merge-feedback で 11 findings 中 **5 件採用** (Tier 1 #1, #2 / Tier 2 #1 / Tier 3 #1, #2) を 5 entries (順位 123-127) で登録。**コア発見**: D-3 (PR #148) / D-4 CR fix (PR #150) / D-5 ×2 (PR #151) の 3 PR・4 push events で「mistral:7b が docs-only diff や `.md` ファイルに対して Rust の `unused-import` を hallucinate する」FP pattern が一貫して観測 = Phase b' fixture では再現しない failure mode。**順位 123 (lint-screen MD 除外フィルター、Tier 1 / M / High freq)** が最重要 = 拡張子ベース mechanical filter で構造的に解消可能、Phase D dogfood 観測から導かれた最も価値ある決定論的防止策。

**Sub-PR 推奨**: k-1 (順位 123 + 126、実装 + ADR-038 codify、Effort M+XS、コア層) / k-2 (順位 124 + 127、TOML test + extensions code comment、Effort S+XS、test gap 補強層) / k-3 (順位 125、UTF-8 boundary 横展開、Effort M、独立) で 3 PR 分割推奨。

**却下** (4 件): UTF-8 lint rule (FP リスク、AST 必須) / `byte_offset_to_line` 強化 (PR #151 で既対応) / UTF-8 guideline + extensions checklist (`feedback_no_unenforced_rules.md` 適用)。**様子見** (3 件): T2 #2 (lint-screen dogfood CI step、L effort + takt test infra 調査依存) / T3 #3 (test 拡充→bug 発見 pattern を ADR-007 記録、1 PR 観測のみ) / T3 #4 (multi-rule scenario fixture pattern を test comment 明文化、Low × Low)。

**本 PR 含意**: Phase D dogfood 観測 (analysis.md L334-340) が直接 actionable な決定論的防止層 (順位 123) に結実、Phase E 採否判定前に systemic FP root cause が解消される構造的進展。

---

## Bundle k 補強 (PR #152 post-merge-feedback、D-6 docs-only PR、2026-05-13)

PR #152 (Phase D D-6 = fix.md instruction-level review-diff refresh + Bundle k 順位 123-127 entry 登録) merge 後の post-merge-feedback で 8 findings 中 4 件採用 (Tier 1 #1 / Tier 2 #1 / Tier 3 #1, #2)。**全 4 件が Bundle k 既存エントリ (順位 123/124/126/127) と完全重複** = post-merge-feedback analyzer 自身が「Bundle k 優先度 X で既に roadmap 済」と明記。新規順位を追加せず、**既存 4 entries (順位 123/124/126/127) に PR #152 を追加観測として追記** (frequency 観測: 3 PR → **4 PR** に更新、Bundle k の優先度 / Sub-PR 分割推奨は不変)。

**注**: 順位 126 (ADR-038 hallucinate codify) は Phase E 採用昇格 PR で ADR-038 へフォルドイン land 済 (2026-05-15)、現 table には不在。

**含意**: PR #152 (docs-only) でも `.md` への `unused-import` FP が同根 root cause で再現したことが「lint_screen FP は diff 内容ではなく hook source 周辺 context を見て hallucinate している」仮説を 4th observation として裏付け = 順位 123 拡張子フィルター実装の confidence 向上。

**様子見** (2 件): PostToolUse hook 自動化 (案 D、Frequency Low) / fix.md 自己参照 ambiguity (1 PR 観測のみ、次回 fix.md 編集機会に opportunistic 適用)。**却下** (2 件): 機械検知不可な `~/.claude/rules/*` 追加 (memory `feedback_no_unenforced_rules.md` 適用)。

---

## Bundle k 補強 (PR #153 post-merge-feedback、analysis.md 軽量化 PR、2026-05-13)

PR #153 (D-6 post-merge follow-up + analysis.md 49KB→26KB split) merge 後の post-merge-feedback で 6 findings 中 **2 件採用** (Tier 3 #1, #2)。**T3 #1 = 順位 126 (ADR-038 hallucinate codify、Phase E でフォルドイン land 済) と完全重複** → 順位 126 entry を「**5 PR 連続観測** (#148/#150/#151/#152/#153)」+ root cause / structural fix の明示記載要件追加で更新 → 最終的に Phase E 採用昇格 PR (2026-05-15) で ADR-038 § Known failure modes に migrate。**T3 #2 = 新規採用** → **順位 128 (CLAUDE.md § Cross-File Reference Lifecycle に多ファイル同時削除 retirement condition checklist 追加)** として登録、PR #133 (todo.md 分割) + PR #153 (analysis.md 分割) の successful pattern を明文化。

**様子見** (2 件): CLAUDE.md → docs-governance.md cross-link / docs-only PR template の Retired sections list (どちらも Frequency Low)。**却下** (2 件): cross-reference lifecycle 自動 lint rule (NLP 必要) / file role scope exceptions guidance (1 観測のみ、過剰一般化リスク)。

**含意**: docs-only PR でも mistral:7b の FP が 5 観測目として再現、Bundle k 順位 126 の優先度を High freq として確定。多ファイル分割の retirement workflow を順位 128 で global rule 化することで、今後の docs/* 50KB 分割 (history.md 等) で同 pattern を mechanical に reproducible 化。

---

## PR #168 post-merge-feedback (Bundle B follow-up、2026-05-21)

PR #168 (Bundle B = 順位 83 cli-pr-monitor 複合 AND guard test 単独検証 + 順位 84 code-review.md checklist 追記) merge 後の post-merge-feedback で 6 findings 中 **1 件採用** (Tier 3 #2) を 順位 139 として登録。**順位 139 = ADR-041: Test Isolation Patterns for Multi-Condition Guards** — PR #120 W-001 初発見 + PR #168 sentinel pattern 実装の **2 PR 横断で Frequency Medium** に達したため、code-review.md global checklist (順位 84 land 済) に加えて project-level ADR で rationale・実装例 (poll.rs `enrich_with_classifier_skips_when_disabled` / `_skips_when_findings_empty`)・PR #120 W-001 history を codify する方針が成立。番号は順位 135 codified placeholder policy に従い当初 `ADR-NNN` で entry 登録 → **本 PR (順位 139 land) で `ADR-041` 確定取得**、順位 78 (旧 ADR-041 予約) を再 placeholder 化。

**様子見** (1 件): T2 #1 (guard isolation test 用 precondition assert helper macro 抽出、複合 guard test 再登場時に再評価)。**却下** (4 件): T1 #1 (NLP 必要 + L effort lint rule) / T1 #2 (Bundle Z #B-α safe-list exception、Frequency Low + 代替策で十分) / T2 #2 (CI coverage section、Very Low freq + ROI 不見合い) / T3 #1 (`~/.claude/rules/rust.md` rule 追加、`feedback_no_unenforced_rules.md` 適用)。

**recovery 含意**: 本 PR の post-merge-feedback は cli-merge-pipeline 経由で起動した workflow が PC crash で abrupt 終了し、ADR-030 §L1 の pre-emptive `.failed` marker (`168.md.failed`) が UserPromptSubmit hook (`hooks-user-prompt-feedback-recovery`) によって次セッションで検出 → `pnpm exec takt -w post-merge-feedback` 直接再起動で aggregation 完了 → manual cleanup (report copy + marker 削除) で復旧。**gap 観測**: 復旧手順 (`168.md.failed` の文書) は takt 再起動コマンドのみ codify されており、`.takt/runs/<run>/reports/feedback-report.md` → `.claude/feedback-reports/<PR>.md` への copy + marker 削除という artifact relocation step が未文書化。次回類似復旧時の摩擦軽減 follow-up 候補。
