# ADR-068: pre-push fix step の権限境界 — 後退検知 backstop と設計級 remedy の human routing

## ステータス

試験運用 (2026-08-03)

> 本 ADR は [ADR-058](adr-058-post-takt-regate.md)（post-takt re-gate）の検証範囲を「ツリーの品質」から「PR の意図の保存」へ拡張し、[ADR-048](adr-048-facet-findings-handoff-markdown-contract.md) の fix facet に処置深度の原則を追加する。[ADR-039](adr-039-experimental-feature-standard-pattern.md) の試験運用標準パターンに従う。

## コンテキスト

### incident (2026-08-02、WP-17 PR 2a)

pre-push review の simplicity facet が REJECT（premature abstraction — lib 抽出 2 crate の正当化根拠である 2 つ目の呼び手が diff にもリポジトリにも存在しない）を出した。指摘自体は証拠に基づく正当なものだった。問題はその後の連鎖である:

1. **iteration 2**: fix step はコメントを未来形へ言い換える最小処置を選んだ。
2. **iteration 3**（再 push 時）: 同一 finding family に対し、fix step は**レビュー済み・テスト済みの lib crate 2 つを丸ごと削除**する処置を選んだ（finding の fix suggestion 第 1 候補 "defer the extraction" の実行）。
3. **post-takt re-gate は全 PASS**。変更をほぼ master に戻す revert は品質ゲートを自明に通過する。
4. **push は「成功」として完走**した。PR の実体は当初意図の 3 分の 1 以下に空洞化していたが、パイプラインのどの層もそれを検知しなかった。

捕捉したのは driver（人間の監督下のセッション）の実測検証だけで、ハーネスは沈黙した。

### 3 つの構造的欠陥

1. **re-gate は意図を見ない**: [ADR-058](adr-058-post-takt-regate.md) の re-gate は「fix 後のツリーが品質ゲートを通るか」を検証する。「fix が改善したのか、PR を消したのか」は区別できない。
2. **pre-push の fix step に決定論的な scope 制約が無い**: [ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) の scope guard（fix diff の allowlist 照合）は post-pr 経路のみに実装されており、pre-push は instruction（助言層）だけだった。しかも fix.md は「決定論 gate が再照合する」と**両経路で真であるかのように記述**しており、存在しない防御を前提にしていた。incident では finding の Location は doc コメント 4 箇所だったが、fix は crate 削除・workspace member 変更・依存除去まで行い、allowlist instruction は守られなかった。
3. **処置深度の選択が fix step の裁量**: 同一 finding に対して最小処置（コメント修正）と最大破壊処置（600 行削除）のどちらを選ぶかが非決定的で、後者を選んでも止める層が無かった。

### なぜ「PR の主変更の取り消し」は fix ではないのか

レビュー指摘への機械的修正（typo、境界条件、lint 違反）と、「この抽出はやめるべき」への対応は質が異なる。後者は **PR のスコープ判断**であり、採るなら履歴の組み直し・PR 分割の再設計・計画書の更新を伴う driver の意思決定である。fix step がこれを単独実行すると、(a) レビュー不能な add-then-revert 履歴が残り、(b) push 成功シグナルが「PR は健在」と偽る。[ADR-022](adr-022-automation-responsibility-separation.md) の責務分離（自動化コンポーネントは user-supplied な意図を書き換えない）の適用対象である。

## 決定 (試験運用)

### 1. fix 後退検知 backstop（決定論層）

`cli-push-runner` の post_takt_regate stage に、takt 前後の PR diff snapshot（ADR-058 が既に保持している材料）の比較による**後退検知**を追加する。

| 検査 | 判定 |
|---|---|
| ファイル脱落 | takt 前の diff に含まれるファイルが後の diff から消えた → **件数によらず block** |
| 追加行の大幅削減 | PR の追加行数（`+` 行）の削減率が閾値（`max_added_line_shrink_pct`、default 50%）を**超えた** → block |

block 時は `[FIX_REGRESSION]` マーカーと対処手順（fix の取り消し / 明示 bypass）を loud 出力し、quality gate は実行せず push を中断する。telemetry verdict は `fix_regression_block`（[ADR-055](adr-055-firing-telemetry-collection.md) 経由で発火実績を観測可能）。

**閾値の方向**: 誤 block は「人間確認 1 回 + 明示 bypass」で回復できるが、取り逃しは空洞化した PR の push に直結する。よって閾値は誤検知側に寄せる（incident の削減率は約 80% で、default 50% は大差で捕捉する）。

**kill-switch**: env `POST_TAKT_REGRESSION_DISABLE=1` で後退検知のみを bypass する。re-gate 本体の `POST_TAKT_REGATE_DISABLE` とは独立 — 「レビュアーが PR の主変更の取り消しを求め、人間がそれを妥当と判断した」正当ケースで、品質再検証は残したまま後退だけを許すため。

**fail 方向の注記**: pre diff から `diff --git` ヘッダが 1 つも取れない場合、ファイル脱落検査は skip し追加行検査のみ行う。diff 形式は config 管理であり fix step には操作できないため、これは fail-open ではなく検査材料の構造的欠如である（--git 形式自体は coverage 検査が別途強制している）。ヘッダ解釈は coverage 検査と同一実装（`parse_git_diff_paths`）を共有し、書式変化への沈黙が片側だけ起きることを防ぐ。

### 2. 設計級 remedy の human routing（instruction 層）

fix facet instruction に 2 原則を追加する:

