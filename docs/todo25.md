# TODO (Part 25)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo24.md がファイルサイズ 50869 B (2026-08-22 時点、50KB = 51200 B の安定読み取り閾値まで残り 331 B) に到達したため、新規エントリは本ファイルに記録する (2026-08-22 新設、週次レビュー 2026-08-22 実行セッションで検出)。**新規エントリの追加先は本ファイル**。ほかの todo ファイル (現存する一覧は [docs/todo.md](todo.md) の preamble が正) の既存エントリは引き続き有効、相互に独立。
>
> **サイズ表記について**: 各記載は**その時点の計測値**であり、現在値と一致しないことがある。現在値が必要なら計測すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 週次レビュー採用 (2026-08-22)

### 順位 495: lib-* crate の責務分類基準が ADR-012 に無い

> **動機**: 現行の `lib-*` crate は shared utility / jj helper / domain logic / state management / external integration の 5 種の責務に分散しているが、ADR-012 (src/ ディレクトリの命名規約) には新規 crate がどのカテゴリに属するかの判定基準が無い。新しい lib-* を足すときに置き場所の判断が属人的になる。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-22-A04 で採用 (severity=medium, facet=architecture, category=module-boundary)。
>
> **参照**: `.claude/weekly-reviews/2026-08-22.md`、`src/lib-*/Cargo.toml`、[ADR-012](adr/adr-012-src-naming-convention.md)

#### 背景

直近だけでも順位 323 で `lib-subprocess`、順位 467 F-2 で `lib-jj-helpers` に手を入れており、「この関数はどの lib に置くべきか」を毎回その場で判断している。ADR-044 (subprocess utility extraction の境界判定) は**抽出するかどうか**の基準を与えるが、**どの crate へ置くか**は扱っていない。

#### 設計決定 (案)

ADR-012 に「lib-* の責務カテゴリと判定順序」を追記する。既存 crate を実際に分類して例示にする (分類できない crate があれば、それ自体が設計の綻びとして記録に値する)。

- [ ] 既存 lib-* を 5 カテゴリへ実際に分類してみる
- [ ] 分類できない / 複数にまたがる crate を洗い出す
- [ ] ADR-012 に判定順序を追記

#### 完了基準

新規 lib-* を足すとき、ADR-012 だけを読んで置き場所が決まること。

---

## docs ファイルサイズの是正 (2026-08-22 週次レビューの決定論 scan 由来)

### 順位 496: 50KB 超過 3 ファイルの物理分割

> **動機**: file-length watchlist が `docs/todo-summary2.md` (70969 B) / `docs/todo22.md` (60685 B) / `docs/todo14.md` (60518 B) の 3 ファイルを 50KB (51200 B) 超過として検出した。**削除漏れではない** — 節数と summary 参照数がほぼ一致しており (14: 31 節/33 参照、22: 30/31)、中身は全て生きたタスクである。刈り込みでは解決せず物理分割が要る。
>
> **本タスクの位置づけ**: 週次レビュー 2026-08-22 の決定論 scan (file-length-watchlist) 由来。findings ではなく機械的観測からの起票。
>
> **参照**: `.claude/weekly-reviews/2026-08-22.md` § File Length Watchlist

#### 背景

前例がある: 2026-07-20 に `todo13.md` を `todo15/16/17` へ、`todo10.md` を `todo18/19` へ物理分割して 50KB 以下に縮小した。同じ手順を踏めばよい。

`todo-summary2.md` だけは事情が違う — 183 行の優先度表 1 枚なので、節ではなく**順位で切る**ことになり、「順位 219 以下 = `docs/todo-summary.md` / 220 以上 = `docs/todo-summary2.md`」という 2 分割規約を 3 分割へ更新する必要がある (`docs/todo.md` preamble と `docs/todo-summary.md` の写し、および summary を読む決定論層 `lib-ledger` の `summary_gate` が対象)。

#### 設計決定 (案)

- `todo14.md` (31 節) / `todo22.md` (30 節): 順位順に 2 分割し、`docs/todo.md` の routing 表へ新ファイルを追記
- `todo-summary2.md`: 順位で切って `todo-summary3.md` を新設。**分割の境界順位を決める前に `lib-ledger` の読み取り経路を確認する** — `parse_summary_entries` は複数 table を走査するので、ファイルが増えたときに呼び出し側が全ファイルを読むかを確かめる
- 分割後に `pnpm lint:docs` / cross-ref 検査が通ることを確認する

