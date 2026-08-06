# ADR-071: 未マージ draft PR 数による背圧 — draft-pr クラスの自主減速

## ステータス

試験運用 (2026-08-06)

> 本 ADR は [ADR-039](adr-039-experimental-feature-standard-pattern.md) の 3 点セット (config opt-in / kill-switch / bounded lifetime) に従う。全体 kill-switch は [ADR-066](adr-066-autonomy-global-kill-switch.md) が持ち、本 ADR はその上に載る**自主減速**の層である。

## コンテキスト

[ADR-052](adr-052-autonomy-execution-boundary-classes.md) 原則 5 は、自動実行可クラス (特に draft PR 作成) を**背圧なしで有効化することをアンチパターンとして明示的に禁止**している。同原則の契約表は「背圧 (未マージ draft 数の監視) または kill-switch が未接続・読み取り不能なら、自動実行可クラスを無効化しゲート必須へ倒す」と定める。

この契約は [ADR-066](adr-066-autonomy-global-kill-switch.md) の実装で**構造的な deny** として具体化されていた — `lib-autonomy-policy` の `Operation::backpressure_connected()` が `DraftPr => false` を固定で返し、kill-switch の 2 面がどちらも有効でも `draft-pr` は通らない。これは未実装の placeholder ではなく、「背圧より先に draft PR 自動作成が有効化される順序事故」を型で塞ぐ意図的な状態だった。

夜間 todo 消化ループ (計画書 WP-18) は draft PR 作成を主たる成果物とするため、この構造的 deny を解除しない限り成立しない。本 ADR は解除の条件 = 背圧の実装を定める。

### なぜ「無料だから積み放題」ではないのか

実行コスト面では制約が緩い。にもかかわらず背圧が要るのは、**律速がマシン資源ではなく人間のレビュー帯域と Max 枠**だからである (§ 外部 SaaS の課金・上限事実)。未マージ draft が積み上がると:

- レビュー待ち行列が人間の処理能力を超え、採用率 (= マージされた割合) が下がる。採用されない draft を作る run は Max 枠を消費するだけになる。
- 同一台帳から次のタスクを選ぶループが、前夜の draft と重複・衝突する変更を作りやすくなる。
- 「止め方」が全体 kill-switch しかないと、減速したいだけの場面で全自律動作を止めることになる。

### 当初案からの格上げ

計画書の WP-19 ステップ 2 の当初案は「routine プロンプト冒頭の自己抑制判定」だった。これは [ADR-028](adr-028-pnpm-create-pr-gate.md) が指摘した「Claude が守る意志に依存する soft 防衛」そのものであり、決定論層 (`cli-autonomy-gate` の入力) へ格上げして実装する。

## 決定 (試験運用)

### 1. 背圧の指標は「未マージ draft PR 数 (`claude/` prefix)」

自律 actor が作る draft PR は `claude/` prefix ブランチに限られる ([ADR-067](adr-067-phase-b-unattended-fix-push.md) の target 軸、ruleset 除外とも整合)。この prefix の **open かつ draft** な PR 件数を背圧の指標とする。

「レビュー待ち行列の長さ」を直接数える指標であり、run 回数や経過時間のような代理指標より、止めたい事象 (人間が捌けない量の未処理成果物) に近い。

### 2. 背圧の状態は `GateInputs` だけが持つ

`Operation` は「どの指標を要求するか」だけを持ち (`requires_draft_backpressure()`)、指標の実測値・閾値は `GateInputs` が持つ。

状態を両方に持たせない。`backpressure_connected()` のような enum 側の状態と入力側のフィールドが併存すると、片方だけを更新した瞬間に判定経路が二股に分かれ、「テストは通るが実運用では別の枝を通る」形の drift を生む。**背圧の真偽を導出する場所は `evaluate()` の 1 箇所**に限る。

`FixPush` が draft 数を要求しないのは背圧が無いからではない。fix push の背圧は cli-pr-monitor の有界 retry (`max_retries`) が担っており、**呼び出しごとに gate へ渡す状態を持たない**ため判定入力に現れない。この非対称は doc コメントとテストの両方で固定する。

### 3. 閾値は `autonomy-config.toml` の `[autonomy] max_open_draft_prs`、判定は `>=`

kill-switch フラグ (`enabled`) と同じファイル・同じ信頼境界 (CI からは master ref の写しを読む) に置く。閾値だけを Actions variable 側へ分けると、リポジトリ内 config と CI 側に自律制御の状態が分散し、[ADR-051](adr-051-cross-system-config-coupling.md) が扱ったクロスシステム drift を招く。

- **`>` ではなく `>=`**。閾値は「これ以上は積まない」上限。`max_open_draft_prs = 3` なら 3 件目が未マージの間は新規作成しない。
- **`0` は draft-pr クラスだけの停止**。fix push は本値の影響を受けないため、全体 kill-switch を倒さずに夜間ループだけを止める操作点になる。
- **キー欠落は既定値へ倒さない**。書き忘れが「勝手に 3 件まで作る」という fail-open にならないよう、欠落は背圧未接続 = deny とする。

