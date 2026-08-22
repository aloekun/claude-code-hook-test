# TODO (Part 25)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo24.md がファイルサイズ 50869 B (2026-08-22 時点、50KB = 51200 B の安定読み取り閾値まで残り 331 B) に到達したため、新規エントリは本ファイルに記録する (2026-08-22 新設、週次レビュー 2026-08-22 実行セッションで検出)。**新規エントリの追加先は本ファイル**。todo.md / todo3.md 〜 todo24.md の既存エントリは引き続き有効、相互に独立。
>
> **サイズ表記について**: 各記載は**その時点の計測値**であり、現在値と一致しないことがある。現在値が必要なら計測すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 週次レビュー採用 (2026-08-22)

### jj の working copy materialize による mtime リセットで「最近 fetch した」「書き込み中」判定が壊れる

> **動機**: `fetch_head_is_recent()` が `.git/FETCH_HEAD` の mtime を「最後に fetch した時刻」として扱っているが、jj が working copy を materialize する際 (`jj new` 等) に全ファイルの mtime が checkout 時刻へ書き換わる。同じ根因で `holder_still_writing()` が空ロックファイルの「書き込み中」判定に mtime を使っており、プロセスクラッシュ後に残った古い空ロックファイルが「たった今作成された」と誤認される。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-22-J01 / WR-2026-08-22-J02 で採用 (severity=high, facet=jj-robustness, category=jj-mtime-staleness)。**2 件は同一の根因クラス**なので 1 タスクとして扱う。
>
> **参照**: `.claude/weekly-reviews/2026-08-22.md`、`src/hooks-session-start/src/jj_helpers.rs` (`fetch_head_is_recent`)、`src/cli-pr-monitor/src/lock.rs` (`holder_still_writing`)

#### 背景

どちらも「ファイルの mtime = そのファイルに対する最後の意味ある操作の時刻」を前提にしている。jj はこの前提を破る — working copy を materialize するとき、内容が変わっていないファイルも含めて mtime が更新されうる。ADR-021 (jj 変更検出ロジックの設計原則) が「commit_id 単独比較の限界」を扱っているのと同じ系統の問題で、**mtime を状態の代理として使うこと自体**が jj 環境では成立しない。

#### 設計決定 (案)

- `fetch_head_is_recent()`: mtime ではなく fetch 実行側が残す明示的な記録 (タイムスタンプファイル / state JSON) を真実源にする。あるいは `jj git fetch` の実行そのものを記録する
- `holder_still_writing()`: 空ロックファイルの「書き込み中」判定を mtime から切り離す。lock 取得側が PID や開始時刻を**内容として**書き、空ファイル = 未完了と扱う (現状は空ファイルを Held 扱いにする設計が memory `verify-concurrency-by-observation` にある — その方針と整合させる)
- **どちらも実測で確かめる**: jj の materialize が実際に mtime を書き換えることを観測してから直す (推論で直すと、直っていないことに気づけない)

- [ ] jj materialize による mtime 書き換えを実測で再現する
- [ ] `fetch_head_is_recent()` を mtime 非依存にする
- [ ] `holder_still_writing()` を mtime 非依存にする
- [ ] 両方に回帰テスト (mtime を人為的に巻き戻しても判定が変わらないこと)

#### 完了基準

mtime を書き換えても両判定の結果が変わらないこと。変異テストで、mtime 依存へ戻すとテストが落ちること。

---

### ADR-032 の「永久欠番」決定が CLAUDE.md の ADR index へ未反映

> **動機**: ADR-032 は 2026-08-12 に「docs-only fast-path として reserved」と判定され、実装は別設計の ADR-057 が実現した。todo.md では「永久欠番として扱う」と決定済みだが、CLAUDE.md の ADR index 等へ未反映で、決定と実態が乖離している。
>
> **本タスクの位置づけ**: 週次レビュー WR-2026-08-22-A01 で採用 (severity=high, facet=architecture, category=adr-alignment)。
>
> **参照**: `.claude/weekly-reviews/2026-08-22.md`、`CLAUDE.md` (ADR index)

#### 背景

CLAUDE.md の ADR index は ADR-001 から ADR-074 まで連番で並ぶが、ADR-032 の行が無いまま番号だけが飛んでいる。読み手には「抜けている」のか「意図的な欠番」なのか判別できず、新規 ADR を起こす人が 032 を再利用しかねない。

