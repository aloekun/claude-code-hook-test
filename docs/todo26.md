# TODO (Part 26)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: `docs/todo25.md` がファイルサイズ 50121 B (2026-08-23 時点、50KB = 51200 B の安定読み取り閾値まで残り 1079 B) に到達したため、新規エントリは本ファイルに記録する (2026-08-23 新設)。**新規エントリの追加先は本ファイル**。ほかの todo ファイル (現存する一覧は [docs/todo.md](todo.md) の preamble が正) の既存エントリは引き続き有効、相互に独立。
>
> **サイズ表記について**: 各記載は**その時点の計測値**であり、現在値と一致しないことがある。現在値が必要なら計測すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## タスク一覧

> 2026-08-25 の「自律実行ガードレールの写しずれ」調査で起票した順位 492 を先頭に置いていたが、
> 同順位は 2026-09-15 に完了・削除した。以後の起票もこの見出しの下へ追記している。


### 順位 503: doc と実装の同期を検査する (exit code 一覧 / 依存者リスト)

> **動機**: module doc に書いた事実が実装から乖離しても誰も気づかない箇所が 2 つある。
> PR [#456](https://github.com/aloekun/claude-code-hook-test/pull/456) /
> PR [#464](https://github.com/aloekun/claude-code-hook-test/pull/464) の post-merge feedback。
>
> - `main.rs` の `EXIT_*` 定数定義と module doc の「終了コード」一覧
> - `Cargo.toml` の依存と、lib 側 module doc が書いている「依存者リスト」
>
> **由来**: `[improvement]`。**実際に壊れた観測はまだ無い** — 予防のための検査であり、
> ADR-079 の線引きに従って defect を名乗らない。

#### 作業内容

- `EXIT_*` 定数と module doc の終了コード一覧が一致することを検査する (cargo test か docs-lint)
- `Cargo.toml` の依存追加時に、対象 lib の module doc 依存者リストが追随しているかを検査する

#### 完了基準

- どちらの検査も、片側だけを書き換えると落ちる
- 現行リポジトリで false positive を出さない

---

### 順位 504: 台帳検査の入力空間を埋める

> **動機**: F3 (順位索引の自己汚染) の実装中に、パーサが入力空間の一部で壊れることを
> 実測で見つけた。PR [#457](https://github.com/aloekun/claude-code-hook-test/pull/457) /
> PR [#458](https://github.com/aloekun/claude-code-hook-test/pull/458) /
> PR [#460](https://github.com/aloekun/claude-code-hook-test/pull/460) の post-merge feedback。
>
> **由来**: `[defect:G2]`。証拠 = PR #457。テストの場はあったが、`#[cfg(test)]` の宣言形の
> 全パターン (親ファイル型 / インライン型 / multi-arg のコンマ終端) を覆っていなかった。

#### 作業内容

- multi-arg `#[cfg(test)]` 関数のコンマ終端パターンの回帰テスト
- 実リポジトリに現存する `cfg(test)` 宣言パターンを全件走査して固定するテスト
- `declared_text` と `repository_text` の非対称性 (宣言先はテストコードを含む / 索引は含まない) の固定
- `SummaryRow.title` のような台帳の自由記述が出力前に `screen_for_public_output` を通ることの固定
- 統合テストの変異テスト標準化 (`split_ledger.rs` で確立した手順を convention として書く)
- **[ADR-049](adr/adr-049-incident-eval-regression-suite.md) に fix-induced-regression の case を追加**
  (fix が隣接エッジに穴を作った incident。case を書かないと再現テストの出所が失われる)

#### 完了基準

- 上記パターンを潰すと落ちるテストが揃っている
- ADR-049 の case 表に今回の incident が載っている

---

### 順位 506: 夜間ループと Node script 層の境界をテストで固定する

> **動機**: 無人経路と手元スクリプトで、実測して直した挙動がテストで固定されていない。
> PR [#466](https://github.com/aloekun/claude-code-hook-test/pull/466) /
> PR [#469](https://github.com/aloekun/claude-code-hook-test/pull/469) /
> PR [#470](https://github.com/aloekun/claude-code-hook-test/pull/470) /
> PR [#471](https://github.com/aloekun/claude-code-hook-test/pull/471) の post-merge feedback。
>
> **由来**: `[defect:G2]`。証拠 = PR #466 (fail-fast の後退を pre-push review が検出) /
> PR #470 (一時ディレクトリのリークを実測) / PR #471 (`change_id` でなく説明文で比較していた)。
> いずれもテストを書ける場にありながら、境界が覆われていなかった。

#### 作業内容

夜間ループ側 (B3):

- `workflow_dispatch` の `dry_run=true` 経路の E2E
- `delete_all` の fail-fast (最初の失敗で打ち切る) という loop-level boundary の固定
- `redact()` のスコープ差 (旧 shell の正規表現マッチ vs 新 exe の完全一致) の固定
- git I/O / network 失敗 / 権限まわりの corner case (SHA 不変、token ordering 等)
- pre-flight gate が背圧で deny し、`Select task from the ledger` step が `if:` で skip される経路

Node script 側 (B4):

- **一時ディレクトリの後始末が保証されること** — `process.exit()` は `finally` を実行せずに
  プロセスを終えるため、`try`/`finally` の中で呼ぶとリークする (実測で 4 個の残存を確認)。
  終了コードは `process.exitCode = main()` の経路で返す。**この形を固定する** (実測済み)
- `spawnSync` の timeout 動作 (`ETIMEDOUT`) — **実測済み、固定するだけ**
- コミット同一性検証が `change_id` を使っていること / `compareCommitSets` の双方向 — **実測済み、固定するだけ**
- `jj rebase -r` で親コミットが落ちる合成ブランチを CI で自動生成し、`pnpm rebase-nightly` が
  検出することを回す (**未実施**。手元の合成ブランチ検証を CI へ移す)

#### 完了基準

- 上記の各挙動を潰すと落ちるテストが揃っている
- 合成ブランチによる `-r` 事故の検出が CI で自動的に回っている

---


### 順位 508: 台帳追加候補の除外クラスを決定論で機械適用する

> **動機**: 2026-09-03 の weekly-review で、台帳未掲載 238 件から追加候補を選ぶ作業を人手で行った。
> [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) の除外クラス 1〜5 のうち複数は決定論で
> 判定できる (同 決定 6 が「対象パスの実在検査は決定論」と分類済み) のに、`ledger-candidates`
> step は**差集合を出すだけ**で絞り込みをしていない。結果、weekly-review の報告時点では
> 「238 件」という数しか見えず、**候補の見落としが構造的に起きる**。
>
> **由来**: `[improvement]`。実際に見落とした観測はまだ無く、運用改善のための機械化である。

#### 設計方針 (2026-09-03 ユーザー決定)

**LLM に適格判定をさせない。** [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 18 が
「skill は昇格を提案しない」と定めた由来は、LLM に適格判定を強制した旧方式が 2 週連続で失敗した
ことである (164 件中約 50 件 / 251 件中 13 件しか判定せず、いずれも「候補 0 件」と報告)。
したがって**禁じられたのは LLM による判定**であって、決定論による絞り込みではない。

決定 18 は「**LLM が適格判定しない**」と読み替え、skill の制約 (「件数と report パスを提示する
だけ」) を改訂する。

#### 機械適用する除外クラス

| クラス | 判定方法 |
|---|---|
| 1 グローバル `~/.claude` の編集 | 本文の語彙 |
| 2 実行環境依存 (hook 発火 / 実走 / `pnpm push` / e2e) | 本文の語彙 |
| 3 Guard 禁止パスの**書き換え** | 対象ファイル欄 × deny リスト (順位 486 が実装する検査と同一) |
| 4 ADR の起票・改訂 | 対象ファイルが `docs/adr/` |
| 5 判断留保 (再選定 / 検討 / 未定 / 複数案 / 着手時判断 / 要設計) | 注意欄・本文のキーワード走査 (順位 447 が実装する検査と同一) |
| **新規: `.claude/` 配下の書き換え** | 対象ファイル欄。agent の `Edit(work/**)` はドット始まりに届かない。**2026-09-15 に `custom-lint-rules.toml` だけが `config/` へ移設され除外対象から外れた** — `hooks-config.toml` 等の `.claude/` 配下は引き続き除外する ([ADR-006](adr/adr-006-config-driven-hooks.md) § 改訂)。Guard 禁止リストは `.claude/**` を持たないため、この分類が唯一の防波堤である |

**決定 3 の 3 種 (文書タスク / 並行性・ロックのテスト / 完了基準が二択) は機械適用しない** —
ADR-074 決定 6 が非決定論と分類済み。残った候補に対して人間が判断する。

#### 着手時判断

- 順位 486 / 447 が実装する検査と**同じ判定ロジックを 2 度書かない**こと。どちらを先に実装するか、
  共通化するかは着手時に決める
- 出力は `ledger-candidates.md` に統合するか、別 report にするかを決める

#### 完了基準

- weekly-review の報告に、除外クラス適用後の候補一覧 (順位 / Tier / 内容 / 除外されなかった理由) が出る
- 除外されたものは件数とクラス別内訳が出る (「0 件」と「未実施」を読み手が区別できる)
- ADR-072 決定 18 と weekly-review skill の制約が改訂されている

---

### 順位 510: 夜間ループの稼働状況を週次レビューで見張る

> **動機**: 直近 8 晩の夜間 run のうち **5 晩が red**、うち**直近 4 晩は連続**している
> (2026-08-30 / 08-31 / 09-01 / 09-02)。ところが 2026-09-03 の weekly-review が出した
> findings 8 件のうち、**夜間ループに言及したものは 0 件**だった。
>
> **なぜ気づけないか**: `weekly-review.yaml` は全 provider に `network_access: false` を課しており、
> facet はソースツリーしか読めない。**run の結果はネットワークの向こう側**にあるため、
> 現在の構成では原理的に観測できない。
>
> **実害**: 夜間ループは**開発作業で生まれたタスクの消化を助ける補助**であって、止まっても主線の
> 開発は進む。だからこそ**無音のまま何晩も過ぎる**。1 晩の red は agent 1 回分 (実測で
> 5.8 分・$1.75、run 33665621808) を捨てており、その間タスクの消化も進まない。
>
> 実際、2026-09-03 のセッションで見つかった 2 件はどちらも**人間がログを手で読んで初めて**
> 判明した — 順位 455 の権限拒否 (run 33665621808、`permission_denials_count: 2`) と、
> 順位 324 の空振り (run 90894308468)。weekly-review の出力には一度も現れていない。
>
> **由来**: `[defect:G1]`。証拠 = run 33665621808 / run 90894308468。観測の場そのものが無かった。

#### 置き場所

**L3 (skill) の決定論 scan** に置く。`gh` が要るため L2 (takt workflow) には置けない
([ADR-031](adr/adr-031-weekly-review-pipeline.md) § L2 に置けない決定論 scan は L3 が直接呼ぶ)。
`pnpm stale-branch-scan` / `pnpm ledger-residue-scan` と同じ配置になる。

#### 出す材料 (案)

- 直近 7 日の run の `conclusion` 集計 (success / failure / 未実行)
- red の run について `[NIGHTLY] cleanup=... publish=... handoff=...` のサマリ行 (どの段で止まったか)
- handoff marker の現存一覧と、それが指す順位
- 連続 red の日数 (「今週たまたま 1 晩落ちた」と「4 晩連続で助けが止まっている」を区別する)

#### 着手時判断

- **どこまでログを読むか**。run の `conclusion` だけなら `gh run list` で軽いが、停止段まで出すには
  各 run のログ取得が要る (1 run 数 MB)。直近 7 日ぶんを毎週取るコストと得られる情報を比較して決める
- agent の消費 (`num_turns` / `total_cost_usd`) を出すかどうか。出せば「回して捨てた量」が見えるが、
  ログ本文の取得が前提になる

#### 統合した材料: 自律アクションの週次棚卸し (2026-09-28、ハーネス改善計画 WP-19 ステップ 3)

ハーネス改善計画 (2026-09-28 退役) の WP-19 ステップ 3「自律アクション一覧を weekly-review の入力に
足し、自律動作の週次棚卸しを人間のレビューポイントとして固定する」は、本エントリと同じ scan・同じ
置き場所になるため、新しい順位を立てずにここへ統合した。追加で出す材料:

- `claude/` ブランチの PR 一覧と状態 (open / merged / closed-without-merge)。浮きブランチ検出は
  `cli-stale-branch-scan` (#377) が既に持つので、重複させず出力を合流させる
- **無人 PR の採用率** (人間がマージした割合) — [ADR-072](adr/adr-072-nightly-todo-loop.md) § 試験運用判断基準
  の decision trigger。**判定期限は 2026-11-06**、測定起点は 2026-08-10
- 日付ごとの run 有無 — PC 電源オフの週末をまたいでも schedule run が欠けずに回っているか
  (WP-17 の受け入れ基準で未検証のまま残った項目)

#### 完了基準

- red が続いている週に、weekly-review の報告へ必ずその事実が現れる
- 無人 PR の採用率が週次の報告に数値で出る (ADR-072 の判定期限までに判定できる)
- 停止段が分かる粒度で出る (「red が 4 晩」だけでなく「guard で 3 晩、verify で 1 晩」)
- 取得に失敗した週は「未確認」と明示される (「0 件」と書かない)

---

### 順位 511: `todo-summary2.md` を 3 分割し、明示列挙している呼び出し元を追随させる

> **動機**: `docs/todo-summary2.md` が **79KB** に達した (50KB が Claude Code の読み取り安定閾値)。
> 2026-07-20 に `todo-summary.md` から分割した後半で、順位が増えるたびに伸び続ける。
>
> **機構側は「一部だけ」3 分割へ対応済み**。Phase F の F1 で name prefix を `SUMMARY_FILE_PREFIX`
> 1 箇所に集約し、`docs_files.rs` の列挙は `todo-summary*.md` を glob するため、**cli-docs-lint の
> 各 check は新しい part を追加するだけで拾う** (テストは `todo-summary3.md` を fixture に使う)。
>
> **一方、台帳削除の経路は 2 ファイル決め打ちのままである。**
> [`src/cli-ledger-cleanup/src/apply.rs`](../src/cli-ledger-cleanup/src/apply.rs) の `plan_summary_removal`
> は `["todo-summary.md", "todo-summary2.md"]` を配列でハードコードしており (88 行)、**3 分割すると
> 第 3 part に載った順位の後始末が「順位 table にありません」で失敗する** (CodeRabbit #473)。
> 夜間ループのマージ経路が壊れるため、分割と同じ PR で直す必要がある。
>
> **由来**: `[improvement]`。閾値超過は観測しているが、読み取りが実際に壊れた観測はまだない。

#### 追随が要る「明示列挙している呼び出し元」

glob ではなく 2 ファイルを並べている 3 箇所は手で足す必要がある。

- `package.json` の `ledger-candidates` スクリプト (`--summary-file` ×2)
- `.github/workflows/nightly-todo.yml` の `Select task from the ledger` step (`--summary-file` ×2)
- **`src/cli-ledger-cleanup/src/apply.rs` の `plan_summary_removal`** (配列のハードコード。ここが漏れると台帳の後始末が失敗する)

加えて `docs/todo.md` の preamble routing 表を更新する。

#### 着手時判断

- **どこで切るか**。順位の境界をどこに置くかは、ファイルサイズと「よく参照する範囲」の兼ね合いで決める
- workflow を触るため [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パスに該当し、**auto lane には載せられない**

#### 完了基準

- 3 つの part すべてが `pnpm lint:docs` / `cargo test` の検査対象に入っている (`todo-summary3.md` を足しても検査が素通りしない)
- 夜間ループの選択が 3 part すべてを見ている (`--summary-file` の追随漏れがない)
- **第 3 part に載った順位を `cli-ledger-cleanup --apply` が後始末できる** (`apply.rs` のハードコードが解消されている)
- 2 ファイル決め打ちが再発しないよう、列挙は `docs_files.rs` の共有層を使うか、使えない理由が書かれている

---

### 順位 512: 50KB 超の詳細エントリファイル (`todo14.md` / `todo22.md`) を分割する

> **動機**: `docs/todo14.md` (61KB) と `docs/todo22.md` (59KB) が閾値を超えている。どちらも
> 「編集・完了削除専用」で新規追加はされないが、既存エントリが残る限り縮まない。
>
> **由来**: `[improvement]`。

#### 作業の性質

**詳細エントリの移動は順位 table の「ファイル」列とセットである。** 移動した各エントリについて
`docs/todo-summary*.md` の該当行が指すファイル名を更新しないと、`entry_pairing` 検査 (順位 441 /
Phase D の D3) が 1:1 対応の破れとして落とす。件数に比例して差分が増える。

#### 着手時判断

- **分割するか、完了エントリの削除で足りるかを先に測る**。両ファイルの全エントリについて、
  対応する順位が順位 table に現存するかを確認し、孤児があればまず削除する
- 分割する場合の新ファイル名 (連番の次) と、`docs/todo.md` preamble への追記

#### 完了基準

- 両ファイルが 50KB 未満
- `pnpm lint:docs` の entry-pairing が緑 (移動したエントリの参照がすべて追随している)

---

### 順位 513: 50KB 超の恒久ドキュメント (ADR-072 / 台帳 / workflow 2 件) の扱いを決める

> **動機**: 週次の file-length watchlist は `docs/todo*.md` と `src/**/*.rs` しか見ていないため、
> **より大きい恒久ドキュメントを構造的に見逃している**。2026-09-03 の実測:
>
> | サイズ | ファイル | 性質 |
> |---|---|---|
> | 126KB | `docs/adr/adr-072-nightly-todo-loop.md` | 恒久 ADR。決定と検証記録が追記され続ける |
> | 60KB | `docs/claude-code-web-tasks.md` | 台帳。恒久 |
> | 67KB | `.github/workflows/nightly-todo.yml` | workflow。コメントが厚いこと自体が価値 |
> | 64KB | `.github/workflows/pr-monitor.yml` | 同上 |
>
> **由来**: `[improvement]`。
>
> **watchlist の走査範囲そのものが問題**である。閾値を超えたファイルに気づけない構造が
> 週次レビューに残っている (2026-09-03 のセッションで、報告されていた 3 件より大きい
> 4 件が見えていなかった)。

#### 着手時判断

**機械的な分割では済まない。** 以下をタスクごとに決める必要がある。

- **ADR-072**: 決定本文と検証記録を分けるか。ADR は 1 決定 1 ファイルが原則で、分割は参照の
  付け替えを伴う。「検証記録だけを appendix ファイルへ出す」案が最有力だが設計判断
- **台帳**: 恒久かつ夜間ループの選択元。分割は選択ロジックに影響する
- **workflow 2 件**: コメントを削ると設計意図が失われる。「コメントを ADR へ移して本体を薄くする」
  のは可能だが、**その場で読める価値**とのトレードオフ
- **watchlist の走査範囲拡張**: `docs/**/*.md` と `.github/workflows/*.yml` を対象に加えるか。
  加えると恒久ファイルが毎週報告され続けるため、「閾値超過が N 週続いたら報告」等の設計が要る

#### 完了基準

- 4 ファイルそれぞれについて「分割する / しない (理由つき)」が決まっている
- watchlist の走査範囲が、決めた方針と整合している

---

### 順位 514: パーサ堅牢化を仕組みで担保できるか調べる

> **動機**: 外部コマンド (`jj` / `gh` / `git`) の出力を parse する箇所で、**同じ形の穴が繰り返し出ている**。
> 直接の由来は 2 件:
>
> | 由来 | 何が起きたか |
> |---|---|
> | #479 (`stray.rs`) | `jj diff --summary` の `D` (削除) を合成テストのみで書いて見落とした。実出力を確認して初めて判明 |
> | #313 (`summary_line_new_path`) | 8 行の関数に 4 iteration で 5 件のパーサ edge case が段階的に発見された |
>
> **由来**: `[improvement]`。いずれも出荷前に捕まえており、不具合として外へ出てはいない。
>
> **本エントリの位置づけ**: post-merge feedback (#479 Tier2 #1) の採用先として当初 順位 461
> (dev-conventions.md への一括追記) を想定したが、**その出口を採らない方針** (決定事項は ADR、
> それ以外は仕組み化。dev-conventions.md は縮小方向) が 2026-09-07 に示されたため、
> 「規約を書く」ではなく「仕組みにできるか」を先に決める形へ組み替えた。

#### 着手時判断

規約にせず仕組みで担保できるかを、次の 3 案で比較してから決める。

- **案 1: 網羅を型で強制する** — status 文字を catch-all 無しの enum へ落とし、`match` の網羅性
  検査に載せる。新しい種別が増えたらコンパイルが通らない。#479 の `D` はこれで防げた。
  ただし「その enum を作る」判断自体は人が要る
- **案 2: 実出力を fixture として取り込む契約** — パーサのテストが実コマンド出力から採った
  fixture を読むことを、テスト側の構造 (共有ヘルパ経由) で強制する。ADR-049 の
  incident fixture 方式が既にあるので、その適用範囲を広げる形になる
- **案 3: 計測だけ入れて様子を見る** — 「同一関数への連続レビューラウンド数」を測り、
  実際に繰り返しているかを数で確かめてから対策を選ぶ

**どれも「ルールを増やすだけ」にはしない。** 案 3 を採る場合も、計測の出力が次の判断材料に
なる形 (telemetry かレビュー記録) まで含めて設計する。

#### 完了基準

- 3 案から採る案が決まり、その根拠 (実測か、防げた範囲の見積り) が残っている
- 採らなかった案について、なぜ採らないかが読める


---

### 順位 518: 順位 516・517 の再評価 (実害が観測されたときだけ着手する見送り follow-up)

> **動機**: 順位 516 (`jj new` 忘れの hook 検知) と 517 (分割 refactor の test count 一致の spike) を
> 2026-09-12 に取り下げた。どちらも実害の観測ではなく「dev-conventions.md を短くしたい」が動機で、
> 前提検証の結果、解くべき問題が無いと判断した。見送りの根拠と実測は
> [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) § 改訂 2026-09-12 が持つ。
> 本エントリは順位 261 の 3 点セット (ADR 記録 / 計画文書の状態更新 / 再評価トリガー付き follow-up) の
> 3 つ目にあたる。**「現時点では見送り」を表現するための行であり、着手対象ではない。**
>
> **由来**: `[improvement]`。
>
> **実行優先度**: Tier 5 — トリガーが観測されるまで着手しない。

#### 再評価トリガー (どちらかが観測されたときのみ)

- **516 の案**: `jj new` 忘れによる別作業の混入が、feedback レポートか incident として再度観測されたとき。
  再評価時は SessionStart 方式ではなく、同一セッション内の作業境界を捉えられる設計から考え直す
  (SessionStart 方式が由来 incident に届かないことは ADR-042 の改訂に実測済み)。
- **517 の案**: 分割 refactor でテストが消えたまま merge された事例が観測されたとき。
  再評価時は「全 PR に効くゲートへスコープが変わる」代償を、観測された実害と比較する。

#### 完了基準

- トリガーが観測されて再評価し、その結果が ADR-042 に追記されている
- または、トリガーが観測されないまま次の月次 ROI レビュー (ADR-062) 2 回分を経過し、本行を削除した

---

### 順位 521: `scope_guard` の bounded-lifetime 判定に必要な実績が 48 日集まっていない

> **動機**: ADR-054 の prompt injection 防御 layer 3 (`scope_guard`) は `enabled=true` / `mode="enforce"` で
> 稼働しているが、enforce mode 開始 (2026-08-01) から観測時点 (2026-09-18) までの 48 日間
> (両端を含めない日付差) で fix step の実行実績が 0 件で、ADR-054 が定める bounded-lifetime の
> 決定トリガー「enforce mode で 3〜5 PR」を満たせていない。
> 設計そのものは健全だが効果を検証したデータが存在せず、**決定期限が事実上無期限に延びている**。
> ADR-039 の experimental feature 標準パターンは bounded lifetime を要求しており、期限が動かない状態は
> その逸脱にあたる。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-09-18-C01 で採用 (severity=medium, facet=security, category=prompt-injection)
>
> **参照**: `.claude/weekly-reviews/2026-09-18.md`、`src/cli-pr-monitor/src/stages/scope_guard.rs`、
> [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)、
> [ADR-039](adr/adr-039-experimental-feature-standard-pattern.md)、
> [ADR-055](adr/adr-055-firing-telemetry-collection.md) (発火実績の観測基盤)

#### 背景

`scope_guard` は post-pr の fix step が「レビュー指摘と無関係なファイルを書き換える」ことを block する層で、
判定機会は fix step が走ったときにしか来ない。つまり**この層の検証は fix step の発生頻度に従属**しており、
頻度が低いまま期限だけが過ぎる構造になっている。ADR-039 は「期限までに判定材料が集まらなければ既定 OFF へ
revert する」ことを求めているので、**材料が集まらなかったこと自体を判定結果として扱う**必要がある。

なお「実績 0 件」は今回 facet が読んだ範囲での観測であり、決定論的な計数ではない。次回以降は印象でなく
数で読めるようにしておく必要がある。

#### 設計決定 (案)

- bounded-lifetime の決定期限を 2026-10-31 として ADR-054 に明記する
- 期限までに fix step が 1 度も発生しなければ `scope_guard` を既定 OFF へ revert し、
  理由 (検証機会が来なかった) を ADR-054 に記録する。「効果が無かった」とは書かない — 観測できていないだけ
- `scope_guard` の判定結果 (許可 / block / skip) を telemetry (ADR-055) の発火として記録するか判断する。
  記録すれば「実績 0 件」を週次レビューと月次 ROI レビューの両方で機械的に読める
- **着手時判断**: telemetry 記録の追加を先に入れるか、期限の再設定だけで済ませるかは着手時に決める。
  前者は ADR-055 の id 契約に触れるため、月次レビュー (ADR-062) の集計軸との整合を確認してから入れる

- [ ] ADR-054 に bounded-lifetime の決定期限 (2026-10-31) と、材料が集まらなかった場合の既定 OFF revert を明記する
- [ ] `scope_guard` の判定結果を telemetry 発火として記録するか判断し、採るなら実装する
- [ ] 期限到来時に enforce 継続 / 既定 OFF revert のどちらかを決め、理由を ADR-054 に残す

#### 完了基準

ADR-054 に決定期限と「材料が集まらなかった場合の既定 OFF revert」が書かれており、期限到来時の判定が
印象ではなく観測値で行える状態になっている。

---

## 不具合収束計画・Claude Code Insights フォローアップの退役に伴う移送 (2026-09-28)

> 2 つの ephemeral 計画書を退役させた際、未着手の作業を順位として移したもの。設計判断は [ADR-079](adr/adr-079-defect-origin-tagging.md) (効果測定) と [ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) § 追記 2026-09-28 (ルール撤廃の型) にある。**いずれも auto lane に載せない** — 523 は `src/lib-ledger/` 系、524-526 は hook / PR 作成 / マージ経路という、無人経路が自分を縛る層を書き換えるため。

### 順位 523: defect 流入の週次集計と退出基準の判定 (機4b)

> **動機**: [ADR-079](adr/adr-079-defect-origin-tagging.md) は由来タグの付与と検査 (機4a、#472) までを実装し、集計と判定を本項へ分けた。タグが溜まらないと集計の設計を実データで確かめられないためである。これが無いと「機構を足したから defect が減った」を印象でしか言えない。
>
> **参照**: ADR-079 § 退出基準 (週次判定の定義) / § 再分類は緩める向きだけ根拠を要求する、`src/lib-ledger/src/summary_gate.rs` (`ORIGIN_BOUNDARY_RANK`)、`src/cli-ledger-candidates/`

- [ ] `cli-ledger-candidates` に境界順位以降の `[defect:*]` 行を ISO 週別に数える集計を足す (起票日は行を追加したコミットの author date)。**完了した `[defect:*]` 行は台帳から削除される** (初例: 順位 500、2026-09-30) ため、現在の summary だけを読むと完了済みの流入が数えられない。行の追加を履歴から数えること
- [ ] 退出基準の判定 (4 週非増加かつ `w4 < w1`、全週 0 件は充足、観測なし週は除外して窓を伸ばす) を純関数で実装する
- [ ] `[defect:*]` → `[improvement]` の再分類に `再分類根拠:` を要求する検査を足す (前の版との比較が要る)
- [ ] weekly-review の決定論 scan から呼ぶ
- [ ] マージ後、保留中の post-merge feedback を一括で採否する (ADR-079 § post-merge feedback の採否は機4b のマージ時にまとめて行う。#454 の `read_dir(...).flatten()` 横断検知が採用候補)

#### 完了基準

weekly-review の報告に defect 流入の週別件数と退出基準の判定結果が出ること。緩める向きの再分類が根拠なしで通らないこと。

---

### 順位 524: `-u` 無しの `jj squash` を PreToolUse で止める

> **動機**: source と destination の両方に description があると `jj squash` は結合用の editor を開き、headless のセッションでは応答が返らず止まる。回避策 (`-u` = `--use-destination-message` か `-m`) は memory にしか無く、ルールのまま残っている (ADR-042 § 撤廃の 3 つの型 の型 A)。
>
> **参照**: `src/hooks-pre-tool-validate/src/presets/`、`.claude/hooks-config.toml` `[pre_tool_validate] blocked_patterns`、memory `jj-squash-editor-hang-headless`

- [ ] `jj squash` で `-u` / `--use-destination-message` / `-m` / `--message` のどれも無い形を検出する preset を足す
- [ ] ブロック時の案内に代替コマンドを出す
- [ ] 既存 preset と同じく good / bad の両方をテストで固定し、toml への登録漏れを既存テストで捕まえる (#522)

#### 完了基準

`-u` 等の無い `jj squash` がブロックされ、付けた形は通ること。

---

### 順位 525: `pnpm create-pr` の `--body` 引数を物理削除する

> **動機**: `--body` は複数行の本文を 1 行目で切り捨てる事故を起こし、改行を再結合する workaround で延命している。安全な `--body-file` 経路が既にあり、prepare-pr skill もそちらを使う。誤用される入口そのものを消す (ADR-042 § 撤廃の 3 つの型 の型 B)。
>
> **参照**: `src/cli-pr-monitor/src/stages/create_pr.rs` (`--body` の受け付けと再結合)、prepare-pr skill、memory `create-pr-multiline-body-truncation`

- [ ] `create_pr.rs` から `--body` の受け付けと再結合 workaround を撤去し、`--body` 指定時は `--body-file` を案内して失敗させる
- [ ] 呼び出し側の案内 (skill / ADR-028 周辺の記述) を `--body-file` に揃える
- [ ] 再結合のテストを「`--body` は拒否される」テストへ置き換える

#### 完了基準

`pnpm create-pr --body ...` が本文を作らずに失敗し、`--body-file` を案内すること。

---

### 順位 526: docs-only PR では post-merge feedback を起動しない

> **動機**: docs-only PR の post-merge feedback は採否判断ごと行わない運用 (ユーザー決定) だが、merge-pipeline は起動してしまい Max 枠と時間を使う。docs-only の決定論判定は [ADR-057](adr/adr-057-docs-only-deterministic-routing.md) の `lib-docs-policy` に既にあるので、起動判定へ繋ぐだけでよい (ADR-042 § 撤廃の 3 つの型 の型 C)。
>
> **参照**: `src/cli-merge-pipeline/` (feedback 起動判定)、`src/lib-docs-policy/src/lib.rs` (`is_docs_only_summary`)、memory `no-feedback-adoption-for-doc-prs`

- [ ] merge-pipeline の feedback 起動前に PR の変更範囲を `lib-docs-policy` で判定し、docs-only なら skip して理由を 1 行出す
- [ ] 判定を写経せず `lib-docs-policy` を呼ぶ (ADR-081)
- [ ] docs-only / 混在の両方をテストで固定する

#### 完了基準

docs-only PR のマージで post-merge feedback が起動せず、skip 理由がログに残ること。

---

### 順位 527: 永続文書から揮発性の成果物への参照を棚卸しする

> **動機**: `docs/` から gitignored の `.claude/feedback-reports/` / `.takt/runs/` / `.claude/weekly-reviews/` への参照が 247 件ある (2026-09-28 実測)。clone 先・CI・クラウドには参照先が無く、根拠を辿れない。規律と検査は順位 358 が担当し、本項は既存違反の後始末を分けたもの (Claude Code Insights 2026-08-11 の「レビュー履歴が監査不能」指摘、2026-08-12 採用)。
>
> **参照**: 順位 358、`rg -n "feedback-reports/|\.takt/runs/|weekly-reviews/" docs`

- [ ] 違反を列挙し、根拠の要旨を committed 側へ転記するか参照を削るかを 1 件ずつ決める (件数が多ければ複数バッチに分ける)
- [ ] 「転記 + 出典として揮発パスを添える」は許容、「参照のみ」を無くす

#### 完了基準

`docs/` の各参照について、揮発パスを読まなくても根拠の要旨が committed 側で読めること。

---

### 順位 528: security-review / supervisor-validation の output-contract を用意する

> **動機**: `.takt/facets/output-contracts/` には `simplicity-review.md` しか無く、`pre-push-review.yaml` の `security-review` / `supervisor-validation` は `format:` 名だけ宣言して契約ファイルが無い (2026-09-28 確認)。verdict 欄の書式が facet の自由記述に委ねられている (Insights「rubric が暗黙」指摘の残件、2026-08-12 採用)。
>
> **参照**: [ADR-048](adr/adr-048-facet-findings-handoff-markdown-contract.md)、[ADR-056](adr/adr-056-review-policy-anomaly-shadow.md)、memory `takt-output-contract-checklist`

- [ ] `simplicity-review.md` を雛形に 2 ファイルを作る (builtin の列構造と casing をミラー / 全 finding 節で列を揃える / finding_id は new・persists・resolved・reopened を通じて不変と明記)
- [ ] 追加 PR 自身の pre-push で dogfood する

#### 完了基準

`pre-push-review.yaml` が宣言する `format:` のすべてに契約ファイルがあること。

---

### 順位 529: 蓄積した feedback レポートを横断して反復する指摘を抽出する (承認付き)

> **動機**: `.claude/feedback-reports/` に 100 件超のレポートがあるが、分析は PR ごとに閉じている。同型の指摘の反復や却下理由の傾向は横断しないと見えない (Insights Horizon 提案のうち承認境界を保つ版、2026-08-12 採用)。完全自動化 (承認なしのルール生成) は不採用と決定済み。
>
> **参照**: `.takt/facets/instructions/aggregate-feedback.md` (承認規約)、[ADR-072](adr/adr-072-nightly-todo-loop.md) (台帳登録はユーザー承認必須の信頼境界)、順位 403 (レポートの主張は実測で二重検証)

- [ ] ローカル実行の分析 (skill か takt facet) で系統別のクラスタと防止策案を**提案レポートまで**生成する
- [ ] 台帳登録・機構化は従来どおりユーザー承認を経る。提案の根拠は実測で確かめる (ADR-075)

#### 完了基準

横断分析の提案レポートが 1 回生成され、採否がユーザー判断で決まること。ルールを足すだけの提案は ADR-042 § 追記 2026-09-28 により却下扱いになる。
