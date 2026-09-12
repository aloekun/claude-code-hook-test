# 開発 convention / チェックリスト

> CLAUDE.md (ADR index) から分離した運用 convention・チェックリスト集。index の肥大化を避けつつ、セッション横断で参照する軽量ガイドを集約する (ADR-022 の責務分離)。
>
> **本ファイルは縮小方向で運用し、新規の節は追加しない。** 決定事項は ADR、それ以外は仕組みで担保する ([ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md))。
>
> **各節は冒頭に `機械化:` / `機械化不能:` / `機械化予定:` の宣言を持つ。** `pnpm lint:docs` の `convention-declaration` 検査が fail-closed で強制する (順位 515)。節を書くたびに「機械化できるのか、できないならなぜか」を明示させることで、判断を経ずにルールだけが増える経路を閉じている。
>
> **縮小の終点**: 機械化できる規約が機構へ移り、残りが判断を要するものだけになった時点で、縮小は完了とする。**節数をさらに減らすことを目的にしない。** `機械化不能:` の節は機構にできない知識であり、それを機械化しようとして生まれる作業は解くべき問題を持たない (決定と経緯は [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) § 改訂 2026-09-12)。ファイル自体の廃止は、`機械化不能:` と `機械化予定:` の節がともに 0 になった場合にだけ検討する — それは目標ではなく結果である。

## 機構への索引

機械化: 下表の各機構。

