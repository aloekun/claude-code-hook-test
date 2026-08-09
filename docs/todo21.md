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