- [ ] `lib-ledger` の summary 読み取り経路が 3 ファイル構成に対応できるか確認
- [ ] `todo14.md` を 2 分割
- [ ] `todo22.md` を 2 分割
- [ ] `todo-summary2.md` を分割し規約を 3 分割へ更新
- [ ] `docs/todo.md` の routing 表を更新

#### 完了基準

`docs/todo*.md` と `docs/todo-summary*.md` のすべてが 51200 B 未満。`pnpm lint:docs` green。

---

### 順位 497: PostToolUse で docs ファイルの 50KB 超過を即時ブロックする

> **動機**: 現在 file-length の検査は**週次レビューの報告のみ**で、超過しても何も止まらない。そのため超過に気づくのは最大 7 日後で、その間に書き足しが進んで分割コストが膨らむ。`.rs` は既に PostToolUse hook (`comment-lint-rust` の `RUST_FILE_TOO_LONG`、800 行) で**書いた瞬間にブロック**されており、同じ機構を docs へ広げれば週次を待つ必要がなくなる (ユーザー判断、2026-08-22)。
>
> **本タスクの位置づけ**: 週次レビュー 2026-08-22 の決定論 scan 由来。上の「物理分割」が対症で、本タスクが再発防止。
>
> **参照**: `.claude/weekly-reviews/2026-08-22.md` § File Length Watchlist、`src/hooks-post-tool-comment-lint-rust/` (既存の RUST_FILE_TOO_LONG 実装)、[ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) (ルール vs 仕組み化の境界基準)

#### 背景

`RUST_FILE_TOO_LONG` は「触られるまで grandfather、触ったら閾値を課す」touch-trigger ratchet として実装済みで、本セッションでも実際に発火して分割を促した (`bookmark_check.rs` / `lib-subprocess`)。docs 側に同じものが無いために、todo ファイルだけが 24 個まで増えた。

**分割の連鎖には二次コストがある** — ファイルが増えるほど `docs/todo.md` の routing preamble が伸び、現在 8162 B に達している。todo.md 自身が 48053 B (残り 3147 B) で、**別方向から閾値に近づいている**。早期ブロックはこの連鎖そのものを抑える。

#### 設計決定 (案)

- 対象は `docs/todo*.md` / `docs/todo-summary*.md` (閾値 51200 B)。他の docs へ広げるかは実測してから決める
- **touch-trigger ratchet を踏襲する** — 既に超過している 3 ファイルを即座に全ブロックすると編集自体ができなくなり、分割作業すら阻む。「触ったファイルが閾値を超えていたらブロック」ではなく「**書き込みの結果として閾値を超えたらブロック**」にするか、超過分の縮小方向の編集は通すか、線引きを決める必要がある
- エラーメッセージには現在サイズ・閾値・次にすべきこと (routing 表の更新を伴う新ファイル作成) を含める。`RUST_FILE_TOO_LONG` の `fix.steps` と同じ流儀

- [ ] 既存 `RUST_FILE_TOO_LONG` の実装と ratchet 判定を読む
- [ ] 超過ファイルの編集を阻まない線引きを決める (縮小方向は通す等)
- [ ] hook に docs 用の検査を追加
- [ ] 回帰テスト (超過を作る書き込み → ブロック / 縮小方向の書き込み → 通す)

#### 完了基準

`docs/todo*.md` を 51200 B 超へ書き足す編集がその場でブロックされること。既に超過しているファイルの**縮小方向の編集は通る**こと。両方をテストで固定。

## post-merge feedback 採用分 (PR #434 / #435 / #436 / #437 の 4 PR 分、2026-08-22 採否確定)

