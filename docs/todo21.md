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
> 2. 2 段目のフォールバック `get_jj_bookmarks()` は [`BOOKMARK_SEARCH_REVSETS = ["@", "@-", "@--"]`](../src/lib-jj-helpers/src/lib.rs#L51) の **3 段しか遡らない**。失敗時に bookmark はそれより深くにあり、空の Vec が返って 2 段目も空振りする
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
> **参照**: [github.rs:103](../src/cli-merge-pipeline/src/github.rs#L103) (`detect_pr_number`)、[lib.rs:51](../src/lib-jj-helpers/src/lib.rs#L51) (`BOOKMARK_SEARCH_REVSETS`)、[ADR-013](adr/adr-013-merge-pipeline.md)、[ADR-021](adr/adr-021-jj-change-detection-principles.md) (jj 変更検出の設計原則)、[ADR-024](adr/adr-024-shared-jj-helpers-library.md) (共有ヘルパーの変更は 3 クレートに効く)。
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

> **由来**: 2026-08-09 に PR [#373](https://github.com/aloekun/claude-code-hook-test/pull/373) で実測した 2 件の観測から、ユーザー判断 (同日) を経て方針を確定したもの。**3 件は 1 本の根から出ている** — 夜間 PR を draft にしたことが CodeRabbit の自動レビュー対象外を招き、その回避策 ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11) が bot 投稿の無視で不成立になった。順位 393 が記録の是正、394 が構造の是正、395 は独立 (ブランチ運用) だが同じセッションの観測に由来する。
>
> **順序**: 393 → 394。393 は現状記録を実測に合わせる作業で、394 の改訂前提になる。395 は独立で並行可。

### ADR-072 決定 11 (CodeRabbit 明示トリガー) を撤回として記録し Phase B 判定を訂正する

> **動機**: [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11 は「draft PR 作成後に `@coderabbitai review` を 1 回投稿する」設計だが、**App token (bot) の投稿は CodeRabbit に無視される**ことが 2026-08-09 に確定した。同一 PR (#373)・同一文言・同一設定で投稿者だけが異なる 2 回の実測:
>
> | 時刻 (UTC) | 投稿者 | 反応 |
> |---|---|---|
> | 08-08 18:10:54 | `nightly-todo-aloekun` (App/bot) | **なし** (約 10 時間) |
> | 08-09 04:10:39 | `aloekun` (人間) | **4 秒後**に応答 → 11 秒後にレビュー開始 |
>
> 決定 11 自身が「bot 同士のループを避けるため他 bot のコメントを無視する実装は珍しくない」と未検証事項に挙げていた仮説が、そのまま実証された。**明示トリガーという方式自体は有効**で ([ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) の fix push 後トリガーは現在も機能している)、効かないのは投稿者が bot の場合だけ。ADR-019 側は `cli-pr-monitor` がローカルの `gh` = **ユーザー資格情報**で投稿しているため成立していた。決定 11 は「ADR-019 と同型」と判断したが、**同型だったのはコマンド文字列だけで投稿者の種別が違っていた**。
>
> あわせて **§ 実走スモークの Phase B 判定も誤り**。「不成立」と記帳しているが、実際は CodeRabbit のコメントが発生した時点で `issue_comment` 経路が発火し、**Phase A が夜間 draft PR で自動起動した** (#373 で 04:12:15 に分析コメント)。経路は生存しており、起動契機が無かっただけである。`coderabbitai[bot]` allowlist の要否も同様に再判定が必要。
>
> **対処案**: 決定 11 を削除せず**撤回として記録**する ([ADR-047](adr/adr-047-prepush-refute-facet.md) が「dogfood の結果 2026-07-19 却下・撤去済」と残している形式に倣う)。撤回理由と実測表を残し、順位 394 の draft 廃止が代替解になることを明記する。§ 実走スモークの Phase B 行と `coderabbitai[bot]` allowlist 行も実測に合わせて訂正する。
>
> **参照**: [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 11 / § 実走スモーク、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md)、[ADR-067](adr/adr-067-phase-b-unattended-fix-push.md) (Phase B 起動経路)、PR [#373](https://github.com/aloekun/claude-code-hook-test/pull/373)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (ADR の記述が実測と食い違ったまま残ると、次の判断が誤った前提に乗る) / Frequency 一度きり / Effort S / Adoption Risk None (docs-only)。

#### 作業計画

- [ ] 決定 11 に撤回の記録を追記する (実測表 + 撤回理由 + 代替解が順位 394 であること)
- [ ] § 実走スモークの Phase B 行を「経路は生存・起動契機が無かった」へ訂正する
- [ ] `coderabbitai[bot]` allowlist の判定不能行を、決定 11 撤回を踏まえた再判定条件へ書き換える

#### 完了基準

- 決定 11 が撤回として記録され、bot 投稿が無視される実測 (投稿者別の対照) が残っていること。
- Phase B 自動起動の判定が「不成立」ではなく実測どおりの記述になっていること。

### 夜間ループの draft PR を通常 PR へ変更し背圧の命名を `autonomous` 系へ揃える

> **動機**: 順位 393 の実測を受けた**構造側の是正**。夜間ループが draft PR を作るために `.coderabbit.yaml` の `reviews.auto_review.drafts: false` と衝突し、レビューが付かない状態を回避策 (決定 11) で埋めようとして失敗した。**draft をやめれば `auto_review.enabled: true` の初回レビューに自然に乗り、回避策そのものが不要になる。**
>
> **ユーザー判断 (2026-08-09)**: 発生トリガーがユーザー指示か自動採択かで扱いを区別しない。有効な修正 PR ならプロジェクトに取り入れてよい。したがって commitment 点は **マージ 1 点**に集約してよく、「ready = レビュー求む」の意思表示を自律 actor が出すことを許容する。
>
> **必ず同時に直す箇所**: [nightly-todo.yml](../.github/workflows/nightly-todo.yml) の背圧計数は `jq '[.[] | select(.isDraft and ...)] | length'` で **`.isDraft` を条件にしている**。draft をやめると**計数が常に 0 になり背圧が完全に無効化される** ([ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md) 原則 5 が禁じる状態)。`--draft` の除去とセットで必須。
>
> **ADR 側の改訂**:
>
> - **ADR-052**: 原則 2 の分類表で「**非 draft PR の作成**」がゲート必須クラスに、「PR の ready 化」も同様に列挙されている。前者を自動実行可クラスへ移す**本体改訂**にあたる (注記の追加では済まない)。改訂理由 (トリガーの別を区別せず commitment 点をマージに集約する) を明記する
> - **ADR-019**: quota 消費が夜間 PR 1 件につき 1 レビュー増える。決定 11 が意図していた消費量と同じ (起動経路が明示トリガーから auto_review に変わるだけ) だが、クォータ設計の前提が変わるため注記する
> - **ADR-071 / ADR-072**: 背圧の指標定義 (「未マージ draft 数」→「未マージの `claude/` PR 数」) と決定 8 の draft 前提記述
>
> **命名 (ユーザー選択 2026-08-09)**: `autonomous` 系へ揃える。`max_open_autonomous_prs` / `--open-autonomous-prs` / `Operation::AutonomousPr` / `--operation autonomous-pr` / `requires_autonomous_pr_backpressure()` / `open_autonomous_prs`、ADR-052 のクラス名は「autonomous-pr クラス」。**[ADR-071](adr/adr-071-draft-pr-backpressure.md) のファイル名は変更しない** — ADR 番号とファイル名は歴史的識別子で、変えると全リンクが壊れる。内容側で意味を再定義する。
>
> **影響範囲**: 12 ファイル 132 箇所。Rust 実装は `lib-autonomy-policy` (54) / `cli-autonomy-gate` (23) / `cli-fix-push-gate` (2)。**PR size gate (1500 行) に掛かった場合は (a) 機械的リネーム (挙動不変) → (b) draft 廃止 の 2 本へ分割し、[ADR-069](adr/adr-069-pr-chain-declaration.md) の chain 宣言を付ける。**
>
> **参照**: [ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md) 原則 2 / 原則 5、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md)、[ADR-071](adr/adr-071-draft-pr-backpressure.md)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 8 / 決定 11、順位 393 (記録側の是正)。
>
> **実行優先度**: 🚀 Tier 1 — Severity High (現状は夜間 PR にレビューが付かず、無人実装の品質が CI だけに依存している) / Frequency 毎晩 / Effort M-L / Adoption Risk Medium (背圧計数の同時修正を落とすと ADR-052 原則 5 違反になる)。

#### 作業計画

- [ ] `gh pr create` から `--draft` を除去する
- [ ] 背圧計数の `.isDraft` 条件を除去する (**落とすと背圧が無効化される**)
- [ ] 決定 11 の `Request a CodeRabbit review` step を撤去する
- [ ] 背圧の命名を `autonomous` 系へ揃える (config / flag / 型 / 文字列 / 関数 / field)
- [ ] ADR-052 原則 2 の分類表を改訂し、ADR-019 / ADR-071 / ADR-072 の該当記述を同期する
- [ ] 実走で夜間 PR に CodeRabbit の初回自動レビューが付くこと、背圧が閾値で止まることを確認する

#### 完了基準

- 夜間ループが通常 PR を作り、CodeRabbit の初回自動レビューが**実走で**付くこと。
- 背圧が draft 廃止後も機能すること (閾値到達で deny することを実測または drill で確認)。
- ADR-052 の分類表が改訂され、改訂理由が記録されていること。

### 週次レビューで浮きブランチを検出し削除を提案する

> **動機**: 2026-08-09 に、クローズ済み PR [#365](https://github.com/aloekun/claude-code-hook-test/pull/365) のブランチを手動削除したことで [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 3 の除外マーカーが消え、同じ順位 203 が再選択された (PR #373)。**決定 3 自体は設計どおり動作している** — ブランチの存在が着手済みマーカーであり、それが人手で消えたことが原因。
>
> **方針 (ユーザー判断 2026-08-09)**: 判定は現行の `git ls-remote` による **branch 単一ソースのまま維持する**。「クローズ済み PR も見る」案は判定ソースが 2 つになり、fail-closed の単純さ (一致なしでも exit 0 + 空出力で「0 件」と「取得失敗」を取り違えない) を崩すため**採らない**。代わりに浮きブランチを週次で片付け、放置によるタスク滞留を解消する。
>
> - **`claude/nightly-*` も削除提案の対象に含める** (除外しない)。ブランチが残る間そのタスクが選べない期間 (最大 7 日程度) は許容する — 自動実行できる todo はほとんどが改善タスクで急がず、重要な todo はメインセッションで消化するため
> - **削除後の再挑戦も許容する**。現状のクローズは夜間ループの機能不全に起因するもので、正常動作後は採用方向の選択が多くなる見込み
>
> **対処案**: weekly-review に「クローズ済み PR の残存ブランチ」検出を追加する。既存の観点⑤ (`review-todo-whole` facet) へ相乗りさせるか決定論的 scan として持つかは実装時判断 ([ADR-031](adr/adr-031-weekly-review-pipeline.md) の構成に従う)。**削除の実行は提案までとし、自動削除はしない** ([ADR-022](adr/adr-022-automation-responsibility-separation.md) / [ADR-028](adr/adr-028-pnpm-create-pr-gate.md))。
>
> **現状**: クローズ由来で残っている `claude/` ブランチは無い。`claude/nightly-203` は open な #373 のもの、`claude/cloudharness-e2e-validation-sptfc7` / `claude/select-next-task-a9aiam` は 7 月下旬から open のままの #320 / #324 のもの (これらは PR 自体の棚卸し対象)。
>
> **参照**: [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 3、[ADR-031](adr/adr-031-weekly-review-pipeline.md) (weekly-review)、[ADR-022](adr/adr-022-automation-responsibility-separation.md)、WP-19 ステップ 3 (監査ループ — 本エントリはその一部を先取りする)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Low (滞留は最大 7 日で解消され、実害は選択機会の遅延のみ) / Frequency Low (週次) / Effort S-M / Adoption Risk Low (提案のみで自動削除しない)。

#### 作業計画

- [ ] クローズ済み PR の残存ブランチを列挙する検出を weekly-review に追加する
- [ ] `claude/nightly-*` を除外せず対象に含める (除外すると滞留が永続する)
- [ ] 削除は提案までとし、実行はユーザー承認を経ることを明示する
- [ ] WP-19 ステップ 3 (自律アクションの週次棚卸し) と重複しない形で載せる

#### 完了基準

- 週次レビューがクローズ済み PR の残存ブランチを列挙し、削除を提案すること。
- 自動削除を行わないこと (提案までで止まる)。
