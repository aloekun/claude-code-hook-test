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

| 順位 | Tier | タスク概要 | 効果 | 作業詳細 |
|---|---|---|---|---|
| 59 | 💎 Tier 3 | ADR-035 docs 評価ポリシー (PR #107 T3-1) | **中核**。`review-simplicity.md` / `analyze-coderabbit.md` に docs-only criterion 拡張、code criteria の docs PR 誤適用を排除 | [todo5.md](todo5.md) |
| 31 | 💎 Tier 3 | review-security.md docs-only fast-approve から `.takt/**` と `.claude/**` を明示除外 (Bundle V) | 順位 59 と同領域。fast-approve の対象を**正しく狭める** precision 向上 | [todo3.md](todo3.md) |
| 32 | 💎 Tier 3 | docs/todo.md ヘッダ「新規タスクは追加しない」表記を実態整合 (Bundle V) | 単発 cleanup。CodeRabbit が誤指摘した根因 = 運用ルールの自己矛盾を解消 | [todo3.md](todo3.md) |

### 🛠 MEDIUM IMPACT — docs 品質 pre-write 保証

| 順位 | Tier | タスク概要 | 効果 | 作業詳細 |
|---|---|---|---|---|
| 4 | 🚀 Tier 1 | Stop hook の `pnpm lint:md` 統合 | Stop 時に markdown lint 自動実行 → post-write iteration 削減 (lint 違反で push 失敗 → 修正の 1 周回避) | [todo3.md](todo3.md) |
| 10 | 🔧 Tier 2 | broken-link-check + 内部アンカー検査統合 (ADR-032 PR-broken-link) | docs/ 内 link 健全性を CI で機械検出、anchor drift を pre-merge で catch | [todo2.md](todo2.md) |
| 29 | 🚀 Tier 1 | 非 docs ファイル `docs/todo` 参照検出 lint rule (Bundle U) | docs ⇄ code 間 reference lifecycle の決定論的防止層、永続成果物への ephemeral 参照を pre-write で block | [todo3.md](todo3.md) |

### 📋 LOW IMPACT — convention グローバル明文化

個別効果は薄いが、bundle 化すれば XS × 6 = 1 PR に集約可能。docs convention 整合性向上に寄与。

| 順位 | Tier | タスク概要 | 作業詳細 |
|---|---|---|---|
| 23 | 💎 Tier 3 | 日付ベース見出しアンカー更新ルールのグローバル明文化 | [todo2.md](todo2.md) |
| 24 | 💎 Tier 3 | jj conflict リカバリ手順のグローバル明文化 | [todo2.md](todo2.md) |
| 25 | 💎 Tier 3 | `__` prefix scratch file 規約のグローバル明文化 | [todo2.md](todo2.md) |
| 26 | 💎 Tier 3 | post-pr-monitor polling 禁止のグローバル明文化 | [todo2.md](todo2.md) |
| 30 | 💎 Tier 3 | Cross-File Reference Lifecycle ルールに具体例追記 (Bundle U) | [todo3.md](todo3.md) |
| 33 | 💎 Tier 3 | code-review.md に新 verdict 経路追加時の 3 点チェックリスト追記 (Bundle V) | [todo3.md](todo3.md) |

---

## 推奨 bundle

### Bundle "docs PR streamline" (最優先)

ユーザー目的「docs-only PR を独立して早急にマージできる環境」の最短達成パス。

| 含む順位 | 概要 | 工数 |
|---|---|---|
| 59 | 中核: ADR-035 docs 評価ポリシー | M |
| 31 | review-security.md fast-approve 精度向上 | XS |
| 32 | todo.md ヘッダ実態整合 | XS |

**合計工数**: M+ (1 セッション内完了可能)、すべて docs / facet instructions 編集で scope clean。

**期待効果**:

- pre-push-review が docs-only PR に対し fast-approve → review time 1 分 30 秒〜30 秒に短縮
- post-pr-review (analyze-coderabbit) が docs-only PR で false REJECT を出さない
- 順位 32 で運用ルールの自己矛盾を解消

**Bundle b との独立性**: Bundle b (順位 53/54/55, CR operation 安定化) は rate-limit 対応で別領域。並行進行可。

### Bundle "docs quality pre-write" (補完、本 bundle 後または並行)

| 含む順位 | 概要 | 工数 |
|---|---|---|
| 4 | Stop hook lint:md | XS |
| 29 | docs/todo 参照検出 lint | S |

write-time に docs 品質を保証する層。docs PR が push まで到達したら **すでに lint が通っている** 状態を作る。

### 後回し可 (low impact bundle)

順位 23-26, 30, 33 (XS × 6) を 1 PR に集約。convention 明文化で long-tail の品質改善。Bundle "docs PR streamline" 完了後の余裕で実施。

順位 10 (broken-link-check) は ADR-032 関連の他タスク (順位 6, 20, 21) との依存があり、独立着手は非効率。ADR-032 全体の進捗に追従する。

---

## 関連ドキュメント

- [docs/todo.md](todo.md) — 推奨実行順序サマリ表 (priority table)
- [docs/pipeline-token-efficiency.md](pipeline-token-efficiency.md) — pipeline efficiency 改善計画 (関連分野、本ドキュメントは docs PR 領域に特化)
- [ADR-019: CodeRabbit レビュー運用のハイブリッド構成](adr/adr-019-coderabbit-review-hybrid-policy.md) — CodeRabbit 運用根拠
- [ADR-027: Push-time review を simplicity に限定](adr/adr-027-push-review-simplicity-focus.md) — review scope の既存 design rationale
- ADR-035: docs 評価ポリシー (順位 59 で作成予定、本 bundle のメイン成果物)