#### 設計決定 (案)

CLAUDE.md の ADR index に ADR-032 の行を **欠番として明示**して戻す (例: `ADR-032: (永久欠番 — docs-only fast-path は ADR-057 が別設計で実現)`)。番号の再利用を防ぐのが目的なので、リンク先の実ファイルは作らない。

- [ ] CLAUDE.md の ADR index に欠番行を追加
- [ ] 他に ADR-032 を参照している箇所が無いか確認 (`grep -rn "ADR-032"`)

#### 完了基準

ADR index を通読して 032 が意図的欠番と分かること。番号の再利用が起きない。

---

### lib-* crate の責務分類基準が ADR-012 に無い

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

### 50KB 超過 3 ファイルの物理分割

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

### PostToolUse で docs ファイルの 50KB 超過を即時ブロックする

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

### `lib-subprocess` の失敗経路を塞ぎ切る (順位 481)

> **動機**: PR #436 (順位 323) で timeout の孫プロセス穴を塞いだが、**正常終了経路
> (`Some(_)` 分岐) の reader thread join だけが `join_within_grace` を経由せず無制限のまま**
> 残っている (実コードで現存を確認済み)。同 PR が修正した Major と同型のギャップで、
> バックグラウンド化した孫がパイプを握り続けるケースで同じ hang が理論上再発する。
> あわせて、同 PR のテストが 2 度空振りした経験から、テスト側の補強も同じ単位で行う。
>
> **本タスクの位置づけ**: post-merge feedback 採用 (#436 Tier1 #1 = bug_fix / Severity High /
> Effort XS、#436 Tier2 #1・#3 = test_addition / Severity Medium / Effort S)。
>
> **参照**: `.claude/feedback-reports/436.md`、`src/lib-subprocess/src/lib.rs` (`run_cmd_shell_with`)、
> [PR #436](https://github.com/aloekun/claude-code-hook-test/pull/436)

#### 背景

`run_cmd_shell_with` は失敗経路 (timeout / wait 失敗) では `kill_process_tree` + `join_within_grace`
で上限付きに回収するが、正常終了経路は素の `.join()` のまま。実 callsite に該当パターンは
未観測 (Frequency Low) だが、**同型のギャップが 1 箇所だけ残っている**状態は次に触る人を誤らせる。

#### 設計決定 (案)

- **正常終了経路も上限付きにする。「理由を doc に書いて無制限のまま残す」は選択肢にしない**
  (PR #439 CodeRabbit Major)。子が終了しても子孫がパイプを握れば hang するのは失敗経路と同じで、
  文書化ではその hang を 1 ミリ秒も縮められない。当初案は「上限を入れるか、入れない理由を doc に
  記録する」と両論併記していたが、**後者は問題を解決しない**
- 上限値を失敗経路と同じにするかは決める。正常終了では出力を取りこぼす損失が失敗経路より重いので、
  猶予を長くするか、**上限に達したこと自体を戻り値や警告で可視化する**かを検討する
  (黙って切ると「コマンドの全出力」と誤読される)
- 既存 timeout テストに **elapsed time assert** を追加する。戻り値の文言だけを見るテストは
  PR #436 で実際に穴を素通りさせた (T6 / PR #283 と同型)
- 非 UTF-8 fixture は `Cursor` で直接バイト列を流す形を維持し、**生成→読み取り→検証の全経路**が
  変異テストで判別することを確認する (シェル経由版はクォートが崩れて不正バイトを 1 つも
  出しておらず、変異テストで素通りした)

- [ ] 正常終了経路の join を上限付きにする
- [ ] 正常終了時に上限へ達した場合の扱い (猶予値 / 可視化) を決める
- [ ] **子孫がパイプを握ったまま子が正常終了するケース**の決定論的テストを追加する
      (`orphan_tests` のプローブを流用できる — 孫だけ生かして親を exit 0 で終わらせる)
- [ ] 既存 timeout テストへ elapsed assert を追加
- [ ] 非 UTF-8 テストの全経路を変異テストで確認
- [ ] `cargo test --workspace` green / 実 Linux (WSL) でも確認

#### 完了基準

**子孫がパイプを握ったまま子が正常終了しても、上限時間内に制御が戻ることをテストで固定**する
(経過時間 assert)。上限を外す変異を入れるとそのテストが落ちること。
`run_cmd_shell_with` のすべての join 経路について、上限値と理由が doc から追える。

---

### 外部コマンド呼び出しの落とし穴を lint で塞ぐ (順位 482)

> **動機**: PR #435 と #437 で、外部コマンドの**無言の切り捨て / 破壊**を 2 件続けて踏んだ。
> (1) `gh pr view --json files` は 100 件で無言に切り捨てる (実測: 185 ファイルの PR で 100 件)。
> (2) `git push --delete` を lease 無しで撃つと、観測から削除までの間に他経路が push した作業を
> 消す。どちらも「書いた時点では気づけず、実行時に静かに壊れる」形。
>
> **本タスクの位置づけ**: post-merge feedback 採用 (#435 Tier1 #1 / Severity High / Effort M、
> #437 Tier1 #3 / Severity High / Effort M)。**規約 (Tier 3) ではなく lint で塞ぐ**判断
> (ユーザー判断: 規約追記は再発を防げなかった実証がある)。
>
> **参照**: `.claude/feedback-reports/435.md` / `437.md`、`.claude/custom-lint-rules.toml`、
> [ADR-007](adr/adr-007-custom-linter-layer-boundary.md) (正規表現層 / AST 層の線引き)

#### 背景

`gh` の pagination 無し呼び出しは repo 全体に散在する (`check-ci-coderabbit` / `cli-merge-pipeline` /
`cli-pr-monitor` / `cli-stale-branch-scan`)。ref を破壊する push は現状 workflow の shell が主だが、
Rust 側から撃つ経路が増えれば同じ穴が開く。

**lease を要求すべき対象は 2 種類あり、同じパターンでは捕まらない** (PR #439 CodeRabbit Major)。
feedback の原文は `--force` だけを挙げていたが、**PR L で実際に踏んだのは `--delete`** だった:

| 対象 | 破壊するもの | lease 無しの実害 |
|---|---|---|
| ref の削除 (`--delete` / `:refs/...` の refspec) | ref そのもの | 観測から削除までの間に他経路が push した作業が消える (PR L で実測) |
| 非 fast-forward な更新 (`--force` / `+refs/...`) | ref の履歴 | 他経路の commit が到達不能になる |

`--delete` は `--force` を含まないので、`--force` だけを見る規則では**削除経路が丸ごと素通り**する。
refspec 形式 (`:refs/heads/X` / `+refs/heads/X`) も同じ意味を持つため、フラグ名だけの照合では足りない。

#### 設計決定 (案)

- **ADR-007 の層判定を先に行う**。`gh api` / `gh pr view --json` の引数照合は正規表現層で足りるか、
  AST 層が要るかを判断してから実装する。`.rs` の文字列リテラル内の引数列を見るだけなら正規表現層
- **ref を破壊する push は上表の 2 種類すべてを対象にする**。フラグ形式 (`--delete` / `--force`) と
  refspec 形式 (`:refs/...` / `+refs/...`) の両方を捕まえ、`--force-with-lease=<ref>:<sha>` が
  付随することを要求する。**2 種類で規則を分けるか 1 つにまとめるかは実装時に決める** —
  分けたほうがメッセージを具体的にできるが、規則が増えると保守点も増える
- **false positive の逃げ道を用意する** — 意図的に pagination 不要な呼び出し (単一 ref の
  `ls-remote` 等) や、lease が不要な push を許可する手段が無いと、lint が邪魔になって無効化される

- [ ] ADR-007 の判定フローで層を決める
- [ ] `gh` の pagination 検査を実装
- [ ] ref 削除 (`--delete` / `:refs/...`) の lease 検査を実装
- [ ] 非 fast-forward 更新 (`--force` / `+refs/...`) の lease 検査を実装
- [ ] 既存の全 `gh` 呼び出しと push 呼び出しを新 lint に通し、false positive を洗い出す
- [ ] 例外指定の手段を用意する

#### 完了基準

pagination 無しの `gh` 呼び出しと lease 無しの `--force` が、書いた時点でブロックされる。
既存コードが false positive を出さない。

---

### エラーメッセージの無制限 debug 補間を lint で検出する (順位 483)

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

### push stage の bare push フォールバック不変条件を seal する (順位 484)

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

### PR L で追加した実装のテスト補強 (順位 485)

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
