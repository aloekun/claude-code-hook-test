# TODO (Part 21)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo20.md がファイルサイズ約 56KB (50KB 安定読み取り閾値超過) に到達したため、新規エントリは本ファイルに記録する (2026-08-08 WP-18 セッションの docs バッチで新設)。**新規エントリの追加先は本ファイル**。todo.md / todo2.md 〜 todo20.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## #364〜#370 セッションで実測した自動化経路の運用問題 (2026-08-08 登録)

> WP-18 の残作業を PR 化する過程 (#364〜#370) で、**PR 監視・自動 fix 経路が後続の決定論コマンドを妨げる**事象を複数実測した。いずれも post-merge feedback には含まれない — feedback は PR の diff とレビュー指摘を入力とするため、**ツール自身の運用中に起きた事象は構造的に拾えない**。この非対称そのものが記録に値する。
>
> いずれも「自動化が別の自動化の前提を崩す」型で、[ADR-022](adr/adr-022-automation-responsibility-separation.md) の責務分離が扱う領域にあたる。

### cli-pr-monitor の lock に liveness check が無く、プロセス死後も最大 30 分監視が skip される

> **動機**: 2026-08-08 の #364 監視中、`cli-pr-monitor` を停止した際に子プロセスが死んで `.claude/pr-monitor.lock` が残り、以降の監視呼び出しがすべて `[lock] 別の cli-pr-monitor が走行中 (pid=42864, age=1242s)、本セッションは skip` で素通りした。**pid 42864 は既に終了していた** (`Get-Process -Id 42864` で確認)。
>
> **実装確認済み** ([lock.rs:27](../src/cli-pr-monitor/src/lock.rs#L27)): stale 判定は `DEFAULT_STALE_THRESHOLD_SECS = 1800` の**経過時間のみ**で、lock に記録された `pid` の生存確認をしていない。したがって holder が crash / kill された場合、次のインスタンスが takeover できるまで**最大 30 分**かかる。
>
> **これは設計どおりの挙動でもある** — [lock.rs](../src/cli-pr-monitor/src/lock.rs) の module doc は「プロセス crash 時は file が残るが、stale 判定で threshold 経過後に次インスタンスが takeover できる」と明記しており、既知のトレードオフとして書かれている。したがって本エントリは**バグ報告ではなく、liveness check を足して復帰窓を 30 分から実質 0 へ縮める価値があるかの判断**である。
>
> **対処案**: takeover 判定に pid の生存確認を追加する。ただし (a) pid 再利用の誤判定、(b) Windows / Linux での実装差、(c) 既存の TOCTOU 設計判断 (順位 301 が「cli-pr-monitor/lock.rs は設計判断済みのため scope 除外必須」と明記) との整合、の 3 点を先に検討すること。**「不要」判断も正規の出口** — 影響は interactive セッション中の監視遅延に限られ、GitHub Actions 経路の pr-monitor は別プロセス・別マシンなので影響を受けない。
>
> **参照**: [lock.rs](../src/cli-pr-monitor/src/lock.rs)、順位 301 / 303 (同ファイルの lock 設計に関する既存エントリ)、[ADR-043](adr/adr-043-security-gates-fail-closed.md) (助言層は fail-open が正しい — 本 lock は助言層)。
>
> **実行優先度**: 🔧 Tier 3 — Severity Low (復帰窓 30 分。無人経路は影響なし) / Frequency Low / Effort S / Adoption Risk Low (pid 再利用の誤判定を入れると逆に壊す)。

#### 作業計画

- [ ] pid 生存確認の要否を判断する (不要なら理由を lock.rs の module doc へ追記して閉じる)
- [ ] 入れる場合、pid 再利用対策 (start_time との併用) と OS 差の吸収を設計する
- [ ] 順位 301 / 303 の既存判断と衝突しないことを確認する

#### 完了基準

- 採用・不採用のいずれかが根拠つきで `lock.rs` の module doc または ADR に記録されていること。

### 監視・自動 fix 経路が積む空コミットで bookmark がずれ、`pnpm merge-pr` / `pnpm push` が失敗する

> **動機**: #364 のマージで `pnpm merge-pr` が `エラー: 現在のブックマークに紐づく PR が見つかりません` で exit 1 した。原因は **PR 監視・自動 fix 経路が jj ツリーへ空コミットを積み、bookmark が探索範囲の外へ出たこと**。本セッション (#364〜#370) で**同型を計 7 回観測**した。復旧は毎回 `jj edit <bookmark>` で足りたが、無人度が上がるほど人手の介入が要る方向に効く。
>
> **実測した内訳**:
>
> 1. `detect_pr_number()` の 1 段目 `gh pr view` は、**jj 併用リポジトリでは原理的に成立しない** — `.git/HEAD` は常に detached (実測時は空コミットの生 commit id が入っていた) ため `could not determine current branch` になる
> 2. 2 段目のフォールバック `get_jj_bookmarks()` は [`BOOKMARK_SEARCH_REVSETS = ["@", "@-", "@--"]`](../src/lib-jj-helpers/src/bookmarks.rs#L25) の **3 段しか遡らない**。失敗時に bookmark はそれより深くにあり、空の Vec が返って 2 段目も空振りする
> 3. **push 経路でも別の顔で出る**: `pnpm push` は「bookmark を `@` (または `@-`) に自動更新」する。@ が監視経路の作った空コミットだと、bookmark がその空コミットへ移り `Won't push commit ... since it has no description` で失敗する (#370 で実測)。ここから `jj bookmark set` の後退拒否・`jj abandon` によるブックマーク消失と復旧がこじれた
>
> **生成元はほぼ確定した**: 空コミットを積むのは **PR 監視 (`cli-pr-monitor --monitor-only`) と自動 fix 経路**である。監視は呼ばれるたびに working-copy を進め、自動 fix は `fix(review): apply CodeRabbit fixes for #NNN` コミット + 前後の空コミットを作る。#364 の記述にあった「生成元未特定」は本セッションの反復観測で解消した。
>
> **対処案** (いずれか、または組み合わせ):
>
> - `BOOKMARK_SEARCH_REVSETS` の段数を増やす — 対症療法。何段積まれるかに上限が無いので根治しない
> - **探索を深さ非依存の revset へ変える** (`heads(::@ & bookmarks())` = @ から祖先方向の bookmark 付きコミット) — 3 クレート共有の `lib-jj-helpers` を触るため 3 クレート (push-runner / pr-monitor / merge-pipeline) 全てで回帰確認が要る。**本命**。ただし `heads(...)` は @ に複数 bookmark が付くと**複数コミットを返す**ため、clone の `--head` / `-b` や PR 番号選択が複数対象にならないよう、trunk 系を除いた単一 bookmark へ絞る (現行 `BOOKMARK_SEARCH_REVSETS` の `is_trunk_bookmark` 除外と同じ規律) か、返り値を最初の 1 件に制限する必要がある
> - push-runner の「bookmark を `@-` に自動更新」を、空コミットを飛ばして直近の非空 bookmark commit へ寄せる
> - 監視・自動 fix 経路が**空コミットを積まない / 積んだら片付ける** ([ADR-022](adr/adr-022-automation-responsibility-separation.md) の責務分離としてはこちらが筋)
>
> **参照**: [github.rs:136](../src/cli-merge-pipeline/src/github.rs#L136) (`detect_pr_number`。順位 397 でリモート追跡 bookmark へのフォールバックと `--pr` を追加済み。本順位が扱う「探索 revset より深い位置」の問題は未解決)、[bookmarks.rs:25](../src/lib-jj-helpers/src/bookmarks.rs#L25) (`BOOKMARK_SEARCH_REVSETS`)、[ADR-013](adr/adr-013-merge-pipeline.md)、[ADR-021](adr/adr-021-jj-change-detection-principles.md) (jj 変更検出の設計原則)、[ADR-024](adr/adr-024-shared-jj-helpers-library.md) (共有ヘルパーの変更は 3 クレートに効く)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (マージ / push 経路が止まる。ただし loud failure で復旧手順も短い) / Frequency High (自動 fix が走った PR で毎回・本セッションで 7 回) / Effort M / Adoption Risk Low。

#### 作業計画

- [ ] 深さ非依存 revset (`heads(::@ & bookmarks())` 等) を採り、`lib-jj-helpers` を触る 3 クレートで回帰を確認する
- [ ] push-runner の「@- 自動更新」も空コミットを飛ばす形へ揃える
- [ ] 自動 fix が走った PR を模した回帰テスト (bookmark が @--- 以深) を追加する

#### 完了基準

- bookmark が 3 段より深い位置にある状態で `pnpm merge-pr` / `pnpm push` が正しく PR / bookmark を解決できること、またはその状態自体が発生しなくなること。どちらを採ったかが根拠つきで記録されていること。

### 自動 fix 経路は push が BLOCK されてもローカル作業コピーを書き換える

> **動機**: #366 で自動 fix の push が scope guard (ADR-054) に BLOCK されたが、**ローカルの workflow ファイルは fix 版に書き換わっており**、`fix(review): apply CodeRabbit fixes for #366` コミットが作業コピーに残っていた。気づけたのは指摘検証のため偶然そのファイルを読んだからで、**気づかなければ次の作業で意図しない変更を混入させていた**。
>
> **問題の型**: 「push は止めたので安全」と「ローカル状態は変えていない」は**別**である。scope guard / gate は外向きの副作用 (push) を止めるが、その手前で自動 fix 経路が作った**ローカルのコミットと working-copy 変更は残る**。#369 / #370 でも自動 fix コミットがローカルに現れ、bookmark がそこへ移っていた (順位 386 と同じ経路)。
>
> **対処案**: 自動 fix / 監視経路が BLOCK・失敗で終わった場合に、(a) 作った fix コミットと空コミットをロールバックする、または (b) 少なくとも「ローカルに未 push の自動生成コミットが残っている」と警告する。どちらも [ADR-022](adr/adr-022-automation-responsibility-separation.md) の「自動化コンポーネントは自分の副作用を後始末する」責務にあたる。
>
> **参照**: [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) (scope guard)、[ADR-068](adr/adr-068-fix-step-authority-boundary.md) (fix step の権限境界)、順位 386 (同経路の bookmark ずれ)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (意図しない変更の混入。ただし loud failure と併発するため気づきやすい) / Frequency Medium (BLOCK / 不完全 fix のたび) / Effort M / Adoption Risk Low。

#### 作業計画

- [ ] 自動 fix / 監視経路の終了パスを洗い、BLOCK・失敗時にローカル副作用が残る箇所を特定する
- [ ] ロールバックか警告のどちらを採るか決め、実装する
- [ ] 順位 386 の bookmark ずれ対処と同一 PR で扱えるか検討する

#### 完了基準

- 自動 fix が BLOCK / 失敗で終わった後、ローカルに未 push の自動生成コミットが残らない、または残ることが明示的に警告されること。

### post-merge-feedback の完了判定が書き込みと race し、成功した feedback を failed marker にする

> **動機**: #367 のマージ後、post-merge-feedback の再実行は**成功していた**が、マーカーが「report 不在」で failed 扱いになっていた (`takt 成功扱いだが report 不在: feedback-report.md が見つかりません`)。実際にはレポート実体は takt の run ディレクトリ (`.takt/runs/*/reports/feedback-report.md`) に**存在しており**、`.claude/feedback-reports/367.md` へ手動コピーしてマーカーを外した。
>
> **問題の型 (要特定)**: `reconcile_takt_output` → `copy_feedback_report` が **`find_latest_run_dir` で選んだ run dir の `reports/feedback-report.md`** をコピーし、無ければ「report 不在」で marker を残す ([mod.rs:147](../src/cli-merge-pipeline/src/feedback/mod.rs#L147) / [takt.rs:84](../src/cli-merge-pipeline/src/feedback/takt.rs#L84))。takt は exit 0 なのに不在になったので、**単純な write race と断定せず**、まず転送順序と契機を特定する。候補は (a) `find_latest_run_dir` が report 書き込み前の run dir を選んだ、(b) 別の新しい run dir を掴んだ (latest 特定のずれ)、(c) source パスの不一致。#367 では実体が run dir に**存在した**ので、copy 実行時点と report 完成時点の前後関係の問題である可能性が高い。
>
> **対処案** (機序特定後): (a) `copy_feedback_report` の source が無いとき短い retry / 待機を入れる、(b) run dir 特定を「report が存在する最新 run」に絞る、(c) takt の完了と report 書き込みを同期させる。ADR-030 (決定論的 post-merge feedback) の marker 設計に属する。
>
> **参照**: [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) (marker + recovery 設計)、[cli-merge-pipeline](../src/cli-merge-pipeline/src/feedback/) (feedback step)。
>
> **実行優先度**: 🔧 Tier 3 — Severity Low (report は run dir に残るため手動回収可・データ損失なし) / Frequency Low (2 回のうち 1 回) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] report 不在判定の前に takt run ディレクトリの `reports/feedback-report.md` を確認するか、retry を入れる
- [ ] 誤 failed marker が出た場合の回収を自動化するか判断する

#### 完了基準

- takt が成功し report が run dir に存在する場合に failed marker が出ないこと。

---

## #369/#370 post-merge feedback 採用分 (2026-08-09 登録)

> WP-18 の prompt injection 対策 PR (#369 / #370) の post-merge feedback が挙げた採用候補のうち、セッション中に未対応で価値の高い 3 件をユーザー承認 (2026-08-09) のうえ登録する。narrow-fix 教訓 (#369/#370 T3) は順位 375 の 5 項目目へ取り込んだため本節には立てない。

### `Write(path)` tool-scope 指定子の no-op を検出する settings validator

> **動機**: `--allowedTools` / `--disallowedTools` および settings.json の permission で、**`Write(path)` 指定子はファイル権限チェックにマッチせず no-op** である (CLI 2.1.218 で実測、`Edit(path)` が Write を含む全編集ツールをカバー)。順位 379 の実装で `Write(work/**)` / `Write(master-ref/**)` を並べており、**deny のつもりの設定が黙って無効化される silent security failure** になっていた。CLI 自身は警告を出すが、CI ログに埋もれて気づけない。
>
> **対処案**: `.claude/settings*.json` と workflow の `claude_args` を検査し、permission rule に `Write(...)` 指定子が現れたら **error (必須 CI check の失敗)** にする。**warning では不十分** — silent security failure（deny の無効化）は [ADR-043](adr/adr-043-security-gates-fail-closed.md) の fail-closed 対象であり、検知しても CI が通ってマージできる状態は穴を残す。ADR-007 の regex 層で足りる (AST 不要)。あわせて [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) / ADR-072 決定 12 に「ファイル編集 scope は Edit(path) で表す」事実を追記済みかを確認する。
>
> **参照**: `.claude/feedback-reports/369.md` Tier 1 #2、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 12 (Write no-op の実測記録)、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md) (regex 層)。
>
> **実行優先度**: 🚀 Tier 1 — Severity High (deny の silent 無効化) / Frequency Low / Effort M / Adoption Risk Low (派生プロジェクト deploy のみ)。

#### 作業計画

- [ ] `Write(...)` 指定子を検出する regex 層ルールを custom-lint-rules.toml に追加
- [ ] `.claude/settings*.json` と `.github/workflows/*.yml` の `claude_args` を検査対象に含める
- [ ] 現行リポジトリで false positive が出ないことを確認 (Edit/Read 指定子は許可)

#### 完了基準

- `Write(path)` 指定子を含む設定が検知され、**必須 CI check が失敗する**こと (warning 止まりにしない)。
- `Edit(path)` / `Read(path)` は検知されないこと。

### 台帳 framing 区切りの定数と workflow リテラルの cross-file 一致を CI で検証

> **動機**: [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 13 の framing は、`ledger.rs` の `LEDGER_DATA_FRAME_MARKER` と `.github/workflows/nightly-todo.yml` の `===BEGIN/END_LEDGER_DATA===` が**対**になって初めて成立する。片方だけ変えると framing が破れる (agent プロンプトの信頼境界が開く) が、現状は doc comment の相互参照だけで機械検証が無い。[ADR-069](adr/adr-069-pr-chain-declaration.md) 決定 7 の sha256 gate-asset check と同格の cross-file 不変量。
>
> **対処案**: 両ファイルから定数 / リテラルを抽出して一致を assert する CI テストを追加する。AST 層 custom linter は overkill で、単純な constant-compare test (Effort M) で同等効果。
>
> **参照**: `.claude/feedback-reports/369.md` Tier 2 #1、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 13、[ADR-051](adr/adr-051-cross-system-config-coupling.md) (cross-system coupling の機械検証)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (framing の片側破れ) / Frequency Low / Effort M / Adoption Risk None。

#### 作業計画

- [ ] `ledger.rs` の `LEDGER_DATA_FRAME_MARKER` と workflow の区切りリテラルを抽出・比較するテストを新設
- [ ] 片方を変えるとテストが落ちることを確認

#### 完了基準

- 定数とリテラルが不一致になる変更が入るとテストが落ちること。

### jj の落とし穴 (squash の方向・空コミットでの bookmark ずれ) を dev-conventions へ

> **動機**: 本セッション (#364〜#371) で jj 運用の落とし穴を繰り返し踏んだ。(a) `jj squash --into <bookmark>` の方向が直感と逆で、ターゲットが description なしの新コミットへ移動する / (b) `jj bookmark set` の後退拒否と `jj abandon` による bookmark 消失・復旧 / (c) `jj new master` を「コミット確定」のつもりで実行して変更を前コミットに取り残す。いずれも復旧に op log 参照が要った。memory には別の jj squash gotcha (headless editor hang) が既に記録済みで、jj 運用の落とし穴は systemic に再発している。
>
> **対処案**: [dev-conventions.md](dev-conventions.md) に「jj 運用の落とし穴と復旧」チェックリストを 1 本追加する。**コミット確定は `jj describe` + `jj bookmark set/create`、`jj new` は新しい作業を始めるときだけ**、`jj squash` は方向を確認、bookmark がずれたら `jj edit <bookmark>` で戻す、を明文化。順位 386 (空コミットでの bookmark ずれ) の機構側対処とは別に、運用ルール側で人間 / agent を守る。
>
> **参照**: `.claude/feedback-reports/369.md` Tier 3 #1、memory `jj-squash-editor-hang-headless` / `jj-concurrent-session-op-divergence`、順位 386 (機構側対処)、[ADR-021](adr/adr-021-jj-change-detection-principles.md)。
>
> **実行優先度**: 🔧 Tier 3 — Severity Medium (作業の取り残し・bookmark 消失。ただし loud で復旧可) / Frequency High (本セッションで複数回) / Effort S / Adoption Risk None (docs-only)。

#### 作業計画

- [ ] `docs/dev-conventions.md` に「jj 運用の落とし穴と復旧」チェックリストを追記 (由来セッション付き)
- [ ] 既存 memory (squash hang / op divergence) と重複せず補完する形にする
- [ ] **操作例は再現可能な最小の初期状態つきで書く** (#372 CodeRabbit 指摘)。`jj squash --into` / `jj new master` の結果は jj バージョン・git リモートの有無・コミットグラフ・bookmark 位置・ワークツリー状態に依存するため、「@ が bookmark より N 段先の空コミットにある」等、読者が実際に再現・検証できる前提条件を明記する。断定形の「必ずこうなる」ではなく前提つきで書く

#### 完了基準

- コミット確定・squash 方向・bookmark ずれ復旧の 3 点が、**再現可能な初期状態つきで**根拠を添えて dev-conventions に存在すること。

---

## WP-18 失敗頻度分析の follow-up (2026-08-09 登録)

### push パイプラインの terminal outcome を telemetry へ記録し、失敗回数・原因を機械集計可能にする

> **動機**: 2026-08-09 の WP-18 失敗頻度分析で、**パイプライン失敗の回数・原因に構造化記録が無い**ことが判明した。定量は `.takt/runs/` のディレクトリ数からの推定に頼り (pre-push run 数 ÷ マージ PR 数 = 08-07 は 18 run/1 PR でベースライン 1.9 の約 9 倍)、失敗の内訳 (レビュー REJECT / quality gate / jj push 段の bookmark 解決失敗など) は trace.md と todo エントリの突き合わせでしか復元できなかった。[ADR-055](adr/adr-055-firing-telemetry-collection.md) の telemetry は rule/preset/hook の**発火**のみを記録し、パイプラインの**結末**は対象外。
>
> **対処案**: push-runner の terminal outcome (成功 / 失敗した stage + reason code) を telemetry へ 1 行追記する。reason は自由文ではなく **stage + 機械可読 code** (例: `review-reject` / `quality-gate` / `jj-push-bookmark-resolution` / `regate`) にする。集計は `cli-telemetry-report` (ADR-062 月次レビューの決定論 exe) へ載せ、月次で「失敗率と内訳の推移」を読める形にする。順位 386/387/376 の対処が入った後の**効果測定**と、未知の失敗モードの早期発見が主目的。merge-pipeline / pr-monitor への同型展開は push-runner で型が固まってから検討する。
>
> **設計上の注意**: (a) telemetry は助言層なので **fail-open** — 記録の失敗がパイプライン本体の exit code を変えてはならない ([ADR-043](adr/adr-043-security-gates-fail-closed.md))。(b) ADR-055 の firing event schema に outcome event を混ぜるか別ファイルにするかは、既存 `firings-*.jsonl` の後方互換 (cli-telemetry-report のパーサ) を確認して決める。
>
> **参照**: [ADR-055](adr/adr-055-firing-telemetry-collection.md)、[ADR-062](adr/adr-062-monthly-harness-roi-review.md)、[ADR-015](adr/adr-015-push-runner-takt-migration.md) (push-runner)、順位 386/387/376 (効果測定の対象となる再発防止策)。
>
> **実行優先度**: 🔧 Tier 3 — Severity Low (観測の欠落であり機能障害ではない) / Frequency Medium (push のたびに記録機会) / Effort M / Adoption Risk Low (fail-open を守る限り本体に影響しない)。順位 386/387 の対処より先に入れると効果測定のベースラインが取れる点は考慮に値する。

#### 作業計画

- [ ] push-runner の terminal 出口 (成功 / 各失敗段) を洗い出し、stage + reason code の一覧を定義する
- [ ] ADR-055 の firing event と同居させるか別ファイルにするかを、`cli-telemetry-report` パーサの後方互換を確認して決める
- [ ] outcome 記録を実装し、記録失敗がパイプラインの exit code を変えないことをテストで固定する
- [ ] `cli-telemetry-report` に期間指定の失敗集計 (回数・stage 別内訳) を追加する

#### 完了基準

- `pnpm push` の各試行が terminal outcome (成功 / 失敗 stage + reason code) を機械可読で残すこと。
- `cli-telemetry-report` で月次レビューが失敗率・内訳を読めること。
- telemetry 書き込み失敗時もパイプライン本体の挙動・exit code が変わらないこと (fail-open のテストで固定)。

---

## 夜間ループの draft 廃止とレビュー起動の是正 (2026-08-09 登録)

> **由来**: 2026-08-09 に PR [#373](https://github.com/aloekun/claude-code-hook-test/pull/373) で実測した 2 件の観測から、ユーザー判断 (同日) を経て方針を確定したもの。**3 件は 1 本の根から出ている** — 夜間 PR を draft にしたことが CodeRabbit の自動レビュー対象外を招き、その回避策 ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11) が bot 投稿の無視で不成立になった。
>
> **順位 393 (記録の是正) と 394 (構造の是正) は実装済み・削除済み** (2026-08-09、PR [#376](https://github.com/aloekun/claude-code-hook-test/pull/376))。撤回記録は [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11 の撤回ブロック、停止点変更は同 決定 15、背圧の指標改訂は [ADR-071](adr/adr-071-draft-pr-backpressure.md)、分類表の本体改訂は [ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md) 原則 2 が正。**実走確認 (夜間 PR に CodeRabbit の初回自動レビューが付くこと) だけが残り**、計画書 WP-18 の残作業表と ADR-072 § 実走スモークが追跡する。
>
> **順位 395 (週次レビューでの浮きブランチ検出) も実装済み・削除済み** (2026-08-09)。`cli-stale-branch-scan` として実装し、`pnpm stale-branch-scan` で実行する。設計は [ADR-031](adr/adr-031-weekly-review-pipeline.md) § 残存ブランチ検出 が正 — **takt workflow はネットワークを持たない** (`network_access: false`) ため決定論 scan を skill 側 (L3) に置いた経緯もそちらに記録した。

## CI 安定性 (2026-08-09 登録)

### hooks smoke suite の並列実行が Linux で `ETXTBSY` を起こす

> **由来**: PR [#376](https://github.com/aloekun/claude-code-hook-test/pull/376) の CI で `rust (ubuntu-latest)` が失敗した。`windows-latest` は成功、変更は当該クレートを 1 ファイルも触っていない。
>
> ```text
> test malformed_stdin_does_not_block ... FAILED
> panicked at src/hooks-pre-tool-validate/tests/smoke.rs:137:
>   spawn hooks-pre-tool-validate: Os { code: 26, kind: ExecutableFileBusy,
>                                       message: "Text file busy" }
> ```
>
> **機構**: [smoke.rs](../src/hooks-pre-tool-validate/tests/smoke.rs) の 2 テストは cargo 既定で並列実行される。各テストは `stage_hook()` で exe を tempdir へ `fs::copy` してから spawn する。片方の `fs::copy` が**書き込み用 fd を開いている最中に**、もう片方の `Command::spawn()` が fork すると、子プロセスがその fd を継承する。copy 側が fd を閉じても fork された子が exec するまで複製が残るため、copy 側の exec が `ETXTBSY` で落ちる。Linux 固有 (Windows では再現しない)。
>
> **頻度**: 直近 15 回の `ci.yml` run で初出。恒常的ではないが、**両 OS matrix (ADR-065) が意味を持つのは CI が信頼できるときだけ**で、原因不明の赤が続くと「また flake だろう」と実バグを見落とす経路になる。
>
> **対処案** (実装時に判断):
>
> - (a) `ci.yml` の hooks smoke step を `--test-threads=1` にする — 最小・即効だが、他 suite の並列性は保たれるので損失は小さい
> - (b) `run_hook` の spawn を `ETXTBSY` でリトライする — 根本に近いが、テストコードに retry を持ち込む
> - (c) staging をやめて `built_exe()` を直接起動する — copy 自体が消えるが、config を tempdir に staging する設計 (テストの副作用隔離) と噛み合うか要確認
>
> **参照**: [ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md) (両 OS matrix の意義)、PR [#376](https://github.com/aloekun/claude-code-hook-test/pull/376)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (実バグではないが CI の信号品質を下げる) / Frequency Low (初観測) / Effort S / Adoption Risk Low (テスト実行方法の変更のみ)。
>
> **2026-08-10 に Tier 1 へ格上げ (ユーザー判断)**。単発の Severity では Tier 2 相当だが、**flaky テストは「また flake だろう」という読み替えを生み、実バグの見落とし経路になる**。両 OS matrix ([ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)) の信号品質そのものを守る意味で早期に潰す。**WP-18 の完了条件には含めない** (別クレートの既存競合で WP-18 の経路と無関係、計画書 § WP-18 残作業 (3) 参照) が、着手は WP-18 と独立に早める。

#### 作業計画

- [ ] 対処案 (a)-(c) から選び、ローカル (WSL Ubuntu) で並列実行を再現させてから直す
- [ ] 修正後に同じ再現手順で `ETXTBSY` が出ないことを確認する
- [ ] 他の smoke/E2E suite に同型の「copy してから spawn」パターンが無いか棚卸しする

#### 完了基準

- hooks smoke suite が Linux で `ETXTBSY` を起こさないこと (再現手順付きで確認)。
- 同型パターンが他 suite に無いこと、またはあれば同じ対処が入っていること。

---

## WP-18 の運用経路で実測した問題 (2026-08-10 登録)

> **由来**: 2026-08-10 のセッションで、夜間ループの PR を**人間がマージする経路**を初めて通した際に実測した 5 件。いずれも「自動化が別の自動化の前提を崩す」型で、[ADR-022](adr/adr-022-automation-responsibility-separation.md) の責務分離が扱う領域にある。
>
> **順位 398-400 は同じ機構 (post-merge-feedback の進行中ガード) の 3 つの面**で、対処は別々だが同一 PR で扱える。

### post-merge-feedback の進行中ガードが、完了済みの run を進行中と誤判定する

> **動機**: #383 を 11:13Z にマージし、その約 4 分後に #382 をマージしたところ、#382 の feedback が `前回の post-merge-feedback workflow がまだ進行中の可能性 (context.json が 223s 前に書かれた)` で失敗した。**実際には #383 の run は完了していた** (`383.md` 生成済み、takt プロセスも不在)。
>
> **問題の型**: ガードは `context.json` の **mtime が 1500 秒以内か**だけを見ており、**run が完了したかを見ていない**。したがって完了済みでも 25 分間は次の feedback が起動できない。マージを連続実行する運用 (本セッションでは 4 本連続) では確実に踏む。
>
> **対処案**: (a) run の完了を示すマーカーを見て、完了していれば通す、(b) context.json に `status` を持たせて mtime 判定をやめる、(c) 進行中判定を takt プロセスの実在確認に変える。**「進行中を防ぐ」目的自体は正しい** — context.json が上書きされると誤った PR の transcript が使われるため (順位 400 参照)。
>
> **完了判定は対象 PR に束縛すること (レビュー指摘、2026-08-10)**。単純に「report が生成されているか」を見るのでは不十分である。`copy_feedback_report` ([takt.rs](../src/cli-merge-pipeline/src/feedback/takt.rs)) は `find_latest_run_dir` で**最新 run を選ぶだけで `pr_number` と照合していない**ため、別 PR の report を現在の PR の `{pr_number}.md` へコピーし得る。また takt の終了は timeout や失敗でも起こるので、**終了した事実は report 完成を証明しない**。完了判定には少なくとも「run が成功終了した」ことと「その run が対象 PR のものである」ことの両方が要る (task label / context.json / report / reconciliation の PR 番号が一致すること)。
>
> **参照**: [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) (marker + recovery 設計)、[cli-merge-pipeline](../src/cli-merge-pipeline/src/feedback/)、順位 388 (同じ feedback 経路の別 race)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (feedback が欠落する。手動復旧は可能) / Frequency Medium (連続マージ時) / Effort S-M / Adoption Risk Low。

#### 作業計画

- [ ] 完了判定の根拠 (report 生成 / takt exit / プロセス実在) を 1 つ選び、mtime 判定と置き換えるか併用するか決める
- [ ] 完了済み context が残っている状態で次の feedback が通ることをテストで固定する
- [ ] 順位 388 (成功したのに failed marker) と同一 PR で扱えるか検討する

#### 完了基準

- 直前の feedback が**完了していれば**、25 分待たずに次の feedback が起動できること。

### `--feedback-only <PR>` が同じガードに阻まれ、復旧手段として機能しない

> **動機**: 順位 398 の失敗後、marker の復旧手順に従って `pnpm merge-pr --feedback-only 382` を実行したが、**同じガードで失敗した** (`context.json が 314s 前に書かれた`)。
>
> **問題の型**: `--feedback-only` は **PR 番号を引数で受け取り context.json に依存しない**設計 ([`run_ai_step_for`](../src/cli-merge-pipeline/src/pipeline.rs) が `pr_number` を直接受ける) なのに、**進行中ガードだけは context.json の鮮度を見る**。結果、**復旧専用のコマンドが復旧に使えない**。
>
> 実際の復旧は「進行中の takt が無いことを確認 → `context.json` を手動削除 → 再実行」でしか通らなかった。これはツールのエラーメッセージが案内する手順ではあるが、**専用コマンドがあるのに手動介入が要る**のは設計の齟齬である。
>
> **対処案**: `--feedback-only` は引数の PR 番号を正とし、進行中ガードを **skip する**か、**別の完了判定 (順位 398)** に置き換える。前者を採る場合、context.json を必ず上書きしてから走らせて stale 読みを防ぐこと。
>
> **参照**: [pipeline.rs](../src/cli-merge-pipeline/src/pipeline.rs) (`run_feedback_only` / `run_ai_step_for`)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)、順位 398 / 400。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (復旧経路が塞がる) / Frequency Medium (順位 398 が起きるたび) / Effort S / Adoption Risk Low。

#### 作業計画

- [ ] `--feedback-only` がガードをどう扱うべきか決める (skip / 完了判定へ置換)
- [ ] 引数の PR 番号で context を再構築してから走ることをテストで固定する
- [ ] 順位 398 と同一 PR で扱う

#### 完了基準

- 失敗した feedback を `--feedback-only <PR>` だけで復旧できること (手動のファイル削除を要しない)。

### post-merge-feedback の marker が、危険な復旧手順を案内し安全な手順に触れていない

> **動機**: `.failed` marker の「復旧手順」は `pnpm exec takt -w post-merge-feedback -t "..."` の直接起動を案内している。しかし**これは `context.json` を読み直すだけ**で、context が別 PR を指していると**誤った PR の transcript でレポートを生成する**。
>
> marker 自身が「再実行前に `pr_number` が一致することを必ず確認してください」と警告してはいるが、**実際に #382 の marker が出た時点で context は #383 を指していた** (2026-08-10 実測)。警告を読み飛ばせば誤ったレポートが生成される。
>
> **一方、安全な `--feedback-only <PR>` には触れていない**。これは PR 番号を引数で受けるため stale context の影響を受けない (ただし順位 399 のガード問題は別途ある)。
>
> **問題の型**: **危険な手順が案内され、安全な手順が案内されていない**。ADR-030 の recovery 設計が `--feedback-only` の追加より前に書かれたまま更新されていない可能性が高い。
>
> **対処案**: marker のテンプレートを更新し、(1) `--feedback-only <PR>` を第一手段として案内、(2) takt 直接起動は「context の pr_number を確認したうえでの最終手段」に降格、(3) 順位 399 が解決したら手動削除の案内も不要になる。
>
> **参照**: [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)、[cli-merge-pipeline の marker 生成箇所](../src/cli-merge-pipeline/src/feedback/)、順位 398 / 399。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (誤ったレポート生成 = 誤情報の永続化) / Frequency Low (marker が出たときだけ) / Effort XS-S / Adoption Risk None。

#### 作業計画

- [ ] marker テンプレートの復旧手順を書き換える (`--feedback-only` を第一手段に)
- [ ] takt 直接起動を残す場合は `pr_number` 確認を手順の前段に置く
- [ ] 順位 399 の解決後に手順を再度見直す

#### 完了基準

- marker の復旧手順に従うだけで、**誤った PR のレポートが生成され得ない**こと。

---

## post-merge feedback 採用分 (#376/#377/#380/#381/#382、2026-08-10 採否確定)

> **由来**: WP-18 の一連 PR の post-merge feedback で挙がった採用候補を、2026-08-10 に系統別へ分類してユーザーが採否を決定した。**系統 A (観測の完全性) / B (重複実装の予防) / C (shell・config パースの安全性) を採用**、系統 D (workflow セキュリティ標準化) / E (PAT 失効監視) は却下 (様子見)。
>
> **系統 F は提案の形を変えて採用**した — 「規約を書く」ではなく「PreToolUse で弾く」(順位 411)。
>
> **却下したもの (negative result として記録)**:
>
> - **系統 D (workflow セキュリティ標準化)** / **系統 E (PAT 失効監視)** — 様子見。有用だが緊急性が低い。
> - **trunk 保護の drift 対処 2 件** (`cli-push-runner` と `cli-stale-branch-scan` の `effective_default_branch()` クロスクレート一致テスト / `lib-config` 抽出) — **却下 (2026-08-10 ユーザー判断)**。pre-push review と post-merge feedback の双方が独立に指摘した Severity High の項目だが、(1) 予防側は順位 405 (新規 crate 実装時の重複確認) で押さえた、(2) 共有 lib 化は `cli-stale-branch-scan` の意図的な network isolation 設計 ([ADR-031](adr/adr-031-weekly-review-pipeline.md)) と抵触しうる、の 2 点から見送る。**再採用条件: 同型の drift が今後も再発する場合**。

### 系統 A-1: 「対処後は効果を観測するまで完了と見なさない」を明文化する

> **動機**: 本セッションで**同じ誤りを 2 回**踏んだ。[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11 は `@coderabbitai review` の投稿が成功したことだけを見て、**相手が反応していない事実に 10 時間気づけなかった**。決定 15 は draft を廃止した後に**同じ症状が続くかを確かめる前**に「解決した」と記録し、原因が別 (author が bot) だと後から判明した。
>
> **問題の型**: 「対処を実施した」と「対処が効いた」を同一視している。助言層を fail-open にすること自体は [ADR-043](adr/adr-043-security-gates-fail-closed.md) に沿って正しいが、**fail-open は効果の観測を別に用意して初めて成立する**。
>
> **対処案**: `docs/dev-conventions.md` に「対処の完了条件は**対処後の観測**である」旨を追記する。最低限含める点: (1) 症状ベースの問題では対処後に同じ症状が消えたことを確認するまで完了としない、(2) 外部サービス依存の対処は「送った」ではなく「相手が反応した」を観測する、(3) 観測できない対処は完了扱いにせず未確定として記録する。
>
> **既に機構化された部分**: `.github/workflows/review-request.yml` は投稿後に CodeRabbit の反応を待ち、無ければ red で落とす (決定 16)。本エントリはこれを**一般則として言語化**するもの。
>
> **参照**: [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11 (撤回) / 決定 15 (前提の訂正) / 決定 16、[ADR-043](adr/adr-043-security-gates-fail-closed.md)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) (本件は判断を伴うため rule 側)。
>
> **実行優先度**: 🚀 Tier 1 — Severity High (誤った「解決済み」記録が次の判断を汚染する) / Frequency Medium (本セッションだけで 2 回) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に節を追加し、上記 3 点を具体例 (決定 11 / 決定 15) 付きで書く
- [ ] 既存の「LLM を含む自動化経路は実走でしか検証できない」節との重複を整理する

#### 完了基準

- 「対処したが効果を確認していない」状態を完了と記録してよいか、convention から判断できること。

### 系統 A-2: AI レビューが出す数値・仕様の主張は仮説として扱い実測で二重検証する

> **動機**: 2026-08-10 に CodeRabbit が「`3 × 15 × 4 × 2` は 360 なので 216 は誤り」と指摘した。**観察は正しかったが提示された数値も誤り**で、実際に列挙を数えると **384** だった (外部フラグは `None` + 7 + 8 = 16 通りで、因数の 15 も誤っていた)。指摘をそのまま採用していれば誤った数値を land させていた。
>
> 同種の例が同セッションで複数出ている: `--paginate --slurp` と `--jq` の併用提案は **gh 2.95.0 で実行時エラー**になり、jq の `test()` を使う正規表現案は**パースエラー**だった。いずれも**観察 (問題の指摘) は正しく、修正手段が誤っていた**。
>
> **問題の型**: AI レビューの finding は「問題の指摘」と「修正案」が同じ確信度で提示されるが、**後者の正しさは前者を保証しない**。特に数値・外部ツールの仕様・API の挙動は、レビュアーが実行環境を持たないまま推論している。
>
> **対処案**: [ADR-050](adr/adr-050-iteration-aware-decision-criteria.md) (multi-iteration workflow の decision criteria) に「AI レビューが提示する数値・外部ツール仕様・API 挙動の主張は仮説として扱い、採用前に実測する」旨を統合する。既存の finding 判定基準の一部として書くのが自然。
>
> **参照**: [ADR-050](adr/adr-050-iteration-aware-decision-criteria.md)、[ADR-047](adr/adr-047-prepush-refute-facet.md) (反証機構の射程)、PR [#377](https://github.com/aloekun/claude-code-hook-test/pull/377) / [#380](https://github.com/aloekun/claude-code-hook-test/pull/380) (実例)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (誤った修正を land させる) / Frequency Medium (本セッションで 3 回) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] ADR-050 へ追記し、実例 (組合せ数の誤り、gh のオプション非互換、jq のパースエラー) を根拠として残す
- [ ] 「観察は採るが修正手段は実測する」という分離を明示する

#### 完了基準

- AI レビューの finding を採用する際、どこまでを信頼しどこから実測するかが ADR-050 から読み取れること。

### 系統 A-3: 外部依存の非同期応答待ちに timeout / retry を明記する convention

> **動機**: `src/cli-stale-branch-scan/src/collect.rs` の初版は `git ls-remote` / `gh pr list` に timeout を持たず、pre-push review が「weekly-review skill 内で同期実行されるため、ここが止まるとパイプライン全体が無診断でハングする」と指摘した。同 PR で `lib_subprocess::wait_with_timeout_basic` による 60 秒 timeout を入れた。
>
> **問題の型**: 外部サービスへの待ちは DNS/TCP hang・一時障害・認証プロンプト待ちで無期限に止まりうる。**同期実行される経路では、1 箇所の hang がパイプライン全体を無診断で止める**。
>
> **対処案**: `docs/dev-conventions.md` に「外部サービスへの待ちには必ず timeout を置き、超過は loud に失敗させる」旨を追記する。あわせて既存の外部待ち step を棚卸しし、timeout の無い箇所を洗い出す (`cli-pr-monitor` の poll、workflow の待機 step 等)。
>
> **参照**: [ADR-016](adr/adr-016-long-running-command-strategy.md) (長時間コマンド実行戦略)、`src/lib-subprocess/`、PR [#377](https://github.com/aloekun/claude-code-hook-test/pull/377)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (無診断ハング) / Frequency Low / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に timeout 必須の旨を追記する
- [ ] 既存の外部待ち経路を棚卸しし、timeout 不在の箇所を列挙する (対処は別エントリでもよい)

#### 完了基準

- 新規に外部サービスを待つコードを書くとき、timeout の要否と失敗時の扱いが convention から決まること。

### 系統 B-1: 新規 crate / exe 実装時に既存同種コンポーネントとの重複を確認する

> **動機**: `cli-stale-branch-scan` が `push-runner-config.toml` の `default_branch` 解決ロジックを `cli-push-runner` から手で複製した。**CodeRabbit 指摘で 1 度直した後、同一 PR 内で再び同型の漏れ**が出ている (top-level のみ読む → section override も読む)。既存実装の存在を先に確認していれば避けられた。
>
> **問題の型**: 新規 crate を作る際、既存 crate に同じ問題を解いたコードがあるかを確認する手順が無い。34 crate 規模では**記憶に頼れない**。
>
> **対処案**: `CLAUDE.md` の開発 convention に、新規 crate / exe 実装時のチェックリストとして (a) 既存同種コンポーネントとの機能重複を確認する、(b) 共用化しない判断をした場合は**ミラー元と理由をコード doc に明記**する、を追加する。(b) は PR #377 で実践済み (`TrunkConfig::effective_default_branch` の doc に `cli-push-runner` のミラーである旨を記載)。
>
> **参照**: [ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md) (共通化と分離の線引き)、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、PR [#377](https://github.com/aloekun/claude-code-hook-test/pull/377)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (silent drift の温床) / Frequency Medium (2 回再発) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `CLAUDE.md` の開発 convention へチェックリストを追加する
- [ ] ADR-044 の抽出境界判断と矛盾しないことを確認する (「必ず共通化せよ」ではない)

#### 完了基準

- 新規 crate を作るとき、既存重複の確認とミラー宣言が手順として踏まれること。

### 系統 B-2: 旧 API 廃止時に enum / config key / CLI flag の 3 形態すべての reject をテストで固定する

> **動機**: `draft-pr` から `autonomous-pr` への改名で、`Operation::parse` と config キーの旧名 reject は unit test で固定したが、**CLI フラグの旧名だけが exe drill 確認どまり**で test suite に入っていなかった。CodeRabbit 指摘で追加した (PR [#376](https://github.com/aloekun/claude-code-hook-test/pull/376))。
>
> **問題の型**: 1 つの概念が **enum variant / config key / CLI flag** の 3 形態で表に出る設計では、改名時に**どれか 1 つが漏れる**。漏れた形態が「別名として通る」と fail-open になる。
>
> **対処案**: `docs/dev-conventions.md` に「旧 API 廃止時は 3 形態すべての reject をテストで固定する」チェックリストを追加する。本リポジトリでは rename が頻出 (ADR 一覧に rename 系決定が多数)。
>
> **参照**: [ADR-071](adr/adr-071-draft-pr-backpressure.md) の unit test 節 (3 形態の reject を固定した実例)、PR [#376](https://github.com/aloekun/claude-code-hook-test/pull/376)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (改名漏れが fail-open になる) / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へチェックリストを追加する
- [ ] 順位 407 (旧語彙 lint) と役割分担を明確にする (lint = live code の検出、本エントリ = reject のテスト固定)

#### 完了基準

- 改名 PR で 3 形態の reject テストが揃っていることをレビューで確認できること。

### 系統 B-3: 旧語彙が live code に出現したら reject するカスタムリントルール

> **動機**: `draft-pr` から `autonomous-pr` への改名 (12 ファイル 132 箇所) で、**CodeRabbit が同一 PR 内だけで 4 箇所の取りこぼしを段階的に指摘**した。10 ファイル以上に跨る rename は本リポジトリで反復的に発生する。
>
> **対処案**: [ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の regex 層 (`.claude/custom-lint-rules.toml`) に、旧語彙が **live code (`rs` / `toml` / `yml` / `yaml`)** に出現したら reject するルールを追加する。**docs は extensions フィルタで自然に除外**される (歴史記録として旧名を残すため)。
>
> **既存基盤で足りる**: post-merge feedback は当初「新規 stop hook とバッチ検証テスト」で Effort M と見積もったが、ADR-007 の確立済み regex 基盤がそのまま使え、extensions を絞れば docs 除外も自動で効くため Effort S に下がる。real-time 検知 (hook) がバッチテストの目的を包含する。
>
> **運用上の注意**: 改名ごとにルールを足す形になるため、**寿命のあるルール**として扱う (改名が浸透したら削除)。[ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) の bounded lifetime と同じ発想。
>
> **参照**: [ADR-007](adr/adr-007-custom-linter-layer-boundary.md)、PR [#376](https://github.com/aloekun/claude-code-hook-test/pull/376)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium (rename のたび) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `.claude/custom-lint-rules.toml` に旧語彙 reject ルールを追加 (extensions は `rs` / `toml` / `yml` / `yaml`。**拡張子は文字列一致** (`eq_ignore_ascii_case`) なので `yml` と `yaml` は別物で、本リポジトリは `.github/workflows/*.yml` と `.coderabbit.yaml` の両方を持つ)
- [ ] `rule_test_coverage_check` / `incident_eval.rs` の既存 3 つの test 群を満たす fixture を用意する
- [ ] ルールの寿命 (いつ削除するか) をコメントに明記する

#### 完了基準

- 旧語彙を live code に書くと hook が reject し、docs では reject されないこと。

### 系統 C-1: safety-critical な config 比較に shell glob を禁止し exact-match を必須化する

> **動機**: `.github/workflows/review-request.yml` の kill-switch 判定が glob による**部分一致**だった。この形では `enabled = false` の行に `true` を含むコメントが付くだけで判定が反転し、**fail-closed を謳う step 自身が fail-open** する。pre-push review が検出し、コメントと空白を除いた値の厳密一致へ修正した (PR [#380](https://github.com/aloekun/claude-code-hook-test/pull/380))。
>
> **問題の型**: shell の glob は「含む」であって「等しい」ではない。**安全装置の判定にこれを使うと、無関係な文字列が混じるだけで反転する**。しかも今日の config には該当コメントが無かったため**症状が出ずに潜伏**していた。
>
> **対処案**: [ADR-043](adr/adr-043-security-gates-fail-closed.md) に「安全装置の config 比較に glob / 部分一致を使わない。コメントと空白を除いた値の厳密一致を用いる」旨を追記する。あわせて `docs/dev-conventions.md` に「構造化 config (TOML/JSON/YAML) を shell や awk で自前パースする場合の注意」を書く。
>
> **参照**: [ADR-043](adr/adr-043-security-gates-fail-closed.md)、[ADR-066](adr/adr-066-autonomy-global-kill-switch.md) (kill-switch の 2 面契約)、PR [#380](https://github.com/aloekun/claude-code-hook-test/pull/380)。
>
> **実行優先度**: 🚀 Tier 1 — Severity High (安全装置の silent fail-open) / Frequency Low / Effort S / Adoption Risk None。

#### 作業計画

- [ ] ADR-043 へ追記する (実例と、なぜ症状が出ずに潜伏するかを含める)
- [ ] `docs/dev-conventions.md` へ shell や awk での config パース時の注意を書く

#### 完了基準

- 安全装置の判定を shell で書くとき、比較方法の選択が ADR-043 から決まること。

### 系統 C-2: shell の部分一致比較を検出するカスタムリントルール

> **動機**: 順位 408 と同じ実例。**規約だけでは同じ形を再び書く**ため、決定論層で検出する。
>
> **対処案**: [ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の regex 層に、shell や workflow 内で glob による値比較をしている箇所を検出するルールを追加する。**boolean や enum らしき値 (`true` / `false` / `enabled` 等) を含む場合に限定**して false positive を抑える。
>
> **判断が要る点**: 部分一致が正当な用途 (文字列検索) もあるため、**検出対象を安全装置の判定に絞れるか**が採否の分かれ目。絞れないなら順位 408 の規約のみで運用し、本エントリは却下してよい ([ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) の mechanizable 判定)。
>
> **参照**: [ADR-007](adr/adr-007-custom-linter-layer-boundary.md)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)、順位 408。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Low / Effort S-M / Adoption Risk Medium (false positive)。

#### 作業計画

- [ ] 検出対象を安全装置の判定に絞れるか調べる (絞れなければ却下し理由を記録する)
- [ ] 絞れる場合、`.claude/custom-lint-rules.toml` へ追加し fixture を用意する

#### 完了基準

- 採用・不採用のいずれかが根拠つきで記録され、採用時は false positive が実運用で問題にならないこと。

### 系統 F (形を変えて採用): `cargo fmt` を PreToolUse でブロックし正しいコマンドを提示する

> **動機**: 本リポジトリは **rustfmt を意図的に適用しない** (引数ペアを 1 行に保つ独自整形)。2026-08-10 に `cargo fmt` を誤実行し、無関係な 3 ファイルに整形差分が入って巻き戻しに工数を要した。
>
> **なぜ規約ではなく機構か (2026-08-10 ユーザー判断)**: 規約は **`CLAUDE.md` に書いた時点で毎セッション読まれ、コンテキストを圧迫する**。一方 PreToolUse hook は**発火するまでコストがゼロ**で、しかも**ブロックと同時に正しいコマンドをフィードバック**できるため、読み手 (Claude Code) は規約を覚えていなくても正しい経路へ到達する。これは [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) の「mechanizable なら仕組み化する」判断そのものであり、`cargo fmt` の検出は**コマンド文字列の一致で足りる**ため mechanizable 判定を満たす。
>
> **この観点は ADR-042 に無い**: 現行 ADR-042 の判断基準は「機械判定できるか」「投資対効果」が中心で、**「規約はコンテキストを消費し続けるが hook は発火時のみ」という非対称**が明示されていない。ルール追加を検討するたびに効く一般則なので、本エントリで併せて追記する。
>
> **対処案**: `src/hooks-pre-tool-validate/src/presets/basic.rs` の既存パターン (`rm -rf` / `cd /d` / `git` シェルラッパー) に倣い、`cargo fmt` をブロックするルールを追加する。メッセージには (1) 本リポジトリが rustfmt 非適用であること、(2) 整形が必要なら手で最小限に行うこと、(3) 例外的に実行したい場合の判断材料、を含める。**代替コマンドは存在しない** (手で直すのが正) ため、提示するのは「正しいコマンド」ではなく**正しい対処**である。
>
> **参照**: [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md)、[ADR-001](adr/adr-001-hooks-implementation-language.md)、`src/hooks-pre-tool-validate/src/presets/basic.rs`、PR [#376](https://github.com/aloekun/claude-code-hook-test/pull/376) (誤実行の実例)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (無関係な差分混入。レビュー負荷と巻き戻し工数) / Frequency Medium (fmt は反射的に実行されやすい) / Effort S / Adoption Risk Low (例外実行の手段を残すこと)。
>
> **早期着手する (2026-08-10 ユーザー判断)**。`cargo fmt` は**反射的に実行されやすい**ため、規約が無い状態が続くほど誤実行の機会が増える。**WP-18 の完了条件には含めない** (対象は開発環境全般で WP-18 の機構と無関係、計画書 § WP-18 残作業 (3) 参照) が、着手は WP-18 と独立に早める。

#### 作業計画

- [ ] **検出対象の範囲を先に決める**。`cargo fmt` の完全一致だけでは `cargo fmt --all` / `cargo +stable fmt` / `rustup run stable cargo fmt` / `cargo-fmt` が素通りする。全形態を弾くなら正規化してから判定し、fixture も全形態を網羅する。完全一致に留めるならその範囲を完了基準へ明記する (レビュー指摘、2026-08-10)
- [ ] `presets/basic.rs` に `cargo fmt` ブロックを追加する (既存 3 パターンの構造を踏襲)
- [ ] ブロックメッセージに「なぜ非適用か」と代替手順を含める
- [ ] `rule_test_coverage_check` / `incident_eval.rs` の fixture を追加する
- [ ] ADR-042 へ「規約は常時コンテキストを消費し、hook は発火時のみ」という非対称を追記する

#### 完了基準

- **決めた検出範囲**において `cargo fmt` が PreToolUse でブロックされ、メッセージだけで正しい対処に到達できること。範囲を完全一致に限定した場合は、素通りする形態 (`--all` 付き / toolchain 指定 / `cargo-fmt`) を完了基準に明記すること。
- ADR-042 に規約と機構のコスト非対称が記録されていること。

---

## PR #385 のレビューで見送った指摘 (2026-08-10 登録)

> **由来**: 順位 397 の PR ([#385](https://github.com/aloekun/claude-code-hook-test/pull/385)) で出た指摘のうち、**PR の性質 (逐語移動が大半) を理由に本 PR では扱わなかった**もの。どちらも指摘自体は妥当で、スレッドに理由を返信済み。

### `resolve_main_workspace_root` の colocated 経路と file 経路で正規化の粒度が違う

> **動機**: [#385](https://github.com/aloekun/claude-code-hook-test/pull/385) の CodeRabbit 指摘。`resolve_main_workspace_root` は `.jj/repo` が**ディレクトリ (colocated)** なら入力パスをそのまま返し、**ファイル (secondary workspace)** なら `canonicalize()` + verbatim prefix 剥がしを通す。同じチェックアウトでも入口によって返るパス文字列の形が変わる。
>
> **現時点で破綻経路は見当たらない**: `cli-telemetry-report` は入力・出力の両方を `canonicalize_or_as_is()` に通すため差分を吸収する。`weekly_review` / `monthly_review` は返り値を `join()` してファイル I/O にしか使っておらず、文字列比較のキーにしていないため OS が等価パスを同じ実体へ解決する。**caller が文字列比較を始めた時点で分裂する**のが本質的なリスク。
>
> **なぜ #385 で直さなかったか**: `workspace.rs` は `lib.rs` の 800 行超過に伴う**移動で作られたファイル**で、#385 は diff 2203 行のうち約 1700 行がこの移動、サイズゲートを override して通している。移動 PR に挙動変更を積み増すほどレビューの前提が崩れるため、**同じ移動ファイル内でも線を引いた**:
>
> | 指摘 | #385 での扱い | 線引きの理由 |
> |---|---|---|
> | `strip_windows_verbatim_prefix` の UNC 未対応 ([該当スレッド](https://github.com/aloekun/claude-code-hook-test/pull/385#discussion_r3751110198)) | **修正した** | 「ネットワーク共有上で `GIT_DIR` が壊れる」という具体的な失敗経路がある明確なバグで、純関数のテストで固定でき、非 UNC パスの挙動は変わらない |
> | 本エントリ (colocated 経路の正規化) | **見送り** | doc comment が「colocated root は入力そのまま返す」と**意図的な設計として明記**しており、変更には caller 3 箇所の検証が要る。現行 caller に破綻経路も見当たらない |
>
> **対処案**: colocated 経路も `canonicalize()` し、失敗時は入力のまま返す (現行 caller の fail-open を維持)。合わせて caller 3 箇所が正規化前提に依存していないことを確認する。
>
> **参照**: [ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) (状態ファイルの workspace 分裂対策)、[workspace.rs](../src/lib-jj-helpers/src/workspace.rs)、[#385 の該当スレッド](https://github.com/aloekun/claude-code-hook-test/pull/385#discussion_r3751110189)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low (現行 caller に破綻経路なし) / Frequency Low / Effort S / Adoption Risk Low (caller 3 箇所の確認が要る)。

#### 作業計画

- [ ] colocated 経路の返り値を `canonicalize()` + verbatim prefix 剥がしに揃える
- [ ] caller 3 箇所 (`cli-telemetry-report` / `weekly_review` / `monthly_review`) が正規化形に依存していないことを確認する
- [ ] colocated / secondary の両経路が同じ形のパスを返すことを固定するテストを追加する

#### 完了基準

- 同じチェックアウトに対し、colocated 経路と secondary 経路が同じ形式のパスを返すこと。

### `CwdRestore` Drop guard がリポジトリ全体で 8 定義 / 6 ファイルに複製されている

> **動機**: [#385](https://github.com/aloekun/claude-code-hook-test/pull/385) の pre-push review 指摘 (non-blocking)。テストで cwd を退避・復元する `CwdRestore` / `CwdGuard` 相当の struct が **8 箇所で定義されている** (2026-08-10 実測: `fix_commit/abandon.rs` に 3 個、`fix_commit/sweep.rs` / `stages/push_jj_bookmark.rs` / `stages/repush.rs` / `stages/scope_guard.rs` に各 1 個、加えて本 PR の `lib-jj-helpers/src/bookmarks.rs`)。レビューは「6 個目」と表現したが、これはファイル数を数えた場合の値。
>
> **問題の型**: [ADR-025](adr/adr-025-cwd-restore-drop-guard.md) 自身が「2 例目が出たら `lib-test-helpers` へ統合する」というトリガーと再評価期限 (2026-07-31) を定めているが、**トリガーを 4 回超過したまま期限も過ぎており、ADR の status が実態と乖離している**。ADR が定めた条件が守られないと、以後の「2 例目で統合」という判断基準そのものが信用できなくなる。
>
> **対処案**: (a) `src/lib-test-helpers/` を新設して `CwdRestore` を抽出する (ADR-025 の当初計画)、(b) 抽出しない判断を下し ADR-025 の status と再評価期限を更新する。**どちらでもよいが、放置は選択肢に含めない**。
>
> **参照**: [ADR-025](adr/adr-025-cwd-restore-drop-guard.md)、[ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md) (共通化と分離の線引き)、[#385](https://github.com/aloekun/claude-code-hook-test/pull/385) の pre-push review。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (ADR の判断基準が形骸化する) / Frequency Low / Effort S-M / Adoption Risk Low。

#### 作業計画

- [ ] 抽出するか status 更新に留めるかを決める (ADR-044 の境界判定を適用する)
- [ ] 抽出する場合は `lib-test-helpers` を新設し 6 箇所を差し替える
- [ ] いずれの場合も ADR-025 の status と再評価期限を実態に合わせて更新する

#### 完了基準

- `CwdRestore` の重複について、抽出したか見送ったかが ADR-025 に根拠つきで記録されていること。
