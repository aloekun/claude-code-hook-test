# ADR-069: PR chain 宣言規約 — 分割チェーンと missing-consumer 検査の両立

## ステータス

試験運用 (2026-08-03)

> 本 ADR は [ADR-056](adr-056-review-policy-anomaly-shadow.md)（pre-push review の anomaly policy 層）の精緻化と、PR 分割の切断点規約を 1 枚で記録する。[ADR-050](adr-050-iteration-aware-decision-criteria.md)（decision criteria の scope 明示）と同じ「判定基準に文脈を与える」パターンの適用である。

## コンテキスト

### 3 つのゲートの合成デッドロック

2026-08-02 の WP-17 PR 2a incident（[ADR-068](adr-068-fix-step-authority-boundary.md) コンテキスト参照）の根本原因は、個別には正しい 3 つの機構が合成で矛盾することだった:

1. **PR size gate** は 1500 行超の PR を block し、分割を強制する（warning 800 / block 1500）。
2. **Multi-PR chaining 規約**（git-workflow.md、1 PR 250〜800 行推奨）はチェーン分割を推奨する。
3. **simplicity review の missing-consumer 検査**（dead-on-arrival / premature abstraction）は「呼び手のいない抽象」を blocking とする。

内部レイヤリング（lib 抽出 + 呼び手、exe + 配線）を持つ大型機能では、**チェーンの先頭 PR は必ず「消費者がまだ存在しない何か」を導入する**。3 つを同時に満たす分割は存在せず、どう切っても先頭 PR が REJECT される。incident では、この REJECT への fix が PR を空洞化させた（fix 側の対策は ADR-068）。

### incident で実際に起きた宣言の欠落

PR 2a の diff 内の計画書は「workflow を既存 exe（`cli-autonomy-gate`）へ接続する」と読める記述のままで、code コメントが約束する新 exe（`cli-fix-push-gate`）をどこも名指ししていなかった。レビュアーは計画書まで突き合わせた上で「存在しないインフラを事実として主張している」と正しく判定した。**欠けていたのはレビュアーの寛容さではなく、チェーンの宣言**である。

## 決定 (試験運用)

### 1. PR chain 宣言規約

複数 PR に分割するチェーンでは、**先頭（および中間）PR の diff 自身に**、次を満たす宣言を含める:

| 要件 | 内容 |
|---|---|
| 置き場所 | **diff 内の計画文書のみ**（plan doc / `docs/todoN.md` エントリを同一 PR で更新する）。diff 外の計画文書は module doc が参照していても不可 — この PR でレビューされていない文書は stale や自己都合の事前記述でありえ、未レビューのファイルにレビューを緩和させることになる |
| 具体性 | 後続 PR と**抽出↔呼び手のペアリング**を具体名で書く（どの crate / exe / 関数を、どの後続変更が消費するか）。「将来使う」だけの宣言は無効 |
| 名前一致 | 宣言中の名前は diff 内の実名と一致していること。矛盾する宣言は降格根拠にならない（incident の形） |

分割時は「各 PR の diff 内文書がその PR の真実を語っているか」を再検証する。分割操作は diff の境界だけでなく**文書とコードの整合の境界**も動かす。

### 2. chain-aware review 降格

simplicity review は、上記の**有効な宣言がある項目に限り**、missing-consumer findings（dead-on-arrival code / premature interfaces・abstractions）を REJECT 根拠ではなく **non-blocking warning に降格**する。

- Warnings への記録は残す。後続 PR が land しないままチェーンが放置された場合、次のレビューが監査痕跡として辿れる。
- **fail-closed**: 宣言なし / ペアリング非具体 / 名前不一致 → 従来どおり blocking。未宣言の投機的抽象への検査は一切緩まない。
- 実装は `review-simplicity.md`（pre-push の blocking 経路）のみ。whole-tree variant（weekly review）は push を block しないため対象外。

### 3. 切断点ヒューリスティクス

PR size gate に当たった際の分割判断:

1. **抽出と最初の呼び手の間で切らない**。ADR-044 層 1 の正当化（呼び手の存在）が diff から消え、本 ADR の宣言でしか救えなくなる。呼び手と同じ PR に抽出を入れれば宣言は不要で、判定も自明になる。
2. 切断点は**関心の境界**（機能 vs 配線、実装 vs docs バッチ）に置く。
3. **良い関節が無ければ `PR_SIZE_CHECK_OVERRIDE=1` + 理由の明記が正当**。size gate の override は「大型 refactoring 等で意図的な場合」のために存在する正規の経路であり、悪い関節で切った分割はチェーン全体のコスト（レビュー回数・宣言管理・矛盾リスク）で上限超過 1 回分を上回り得る。incident の初回分割（1613 行 = block の 8% 超過を 2 分割し、抽出と呼び手を分離）はこの判断を誤った実例。

### 4. fix suggestion の記述規約（ADR-068 残課題の引き取り）

複数の remedy がありうる finding では、**最も破壊的でない処置を Fix Suggestion の先頭に書く**。fix step は先頭候補に従う傾向があり、incident では最破壊処置（"defer the extraction" = revert）が先頭だったことが gut-revert の一因になった。処置の実行側の防御は ADR-068（最小処置原則 + backstop）が持ち、本規約は発生源側の手当てである。

## 試験運用判断基準

instruction / 規約層のみの変更のため config opt-in は無い（kill-switch は instruction の revert）。次を観測して本採用 / 改訂を判断する:

- **decision trigger**: 宣言付き chain PR が 3〜5 本流れた時点で、(a) 有効な宣言を持つ先頭 PR が missing-consumer で REJECT されないこと、(b) 未宣言の投機的抽象が引き続き REJECT されること、(c) 名前不一致が blocking のままであること、(d) 宣言が欠落・非具体（「将来使う」レベル）のケースが blocking のままであること、を確認する（(b)〜(d) で fail-closed 3 条件の全てを検証対象にする）。
- **期限**: 2026-11-03 までに判定材料が集まらなければ、chain 分割の発生頻度に照らして延長 / 却下を決める。
- 直近の検証機会: WP-17 の再分割チェーン（2a / 2b / 2c）が最初の宣言付き chain になる。

## 帰結

### 利点

- size gate・chaining 規約・missing-consumer 検査が両立可能になり、「分割したら REJECT される」構造矛盾が解消する。
- 宣言は要求が具体的（ペアリング + 名前一致）なため、「後で使うつもり」と書くだけの逃げ道にはならない。
- レビュアーが実際に diff 内計画書を読むことは incident で実証済みで、宣言の検証コストは既存のレビュー動作に乗る。

### 欠点 / 留意点

- 宣言の維持コスト: チェーン構成が変わったら宣言も更新が要る。stale な宣言は名前不一致で fail-closed に倒れる（安全側だが手戻り）。
- 降格は LLM instruction 層であり決定論的ではない。降格の誤適用（無効な宣言を有効と誤読）が起きた場合、**投機的抽象が blocking レビューを受けずに land するリスクは残る** — [ADR-068](adr-068-fix-step-authority-boundary.md) backstop が守るのは「fix step による PR の後退」だけ、quality gate が守るのは「ビルド・テストの成立」だけで、どちらも未消費抽象の設計妥当性は検証しない。残る防御は Warnings 記録の監査痕跡（後続 PR が land しないまま放置されたチェーンを次のレビューが辿れる）に限られる。
- 中間 PR（呼び手はあるが自分も次への供給を含む）は宣言を両方向に書く必要がある。

### 残課題

- 宣言の機械検証（宣言中の名前が diff の実名と一致するかの決定論チェック）は未実装。降格の誤適用が観測されたら検討する。

## 関連

- [ADR-068](adr-068-fix-step-authority-boundary.md) — fix step の権限境界。incident の fix 側対策。fix suggestion 記述規約（決定 4）の残課題元
- [ADR-056](adr-056-review-policy-anomaly-shadow.md) — pre-push review policy 層。本 ADR はその anomaly 判定に文脈（chain 宣言）を与える
- [ADR-050](adr-050-iteration-aware-decision-criteria.md) — decision criteria の scope 明示パターンの先行例
- [ADR-044](adr-044-subprocess-utility-extraction-boundary.md) — lib 抽出の境界基準。切断点ヒューリスティクス 1 の根拠
- [ADR-035](adr-035-doc-evaluation-policy.md) — docs-only 判定。YAGNI 検査の scope 境界の先行定義
- 開発 convention: [dev-conventions.md](../dev-conventions.md) § PR chain の分割と宣言