toml の parse はファイル単位なので、本キーが型違い (文字列 / 負値 / 小数) だと `enabled` も含めて全フィールドが読めなくなり自律動作が全停止する。config が半壊した状態を「kill-switch だけ有効」で運転させないための意図した挙動である ([ADR-043](adr-043-security-gates-fail-closed.md))。

### 4. 実測件数を数えるのは呼び手、判断するのは gate

`cli-autonomy-gate` は自分で GitHub を数えない。呼び手 (workflow step の `gh api`) が数え、`--open-draft-prs <count>` で渡す。

- gate の I/O 面を config と env だけに保つ ([ADR-066](adr-066-autonomy-global-kill-switch.md) の `sources.rs` の設計をそのまま維持する)。exe が `gh` に依存すると、ローカル drill が GitHub 到達性と認証に依存して**再現可能な安全装置の検証ができなくなる**。
- 数える主体と判断する主体を分ける構図は [ADR-067](adr-067-phase-b-unattended-fix-push.md) と同型 (findings を出す agent と push を判断する gate が別)。

**呼び手が数を偽れる**という指摘は成立するが、この経路の呼び手は schedule イベントで起動する workflow であり、GitHub の仕様上 **default branch (master) の workflow 定義が実行される**。PR ブランチ上で数え方を書き換えても schedule 実行には反映されない。これは config の master ref 契約 ([ADR-066](adr-066-autonomy-global-kill-switch.md) § 決定 3) と同じ信頼境界に乗っている。

### 5. 数えられなかったことは 0 件ではない

`--open-draft-prs` の値は `u32` としてパースし、空文字 / 負値 / 小数 / 非数値は**引数不正 (exit 2)** として loud に落とす。`None` へ黙って潰さない。

`gh api` が失敗したときの出力 (空文字など) を 0 件と読み違えて「draft が 1 件も無いので作ってよい」に倒れるのが、この機構で最も避けたい failure mode である。フラグ自体の省略は `None` = 背圧未接続 = deny なので、**省略も不正値も許可へは倒れない**。

## 外部 SaaS の課金・上限事実 (2026-08-06 再確認)

計画書 (ephemeral) が保持していた前提事実のうち、本 WP に関係するものを永続化する (計画書 § 2 の移管義務)。research preview 由来の仕様変動があるため、確認日を明記する。

| 事実 | 確認日 | 備考 |
|---|---|---|
| public リポジトリ + standard GitHub-hosted runner の Actions 実行は無料。分数は無制限 | 2026-08-06 | GitHub 公式 docs の原文: "GitHub Actions usage is free for self-hosted runners and for public repositories that use standard GitHub-hosted runners" |
| GitHub Free の 2,000 分/月は **private リポジトリのみ**に適用 | 2026-08-06 | 本リポジトリ (aloekun/claude-code-hook-test) は public のため対象外 |
| `anthropics/claude-code-action@v1` は `claude_code_oauth_token` (Pro/Max のサブスク認証) で動く | 2026-08-06 | 本リポジトリの `.github/workflows/pr-monitor.yml` が 3 箇所で実運用中 |
| OAuth token は**個人サブスクに紐づき**、自動化の消費が対話作業のレート枠を圧迫しうる | 2026-08-06 | 夜間ループの実質コストはここ。背圧の経済的根拠 (§ コンテキスト) |

要約すると、**Actions の実行時間は無料だが Max 枠は有限**であり、夜間ループのコスト上限は「無駄な run をどれだけ抑えられるか」で決まる。背圧はその抑制の第一段である。

## 試験運用判断基準 (ADR-039)

| 項目 | 内容 |
|---|---|
| **Config opt-in** | `[autonomy] max_open_draft_prs`。キーが無ければ背圧未接続 = `draft-pr` は deny。既定が OFF かつ fail-closed 方向で、[ADR-066](adr-066-autonomy-global-kill-switch.md) と同じく両原則が同じ向きを指す |
| **Kill-switch** | draft-pr クラスだけ止めるなら `max_open_draft_prs = 0`。全自律動作を止めるなら `enabled = false` または Actions variable `AUTONOMY_ENABLED` の削除 (ADR-066 の操作反射をそのまま使う。新しい停止操作を増やさない) |
| **Bounded lifetime** | decision trigger: 夜間ループ稼働後に (a) 閾値未満で draft PR 作成が通ること、(b) **閾値到達で実際に次の run が `backpressure-saturated` で止まること**、(c) deny 理由が run log 1 行で切り分けられること、(d) 閾値 3 が運用実態 (滞留時間・採用率) に合っていること、を確認したら本採用。**2026-11-06 までに判定材料が集まらなければ延長 / 却下を判断する** |

(b) は本 ADR に固有の観測点である。(a) と (c) は exe 単体 drill で固定できるが、**閾値に到達する状態は夜間ループが実際に draft を積まないと作れない**。閾値 3 は初期値であり、[dev-conventions](../dev-conventions.md) § LLM を含む自動化経路は実走でしか検証できない、の適用対象。

