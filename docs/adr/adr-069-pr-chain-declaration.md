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

### 実測 1: PR #351（WP-17 2b、2026-08-03） — trigger (a) 充足

宣言付き先頭 PR が missing-consumer で REJECT されないこと（decision trigger (a)）を実測した。

- **対象**: 2b が `cli-fix-push-gate` を導入し、その **workflow 呼び手が後続 2c にしか存在しない**構成。lib 2 件（`lib-scope-guard` / `lib-autonomy-policy`）は呼び手 2 件が 2b の diff 内に揃うため、未消費は workflow 呼び手 1 つだけ。
- **結果**: pre-push の simplicity review は **APPROVE**。宣言を読んだうえで 決定 1 の 3 要件を個別に照合し（diff 内の計画書である / 後続 PR と step 名を具体名で指名している / 引数 4 種が `main.rs` の `parse_args` と一致する）、missing-consumer を **non-blocking warning へ降格**した。降格は設計どおり Warnings に記録され、監査痕跡が残った。
- **示唆**: レビュアーは宣言を字面で受け取らず、**diff 内の実装と突き合わせて**有効性を判定した。決定 1 の「名前一致」要件が実際に検査されることを確認できた。

### 実測 2: 同 PR — 宣言の「検証可能性」という欠落要件

上記と同じ PR で、宣言の記述をめぐり CodeRabbit → fix step → 手動訂正の往復が発生した。決定 1 の 3 要件では捉えられない問題が露出したため記録する。

- 当初の宣言は「step 名・パス・引数を **2c の実体で照合済み**」と書いた。これは**事実**（照合はローカルの未 land コミットに対して実施済み）だが、**PR の diff だけを見るレビュアーには検証できない主張**だった。CodeRabbit はこれを指摘した（妥当）。
- post-PR の fix step はこれを「実装済みの実体と照合済みではない」へ書き換えて auto-push した。これは**事実に反する**。結果として「偽だが検証可能」な記述に置き換わった。
- 最終的に、主張を **diff 内で照合できるもの**（引数 4 種・exe 名）と **diff 外の主張**（step 名・exe パス）に分け、後者は「照合済みだが本 PR の diff だけでは検証できない」と明示する形へ手動訂正した。
- **決定 1 への含意**: 「名前一致」要件は暗黙に *diff 内で照合できること* を前提としている。後続 PR の実体（未 land / 別 PR）にしか存在しない名前を宣言に含める場合、**その部分がレビュアーにとって検証不能である旨を宣言自身が明示する**のが正しい形である。本 ADR の本採用時に決定 1 の要件表へ反映するか判断する（現時点では実測 1 件のため要件化しない）。
- この往復は「fix step の出力を実測検証する」（[ADR-068](adr-068-fix-step-authority-boundary.md)）が **docs 領域でも必要**であることの実例でもある。詳細は ADR-068 § 検証記録を参照。

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