- **最小処置原則**: finding を解消する最も破壊的でない処置を選ぶ。コメントの正確性への指摘はコメント修正で解消する。同一 `family_tag` への処置深度を iteration 間で独断エスカレートしない。
- **設計級 remedy の escalation**: PR の主追加の revert / 削除でしか解消できない finding は修正せず `### Design-level remedy (escalated)` で報告し、driver に委ねる。

決定論的な保証は決定 1 の backstop が担い、instruction はその手前で無駄 iteration を減らす助言層（[ADR-042](adr-042-rule-vs-mechanism-boundary.md) の役割分担）。**[ADR-048](adr-048-facet-findings-handoff-markdown-contract.md) の contract への remedy 区分列の追加は見送る** — canonical 列は全 facet 統一義務があり波及が大きい。instruction + backstop で不足が観測されたら再検討する（YAGNI）。

### 3. fix.md の経路別防御の明記（誤記述の訂正）

fix.md の「決定論 scope guard が fix diff を再照合する」という記述を経路別に訂正した: post-pr = ADR-054 scope guard / pre-push = 本 ADR の後退検知。**フル scope guard（finding 由来 allowlist の照合）の pre-push 展開は todo 順位 364** として登録し、ADR-054 が予告していた展開（同 ADR § 欠点）の実装先を確定する。後退検知は 80/20 の暫定であり、allowlist 照合が入れば「finding 対象外ファイルへの一切の書き込み」まで検知範囲が広がる。

## ADR-039 3 点セットの適用

| 項目 | 内容 |
|---|---|
| **Config opt-in** | 後退検知は `[post_takt_regate] enabled = true`（既存 opt-in）に相乗りする。re-gate が OFF なら後退検知も動かない（stage 自体が skip されるため）。閾値は `max_added_line_shrink_pct` で調整可 |
| **Kill-switch** | env `POST_TAKT_REGRESSION_DISABLE=1`（後退検知のみ）/ `POST_TAKT_REGATE_DISABLE=1`（stage 全体）。恒久停止は `enabled = false` |
| **Bounded lifetime** | decision trigger: **fix が作業コピーを変更した push 3〜5 回で、(a) 正当な fix を誤 block しないこと、(b) telemetry の `fix_regression_block` 発火が実際の後退と一致すること、を確認して本採用 / 閾値改訂 / 却下を判断**する。**2026-11-03 までに判定材料が集まらなければ、fix step の発生頻度に照らして延長 / 却下を決める**。trigger の永続記録は本 ADR + `push-runner-config.toml` の section コメントの 2 箇所 |

## 検証記録

- unit test 291 件 pass（新規 9 件: incident 再現 = **gate が PASS しても後退検知が block する**、全面 revert、閾値境界 50/51%、小規模削減は通す、検知 OFF フォールバック、非 --git 形式、config default / カスタム値）。
- incident 再現テスト `regate_blocks_regression_even_when_gate_would_pass` が本 ADR の中核契約（品質ゲート通過と意図保存は別物）を machine-enforce する。

## 帰結

### 利点

- 「fix が PR を空洞化させたまま push が成功する」経路が決定論的に閉じた。捕捉が driver の注意力に依存しない。
- 後退検知の材料は ADR-058 の既存 snapshot で、追加の jj 呼び出しはなく、snapshot の解析・比較コストのみ発生する（数百 KB の文字列走査で、quality gate 再実行の 60 秒超に対し無視できる規模）。
- 誤 block からの回復経路（明示 bypass）が段階化されており、review の検知強度は一切落とさない。

### 欠点 / 留意点

- **後退検知は allowlist 照合ではない**。finding 対象外ファイルへの「追加・書き換え」（削除を伴わない injection）は検知できない。それはフル scope guard（todo 順位 364）の領分。
- **正当な大規模削除 fix は明示 bypass が必要**になる。運用上は「REJECT → fix が escalation 報告 → driver が判断」の流れが先に走るため、bypass が要るのは driver が revert を承認した場合のみ。
- 閾値 50% は初期値であり、bounded lifetime の観測で改訂しうる。

### 残課題

- フル scope guard（finding 由来 allowlist）の pre-push 展開（todo 順位 364）。
- reviewer の fix suggestion の並び順が処置選択に影響する問題（incident では第 1 候補が最破壊処置だった）。chain-aware review の ADR（PR chain 宣言規約）側で fix suggestion の記述規約とあわせて扱う。

## 関連

- [ADR-058](adr-058-post-takt-regate.md) — post-takt re-gate。本 ADR はその検証範囲の拡張で、snapshot 材料を共有する
- [ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) — scope guard。予告されていた pre-push 展開の実装先を todo 順位 364 に確定
- [ADR-048](adr-048-facet-findings-handoff-markdown-contract.md) — findings contract。remedy 区分列の追加は YAGNI で見送り
- [ADR-042](adr-042-rule-vs-mechanism-boundary.md) — ルール vs 仕組み化。backstop（仕組み）と instruction（ルール）の役割分担の根拠
- [ADR-043](adr-043-security-gates-fail-closed.md) — fail-closed。閾値を誤検知側へ寄せる方向の根拠
- [ADR-022](adr-022-automation-responsibility-separation.md) — 責務分離。「revert はスコープ判断 = driver の専権」の根拠
- [ADR-055](adr-055-firing-telemetry-collection.md) — telemetry。`fix_regression_block` の観測基盤
- incident: 2026-08-02 WP-17 PR 2a の push run（simplicity REJECT → fix が lib 抽出 2 crate を削除 → gate 全 PASS で push 成功）