> 不具合修正バックログ消化計画 (PR I-L) の post-merge feedback **全 40 提案**を採否判定した。
> 内訳は表に載った 36 件 (**採用候補 21 / 様子見 7 / 却下推奨 8**) + analyzer が Phase 1 品質
> フィルタで表から除外した 4 件 (#435 で 3 件、#437 で 1 件)。さらに採用候補のうち 1 件は
> 実コード確認で脱落した (#437 T1-4「parse エラーに行番号 + 行の中身」は PR L の D-2 で実装済み)。
>
> **件数は数え直すこと** (PR #439 CodeRabbit Minor): 当初ここに「全 48 提案 / 様子見 11 /
> 却下 12」と書いたが、それは前回バッチ (PR E-H) の数字を数え直さずに流用したもので、
> 内訳の合計が総数と合っていなかった。レポートを機械的に数えれば 5 秒で分かる値だった。
>
> **ユーザー判断 (2026-08-22)**: Tier 1 (決定論的防止) は全 4 件採用、Tier 2 (テスト/自動化) は
> **実装の穴埋めに直結する 5 件**を採用、**Tier 3 (ドキュメント/ルール) は 8 件すべて却下**。
> 却下の根拠は本 feedback 自身が示した実証 — 「routing 更新チェックリスト」は既に
> `docs/dev-conventions.md` に存在したのに **3 件目の再発を防げなかった**。規約追記の有効性が
> 否定的に実証された以上、同じ形の 8 件を足す理由が無い。内容は各 PR の doc コメントと
> PR 本文に記録済みで、失われるものは無い。
>
> 統合の単位は「そのまま 1 PR になる粒度」。

### 順位 483: エラーメッセージの無制限 debug 補間を lint で検出する (順位 483)

> **動機**: PR #437 で `clip_for_message()` を導入したのに、順位セル (`{raw:?}`) だけがそれを
> 経由しておらず、長い非数値セルで切り詰め保証が崩れていた (CodeRabbit Minor)。**当初のテストは
> 長い文字列をタイトル列に置いていたためこの経路を一度も通らず、誤った安心を与えていた**。
>
> **本タスクの位置づけ**: post-merge feedback 採用 (#437 Tier1 #1 / custom_lint_rule /
> Severity Medium / Frequency Medium / Effort S)。
>
> **参照**: `.claude/feedback-reports/437.md`、`src/lib-ledger/src/summary_gate.rs` (`clip_for_message`)

#### 背景

truncation wrapper を導入しても、**載せる文字列の一部がそれを通らなければ上限は意味を失う**。
人間のレビューでも見落とされ、CodeRabbit が拾った。

#### 設計決定 (案)

エラー / ログ macro の引数内で、truncation を経由しない `{:?}` / `{}` 補間を検出する。
**どこまでを対象にするかが設計の肝** — 全 `{:?}` を禁じると誤検知だらけになるので、
「truncation wrapper が存在する module 内」等の絞り込みが要る。ADR-007 の層判定を先に行う。

- [ ] 検出範囲の絞り込み方を決める (module 単位 / 関数単位 / 型単位)
- [ ] ADR-007 の判定フローで層を決める
- [ ] 実装 + 既存コードでの false positive 確認

#### 完了基準

truncation wrapper を持つ module で、それを経由しない補間が書いた時点で検出される。

---

### 順位 484: push stage の bare push フォールバック不変条件を seal する (順位 484)

> **動機**: PR #434 (順位 288(b)) で `bookmark_check` の fail-open を塞いだ結果、
> `run_bookmark_check()` は `Some` を返すとき必ず 1 件以上、という不変条件が成立した。
> しかし **`build_push_command` 側にはその前提を固定するテストが無い** — 空リストで
> bare push にフォールバックする経路が残っており、そこへ到達しないことが保証されていない。
>
> **本タスクの位置づけ**: post-merge feedback 採用 (#434 Tier2 #1 / test_addition /
> Severity High / Frequency Medium / Effort M)。
>
> **参照**: `.claude/feedback-reports/434.md`、`src/cli-push-runner/src/stages/push.rs`
> (`build_push_command`)、`src/cli-push-runner/src/stages/bookmark_check.rs`

#### 背景

fail-closed の判定結果 (空リスト) が上流の fallback logic に無視される、という execution-contract
違反が PR #434 の incident の直接の根因だった。修正はしたが、**不変条件はテストで固定されていない**
ため、将来 `Some(空)` を返す経路が復活しても気づけない。

#### 設計決定 (案)

- `BookmarkCheckOutcome::Proceed` が空リストを運ばないことを型か テストで固定する
  (**型で表現できるなら型が良い** — 非空 Vec 型にすればテスト無しで保証できる)
- `build_push_command` の空リスト fallback は派生プロジェクト config 専用の経路であることを
  テストで明示する (現状は doc コメントのみ)

- [ ] 非空を型で表現できるか検討する
- [ ] 型で無理ならテストで seal する
- [ ] `build_push_command` の fallback 到達条件をテストで明示

#### 完了基準

`Some(空)` を返す変異を入れると、いずれかのテストが落ちる。

---

### 順位 485: PR L で追加した実装のテスト補強 (順位 485)

> **動機**: PR #437 で追加した 2 つの実装にテストの穴がある。(1) `inject_git_dir_for_gh_with` の
> `warn_when_unresolved` は条件パラメータなのに、**false 側 (警告抑止) のテストが無い**。
> (2) `clip_for_message` は導入時にタイトル列でしかテストされず、順位セル経由の穴を見逃した
> (CodeRabbit が指摘し PR 内で修正済みだが、**同じ形の見落としを繰り返さない仕組み**が要る)。
>
> **本タスクの位置づけ**: post-merge feedback 採用 (#437 Tier2 #1・#2 / test_addition /
> Severity Medium / Effort S)。
>
> **参照**: `.claude/feedback-reports/437.md`、`src/lib-jj-helpers/src/workspace.rs`、
> `src/lib-ledger/src/summary_gate.rs`

#### 背景

どちらも「**追加した機能の一部の経路しかテストしていない**」形。順位 483 の lint 化と相補で、
こちらは実際のテストを足す側。

#### 設計決定 (案)

- `inject_git_dir_for_gh_with`: (条件 true/false) × (resolved / unresolved) の 4 通りをテストする。
  ログ出力を観測するため、logger を注入可能にする必要があるかを確認する
  (現状 `fn(&str)` なので closure が capture できない)
- **プロセス全体状態の隔離が先に要る** (PR #439 CodeRabbit Major)。本関数は `GIT_DIR` 環境変数と
  cwd を**読み書きする**ため、`cargo test` の既定 (並列) では他テストと競合し、**書いた本人だけが
  通って他テストを壊す**形になりうる。隔離せずにテストを足すと、今回のセッションで 2 度踏んだ
  「テストが空振りする」の別型 (今度は他テストを巻き込む) を作る
  - 復元は Drop guard で行う ([ADR-025](adr/adr-025-cwd-restore-drop-guard.md) の `CwdRestore` が前例)。
    **`GIT_DIR` は「未設定」も状態**なので、`Some`/`None` を区別して復元する
  - 変更から復元までを共有 mutex で直列化する ([ADR-041](adr/adr-041-test-isolation-patterns.md))
- `clip_for_message`: **メッセージに載る全フィールド種別**でテストする。どの種別があるかを
  列挙してから書く (数えるのを人間の記憶に頼らない)

- [ ] `GIT_DIR` (未設定を含む) と cwd を保存・復元する Drop guard を用意する
- [ ] 状態変更から復元までを共有 mutex で直列化する
- [ ] `warn_when_unresolved` の 4 通りをテスト
- [ ] `clip_for_message` を通る全フィールドを列挙し、それぞれでテスト
- [ ] 変異テストで各テストの判別力を確認
- [ ] **並列実行 (`cargo test` 既定) と直列実行の両方で green** — 片方だけで通るなら隔離が不完全

#### 完了基準

条件パラメータの両方の値、および truncation を通る全フィールドについて、変異を入れると
テストが落ちる。**かつ `cargo test` の並列実行で他テストを壊さない** (並列 / 直列の両方で green)。

---

## 夜間ループ停止の調査由来 (2026-08-22)

> 2026-08-20 / 08-21 の `nightly-todo` run 2 本 (run 87837551740 / 88134039080) が
> ともに `[NIGHTLY_DENY]` で停止した件の原因調査から起票した。**停止理由は 2 晩で別**で、
> 8/20 は Guard 禁止パスの変更 (順位 383)、8/21 は変更 0 件 (順位 228) だった。
>
> **ガードレールは設計どおり fail-closed で働いている。** 汚染された PR は 1 本も作られて
> いない。問題は「毎晩 agent を 1 回まるごと走らせて最後に必ず落ちる」経路が台帳側に
> 残っていること (Max 枠の空費) と、master の読み取りが pin されていないことである。
>
> 台帳の lane 引き取り (順位 383 / 454 / 368 / 360 / 361 を `✅` → `—`) と
> `claude/nightly-383` marker の削除は**即時の運用対処**として別途行う。本節の 2 件は
> その再発防止にあたる。

### 順位 486: auto lane の対象ファイルが Guard 禁止パスに当たる行を決定論的に弾く (順位 486)

> **動機**: 2026-08-20 の run が順位 383 を選び、agent が `src/lib-ledger/src/lib.rs` を
> 変更し、Guard step が `[NIGHTLY_DENY] 自律動作のガードレールを変更しているため push しません`
> で落とした。383 は**構造的に完了不能**である — 実装すれば Guard が拒否し、実装しなければ
> 「変更がありません」で落ちる。それが `✅` (auto lane) で割り当てられていた。
>
> **単発ではない**: auto lane 22 行を deny リストと全件照合したところ、**5 行が該当**した
> (383 / 454 / 368 / 360 / 361)。383 は発火済み、残り 4 件は未発火の地雷である。
>
> **本タスクの位置づけ**: [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2
> クラス 3 の判定を決定論化する。同 ADR 決定 6 の表がこの検査を
> 「**✅ 決定論だが未実装**」と自認しており、今回その穴が実際に発火した。
>
> **参照**: [`.github/workflows/nightly-todo.yml`](../.github/workflows/nightly-todo.yml)
> (Guard step の deny 正規表現 / agent プロンプトの禁止パス)、
> [`src/lib-ledger/src/deployed_ledger.rs`](../src/lib-ledger/src/deployed_ledger.rs)
> (実台帳を `cargo test` で検査する既存 precedent)、
> [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6
>
> **実行優先度**: **Tier 1** — Severity Medium (PR 汚染は起きない。実害は Max 枠の空費と
> 人間の marker 後始末) / Frequency **High** (残り 4 件が順に選ばれる) / Effort S /
> Adoption Risk None。

#### 背景

ADR-074 の**判定契約そのものは正しく書かれている** — 「そのタスクが変更するファイルのパスを
取り、Guard 禁止パスと比較する」。誤ったのは適用のほうで、2026-08-17 の選別は deny リストの
うち `.github/workflows/` としか比較していない。同 ADR は順位 360 / 361 / 454 について
「成果物が Rust ファイルなので誤除外するところだった」と記録しているが、その Rust ファイルの
パス (`src/lib-autonomy-policy/` / `src/cli-nightly-task-select/`) が**deny リストの別の
エントリに当たること**は見ていない。

**人手で正しく適用し続けることを前提にした契約は、この形で外れる。**

#### 設計決定 (案)

- 判定は「対象ファイル欄から変更対象パスを抽出 → deny リスト全体と比較」。抽出は
  `lib-ledger` の `parse_target_files` が既に持っている (注釈の丸括弧は本体から外れる契約)
- **deny リストの出所を 1 箇所にハードコードしない。** 同じリストは workflow の Guard step /
  agent プロンプト / ADR-072 決定 6 の 3 箇所にあり、その 3 点同期の検査は順位 454 が担当する。
  本タスクが 4 つ目の写しを作ると、454 が検査すべき対象が増えたのに 454 は知らない状態になる。
  **454 は 2026-10-05 に完了した**: 単一定義先は作らず、`cli-nightly-task-select` のテスト
  `guard_list_sync.rs` が 3 箇所の一致を検査する形にした。本タスクが写しを持つなら、その写しも
  同テストの比較対象に足すこと (足さなければ 4 つ目だけがずれても赤くならない)
- 検査の置き場所は 2 択。実装時に選ぶ
  - (a) `cargo test` で実台帳の全 `✅` 行を検査する (`deployed_ledger.rs` と同型。台帳を
    書き換えた時点で落ちる = 最も早く止まる)
  - (b) `cli-ledger-candidates` の出力に警告として載せる (選別作業の支援どまり)
  - **(a) を推す** — ADR-042 の「ルールではなく仕組み」に沿う。ADR-074 が人手適用で外れた
    のが本件の根因なので、支援情報を増やす (b) は同じ失敗を繰り返しうる
- **本タスク自身を auto lane に載せてはいけない** — 実装先 (`src/lib-ledger/` または
  `src/cli-ledger-candidates/`) が deny リストに当たる。この規則の最初の適用対象が本タスク自身である

- **順位 491 (台帳の実体整合検査、#447) とは統合しない** (2026-08-25 判断)。置き場所
  (`deployed_ledger.rs`) と走行タイミング (毎 `cargo test`) は共有するが判定ロジックは重ならない
  (486 = 宣言パス × deny リスト / 491-A = 順位集合の矛盾 / 491-B = 宣言パス × 実ファイルの中身)。
  共有部分の「対象ファイル欄 → パス抽出」は `parse_target_files` として実装済み

- [ ] deny リストの読み先を決める (順位 454 は単一定義を作らず 3 箇所の一致テストで決着した。写しを持つなら同テストに足す)
- [ ] 対象ファイル欄 × deny リストの照合を実装する
- [ ] **lane 引き取り前の台帳** (PR #440 の parent 時点) を fixture にして 383 / 454 / 368 / 360 / 361 の 5 件が検出されることを確認する
- [ ] **現行台帳では 5 件が既に `—`** のため検査が green になることを確認する
- [ ] deny 該当行を 1 行足すと落ちることを確認する (変異テスト)

#### 完了基準

Guard 禁止パスを成果物とする行に `✅` を付けると、**台帳を書き換えた時点で**決定論的に
検出されること。lane 引き取り前の台帳を fixture にすると 5 件が検出でき、現行台帳では
green になること。検査を外す変異で落ちること。

---

### 順位 487: nightly-todo の master 参照を SHA で pin する (順位 487)

> **動機**: 2026-08-21 の run が順位 228 を選んだが、228 は**その run の最中にマージされた
> PR #422 で実装済み**になり、agent は変更 0 件で終わった
> (`[NIGHTLY_DENY] 変更がありません`)。workflow は master を 3 回別々に読むのに、その間で
> SHA を pin していない。
>
> **実測 (run 88134039080 のログ)**:
>
> | 時刻 (UTC) | 読み取り | SHA |
> |---|---|---|
> | 18:08:28 | `master-ref` checkout (台帳・ゲートの調達元) | `7539551f` (#437) |
> | 18:08:33 | — | **PR #422 がマージ** → `868c9316` |
> | 18:08:57 | `git ls-remote` の着手済み判定 | `claude/nightly-228` は削除済 → 除外から外れる |
> | 18:08:57 | 台帳から選択 (**古い** master-ref を読む) | 228 を未実装として選択 |
> | 18:08:59 | `work` checkout (agent の作業ツリー) | `868c9316` = **実装済み** |
>
> 除外リストの推移も裏づけている — 8/20 は `着手済み順位=[228,324]`、8/21 は `[324,383]`。
>
> **本タスクの位置づけ**: 選択の入力 (台帳) と実装の対象 (作業ツリー) が別コミットを見る
> 経路を塞ぐ。31 秒の窓にマージが挟まれば再発する。
>
> **参照**: [`.github/workflows/nightly-todo.yml`](../.github/workflows/nightly-todo.yml)
> (`Checkout master (source of truth)` / `Count open claude/ PRs` / `work` の checkout)、
> [ADR-072](adr/adr-072-nightly-todo-loop.md) § 信頼境界の要
>
> **実行優先度**: **Tier 1** — Severity Medium (実害は空振り 1 回 + marker の後始末。
> 誤った実装が push される経路ではない) / Frequency Low (マージが窓に挟まったときのみ。
> 実測 1 回) / Effort S / Adoption Risk Low (workflow の checkout 引数のみ)。

#### 背景

3 つの読み取りのうち、**信頼境界上 master-ref が正**である (ADR-072 § 信頼境界の要 —
台帳・ゲート exe・config はすべて master ref の写しから調達する)。したがって pin の向きは
「work と ls-remote を master-ref の SHA に合わせる」であって逆ではない。

#### 設計決定 (案)

- `master-ref` の checkout 後に `git rev-parse HEAD` を step output へ出し、`work` の
  checkout を `ref: <その SHA>` にする
- **`ls-remote` による着手済み判定は SHA で pin できない** (リモートの現在のブランチ集合を
  見る操作であり、過去の SHA 時点の集合は取れない)。ここは別の扱いが要る。少なくとも
  「選択した順位が work 側で既に実装済みだった」場合を**空振りではなく識別可能な終了**に
  する。実装時に (a) marker を作らず正常終了扱いにする / (b) 専用の終了メッセージを出す
  のどちらかを決める
- BASE_SHA (publish-tree が checkout する基点) との整合を確認する。現状は work の HEAD を
  使っている。master-ref へ pin すると基点が数十秒古くなるため、push 時の
  non-fast-forward の有無を実装時に確認する
- **本タスクを auto lane に載せてはいけない** — `.github/workflows/` は deny リストに当たる
- **実装方針の変更 (2026-08-25)**: shell の修正ではなく、**判定を exe へ移してテストの場を作る**形で
  実装する。掃除ループを `cli-branch-cleanup` へ移した #466 (ADR-072 決定 1「回帰テストの場が無い判定を
  無人経路に置かない」) と同じ扱いで、本件は G1 (判定が I/O と癒着) の残り site である
  ([ADR-079](adr/adr-079-defect-origin-tagging.md) § G1 / G2 の出所)。上の設計決定 (案) の
  step output / checkout 引数は、exe が返す SHA を workflow が受ける形に読み替える

- [ ] `master-ref` の SHA を step output へ出す
- [ ] `work` の checkout をその SHA へ pin する
- [ ] BASE_SHA との整合を確認する (push が non-fast-forward にならないこと)
- [ ] 「選択した順位が既に実装済み」の終了経路を空振りと区別する
- [ ] `pnpm lint:workflows` green

#### 完了基準

台帳を読む step と agent が触る作業ツリーが**同一 SHA を見ている**ことがログから確認できる
こと。run の途中で master が進んでも選択と実装がずれないこと。


### 順位 498: 非主要拡張子の coverage を拡張子ごとに要求する (`other_ext_tests` の map 化)

> **実行優先度**: **Tier 2** — 検査の穴であり実害はまだ出ていないが、rule に非主要拡張子を足したときに coverage 不足を見逃す。

**動機**: `custom-lint-rules.toml` の `test_coverage` は主要拡張子 (`rs` / `toml` / `yaml` / `yml`) を `main_ext_tests: BTreeMap<拡張子, Vec<テスト名>>` で拡張子ごとに持つ一方、非主要拡張子は `other_ext_tests: Vec<テスト名>` で**拡張子との対応を持たない**。そのため `jsonc` と `json` を宣言し `jsonc` 用テストだけを登録した rule が検査を通る (PR [#461](https://github.com/aloekun/claude-code-hook-test/pull/461) の CodeRabbit 指摘)。
**これは実装漏れではなく契約**である — 順位 137 が定めた非主要拡張子の要件は「rule あたり 1+ positive test」で、`.claude/custom-lint-rules.toml` のコメントにもそう書いてある。契約を強める作業なので別起票にした。現行契約は `non_main_extension_coverage_is_per_rule_not_per_extension` が固定しており、意図せず緩んだ場合はそこで落ちる。

#### 作業内容

1. `CustomRuleTestCoverage::other_ext_tests` を `Vec<String>` から `BTreeMap<String, Vec<String>>` (拡張子 → テスト名) へ変える
2. `.claude/custom-lint-rules.toml` の既存 rule をすべて新形式へ移す。**平坦なリストを拡張子へ割り当て直すには、各テストがどの拡張子の fixture を実際に通しているかを読む必要がある** — ここが本タスクで一番時間を使う部分で、機械的な変換ではない
3. `extension_coverage_gaps` / `check_other_ext_coverage` を拡張子ごとの判定へ更新する
4. 契約を固定していた `non_main_extension_coverage_is_per_rule_not_per_extension` を、新契約 (拡張子ごと) を固定するテストへ差し替える

#### 完了基準

- `jsonc` と `json` を宣言し `jsonc` 用テストだけを持つ rule が **検査で落ちる**ことを、fixture ベースのテストで固定する
- 既存 rule のすべてが新形式で `rule_test_coverage_check` を通る (移行漏れがないこと)
- 実 `.claude/custom-lint-rules.toml` を読む検査が green のままであること

#### 無人可にしない理由

対象ファイルは Guard 禁止パスに当たらないが、**手順 2 が判断を要する** (どのテストがどの拡張子を通しているかの読み取り)。機械的な置換ではないため人間の lane に置く ([ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2)。
