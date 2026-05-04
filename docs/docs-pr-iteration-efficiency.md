# ドキュメント PR 作業効率改善

> **目的**: フィードバックの都度改善内容をドキュメントに反映するため、**docs-only PR を独立して早急にマージできる環境** を確立する。
>
> **本ドキュメントの役割**: docs-only PR の iteration 改善に関する task 分類・bundle 案を集約する index。各 task の作業詳細は `docs/todo*.md` 系列に置き、本ファイルは概要 + リンクに留める。
>
> **状態**: 試験運用 (本ドキュメントは "計画書" であり、bundle が消化されたら役割を終える)
>
> **想定読者**: 本リポジトリで docs-only PR を回す際に「どの task が iteration 短縮に効くか」を 1 ページで把握したいユーザー。

---

## 現状の課題

- docs-only PR が次の実装 PR に混ぜてコミットされる傾向 (= 独立 merge せず後回しになる)
- 主因はイテレーションコスト: docs PR 1 本あたり pre-push-review + post-pr-review + post-merge-feedback で 15-30 分かかる
- false REJECT (code-specific criteria を docs PR に誤適用) で iter が増える

## ボトルネック分析

| ボトルネック | 現状 | 改善の方向 |
|---|---|---|
| pre-push-review time | docs-only PR でも全 criteria 適用、5-10 分 | reviewer facet に docs-only fast-approve 拡張 |
| post-pr-review time | CodeRabbit + takt analyze-coderabbit が code criteria を docs PR に誤適用、5-10 分 | analyze-coderabbit にも docs-only filter 拡張 |
| post-merge-feedback time | trivial PR skip で `*.md` only + 1 commit + < 50 行 は skip 済 (PR #102) | 既存条件で十分、追加最適化不要 |
| docs 品質 pre-write | markdown lint が手動、anchor / link drift が後段で発覚 | Stop hook lint + broken-link-check + cross-reference lint を pre-write で機械検出 |
| review false REJECT | reviewer が mutation / error handling / test coverage を docs PR に要求 | docs 評価ポリシー ADR で criteria を構造的に分離 |

---

## 改善 task 分類

各 task の詳細 (動機 / 設計決定 / 作業計画 / 完了基準) は `docs/todo*.md` 系列を参照すること。本セクションは概要 + 効果のみ記載。

### 🎯 HIGH IMPACT — review acceleration

Bundle "docs PR streamline" (順位 59 / 31 / 32) は本セッションで land 済 → [ADR-035](adr/adr-035-doc-evaluation-policy.md) として集約。残るタスクは下記 MEDIUM / LOW IMPACT のみ。

### 🛠 MEDIUM IMPACT — docs 品質 pre-write 保証

Bundle "docs quality pre-write" (順位 4 / 29) は本セッションで land 済 → `.claude/hooks-config.toml` に `lint:md` Stop step 追加 + `.claude/custom-lint-rules.toml` に `no-ephemeral-todo-reference` rule 追加。残るは下記 順位 10 のみ (ADR-032 系列待ち)。

| 順位 | Tier | タスク概要 | 効果 | 作業詳細 |
|---|---|---|---|---|
| 10 | 🔧 Tier 2 | broken-link-check + 内部アンカー検査統合 (ADR-032 PR-broken-link) | docs/ 内 link 健全性を CI で機械検出、anchor drift を pre-merge で catch | [todo2.md](todo2.md) |

### 📋 LOW IMPACT — convention グローバル明文化

Bundle "e" (順位 23 / 24 / 25 / 26 / 30 / 33 / 70) は本セッションで land 済 → `~/.claude/rules/common/{coding-style,git-workflow,development-workflow,code-review}.md` + **global** `~/.claude/CLAUDE.md` に 7 項目 codify (プロジェクトローカルの `CLAUDE.md` ではなく、ユーザーグローバルの `~/.claude/CLAUDE.md`)。convention 明文化の long-tail を一掃。

---

## 推奨 bundle

### Bundle "docs PR streamline" ✅ 完了

ユーザー目的「docs-only PR を独立して早急にマージできる環境」の最短達成パス。本セッションで 3 件 land。

| 含む順位 | 概要 | 反映先 |
|---|---|---|
| 59 | 中核: ADR-035 docs 評価ポリシー | [ADR-035](adr/adr-035-doc-evaluation-policy.md) + `review-simplicity.md` / `review-security.md` / `analyze-coderabbit.md` 引用 |
| 31 | review-security.md fast-approve 精度向上 | `review-security.md` Excluded paths (`.takt/**` / `.claude/**`) 追記 |
| 32 | todo.md ヘッダ実態整合 | `docs/todo.md` 冒頭運用ルール |

**期待効果** (検証は本 PR merge 後の dogfood で実施):

- pre-push-review が docs-only PR に対し fast-approve → review time 1 分 30 秒〜30 秒に短縮
- post-pr-review (analyze-coderabbit) が docs-only PR で false REJECT を出さない
- 順位 32 で運用ルールの自己矛盾を解消

**Bundle b との独立性**: Bundle b (順位 53/54/55, CR operation 安定化) は rate-limit 対応で別領域。並行進行可。

### Bundle "docs quality pre-write" ✅ 完了

write-time に docs 品質を保証する層。本セッションで 2 件 land。

| 含む順位 | 概要 | 反映先 |
|---|---|---|
| 4 | Stop hook lint:md | `.claude/hooks-config.toml` `[[stop_quality.steps]]` に `lint:md` 追加 |
| 29 | docs/todo 参照検出 lint | `.claude/custom-lint-rules.toml` rule⑥ `no-ephemeral-todo-reference` (severity warning、`docs/todo[0-9]*\.md` を `rs` / `toml` / `jsonc` / `json` / `yaml` / `ts` / `py` / `ps1` 等で検出) |

**期待効果**: 本セッションでの dogfood で `docs/todo3.md` を含む `__dogfood_lint.rs` を作成 → rule が warning 2 件発火を確認済。今後 `.rs` / `.toml` / config で ephemeral todo reference を書こうとした時点で hook が警告。

### Bundle "e" (convention long-tail) ✅ 完了

順位 23-26 + 30 + 33 + Bundle d 残の 順位 70 を 1 PR に集約。convention 明文化で long-tail の品質改善。本セッションで 7 件 land。

| 含む順位 | 概要 | 反映先 |
|---|---|---|
| 23 | 日付ベース見出しアンカー安定識別子優先 | `~/.claude/rules/common/coding-style.md` Markdown 節新設 |
| 24 | jj conflict リカバリ手順 | `~/.claude/rules/common/git-workflow.md` jj Operations 節拡張 |
| 25 | `__` prefix scratch file 規約 | `~/.claude/CLAUDE.md` Personal Preferences > Code Style 拡張 |
| 26 | post-pr-monitor polling 禁止 | `~/.claude/rules/common/development-workflow.md` 「背景タスクの待機方針」節新設 |
| 30 | Cross-File Reference Lifecycle 具体例 | `~/.claude/rules/common/coding-style.md` の同セクションに 3 種類のファイル例 + raw string 編集時補助 |
| 33 | 新 verdict 経路追加時の 3 点同期チェック | `~/.claude/rules/common/code-review.md` Multi-point synchronization 節新設 |
| 70 | 設計 doc / 実装の同期チェック | 同上 (33 と同セクション内、相補項目) |

**期待効果**: AI / 人間の共通言語として global rules に codify されたため、新セッションでも convention が自動参照される。Bundle "docs quality pre-write" の決定論層 (lint rule) と本 bundle の preventive guidance (rule) で **machine + guideline の二層防御** を構築。

### 保留中

順位 10 (broken-link-check) は ADR-032 関連の他タスク (順位 6, 20, 21) との依存があり、独立着手は非効率。ADR-032 全体の進捗に追従する。

---

## 関連ドキュメント

- [docs/todo.md](todo.md) — 推奨実行順序サマリ表 (priority table)
- [docs/coderabbit-monitoring-efficiency.md](coderabbit-monitoring-efficiency.md) — CodeRabbit 監視機能改善 (並列の領域特化計画書)
- [ADR-036: Bundle Z 3 層アーキテクチャ](adr/adr-036-bundle-z-three-layer-review.md) — pre-push-review 設計根拠 (旧 docs/pipeline-token-efficiency.md #B を ADR 化)
- [ADR-019: CodeRabbit レビュー運用のハイブリッド構成](adr/adr-019-coderabbit-review-hybrid-policy.md) — CodeRabbit 運用根拠
- [ADR-027: Push-time review を simplicity に限定](adr/adr-027-push-review-simplicity-focus.md) — review scope の既存 design rationale
- ADR-035: docs 評価ポリシー (順位 59 で作成予定、本 bundle のメイン成果物)