## 検証記録

### 背圧 drill 12 シナリオ (2026-08-06、release build の実 exe)

`cli-autonomy-gate.exe` を実バイナリで直接起動し、境界と欠損の全経路を確認した。全て設計どおり。

| # | 入力 | 期待 | 結果 |
|---|---|---|---|
| 1 | `draft-pr` / 外部フラグ未設定 / config 正常 / open=0 | deny `external-unset`、exit 1 | 一致 |
| 2 | `draft-pr` / 外部 `1` / config `enabled=false` | deny `repo-config-disabled`、exit 1 | 一致 |
| 3 | `draft-pr` / 外部 `1` / config ファイル不在 | deny `repo-config-unavailable`、exit 1 | 一致 |
| 4 | `draft-pr` / config 正常 / `--open-draft-prs` 省略 | deny `backpressure-unavailable`、exit 1 | 一致 |
| 5 | `draft-pr` / config に閾値キー無し / open=0 | deny `backpressure-unavailable`、exit 1 | 一致 |
| 6 | `draft-pr` / limit=3 / open=2 | allow、`backpressure(draft-pr)=ok(2/3)`、exit 0 | 一致 |
| 7 | `draft-pr` / limit=3 / open=3 | deny `backpressure-saturated`、`saturated(3/3)`、exit 1 | 一致 |
| 8 | `draft-pr` / limit=0 / open=0 | deny `backpressure-saturated`、`saturated(0/0)`、exit 1 | 一致 |
| 9 | `fix-push` / limit=3 / open=99 | allow、`backpressure(fix-push)=structural`、exit 0 | 一致 |
| 10 | `draft-pr` / 閾値が文字列 `"3"` | deny `repo-config-unavailable` (半壊 config は `enabled` ごと停止)、exit 1 | 一致 |
| 11 | `--open-draft-prs abc` | 引数不正、exit 2 | 一致 |
| 12 | `--open-draft-prs -1` | 引数不正、exit 2 | 一致 |

#6 と #7 の対で `>=` 境界が、#8 で `limit = 0` の停止が、#9 で fix push が draft 数から独立していることが、それぞれ実バイナリ上で確認できている。

#1 / #2 の状態行には deny 理由が背圧でないにもかかわらず `backpressure(draft-pr)=ok(0/3)` が出る。これは `describe_sources` が deny 理由を 1 つに絞るのとは別に**全ソースの状態を独立に出す**設計 ([ADR-066](adr-066-autonomy-global-kill-switch.md)) によるもので、「フラグを 1 つ直したのにまだ止まる」の切り分けを 1 run で終わらせるための意図した冗長性である。

### unit test

判定コア 22 件 / sources 11 件 / 引数解析 7 件。うち網羅走査 1 件が **216 組合せ** (repo config 3 × 外部フラグ 15 × 背圧 4 × 操作 2) を走査し、許可される組合せ数を式で固定している。

## 帰結

### 利点

- ADR-052 原則 5 の契約が `draft-pr` についても機械判定可能な実体を得た。夜間ループ (WP-18 PR 3) は gate を呼ぶだけで契約を満たせる。
- 「全部止める」以外の減速手段ができた。`max_open_draft_prs = 0` は fix push を生かしたまま夜間ループだけを止める。
- 実測値と閾値を状態行に必ず併記するため、「数え損ねて止まった」と「積み過ぎて止まった」が run log 1 行で分かれる。
- 停止操作を新設していない。既存の 2 つ (config / Actions variable) に 1 つの数値キーが増えただけで、緊急時の操作反射は変わらない。

### 欠点 / 留意点

- **背圧の実測は呼び手依存**。gate は渡された数を信じるしかなく、数え方の誤り (prefix の取りこぼし、draft でない PR の混入) は gate では捕捉できない。schedule イベントが master の workflow 定義を使う点が唯一の構造的担保であり、数え方そのものは workflow レビューで守る。
- **閾値 3 に実測の裏付けが無い**。「1 セッションで捌ける上限」の見積りにすぎず、bounded lifetime (d) で見直す。
- **半壊 config が全停止を招く**。閾値キーの typo で `enabled` ごと読めなくなるのは fail-closed として正しいが、運用上は「1 文字直したら全部止まった」に見える。config のコメントに `pnpm autonomy-status` での確認を明記して緩和した。
- 背圧が効くのは**次の判定時点**であり、既に起動済みの run は止まらない (ADR-052 原則 5 の停止手順と同じ性質)。

### 残課題

- 実測件数を渡す呼び手 (夜間 workflow の `gh api` step) は本 ADR の PR には含まれない。WP-18 PR 3 で追加する。それまで `draft-pr` は `backpressure-unavailable` で deny のままであり、**運用挙動は変わらない**。
- bounded lifetime (b) の観測 = 閾値到達での実停止は、夜間ループが 3 件の draft を積むまで発生しない。WP-18 の 2 週間の試験運用期間中に観測できなければ、閾値を一時的に下げて意図的に到達させる。
