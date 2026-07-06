# 開発 convention / チェックリスト

> CLAUDE.md (ADR index) から分離した運用 convention・チェックリスト集。index の肥大化を避けつつ、セッション横断で参照する軽量ガイドを集約する (ADR-022 の責務分離)。

## spike / 実験タスクの見送り (negative result) 永続化 convention (順位261)

spike・実験タスクを見送る (採用しない) と判断したときは、negative result の知見が散逸しないよう以下の **3 点セット** を必ず実施する:

1. **ADR に結論と実測根拠を記録** — 見送り判断・数値根拠・比較対象を該当 ADR (新規 or amendment) に永続化する。「なぜ見送ったか」を後続セッションが再構築できる粒度で書く。
2. **計画文書の状態列を更新** — 該当タスクの計画文書 (例: `docs/harness-improvement-plan.md` 等の ephemeral 計画) の状態を「見送り / 却下」に更新し、宙吊りの検討を残さない。
3. **再評価トリガー付き follow-up を Tier 5 todo 化** — 「どういう条件が変われば再評価するか」(新モデル出現 / プロンプト改善 / GPU 更新 等) を明示した follow-up を Tier 5 (⏳) todo として登録する。恒久見送りではなく「現時点では見送り」を表現する。

**確立事例** (2 例で成立):

- WP-01 (ローカル LLM pre-push レビュアー選定) → [ADR-046](adr/adr-046-local-llm-review-spike.md) で却下記録 + follow-up を順位 255 に todo 化
- WP-04 (classifier モデル格上げ) → [ADR-038](adr/adr-038-local-llm-finding-classification.md) § classify モデル格上げの評価と見送り で amendment 記録 + follow-up を順位 256 に todo 化

3 例目以降の spike 見送りも本 convention を参照して同型に処理する。

## 外部 SaaS 無料枠 / 制限の調査チェックリスト (順位262)

外部サービス (CodeRabbit / LLM API / CI/CD provider 等) の無料枠・制限を調査するときは、「free tier」の一語で判断せず、以下の **各次元を個別に確認** する。単一の緩和 (例:「public リポは Pro 機能無償」) を「全制限撤廃」と誤解しないため:

1. **月間上限** — 月あたりの総回数 / 総量の上限。
2. **時間単位 rate limit** — 1 時間 / 1 分あたりの上限。月間上限とは **別次元** で、月間に余裕があっても時間単位で先に当たることがある。
3. **適用単位** — per-user / per-org / per-repo のどれで計量・課金されるか。fork / 別アカウント運用で分離できるかにも関わる。
4. **plan tier による差** — free / pro / enterprise で緩和される制限の種類。
5. **public リポ特典の適用範囲** — public リポで無償化される「機能」と、緩和されない「rate limit」を区別する。

**由来** (WP-03、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § CodeRabbit クォータ設計): CodeRabbit の「public リポ向け Pro 機能無償提供」を「rate limit 撤廃」と誤解しかけたが、実際には月間上限と時間単位 rate limit は別次元で、時間単位上限 (3〜4 回 / 時) は残存していた (2026-07-04 ユーザー確認)。この誤解は LLM API・CI 等の他 SaaS 統合でも再発しうる汎用パターン。