**本節は索引であって、守らせる対象ではない。** 規約の中身は機構側が持つ — custom lint rule なら `why` / `message` / `fix.steps`、検査スクリプトなら module コメントと検査メッセージである。ここへ要旨や由来を書き写すと、片方だけが古くなる ([同一事実が複数箇所に分散する場合の変更手順](#同一事実が複数箇所に分散する場合の変更手順) がまさにその失敗)。

| 規約 | 強制している機構 | 中身の在り処 |
|---|---|---|
| 外部 exe を spawn する統合テストは無期限に待たない | custom lint rule `no-unbounded-child-wait` | [.claude/custom-lint-rules.toml](../.claude/custom-lint-rules.toml) |
| GitHub Actions の `run:` は常に `-e` 付きで起動する | `pnpm lint:workflows` の契約検査 3 | [scripts/lint-workflows-run-blocks.mjs](../scripts/lint-workflows-run-blocks.mjs) |
| takt facet の出力言語は各 instruction に直書きする | `pnpm lint:takt-facets` | [scripts/lint-takt-facets.mjs](../scripts/lint-takt-facets.mjs) |

## spike / 実験タスクの見送り (negative result) 永続化 convention (順位261)

機械化不能: 「spike を見送った」という判断の発生自体を機械が検知できない (ADR-042 Step 1)。決定の有無は人の頭の中にあり、コードにも diff にも痕跡が出ない。

spike・実験タスクを見送る (採用しない) と判断したときは、negative result の知見が散逸しないよう以下の **3 点セット** を必ず実施する:

1. **ADR に結論と実測根拠を記録** — 見送り判断・数値根拠・比較対象を該当 ADR (新規 or amendment) に永続化する。「なぜ見送ったか」を後続セッションが再構築できる粒度で書く。
2. **計画文書の状態列を更新** — 該当タスクの計画文書の状態を「見送り / 却下」に更新し、宙吊りの検討を残さない。
3. **再評価トリガー付き follow-up を Tier 5 todo 化** — 「どういう条件が変われば再評価するか」(新モデル出現 / プロンプト改善 / GPU 更新 等) を明示した follow-up を登録する。恒久見送りではなく「現時点では見送り」を表現する。

**確立事例** (2 例で成立): WP-01 → [ADR-046](adr/adr-046-local-llm-review-spike.md) / WP-04 → [ADR-038](adr/adr-038-local-llm-finding-classification.md) § classify モデル格上げの評価と見送り。

## 外部 fixture 参照テストは値まで assert (順位274)

機械化不能: fixture ごとにスキーマが異なり、「どの値がテストの前提か」は regex でも AST でも判定できない (ADR-042 Step 1)。

テストが外部ファイル (実 config / 共有 fixture 等) を参照する場合、「section / キーの存在」だけでなく **テストの前提とする具体値まで assert** する:

1. **存在チェックだけでは silent break する** — 「section がある」だけを assert すると、外部ファイル側で値が変わってもテストは緑のまま、前提の乖離が別テストの原因の見えない失敗として遅れて表面化する。
2. **値ずれ時に更新箇所を指し示す** — assert メッセージに「この値を変えたらどのテストの期待値を更新すべきか」を明記する。

**由来** (PR #261 T3-#2): `hooks-stop-tool-call-leak` の E2E が `[stop_tool_call_leak]` section の存在しか assert しておらず、`enabled` / `max_consecutive_blocks` の値変更が cap 境界テストを原因の見えない形で silent break させるリスクを 3 ソースが独立指摘した。関連する test isolation の一般原則は [ADR-041](adr/adr-041-test-isolation-patterns.md) を参照。実装例は `src/hooks-stop-tool-call-leak/tests/e2e.rs`。

## jj: ファイル編集を始める前に `jj new` する

機械化不能: 「`@` が前ターン以前に確定した別作業のコミットか」はセッション文脈に依存し、hook から判定できない (ADR-042 Step 1)。`jj new` 直後の `@` も description を持つため、状態だけでは区別がつかない。セッション開始時の change_id を記録する案の実測と見送りは [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) § 改訂 2026-09-12 が持つ。

**別作業で作られた既存コミットが `@` の状態でファイルを編集しない。** 編集を始める前に `jj new -m "wip: <内容>"` でそのターンの作業コミットを作る。

理由: jj は working copy をそのままコミットへ反映するため、既存コミットが `@` のままだと編集内容がそのコミットへ吸収される。その後 `jj describe` を実行すると**そのコミットのメッセージが上書きされ**、無関係な変更が既存コミットへ混入した状態で push されうる。

**由来** (2026-08-02 WP-17 PR 2 の実装セッション): 同一セッション中に 3 回発生した。

## LLM を含む自動化経路は実走でしか検証できない (ADR-067)

機械化不能: 「その API 呼び出しが実際に何を返すか」「agent が実際に何を読めるか」は静的検査の定義域の外にある (ADR-042 Step 1)。実走の要否を判定する検査を書いても、判定自体が同じ壁に当たる。

LLM を step に含む workflow を**新規に組んだとき、および既存経路の LLM step を追加・変更したとき**は、静的検査の通過を完了条件にしない。実走スモークを必須の受け入れ基準として設計する。

1. **静的検査は「LLM がいる経路」を素通りする** — ADR-067 段 2 で検出した 3 件はすべて pre-push simplicity / security review・CodeRabbit・js-yaml 構文検証の 4 種を通過していた。
2. **設計文書に書かれた修正方針も検査対象である** — ADR-067 § 残課題に書いた修正方針自体が誤りで、実装時の pre-push security review が REJECT した。
3. **反復は ref 指定の dispatch で行い、マージを検証の前提にしない** — `workflow_dispatch` は ref を選べる。
4. **最初の失敗で停止する経路では、1 回の実走で見つかるバグは高々 1 個** — n 個のバグには n 回の実走が要る。並列実行や `continue-on-error` の経路では当てはまらないので、**対象経路の停止条件を確認してから見積もる**。

実測記録は [ADR-067](adr/adr-067-phase-b-unattended-fix-push.md) § 検証記録 が持つ。

## Rust ファイル分割の制約条件

機械化不能: 「behavior 不変の機械的な移動か」「その `pub(crate)` が妥当か」は diff の意味を読む必要があり、regex / AST では判定できない (ADR-042 Step 1)。関数長 50 行・ファイル長 800 行・非 doc コメント禁止は既に機構が強制しているため、本節からは外した。項目 1 の test count 一致を機構化しない判断は [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) § 改訂 2026-09-12 が持つ。

800 行超 `.rs` file の module 分割に適用する制約:

1. **behavior 不変** — 関数 signature 変更 / field rename / default 値変更をしない。機械的な移動 + visibility 調整のみ。test count も分割前後で一致させる (着手前に master HEAD で baseline を測定)。
2. **Cross-module visibility は `pub(crate)`** — `pub` は crate 外公開を意味するため使わない。
3. **test helper は per-module duplicate** — `unique_temp_root` 等の test helper は共有 module を抽出せず、各 test module に独立 copy する。共有 test util module は anti-pattern。

並列 PR で `Cargo.lock` が競合したときの rebase 手順は [ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) § 調整ポイント 2 が持つ。`PR_SIZE_CHECK_OVERRIDE` / `FILE_LENGTH_CHECK_OVERRIDE` の正当な use case は [ADR-069](adr/adr-069-pr-chain-declaration.md) § 2 と `.claude/hooks-config.toml` の該当 section が持つ。

## 同一事実が複数箇所に分散する場合の変更手順

機械化予定: 順位 445 (todo preamble ⇄ facet routing 記述の集合比較 lint)。routing 系が機械側へ移った時点で本節の該当部分を撤去する。それ以外の分散 (閾値・設定値) は対象が定まらず、現時点では検査を書けない。

1 つの事実 (設定値・routing ポインタ・閾値等) が複数箇所に書かれている場合、変更時は**全箇所を 1 つの PR で揃える**。片方だけ直すと、残った側が「古い前提」を語り続け、後続のレビューを誤誘導する。

1. **変更前に反映先を数え上げる** — `grep` で当該の値・ポインタを含む全ファイルを洗い出し、PR 内で反映先を列挙する。
2. **「暫定措置」と書いた記述は、恒久化した時点で必ず書き換える** — 条件付き記述を残したまま条件が消えると、レビュアーが毎回「未文書化の暫定措置」として誤検出する。
3. **反映先リストを ADR 側に残す** — 次に変更する人が数え上げからやり直さずに済む。
4. **記述を撤去するときは、その記述を指す参照も同時に消す** — 節を消して参照を残すと、次に読む人が存在しない節を探す。撤去前に `grep` で `§ <節名>` を洗う。

**由来** (2026-08-13、PR #395 / #396 の post-merge feedback で独立に 2 件観測): (a) preamble の routing 更新に対し facet 側の固定値が取り残された。(b) weekly reminder の閾値 7 日が 4 箇所に分散し、うち 3 箇所が古い前提のままで週次レビューが誤検出した。項目 4 は 2026-09-11 に本ファイル自身で踏んだ (PR #492 で節末尾の注意書きを消し、それを指す台帳エントリが宙に浮いた)。

## 複合タスクの仕様には各項目の処置と除外根拠を書く

機械化不能: 「仕様が挙げた対象」と「実装が扱った対象」の対応判定は自然言語の意味理解を要する (ADR-042 Step 1)。数が合うかだけを見る検査は、意図的な絞り込みと見落としを区別できない。

複数行 / 複数ファイル / 複数バッチにまたがるタスクを `docs/todo*.md` に書くときは、**対象として挙げた全項目について処置 (実施 / 除外) と、除外する場合の根拠**を仕様側に明記する。

1. **仕様側の対象数と実装側の対象数がずれる** — 根拠が書かれていなければレビューでは「意図的な絞り込み」と「見落とし」を区別できない。
2. **除外は消さずに残す** — 除外した項目を仕様から削ると、次に読む人が「なぜこれは対象外なのか」を再調査することになる。

**由来** (2026-08-13 PR #395、PR diff + pre-push simplicity の 2 ソースが独立指摘): 5 行を降格対象と記述する一方、実装タスクは 4 行のみを扱い、1 行の処置が仕様にも実装にも現れない状態だった。
