# 保留: 夜間ループ (nightly-todo) の改善タスク

> **本ファイルの位置付け**: 夜間ループ ([ADR-072](../adr/adr-072-nightly-todo-loop.md)) と auto lane の仕組みを直す・改善するタスクの保留置き場。2026-11-01 に todo 系列 (`docs/todo*.md` / `docs/todo-summary*.md`) と台帳から、順位 table の行・詳細エントリ・台帳の行をまとめて移した。
>
> **ユーザーの指示が無い限り、本ファイルからタスクを選ばない。** タスクのピックアップや優先度付けでは、本ファイルを読まない。
>
> **保留の理由**: 2026-09 下旬から、CodeRabbit は seat を持たない bot が作った PR をレビューしなくなった ([ADR-085](../adr/adr-085-third-party-review-means.md))。夜間ループの PR はすべて GitHub App が作るので、レビューを受けられない。このため夜間ループは 2026-10-18 から休止している ([ADR-072](../adr/adr-072-nightly-todo-loop.md) § 休止)。休止中は、ここにあるタスクを済ませても効果が無い。
>
> **todo 系列の外に置いた理由**: todo 系列に残すと、新しいセッションや週次レビューが Tier の高いタスクとして拾う。2026-11-01 のセッションでは、休止中の夜間ループの都合 (auto lane に置けるか) を優先度の根拠にしてタスクを選んでしまった。lane の記述を整えても、todo 系列にある限り同じことが起こりうる。本ファイルは `docs/` 直下の `todo*.md` に当たらないので、`pnpm lint:docs`・夜間ループのタスク選択・週次レビューの読み取り対象にならない。
>
> **選別の基準**: 夜間ループや auto lane の**仕組み**を改善するタスクだけを置く。auto で実行できるだけで、本リポジトリを改善するタスクは todo 系列と台帳 (`docs/claude-code-web-tasks.md`) に残す。台帳の lane 表も維持する。
>
> **Tier** はタスクの種類を表す ([ADR-090](../adr/adr-090-todo-tier-as-kind.md))。2026-11-13 に種類で付け直した。
>
> **順位の採番**: 順位は todo 系列と共有する追記型の ID である。本ファイルの順位 (589 など) は使用済みで、todo 系列で新しく採番するときは本ファイルの順位も含めた最大 + 1 を使う。
>
> **新規の起票**: 夜間ループか auto lane の仕組みを改善するタスクは、本ファイルへ起票する。
>
> **完了時**: 本ファイルにあるまま済ませたタスクは、詳細エントリと本ファイル順位 table の行を消す。台帳から移した行があれば、それも消す。
>
> **再開時の戻し方**: 夜間ループを再開するときは、次の手順で戻す。
>
> 1. 順位 table の行を `docs/todo-summary3.md` の末尾へ戻す。順位は変えず、ファイル列を戻し先にする
> 2. 詳細エントリを、[docs/todo.md](../todo.md) の preamble が指す現在の追加先へ移す
> 3. 台帳の行を `docs/claude-code-web-tasks.md` の元の表へ戻す
> 4. `pnpm lint:docs` で順位 table と詳細エントリの対応を確かめる

## 順位 table

| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 374 | Tier 1 | **WP-18 夜間ループの実走スモーク実施** | nightly-loop.md (元 todo20.md) | M | #363 マージ (WP-18 受け入れ基準の中核。観測項目の一覧は ADR-072 の実走スモーク節が正で本 todo には複製しない) |
| 377 | Tier 3 | **夜間ループの防御を検知から防止へ格上げする判断** | nightly-loop.md (元 todo20.md) | M-L | 順位 374 + 2 週間の試験運用 (ADR-072 残課題 3 件。実運用の観測が判断材料で、観測前の着手は過剰設計。「不要」判断も正規の出口) |
| 382 | Tier 2 | **台帳 prompt injection payload の regression test (#363 Tier2 #1)** | nightly-loop2.md (元 todo20.md) | M | なし (依存だった順位 380 の framing は 2026-08-08 実装済み = ADR-072 決定 13。順位 378-381 は完了・削除済み) |
| 390 | Tier 2 | **台帳 framing 区切りの定数と workflow リテラルの cross-file 一致を CI 検証 (#369 T2 採用)** | nightly-loop.md (元 todo21.md) | M | なし (LEDGER_DATA_FRAME_MARKER と ===BEGIN/END_LEDGER_DATA=== が対。片側変更で ADR-072 決定 13 の framing が破れる) |
| 431 | Tier 2 | **`review-request` の成功判定を初回レビュー取得まで遅らせる** | nightly-loop.md (元 todo27.md) | S-M | なし (2026-08-11 実走で判明。レート制限による拒否も success として記録され、未レビューの自律 PR が信号として残らない。ADR-019 § M5 の方針によりリトライは作らず検出と可視化に留める) |
| 447 | Tier 1 | **台帳の `✅無人可` と判断留保キーワードの矛盾を決定論層で検出 (PR #400 T1-2)** | nightly-loop.md (元 todo23.md) | S | なし (2026-08-14 採用。#400 の正準タグ規約は instruction 層のみで機械強制が無い。実装先は custom lint rule か ledger.rs の fail-closed 検査かを着手時に決める) |
| 448 | Tier 2 | **判断留保キーワード検査の回帰テスト (canonical / tagged / untagged の 3 分類) (PR #400 T2-1)** | nightly-loop.md (元 todo23.md) | S | 447 (検証対象が 447 の成果物。走査の実体が現状 Rust に無いため単独着手は不可) |
| 464 | Tier 2 | **`review-todo-whole` facet が読む台帳の事実を `cli-ledger-candidates` の出力へ寄せる** | nightly-loop.md (元 todo24.md) | S | なし (2026-08-17 に再 rescope。Criterion 3-2 は決定論 exe へ置換済み・3-3 のブランチ走査は消滅。残るのは 3-1 の逆向き差集合と `✅` 行の特定で、同 exe に出力を足すだけで足りる) |
| 466 | Tier 3 | **出力先と検証設計の convention を明文化する (#409-#414 feedback 系統 C+E を統合)** | nightly-loop.md (元 todo24.md) | S | なし (2026-08-17 採用。docs のみ。出力の visible paths / fixture と実データの対 / step outcome の組み合わせ の 3 点) |
| 474 | Tier 2 | **夜間 auto lane とユーザー割当 PR の同一ファイル競合を自動検知する (#424 feedback 採用、系統 G)** | nightly-loop.md (元 todo24.md) | S | なし (ADR-074 は lane 割当基準のみで並行競合検知は範囲外) |
| 486 | Tier 1 | **auto lane の対象ファイルが Guard 禁止パスに当たる行を決定論的に弾く (夜間ループ停止調査 2026-08-22 由来)** | nightly-loop2.md (元 todo25.md) | S | なし (2026-08-20 の run が順位 383 を選び `src/lib-ledger/src/lib.rs` の変更で `[NIGHTLY_DENY]` 停止。auto lane 22 行の全件照合で 5 行が deny リスト該当 (383 / 454 / 368 / 360 / 361)。ADR-074 決定 2 クラス 3 の判定を決定論化する — 同 ADR 決定 6 が「決定論だが未実装」と自認している穴。**実装先が deny リスト配下のため auto lane に載せない**) |
| 506 | Tier 2 | **[defect:G2] 夜間ループと Node script 層の境界をテストで固定する** | nightly-loop2.md (元 todo26.md) | M | なし (PR #466 / #469 / #470 / #471 feedback。B4 の 4 件は実測済みで固定するだけ、合成ブランチの CI 化のみ新規) |
| 508 | Tier 2 | **[improvement] 台帳追加候補の除外クラスを決定論で機械適用する** | nightly-loop2.md (元 todo26.md) | M | なし (2026-09-03 weekly-review で 238 件を人手選別した。ADR-072 決定 18 の読み替えと skill 制約の改訂を伴う。着手時判断: 順位 486/447 の検査と判定ロジックを共通化するか) |
| 510 | Tier 1 | **[defect:G1] 夜間ループの稼働状況を週次レビューで見張る** | nightly-loop2.md (元 todo26.md) | M | なし (直近 8 晩で 5 晩 red・直近 4 晩連続なのに 2026-09-03 の findings 8 件に言及 0 件。gh が要るため L3 の決定論 scan に置く。着手時判断: ログをどこまで読むか = 停止段まで出すか conclusion だけか。2026-09-28 にハーネス改善計画 WP-19 ステップ 3 = 自律 PR の棚卸しと無人 PR 採用率の測定 (ADR-072 の判定期限 2026-11-06) を統合。2026-10-11 追記: 09-20〜10-10 の 21 晩は自動 lane の `✅` 行が 0 件で、毎晩 green のまま何もしていなかった。red の監視ではタスク切れが見えないので、implement 未到達の連続日数と `✅` 行の残数も出す) |
| 566 | Tier 2 | **[improvement] review-request の依頼済み判定と待機時間の記述を締める** | nightly-loop2.md (元 todo29.md) | S | なし (2026-10-12、#496 / #513 feedback。Guard 禁止パスの変更なので human lane) |
| 586 | Tier 2 | **[improvement] 夜間ループの再開前に review-request.yml のスキップ通知の扱いと待ち時間を決める** | nightly-loop2.md (元 todo29.md) | S | 夜間ループの再開 (2026-10-30、順位 585 から切り出し。待ち時間 900 秒が実測の約 18 分より短い。値は再開時の実測で決める) |
| 589 | Tier 2 | **[improvement] 夜間 verify で vitest と lint:workflows を走らせる** | nightly-loop2.md (順位 562 から分割) | S | なし (2026-11-01 に順位 562 から分けた。scripts/ 系のテストを auto lane に置くための前提。順位 563 / 564 を auto lane へ移す前提) |

## 台帳 (`docs/claude-code-web-tasks.md` § Batch 1) から移した行

| 順位 | Tier | 無人可 | 内容 | 対象ファイル (実パス) | 工数 | 注意 | PRタイトル |
|---|---|---|---|---|---|---|---|
| 447 | T1 | — | 台帳の `✅無人可` と判断留保キーワード (再選定 / 着手時判断 / 見積り / 検討) の矛盾を決定論層で検出する | `src/lib-ledger/src/deployed_ledger.rs` | S | 転記規約 (本ファイル § 自律実行可否の 2 段階分類) の機械強制。検査は台帳を読むだけで書き換えない。順位 486 と同一ファイル・同一層なので実装順の調整が要る。**`src/lib-ledger/` は Guard 禁止パス**のため human lane (順位 383 と同じ理由、ADR-074 除外クラス 3) | test(ledger): 無人可と判断留保キーワードの矛盾を検出する |
| 486 | T1 | — | auto lane の対象ファイルが Guard 禁止パスに当たる行を決定論的に弾く | `src/lib-ledger/src/deployed_ledger.rs` | S | **着手時判断: 禁止パスの読み先が未決定** (順位 454 は単一定義を作らず 3 箇所の一致テストで決着した)。deny リストをどこから読むかを着手時に決める必要がある。順位 447 と同一ファイル・同一層。**`src/lib-ledger/` は Guard 禁止パス**のため human lane (ADR-074 除外クラス 3) | test(ledger): auto lane の対象パスが Guard 禁止パスに当たる行を弾く |

## 詳細エントリ

### 順位 374: WP-18 夜間ループの実走スモーク実施

> 元: `docs/todo20.md` (2026-11-01 移送)
>
> **動機**: WP-18 の受け入れ基準の中核でありながら**未実施**。#363 の時点では (a) workflow が master に無い、(b) 台帳の無人可マークが master に無い、の両方が未達だった。#361 / #362 のマージで (b) は解消し、#363 のマージで (a) が解消する。
>
> [ADR-067](../adr/adr-067-phase-b-unattended-fix-push.md) § LLM を含む自動化経路は実走でしか検証できない が本エントリの根拠。#363 は unit test 25 件・実データ選択・改ざん検知 drill 4 シナリオを通しているが、**agent が実際に何を書くか**と **GitHub Actions ランタイム上の挙動**はどれも捕捉できない。
>
> **対処案**: #363 マージ後、[ADR-067](../adr/adr-067-phase-b-unattended-fix-push.md) 段 2 の知見 2 に従い**マージせずブランチ ref への `workflow_dispatch`** で反復する (`dry_run` 入力でゲート通過まで走らせ push を止められる)。1 バグ 1 サイクルの手戻りを避けるため、マージは完走を確認してから 1 回だけ行う。
>
> **観測項目の一覧は `ADR-072` の実走スモーク節にある表が正**。本エントリには複製しない (同じチェックリストを 2 箇所で管理すると必ず drift する — #362 の post-merge feedback が指摘した single source-of-truth 問題と同型)。現時点で 8 項目あり、うち 2 件は WP-17 から引き継いだ残課題。
>
> **参照**: `ADR-072` の実走スモーク節 / 試験運用判断基準節、[ADR-067](../adr/adr-067-phase-b-unattended-fix-push.md) (WP-17 残課題 2 件の出所、および § LLM を含む自動化経路は実走でしか検証できない)。
>
> **種類**: Tier 2 — 夜間ループの実走による検証。工数 M。

#### 作業計画

- [x] draft PR 作成までの完走 (2026-08-08、schedule 初回実走 = PR #365。dispatch でなく本番 run が先に消化した経緯は [ADR-072](../adr/adr-072-nightly-todo-loop.md) § 残課題)
- [x] **停止側の実走確認** (2026-08-08、`'false'` / 未設定 × dry_run オフで 2 回とも job skip。[ADR-066](../adr/adr-066-autonomy-global-kill-switch.md) § 実走観測 2)
- [x] スモーク観測項目の記帳 (10 項目中 8 充足 / 1 不成立 = 決定 11 で対処 / 1 保留 = トークン露出。ADR-072 § 実走スモーク)
- [x] 順位 384 (外部設定の実体記録) の同時実施 (2026-08-08 完了、ADR-072 § 外部設定の実体)
- [ ] **残り: トークン露出 probe** — 初版 probe の設計欠陥を解消した安全な probe を設計してから 1 回で観測 (意図的保留、ADR-072 § 残課題)
- [ ] 決定 11 の実測 (CodeRabbit が bot 投稿の `@coderabbitai review` に反応するか) 完了後、2 週間の採用率測定を開始する (2026-08-09 ユーザー決定: トークン露出の保留は測定開始をブロックしない)
- [ ] 本エントリ削除 + 本ファイル順位 table の 374 行削除

#### 完了基準

- 有効時のみ `claude/nightly-*` の draft PR が作られることを実走で確認。
- `ADR-072` の実走スモーク 8 項目すべてに観測結果が記帳されていること (未観測の項目が残るなら、その理由も記帳)。
- **`AUTONOMY_ENABLED` の 3 状態 (`'true'` / `'false'` / 未設定) すべてで実走の観測結果が記帳されていること。** 無効 2 状態では workflow job・ブランチ・draft PR・App token のいずれも作られないことを確認する。
- 計画書の WP-18 受け入れ基準表から「未実施」が消えていること。

### 順位 377: 夜間ループの防御を検知から防止へ格上げする判断

> 元: `docs/todo20.md` (2026-11-01 移送)
>
> **動機**: `ADR-072` の残課題 3 件。いずれも**実運用の観測が判断材料**で、観測前に着手すると過剰設計になりうる。
>
> 1. **`master-ref/` を agent のファイルシステムから外す** — 現状は Build 段で採った sha256 を authority gate 直前で照合する**検知**のみ。agent の file tools は `$GITHUB_WORKSPACE` 全体に届くため、書き込み自体は防止していない。防止するには別 job + artifact 受け渡しへの構造変更が要る
> 2. **authority gate の draft 数再計数** — authority が読み直すのは kill-switch の 2 拠点だけで、未マージ draft 数は job 冒頭のスナップショットを使い回す。実装 step が最大 60 ターン走る間に別経路で draft が増えると、閾値を 1 件超えて push されうる。「kill-switch は即時・背圧は run 単位」という粒度差として現状は受容している
> 3. **ガードレール禁止リストの allowlist 化** — 台帳の「対象ファイル」列が自由記述の markdown で path allowlist に落とせないため、現状は禁止リスト。列挙し忘れたガードレールは守られない
>
> **対処案**: 実走スモークと 2 週間の試験運用で agent の実挙動を観測してから、3 件それぞれの着手可否を判断する。判断材料は (1) agent がワークスペース外へ実際に手を伸ばすか、(2) 閾値超過が実際に起きるか、(3) 禁止リストの誤検知・取りこぼしが起きるか。**観測の結果「不要」と判断することも正規の出口**とする。
>
> 3 は台帳の機械可読化 (別列に正規化パスを持つ等) が前提なので、着手するなら台帳側の変更とセットになる。
>
> **参照**: `ADR-072` の残課題節 / 欠点・留意点節、[ADR-052](../adr/adr-052-autonomy-execution-boundary-classes.md) 原則 5 (背圧の契約)、[ADR-039](../adr/adr-039-experimental-feature-standard-pattern.md) (bounded lifetime)。
>
> **種類**: Tier 1 — 検知から防止への格上げ (見込みの成果物は防止の仕組み)。工数 M-L (1 は構造変更)。**実走スモークと 2 週間の試験運用より後**。

#### 作業計画

- [ ] 実走スモーク完了後、agent がワークスペース外へ手を伸ばした形跡があるか run log で確認
- [ ] 2 週間の試験運用で閾値超過 / 禁止リストの誤検知が起きたか集計
- [ ] 3 件それぞれ「着手」「不要」を判断し `ADR-072` の残課題節へ結果を記帳
- [ ] 着手すると決めたものだけ実装する
- [ ] 本エントリ削除 + 本ファイル順位 table の 377 行削除

#### 完了基準

- 3 件それぞれに「着手 / 不要」の判断と根拠 (観測データ) が `ADR-072` へ記帳されていること。
- 着手と判断したものが実装済みであること。

### 順位 390: 台帳 framing 区切りの定数と workflow リテラルの cross-file 一致を CI で検証

> 元: `docs/todo21.md` (2026-11-01 移送)
>
> **動機**: [ADR-072](../adr/adr-072-nightly-todo-loop.md) 決定 13 の framing は、`ledger.rs` の `LEDGER_DATA_FRAME_MARKER` と `.github/workflows/nightly-todo.yml` の `===BEGIN/END_LEDGER_DATA===` が**対**になって初めて成立する。片方だけ変えると framing が破れる (agent プロンプトの信頼境界が開く) が、現状は doc comment の相互参照だけで機械検証が無い。[ADR-069](../adr/adr-069-pr-chain-declaration.md) 決定 7 の sha256 gate-asset check と同格の cross-file 不変量。
>
> **対処案**: 両ファイルから定数 / リテラルを抽出して一致を assert する CI テストを追加する。AST 層 custom linter は overkill で、単純な constant-compare test (Effort M) で同等効果。
>
> **参照**: `.claude/feedback-reports/369.md` Tier 2 #1、[ADR-072](../adr/adr-072-nightly-todo-loop.md) 決定 13、[ADR-051](../adr/adr-051-cross-system-config-coupling.md) (cross-system coupling の機械検証)。
>
> **種類**: Tier 2 — CI での一致検証。工数 M。

#### 作業計画

- [ ] `ledger.rs` の `LEDGER_DATA_FRAME_MARKER` と workflow の区切りリテラルを抽出・比較するテストを新設
- [ ] 片方を変えるとテストが落ちることを確認

#### 完了基準

- 定数とリテラルが不一致になる変更が入るとテストが落ちること。

### 順位 431: `review-request` の成功判定を初回レビュー取得まで遅らせる

> 元: `docs/todo27.md` (2026-11-01 移送)
>
> **動機**: 2026-08-11 の夜間ループ実走 (PR [#387](https://github.com/aloekun/claude-code-hook-test/pull/387)) で、CodeRabbit がレート制限により `Review limit reached` を返した。`review-request` の検証は**要求後に CodeRabbit のコメントが 1 件以上付いたか**だけを見るため、**拒否も success として記録**され、run は緑で終わった。
>
> **契約どおりではある**: この検証は決定 11 の失敗 (10 時間の無反応に気づけなかった) への対策で、workflow のコメントにも「投稿の成否ではなく CodeRabbit の反応を待つ」と明記されている。**設計の欠陥ではない。**
>
> **問題は残る**: 結果として「レビューが付かないまま success で終わった自律 PR」がどこにも信号として残らない。[ADR-019](../adr/adr-019-coderabbit-review-hybrid-policy.md) § M5 の方針でリトライ機構を持たないため、解除後に自動で再要求されることもない。翌朝レビューする人間は、PR を開くまで未レビューだと分からない。
>
> **対処案**: (a) 反応の**中身**を判別し、レート制限による拒否は success としない (run を warning か failure にする)、(b) 未レビューの自律 PR を検出する信号を別に持つ (weekly-review の自律アクション棚卸しに載せる = WP-19 ステップ 3)、(c) 解除見込み時刻を run log に出して人間が再要求できるようにする。**リトライ機構の作り込みは § M5 の方針に反する**ため、検出と可視化に留めること。
>
> **参照**: [review-request.yml](../../.github/workflows/review-request.yml)、[ADR-072](../adr/adr-072-nightly-todo-loop.md) § 定常運用 2 巡目の実走観測、[ADR-019](../adr/adr-019-coderabbit-review-hybrid-policy.md) § 無料枠の窓は固定時刻ではなく直近の消費に追随する / § M5、[ADR-064](../adr/adr-064-monitor-success-positive-evidence.md) (陽性証拠の要求)。
>
> **種類**: Tier 2 — workflow (`review-request`) の判定の変更。工数 S-M。

#### 作業計画

- [x] CodeRabbit の応答パターンを分類する (2026-08-20 実装)。`REVIEWED` (walkthrough marker) / `RATE_LIMITED` / `OTHER` の 3 分類とし、`OTHER` (ack・skip 通知・未知 format) は陽性証拠に数えず deadline 到達で red = 安全側 (未取得扱い) へ倒す。
  - **台帳の前提がずれていた**: 拒否の実体は walkthrough の `Review limit reached` placeholder ではなく **command ack** (`<!-- This is an auto-generated reply by CodeRabbit -->` + `⚠️ Action not completed` + `Review rate limited.`) で、`markers.rs` の `RATE_LIMIT_MARKERS` の**どちらにも一致しない** (PR #387 の実 body を `gh api` で確認)。本 workflow の marker は markers.rs の上位集合とし、`Review rate limited.` を workflow 固有 marker として追加した。
  - rate-limit の判定は陽性証拠より**先**に行う。#387 では拒否 ack と summarize marker 付き placeholder が 3 秒差で並んでおり、先に陽性証拠を探すと placeholder をレビュー実体と読んで silent success に戻る。
- [x] success 判定を**陽性証拠へ寄せる**方針に決定 (2026-08-20、ユーザー判断)。レート制限検出時は `[REVIEW_REQUEST_RATE_LIMITED]` を出して **red で落とす**。リトライは作らない (ADR-019 § M5) ため、run の色が「この PR は未レビューのまま残った」の即時信号になる。成功条件を厳しくしたぶん待機上限を 10 分 → 15 分へ延長 (job の `timeout-minutes` も 15 → 20)。
- [x] weekly-review との役割分担を workflow 先頭に明記 (2026-08-20)。本 workflow は**即時信号のみ**で再要求はせず、未レビュー PR を全体で拾い直すのは weekly-review の自律アクション棚卸し (WP-19 ステップ 3) 側とする。両者は独立に動く。
- [ ] **実走観測**: 次にレート制限が起きた夜間 run で red + `[REVIEW_REQUEST_RATE_LIMITED]` が出ることを確認してから本エントリを削除する。

#### 完了基準

- レート制限で弾かれた自律 PR が、run の色か後続の棚卸しのいずれかで**未レビューと分かる**こと。

> **現在地 (2026-08-20)**: 実装済み。分類 jq は workflow ファイルから切り出して実 PR (#387 / #426 / #421 / #419) のコメント列へ適用し、**#387 (本エントリの由来 incident) が `RATE_LIMITED` = red、正常レビュー済みの 3 件が `REVIEWED` = green** になることを実測した。marker が複数層に分散する構造なので `scripts/lint-workflows.mjs` に同期検査を追加している (片方だけ変えると silent success に戻るため)。**完了基準の判定は実走観測待ち。**

### 順位 447: 台帳の `✅無人可` と判断留保キーワードの矛盾を決定論層で検出する (出典: PR #400 Tier1 #2)

> 元: `docs/todo23.md` (2026-11-01 移送)
>
> **動機**: CodeRabbit 指摘 (#400 の 2 件目) — 無人可判定の条件 1 は `注意` 欄の 4 語走査だけを見るため、同義表現 (「未定」「複数案あり」) はすり抜ける。#400 で正準タグ `「着手時判断: <原文>」` を規約化したが、**規約は instruction 層にあり機械強制が無い**。人間がタグを付け忘れれば従来どおりすり抜ける。
>
> **本タスクの位置づけ**: 台帳 (`docs/claude-code-web-tasks.md`) と facet instruction に入れた転記規約の決定論的な裏付け。「判断留保キーワード検査の回帰テスト」の前提タスクでもある (あちらの検証対象が本タスクの成果物になる)。
>
> **検出ロジック**: 「`無人可=✅` の行の `注意` セルに 4 語のいずれかが含まれる」を矛盾として検出する。正準タグ規約により同義表現もこの検査に落ちるため、**synonym の列挙に依存しない**のが利点。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 1 #2、[ADR-043](../adr/adr-043-security-gates-fail-closed.md)、[ADR-072](../adr/adr-072-nightly-todo-loop.md) 決定 1、[docs/claude-code-web-tasks.md](../claude-code-web-tasks.md) § 自律実行可否の 2 段階分類
>
> **種類**: Tier 1 — 決定論層での矛盾の検出。工数 S。夜間ループが人間の意図と違うタスクを実装する経路を塞ぐ。

#### 設計決定 (案)

実装先の候補が 2 つあり、着手時に選ぶ (両方入れる選択もある):

- **(a) custom lint rule** (`.claude/custom-lint-rules.toml`、`paths=["docs/claude-code-web-tasks.md"]`) — 台帳を編集した人へ即時フィードバック。既存 12 rule の確立パターンに乗る
- **(b) 台帳パーサの fail-closed 検査** (`src/lib-ledger/src/lib.rs`) — 夜間ループがタスクを選ぶ瞬間に停止する。同 crate の設計方針「曖昧さはすべて停止側へ」および [ADR-043](../adr/adr-043-security-gates-fail-closed.md) と整合し、**マークが誤っていても自動実行に到達しない**

(a) は書き手への予防、(b) は自動実行の直前での遮断で、守る対象が違う。

#### 作業計画

- [ ] 実装先を (a) / (b) / 両方 から決める (夜間ループの fail-closed 経路に置くかが判断の軸)
- [ ] **キーワードの一致条件を確定する** — 現状、表記が揺れている (台帳の条件 1 は「再選定**する**」、facet instruction / SKILL.md / 本エントリは「再選定」。CodeRabbit #401 指摘)。語幹「再選定」の部分一致 (substring match) を採用すれば両表記を包含でき、活用形の列挙が不要になる。完全一致を採用する場合は関連 4 文書 (台帳 / facet / SKILL.md / 本 corpus) の表記を同一 PR で統一する。決定した契約は検査実装のコメントと台帳の転記規則の両方に明記する
- [ ] 検査を実装する
- [ ] good / bad fixture を追加する (実装先が lint rule なら `rule_test_coverage_check` / `incident_eval.rs` の 3 点セット)
- [ ] 台帳の現行 12 行が検査を通ることを確認する (dogfood)

#### 完了基準

- `無人可=✅` かつ `注意` に判断留保キーワードを含む行が、人手のレビューを介さず検出される

#### 詰まっている箇所

なし

### 順位 448: 判断留保キーワード検査の回帰テスト (出典: PR #400 Tier2 #1)

> 元: `docs/todo23.md` (2026-11-01 移送)
>
> **動機**: CodeRabbit 指摘 (#400 の 2 件目) の回帰テスト。canonical 4 語 / 正準タグ付き同義語 / タグなし同義語の 3 分類で、検出・非検出の境界を固定する。
>
> **本タスクの位置づけ**: 「台帳の `✅無人可` と判断留保キーワードの矛盾を決定論層で検出する」の**後続タスク**。検証対象はあちらの成果物であり、**単独では着手できない** (現時点で走査の実体が Rust に無いため、テストの置き場所も決まらない)。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 2 #1
>
> **種類**: Tier 2 — 回帰テストの追加。工数 S。

#### 設計決定 (案)

先行タスクの実装先に応じてテストの置き場所が決まる:

- 実装先が **custom lint rule** の場合 → `rule_tests_extras.rs` + `tests/fixtures/incidents/{bad,good}/` + `tests/incident_eval.rs` の 3 点セット (既存 rule の確立パターン)
- 実装先が **台帳パーサ** の場合 → `ledger.rs` の `mod tests` に追加

入力の 3 分類と期待結果:

| 分類 | 例 | 期待 |
|---|---|---|
| canonical | 「再選定する」「着手時判断」「見積り」「検討」 | 検出 |
| canonical の活用形 / 語幹 | 「再選定」単体、「検討中」 | 先行タスクで確定した一致条件に従う (語幹部分一致なら検出)。**表記ゆれの両形 (「再選定」「再選定する」) を必ず fixture に含める** (CodeRabbit #401 指摘) |
| tagged synonym | 「着手時判断: 実装方式が未定」 | 検出 (タグが canonical 語を含むため) |
| untagged synonym | 「未定」「複数案あり」 | **非検出** |

3 行目は**仕様であって欠陥ではない**。タグ無し同義語を機械検出しようとすると synonym 列挙に戻り、[ADR-007](../adr/adr-007-custom-linter-layer-boundary.md) の regex 層の限界に当たる。テストはこの境界を明示的に固定し、「ここから先は運用規律が担保する」と線を引くために書く。

#### 作業計画

- [ ] 先行タスクの実装先が決まるのを待つ
- [ ] 3 分類の fixture を作る
- [ ] 境界 (特に untagged synonym の非検出) を assert し、意図的な仕様であるとコメントで残す

#### 完了基準

- 3 分類すべてが自動テストで固定され、`cargo test` で回帰検出できる

#### 詰まっている箇所

前提タスク (決定論層の実装先確定) 待ち

### 順位 464: `review-todo-whole` facet の台帳読み取りが現物と食い違う — 報告の信頼性を担保する

> 元: `docs/todo24.md` (2026-11-01 移送)
>
> **動機**: 2026-08-15 の run で、`review-todo-whole` facet が台帳の `無人可` 列を**誤読した**。順位 284 を `✅` と報告したが、[docs/claude-code-web-tasks.md](../claude-code-web-tasks.md) の現物は `—` であり、しかも同ファイルの § 無人可としなかった…理由 表に 284 が条件 3 違反として明記されている。facet は**自分が読んだはずのファイルに書いてある反証を見落として**逆の報告を出した。
>
> **なぜ問題か**: 本 facet の Criterion 3 は「台帳の外の状態 (未マージブランチ・進行中 PR) を見ないと判定できない」ことを唯一の存在理由にしている。その報告が台帳の現物とすら一致しないなら、外部状態の報告も同じ精度で疑う必要がある。実際 2026-08-15 の run は条件 3 を 3 件「unverified」と正しく報告しており (workflow が `network_access: false` のため `gh` に届かない)、**正しい部分と誤った部分が同じレポートに混在していた**。読み手が現物と突き合わせない限り区別できない。
>
> **同根の問題**: 同 facet は昇格候補チェックでも 251 件中 13 件しか判定せず、「token 予算不足」という**実測と矛盾する理由**を書いている (実行 2 分 37 秒 / iteration 1 回 / 予算制約の注入なし)。
>
> **本タスクの位置づけ**: facet は `model: haiku` で動いている (`.takt/workflows/weekly-review.yaml`)。台帳のセル値のような**機械的に読める事実**を LLM に読ませていること自体が設計の問題で、`lib-ledger` が既に台帳パーサを持っている以上、決定論層へ寄せられる。
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`.takt/runs/20260815-100604-weekly-review-2026-08-15/reports/review-todo-whole.md` (Criterion 3-3 の表)、`.takt/workflows/weekly-review.yaml` (`model: haiku`)、`src/lib-ledger/`、`src/cli-ledger-candidates/`
>
> **種類**: Tier 2 — exe (`cli-ledger-candidates`) の出力の拡張。工数 S。

#### rescope (2026-08-17): 残る範囲を確定した — 逆向き差集合の決定論化

lane モデルへの移行 ([ADR-072](../adr/adr-072-nightly-todo-loop.md) 決定 18) で facet の報告範囲を 2 度縮小し、そのうえで**まだ残る機械的読み取り**を確定させた。

| facet の検査 | 現状 | 決定論化の要否 |
|---|---|---|
| Criterion 3-2 (未掲載順位の列挙) | `cli-ledger-candidates` の出力への参照に置換済み | **完了** (LLM は数えなくなった) |
| Criterion 3-3 (未マージブランチ走査) | 撤去済み (条件 3 廃止) | **消滅** |
| Criterion 3-1 (台帳にあるが順位 table に無い行) | facet が台帳の順位列を読んで summary を grep する | **要** — `cli-ledger-candidates` の**逆向き差集合**そのもので、同じパーサで出せる |
| Criterion 3-3 (`✅` 行の `注意` 列の読み直し) | facet が `無人可` と `注意` のセルを読む | **一部要** — 判断留保の語を読むのは自然言語判断で LLM が適任だが、**どの行が `✅` か**は機械可読な事実で、2026-08-15 に誤読した (順位 284) のはまさにここ |

**したがって「不要」ではない。** ただし残りは当初より小さく、`cli-ledger-candidates` に逆向き出力 (台帳にあるが順位 table に無い順位) と `✅` 行の一覧を足すだけで足りる見込み。新しい経路の設計は要らない。

#### 作業計画

- [ ] `cli-ledger-candidates` に逆向き差集合 (台帳にあるが順位 table に無い順位) の出力を足す — Criterion 3-1 の置換先
- [ ] 同 exe に `無人可` 列の値つき行一覧を出す経路を足すか判断する (facet が `✅` 行を自分で読まなくて済むように)
- [ ] `review-todo-whole` の Criterion 3-1 / 3-3 を、その出力への参照に置き換える
- [ ] 次回 weekly-review の実走で、facet が報告する台帳の事実が現物と一致することを確認する

#### 完了基準

- facet が報告する台帳の機械可読な事実 (順位・`無人可`) が exe 由来になり、LLM の読み取り精度に依存しない
- 自然言語判断 (`注意` 欄の判断留保の読み取り) だけが facet に残る

#### 詰まっている箇所

- なし。当初のブロッカーだった PR-5 (`cli-ledger-candidates`) は [#414](https://github.com/aloekun/claude-code-hook-test/pull/414) で着地済み、その後の weekly-review 実走確認も完了した（本ファイル冒頭の完了報告テーブル参照）。本節の作業計画 (逆向き差集合の追加・Criterion 3-1/3-3 の置き換え) 自体は未着手のまま残る

### 順位 466: 出力先と検証設計の convention を明文化する (系統 C + E)

> 元: `docs/todo24.md` (2026-11-01 移送)
>
> **動機**: lane モデルの 5 PR で、同じ種類の設計ミスを 3 回踏んだ。(1) **出力先の見落とし** — 警告を stdout に出したが、workflow が stdout を捨てて許可リストの行だけ転送する設計だったため、最も見せたい経路 (exit 3) で警告が消えていた。(2) **fixture が実データを代表していない** — 順位 table 途中の空行という実データの癖を fixture が持たず、初版パーサが順位 193 以降を全部取りこぼした。実 exe を実ファイルに当てて初めて露見した。(3) **step outcome の組み合わせ** — `publish-tree` 失敗時に下流が `skipped` になり、`skipped != 'success'` で失敗マーカーが誤作成される経路を踏んだ。
>
> **なぜ 1 タスクか**: 3 件とも「**書く前に考える対象**」の明文化で、行き先が `docs/dev-conventions.md` と `ADR-072` に閉じる。実装を伴わない。
>
> **参照**: `.claude/feedback-reports/411.md` Tier3 #1・#2、`.claude/feedback-reports/412.md` Tier3 #2、ADR-043 / ADR-057 / ADR-064 (stdout/exit code の扱いを扱う既存 ADR 群)
>
> **種類**: Tier 3 — 規約の明文化。工数 S。

#### 設計決定 (案)

- ~~**C-1 出力の visible paths チェックリスト** (`docs/dev-conventions.md`)~~ **(2026-09-08 に却下)** — 実害が 1 件で機械化もできないため落とす
- ~~**C-2 fixture と実データの対** (`docs/dev-conventions.md`)~~ **(2026-09-08 に順位 514 へ統合)** — パーサ堅牢化を仕組みで担保できるかの検討に含めた
- **E-1 step outcome の組み合わせ** (ADR-072 へ追記): `skipped` は `success` ではない。上流が失敗して下流が skip された場合と、下流自身が deny した場合を条件式で区別する。現状は `nightly-todo.yml` のコードコメントに埋没している

#### 作業計画

- [ ] C-1 (出力の visible paths) は落とす。C-2 (fixture と実データの対) は順位 514 へ統合済み (2026-09-08 に出口を再設計。dev-conventions.md へは書かない)
- [ ] ADR-072 に E-1 を追記 (決定 19 の実装節が適切)
- [ ] `pnpm lint:docs` + markdownlint clean

#### 完了基準

- E-1 (`skipped` は `success` ではない) が ADR-072 に記録され、コードコメントにしか無い状態が解消している。C-1 は却下、C-2 は順位 514 へ統合した。

#### 詰まっている箇所

なし

### 順位 474: 夜間 auto lane とユーザー割当 PR の同一ファイル競合を自動検知する (系統 G)

> 元: `docs/todo24.md` (2026-11-01 移送)
>
> **動機**: PR D の着手前に、台帳・作業計画書・実装の 3 箇所を人手で横断確認する必要が生じた
> (2026-08-19)。台帳の順位 176 (auto lane) が PR D と同じ `src/check-ci-coderabbit/src/rate_limit.rs` を
> 触る想定だったため。結果的に PR D はそのファイルを触らず競合しなかったが、**それを判定するのに
> 手作業の横断確認が要った**。
>
> [ADR-074](../adr/adr-074-auto-lane-screening-criteria.md) は lane 割当の判断基準を定めるが、
> **同一ファイルの並行競合検知は範囲外**で重複しない。
>
> **参照**: `.claude/feedback-reports/424.md` Tier2 #1、`lib_ledger::select` (文書順で 1 件選択)
>
> **種類**: Tier 1 — 同一ファイル競合の自動検知。工数 S。

#### 作業計画

- [ ] 検知の入力を決める (台帳の対象ファイル欄 × open PR の changed files)
- [ ] 実装先を決める (`nightly-todo.yml` の step か `cli-nightly-task-select` の拡張か)
- [ ] 競合時の挙動を決める (選定から除外するか、warning に留めるか)

#### 完了基準

- auto lane の選定が、ユーザー割当 PR と同一ファイルを触る順位を**機械的に検知し、選定から除外するか警告として報告する**こと (どちらを採るかは作業計画で決める。warning に留める場合も、検知結果が run log から読めること)

> 詳細エントリの続き (順位 506 以降と、節ごと移した 2 節) は [nightly-loop2.md](nightly-loop2.md) にある。
