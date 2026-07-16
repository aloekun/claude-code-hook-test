# ADR-047: pre-push review の反証 (refute) facet

## ステータス

試験運用 (2026-07-06) / **dogfood 中 (2026-07-17 開始、判定期限 2026-07-31)**

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。

## コンテキスト

`pre-push-review` パイプラインは `reviewers (simplicity + security、sonnet) → fix → supervise` のフローで動く。reviewer が出す finding には **false positive が混じり**、fix step がそれを真に受けて修正しようとすることで **無駄な fix iteration** が発生する。過去に 6-iter / 17-18 分の outlier を 2 回観測しており ([ADR-036](adr-036-bundle-z-three-layer-review.md) コンテキスト)、その根因の 1 つが reviewer の検出精度のばらつきであった。

[ADR-036](adr-036-bundle-z-three-layer-review.md) の Bundle Z は「決定論層 → 制約付き修正 → 異常検知 reviewer」の 3 層で review 品質を構造化したが、**reviewer が出した finding の真偽を fix 前に検証する層**は持たない。fix は与えられた finding を無条件に修正対象とする。

WP-06 (`docs/harness-improvement-plan.md`) は、reviewers と fix の間に **verify (refute) step** を挟み、finding を **反証 (adversarial verification)** する。

## 決定

`pre-push-review-refute` workflow を新設し、reviewers と fix の間に verify step を挟む:

```text
reviewers (simplicity + security, sonnet)
   |  any needs_fix
   v
verify (refute, haiku)  <- 各 finding を反証。false positive を reject
   |
   +- SOME_SURVIVE -> fix (survived finding のみ)
   +- ALL_REFUTED  -> supervise (コード変更なし、push 前最終確認)
```

### verify (refute) step の設計

- **モデル**: haiku (安価)。sonnet reviewer の finding を安く検証する非対称構成。
- **入力**: Report Directory の `simplicity-review.md` / `security-review.md` をパス Read。
- **反証手順**: 各 blocking finding について対象コードを実際に Read し、(a) 指摘が再現するか、(b) 前提が成立するか を確認。再現しない・前提が誤っている・**確信が持てない** finding は reject。
- **バイアス**: 不確実なら reject に倒す。理由は非対称なコスト — 誤って survive させた finding は fix→reviewers の 1 cycle を無駄にする (pre-push で高コスト) が、誤って reject した finding は **post-pr の CodeRabbit 層で回収される** (安全網、[ADR-019](adr-019-coderabbit-review-hybrid-policy.md))。
- **出力**: `refutation-report.md` (survived findings = fix 対象 / rejected findings = 却下・監査ログ / verdict)。コード編集はしない (edit: false)。

### verify 全却下時は supervise 経由

reviewer 指摘を verify が全て反証した (ALL_REFUTED) 場合、fix は skip するが **COMPLETE には直行せず supervise を通す**。reviewer(sonnet) と verify(haiku) の判断が食い違う状態のため、supervise (sonnet) が rejection reason の妥当性を確認してから push 可否を判断する (安全側)。

### fix / supervise facet の後方互換 (ADR-020)

`fix.md` / `supervise.md` は pre-push / post-pr で共有 ([ADR-020](adr-020-takt-facets-sharing.md))。両者に「Report Directory に `refutation-report.md` が**存在する場合のみ**参照する」追記を行った。存在しない場合 (post-pr-review / refute 無効時) は従来通り動作する = 入力ソース非依存の原則を維持。

### 膠着検出 (loop_monitor) の cycle 更新

takt の cycle-detector は連続一致で膠着を検出する。verify を挟むと実 history は `reviewers → verify → fix` の反復になるため、`loop_monitors.cycle` を `[reviewers, fix]` から **`[reviewers, verify, fix]`** に更新した。これを怠ると連続一致が途切れて膠着検出 judge が発火せず、`max_steps` まで空回りする (takt cycle-detector の連続一致仕様に由来)。

## ADR-039 3 点セット

### Config opt-in (default OFF)

`push-runner-config.toml` の `[pre_push_review]` section:

```toml
[pre_push_review]
refute_enabled = true                        # 本リポジトリは 2026-07-17 に dogfood 開始 (push T4)
refute_workflow = "pre-push-review-refute"
```

