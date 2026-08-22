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
