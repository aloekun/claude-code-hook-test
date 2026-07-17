# ADR-037: takt fix-trust shortcut — convergence_verdict による Iter 3 短絡

## ステータス

試験運用 (2026-05-04)

## コンテキスト

`post-pr-review` パイプラインで 3-iter outlier が発生する根因分析より、Iter 3 の analyze step が **本質的に冗長** であることが判明した:

> Iter 3 output 例 (PR #96 の auto-fix run、10m 19s):
> 「`.takt/review-comments.json` is a snapshot captured before fix iteration 1 ran; CodeRabbit has not re-reviewed yet, so the same 2 findings appear. The previous fix step report indicates both were addressed. **I verified each by reading the current source**...」

つまり Iter 3 の analyze は「前 iter の fix step が言う通りに本当に修正されているか」をソースを読んで再確認している作業に過ぎない。fix step を信頼すれば不要。

同様の構造は `pre-push-review` パイプラインの 6-iter outlier でも観測されており、reviewers→fix→reviewers の 2 cycle 後に supervise → fix_supervisor のエスカレーションが発生する。これも fix step の自己評価を信頼できれば短絡可能。

LLM の self-evaluation 信頼性は本質的に揺らぐが、**機械的に検証可能な指標** (Convergence gate metrics) を fix step に emit させ、その出力を後段の routing に直接使うことで信頼性を確保できる。

## 決定

`fix.md` instruction に **convergence_verdict** マーカー emit を必須化し、workflow yaml の routing rule に「fully_resolved → COMPLETE 直行」condition を追加する。

### convergence_verdict 仕様

`fix.md` の Convergence gate (既存) は以下 4 metrics を fix step に emit させる:

| Metric | Count |
|--------|-------|
| new (fixed in this iteration) | {N} |
| reopened (recurrence fixed) | {N} |
| persists (carried over, not addressed this iteration) | {N} |
| misdirected (suggestion pointed at a read-only zone, skipped) | {N} |

これに加えて、fix step は report 末尾に以下のいずれかを single bare line で emit する (REQUIRED):

- `convergence_verdict: fully_resolved` — `persists == 0` AND `misdirected == 0`、すべての findings が解決済み
- `convergence_verdict: partial` — `persists > 0` OR `misdirected > 0`、再 analyze が必要

### workflow yaml routing

`post-pr-review.yaml` + `pre-push-review.yaml` の fix step rules に以下の condition を追加:

```yaml
rules:
  - condition: |
      Report ends with `convergence_verdict: fully_resolved` (Convergence
      gate shows persists: 0 AND misdirected: 0). All findings of this
      iteration are fully resolved with no remaining work.
    next: COMPLETE
  - condition: |
      Fixes applied but `convergence_verdict: partial` (some findings
      persist or were misdirected, re-analysis needed).
    next: analyze   # post-pr-review の場合 / reviewers (pre-push-review の場合)
  - condition: Unable to proceed with fixes
    next: supervise
```

### Honesty constraint

`fix.md` の convergence_verdict セクションに以下を明記:

> **Honesty constraint**: This verdict gates whether the analyze step runs again. Reporting `fully_resolved` while leaving findings unaddressed bypasses the safety re-check. If you are uncertain whether a finding was truly resolved (e.g., you applied a fix but did not verify the build passes), emit `partial` so the analyze step can re-evaluate.

虚偽申告 (= 未修正で fully_resolved を emit) は安全網 bypass に直結するため、不確実な場合は `partial` を選ぶよう明示。

## 設計哲学

本決定は **「LLM が出した結果を後段で再検証しない」** 原則の応用:

- 従来: fix step → analyze step (= ソースを読んで再確認) → fix step → ...
- 改訂後: fix step → (verdict ベース routing) → COMPLETE / analyze step

LLM 同士で互いを再評価する loop は token と時間を消費するが、信頼境界 (= verdict) を明確にすれば中間ステップを構造的に削減できる。

ADR-036 の Bundle Z 3 層アーキテクチャと同じ思想:

- Bundle Z #B-γ (異常検知 reviewer): **下層が intercept した metric を上層は skip** (上層は下層を信頼)
- 本 ADR (fix-trust): **fix step が emit した verdict を analyze step は再確認しない** (analyze は fix を信頼)

## 影響

### Positive

- ✅ post-pr-review の 3-iter outlier 圧縮: 3-iter run → 2-iter (~3 分削減/run)
- ✅ pre-push-review の reviewers→fix loop 短縮: 全 finding が一発で fix された場合の 2 cycle 目を skip
- ✅ 設計原則の明文化: 「LLM 結果の信頼境界」を verdict で portable に表現

### Negative

- ⚠️ fix step の自己評価信頼性に依存 (虚偽 fully_resolved → 未修正 finding land リスク)
- ⚠️ 後続 supervise step でカバーされない場合は安全網が薄くなる

### Mitigations

- **Honesty constraint** で安全網 bypass リスクを fix step に明示
- 不確実な場合は `partial` を選ぶデフォルトを推奨
- dogfood で虚偽 fully_resolved が観測されたら順位 53 系列の post-merge-feedback follow-up (T1-1: convergence_verdict gate validator) を採用検討
- **(2026-07-03 追記) auto-push 前の決定論 gate による機械的 backstop**: PR #224 で「虚偽ではないが検証不足の `fully_resolved`」(`cargo test` のみで `#[ignore]` 統合テスト未実行) が回帰を素通しさせた実害を受け、cli-pr-monitor の auto-push 経路に決定論 gate (`src/cli-pr-monitor/src/stages/gate.rs`、push-runner-config.toml の quality_gate group を push 前に実行) を導入。誤った `fully_resolved` が emit されても、`cargo test -- --ignored` を含む gate が remote 到達前に遮断する (fail-closed、ADR-043)。fix.md 側にも `--ignored` 条件付き必須ゲートを追加済み
- **(2026-07-18 追記 / T12) honesty constraint の機械的 backstop を pre-push 経路にも拡張**: 上記 gate は post-pr (auto-push) 経路のみで、**pre-push (`pnpm push`) 経路には backstop が無かった**。`cli-push-runner` に post-takt re-gate stage を追加し (`src/cli-push-runner/src/stages/post_takt_regate.rs`、[ADR-058](adr-058-post-takt-regate.md))、takt fix が作業コピーを書き換えた場合に quality_gate を再実行して block する。両経路で「LLM の convergence_verdict 自己申告を決定論 gate で backstop する」構造が揃った。あわせて、backstop が両経路に揃ったことを前提に **fix.md の workspace 全体 build/test + `--ignored` 統合テストの自己申告義務を撤去し、影響 crate の `build -p` + `test -p` に縮小**した (自己検証を fix iteration ごとに払う冗長を解消し、重いスイートは gate で 1 度だけ払う)。上記「fix.md 側にも `--ignored` 条件付き必須ゲートを追加済み」は本追記で置換された (自己申告ではなく決定論 gate が担う)。fail 方向は gate 系 fail-closed で、ADR-021 原則 4 の repush 系 fail-safe とは逆向き (ADR-058 参照)

## 完了状態

- 完了 PR: #106 (Bundle Z Phase 3 と同梱)
- 影響範囲: `post-pr-review.yaml` / `pre-push-review.yaml` (fix step rules) + `fix.md` (Convergence verdict セクション追加)
- 派生プロジェクトへの展開: facet instructions は本リポジトリと派生プロジェクトで個別更新する運用 (ADR-036 と同じ)

## 関連 ADR

- **ADR-036**: Bundle Z 3 層アーキテクチャ (本 ADR と同じ「LLM 結果の信頼境界」原則を pre-push-review に適用)
- **ADR-008**: Push Pipeline ハーネスの実装 (workflow yaml 設計の前提)
- **ADR-015**: Push Pipeline を takt ベースの push-runner に移行 (workflow + facet 機構)
- **ADR-018**: cli-pr-monitor takt 化 (post-pr-review.yaml の起源)

## References

- 元セッション: PR #97 セッション (post-pr-review 3-iter outlier の root cause 分析)
- 完了 PR: #106 (Bundle Z Phase 3 + #C-2 同梱)
- (削除済) `docs/pipeline-token-efficiency.md` #C-2 セクション — 本 ADR が代替