`refute_enabled = true` かつ `refute_workflow` 指定時のみ verify 入り workflow に切り替わる。section 不在 / `refute_enabled` 未設定 / `false` では現行 `pre-push-review` (verify なし) を使う。切替判定は cli-push-runner の `resolve_takt_workflow` に単一集約 (ADR-039 §設計6点 #5)。派生プロジェクト (`templates/push-runner-config.toml`) は `refute_enabled = false` で default OFF を継承。

導入 PR は `refute_enabled = false` (OFF) とし、本リポジトリの dogfood 開始 (`refute_enabled = true`) は refute workflow が master で検証された後の別 PR で行う方針を採った。これにより、未検証の refute workflow でこの導入 PR 自体を自己レビューするブートストラップを回避した。**dogfood は 2026-07-17 に開始済み** (`docs/push-pipeline-fix-plan.md` の push T4)。

### Kill-switch

| 起動経路 | 停止コマンド | 影響範囲 |
|---|---|---|
| `refute_enabled = true` | `refute_enabled = false` (or section 削除) | 即座に現行 `pre-push-review` (verify なし) へ。verify step は完全 skip |
| — | 永続停止: `.takt/workflows/pre-push-review-refute.yaml` を削除する revert PR | refute facet 群を撤去 |

### Bounded lifetime

**dogfood 有効化 (`refute_enabled = true`) から 2 週間**を decision trigger とする。
**開始 2026-07-17 → 判定期限 2026-07-31**。判定基準:

- **採用**: fix iteration 数がベースライン (`pre-push-review` run) 比で減少、**かつ** reject 誤り (reject した finding が後の CodeRabbit で再指摘された数) が CodeRabbit 層で回収されている (安全網の実証)。採用時は `pre-push-review.yaml` へ verify を統合し refute variant を退役。
- **却下**: fix iteration が減らない / reject 誤りが多く安全網でも回収しきれない。`pre-push-review-refute.yaml` を削除する revert PR + 本 ADR を「却下」に更新。
- 判定結果は本 ADR + `push-runner-config.toml [pre_push_review]` コメントに反映。

### dogfood 計測項目

- **refute run の特定方法**: takt の run ディレクトリ名は **workflow 名ではなく task 名**から作られる (`runSlug` = `<UTC timestamp>-pre-push-review`)。したがって `.takt/runs/*-pre-push-review-refute/` は **1 件もマッチしない** (2026-07-17 の dogfood 初回 push で実測確認。当初この glob を計測手順に書いていたが誤りだった)。refute run は `meta.json` の `piece` フィールドで識別する:

  ```sh
  grep -l '"piece": "pre-push-review-refute"' .takt/runs/*/meta.json
  ```

  `trace.md` 冒頭の `# Execution Trace: pre-push-review-refute` でも同定できる。なお runSlug の timestamp は **UTC** (例: JST 2026-07-17 03:25 の run → `20260716-182505-...`) のため、JST の日付でディレクトリ名を絞らないこと。
- **fix iteration 数**: 上記で特定した run の `trace.md` の reviewers↔fix cycle 数。
- **reject 率 / reject 誤り率**: `refutation-report.md` の rejected findings を監査ログとし、後続 PR の CodeRabbit 再指摘と照合。

## 帰結

### 利点

- reviewer の false positive を fix 前に弾き、無駄な fix iteration を削減。
- 非対称モデル構成 (sonnet reviewer → haiku verify) で安価に検証。
- 誤 reject は CodeRabbit 層で回収される多層防御。

### 欠点 / 留意点

- verify(haiku) が真の finding を誤 reject するリスク。安全網 (CodeRabbit) 前提のため、CodeRabbit を無効化した運用とは併用しないこと。
- workflow が 2 系統 (`pre-push-review.yaml` + `pre-push-review-refute.yaml`) になり、dogfood 期間中は reviewers / fix / supervise step が重複する。採用時に統合、却下時に refute variant 削除で解消する。
- verify → supervise → fix_supervisor 経路のコストは、全却下時にも supervise 1 回分増える (安全側のトレードオフ)。

### ADR-036 / ADR-037 との関係

- **[ADR-036](adr-036-bundle-z-three-layer-review.md) の第 4 層**: 決定論層 / 制約付き修正 / 異常検知 reviewer に続く「finding 反証層」。上層が下層を信頼する設計思想を踏襲 (fix は verify が通した finding のみを信頼)。
- **[ADR-037](adr-037-takt-fix-trust-shortcut.md) の補完**: ADR-037 が *fix 後の* 再 review を verdict で省くのに対し、本 ADR は *fix 前の* false positive を反証で弾く。両者とも「LLM 結果の信頼境界を明示して無駄な iteration を削る」原則の応用。

## 関連 ADR

- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン (本 ADR の trigger 事例)
- [ADR-036](adr-036-bundle-z-three-layer-review.md) — Bundle Z 3 層 review (本 ADR は第 4 層)
- [ADR-037](adr-037-takt-fix-trust-shortcut.md) — fix-trust shortcut (本 ADR と同じ信頼境界原則)
- [ADR-020](adr-020-takt-facets-sharing.md) — facet 共有 (fix / supervise の後方互換追記)
- [ADR-019](adr-019-coderabbit-review-hybrid-policy.md) — CodeRabbit ハイブリッド (誤 reject の安全網)
