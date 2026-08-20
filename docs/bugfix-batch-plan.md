# 不具合修正バックログ消化計画 (PR 12 本)

> **状態**: 進行中 (2026-08-17 作成) / **本ファイルは ephemeral な作業計画書**である。
>
> **最終目標**: 下記 12 本の PR をすべてマージし、残観測を消化し、各 todo エントリの後始末を終えたうえで、**本ファイル自身を削除する** (→ [§ 本計画書の退役手順](#本計画書の退役手順))。
>
> **作成経緯**: [docs/todo-summary.md](todo-summary.md) / [docs/todo-summary2.md](todo-summary2.md) の全順位から、(1) [docs/claude-code-web-tasks.md](claude-code-web-tasks.md) (夜間ループ台帳) に**未掲載**で、(2) **実観測された不具合の修正**にあたる 23 タスクを抽出し、同一 crate・同一障害クラスで PR 12 本に統合した。統合の理由は各 PR の節に記載しており、**PR description にも束ねた理由を明記すること** (リポジトリ規約)。

## 進行表

各 PR は「実装 + push → PR 作成承認 → マージ → エントリ後始末」で 1 サイクル。E / L は完了基準に実走観測を含むため、エントリ後始末が観測後に遅延する (→ [§ 残観測トラッキング](#残観測トラッキング))。

| # | PR | 対象順位 | 状態 |
|---|---|---|---|
| A | fix(merge-pipeline): feedback ループの誤 bail・誤ブロック解消 | 444 + 328 + 347 | **完了。** 444 は [PR #417](https://github.com/aloekun/claude-code-hook-test/pull/417) でマージ済み。328 は順位 398 の guard 変更で既に解消済みと判明し、再発防止テストのみ追加。347 は実装済み |
| B-1 | fix(merge-pipeline): transcript の連結順序を時系列にする | 446 (再定義) | **完了** ([PR #419](https://github.com/aloekun/claude-code-hook-test/pull/419)) |
| B-2 | fix(merge-pipeline): 分析ソース選定を陽性照合ベースに統一 | 336 + 288(a) | **完了** ([PR #420](https://github.com/aloekun/claude-code-hook-test/pull/420))。実 run で照合成功を確認済み |
| B-3 | fix(merge-pipeline): transcript 抽出を workspace 横断にする | 469 (446 から分離) | **完了** ([PR #421](https://github.com/aloekun/claude-code-hook-test/pull/421)) |
| C | fix(hooks): smoke suite の ETXTBSY 解消 | 396 | **完了** ([PR #423](https://github.com/aloekun/claude-code-hook-test/pull/423))。台帳の 3 案はいずれも副作用があり、copy/spawn の相互排除に切り替えた |
| D | fix(coderabbit-review): レビュー実施の陽性証拠を facet/prompt 層にも要求する | 318 + 320 | 実装済み。**318 は全項目・320 は決定論層が既に実装済みだった**ため、facet gap 修正 + 後始末に縮小 |
| E | fix(ci): 監視系 workflow の誤動作修正 | 319 + 431 | **実装済み。** 319 は案 (a)+(b) 併用 (body 空 review の除外 + head SHA 冪等キー)、431 は rate-limit を red で落とす方式。**431 は台帳の前提がずれていた** (下表参照)。エントリ後始末は実走観測後 |
| F | fix(pr-monitor): cli-pr-monitor 小修正束 | 246 + 292 + 385 | **完了 (実装)。** 246 は**前提消滅 + 実装済み**で連結 regression test のみ、292 は token 方式へ統一、385 は**不採用**を module doc へ記録。エントリ後始末も同 PR に含む |
| G | fix(jj-helpers): bookmark 探索の深さ非依存化 + 自動 fix 後始末 | 386 + 387 | 未着手 |
| H | fix(push-runner): push 経路 stage 修正束 | 376 + 254 + 322 | 未着手 |
| I | fix(push-runner): bookmark_check の未レビュー祖先 fail-closed | 288(b) | 未着手 |
| J | fix(pr-monitor): post-pr-review の docs-only 判定を PR 全体基準に | 233 | 未着手 |
| K | fix(subprocess): timeout の孫プロセス穴を塞ぐ | 323 | 未着手 |
| L | fix(automation): 自動化経路の小穴・ノイズ修正束 | 467 + 181 | 未着手 |

**挿入 (2026-08-20、完了)**: 順位 431 の調査で `markers.rs` の rate-limit marker が CodeRabbit の **command ack 形式**を拾わないことが判明した (PR #412 / #387 の実データ)。検出層の穴なので PR F を保留し、先に [PR #429](https://github.com/aloekun/claude-code-hook-test/pull/429) として処理した (ユーザー判断)。**台帳の前提が変わった項目に着手したら、周辺への影響まで確認する** — 本計画の 9 件中 7 件でずれが出ている以上、ずれの周辺は常に疑う。この 1 件は「前提が変わった理由を追ったら別の穴が見えた」形だった。

消化順は A → L の表の順。**PR をスタックしない** — 順位 376 (PR H で修正するまで push-runner の bookmark 自動前進がスタック境界を壊す既知バグ) を踏むため、各 PR は前の PR がマージされてから master 起点で作る。

## 共通の運用ルール

1. **PR 作成前の承認**: push 完了後、PR タイトル・ボディを提示してユーザーの明示承認を得てから `pnpm create-pr` を実行する (ADR-028。スコープ承認 ≠ 作成許可)。
2. **PR body は `--body-file` で渡す** (`--body` は 1 行目で切れる)。body ファイルは push 完了後に scratchpad で作る (working copy に置くと snapshot 混入する)。
3. **エントリ後始末は実装と同じ PR に含める**: 該当 `docs/todoN.md` の節削除 + [todo-summary.md](todo-summary.md) (順位 219 以下) / [todo-summary2.md](todo-summary2.md) (順位 220 以上) の行削除。ただし**完了基準に実走観測を含むタスク (319 / 431 / 467 / 181) はエントリを残し**、観測確認後の docs バッチで削除する。
4. **複数コミットのスタックを 1 PR で push する場合**は `jj edit @-` 等で tip を揃える (push-runner のレビューは `<base>..@` の PR 全体を見る)。行削除の編集に PowerShell の `Set-Content` を使わない (CRLF 化で全行 diff になる) — Edit ツールか sed を使う。
5. **jj squash は `-u` を付ける** (source/dest 両方に description があると editor 起動で headless hang)。
6. 各 PR の DoD: `cargo test --workspace` green (+ 該当 crate の clippy)。workflow を触る PR は `pnpm lint:workflows` も。
7. **夜間ループとの競合**: 着手前に該当ファイルを触る `claude/nightly-*` ブランチが無いか確認する。既知の衝突は PR D の節に記載。
8. **マージ後は `pnpm build:all` を実行する**。`.claude/*.exe` は gitignore 対象で、**マージしただけでは挙動が変わらない**。PATH に Git の coreutils が要る (`export PATH="$PATH:/c/Program Files/Git/usr/bin"`、`cp.exe` のため)。
   - **ビルドの成否は exe の mtime で判断しない。** 完了通知とファイル書き込みの間にずれがあり、2026-08-19 に「更新されていない」と誤判定した。中身で確かめる: `grep -c "<その PR で追加した文字列>" .claude/<exe>`

## 着手前に必ずやること

本計画の PR A〜B-3 (2026-08-18〜19) で実際に踏んだ穴。以降の PR C〜L でも同じ形で再発する。

**台帳の記述をそのまま信じない。** 本計画で着手した 9 件のうち **7 件で台帳と実態がずれていた**。

| 順位 | 台帳の記述 | 実際 |
|---|---|---|
| 444 | 未着手 | 本計画とは独立に PR #417 として起票済みだった |
| 328 | leftover context.json で誤 bail する | 順位 398 の guard 変更で**前提が消滅**していた |
| 347 | `cli-merge-pipeline` の欠陥 | 実装先は `cli-pr-monitor` |
| 446 | 並列 workspace のセッションが不可視 | 真因は**連結順序が時系列でないこと** |
| 336 | 時刻範囲のみで照合しない | 時刻範囲すら使わず**辞書順で最新 1 件** |
| 318 | 第 3 format 未対応 + silent 化が残る | **4 項目すべて実装済み** (PR #309 ほか)。本丸の既定 30 分 park も入っていた |
| 320 | check pass の誤報が残る | 決定論層は [ADR-064](adr/adr-064-monitor-success-positive-evidence.md) で**実装済み**。残件は facet/prompt 層のみ |
| 431 | `Review limit reached` を判別すればよい | 拒否の実体は **command ack** (`⚠️ Action not completed` / `Review rate limited.`) で、`markers.rs` の marker の**どちらにも一致しない** |
| 246 | CodeRabbit-only 構成で幻の CI pending が残る | 短絡は**実装済み** (`decide()` の `!ci.runs.is_empty()`)。さらに前提の「実 CI が存在しない構成」自体が [ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md) の `ci.yml` (paths フィルタ無し) で消滅していた |

台帳は起票時点のスナップショットで、実装が動くほどずれる。328 と 446 は、台帳どおりに実装すれば**存在しない不具合を直すか誤った箇所を直す**ところだった。**自分が前日に書いたエントリでも同じ** — 順位 469 の Frequency 評価は実測で覆った。

**自分の修正が下流の分岐を変えていないか追う。** CodeRabbit の Major 指摘 2 件が同じ構造だった。

- [PR #418](https://github.com/aloekun/claude-code-hook-test/pull/418): `FixCommitState::None` を返す変更が、下流 `decide_repush_action` の `(HasChange, _, true) => AutoPush` 経路を開いた (分離コミットなしで push される)
- [PR #421](https://github.com/aloekun/claude-code-hook-test/pull/421): 走査 source を単数→複数にしたのに `?` を残し、1 ディレクトリの失敗で全 workspace 分が失われる構造になった

いずれも**局所的には正しい変更が、周囲の構造が変わったことで別の意味を持った**。シグネチャ・戻り値・引数の数を変えたら、呼び出し元と下流の `match` を必ず追う。

**識別子を照合キーにするなら、一意性の根拠をリポジトリ全体で確認する。** [PR #420](https://github.com/aloekun/claude-code-hook-test/pull/420) で bookmark 名を一意キーと仮定したが、`claude/nightly-<順位>` は夜間ループが再利用する。CodeRabbit はリポジトリ全体を走査して気づき、私は変更箇所の周辺しか見ていなかった。

**外部 SaaS の marker を流用する前に、自分が観測する comment class の実データを見る。** 順位 431 で `markers.rs` の `RATE_LIMIT_MARKERS` をそのまま使ったが、それは CodeRabbit の **walkthrough placeholder** 向けの marker で、`@coderabbitai review` への直接の応答である **command ack** (`Review rate limited.`) には一致しない。PR #387 のコメント body を実際に `gh api` で読むまで気づかなかった (読まずに land していれば、レート制限を検知できない検知機構ができていた)。同じ SaaS でも comment class ごとに文言体系が違う。

**doc に「機構がある」と書いたら、その機構が実在するか確かめる。** 3 度やった — `RUN_REPORT_FILE_NAME` の pin テスト (存在しなかった / [#418](https://github.com/aloekun/claude-code-hook-test/pull/418) で追加)、fix step が書いた「3 crate を共通化」(1 crate だけだった)、`sort_key` の mtime tie-break (`source_path` へ変えた後も doc が残っていた)。

---

## PR A: fix(merge-pipeline): feedback ループの誤 bail・誤ブロック解消 (順位 444 + 328 + 347)

**束ねた理由 (起案時)**: 3 件とも post-merge-feedback 経路の欠陥で、444 (stale meta による恒久ブロック) と 328 (leftover context.json による誤 bail) は同じ「進行中誤判定」機構の裏表、347 (空 fix commit) も同経路の後始末不備、と見立てていた。**実際には 347 の実装先は `cli-merge-pipeline` ではなく `cli-pr-monitor` だった** (下記 347 の節)。444 / 328 は見立てどおり `cli-merge-pipeline` (と reaper 側の `hooks-session-start`)。

> **PR A は完了 (2026-08-18)。** 順位 444 は [PR #417](https://github.com/aloekun/claude-code-hook-test/pull/417) で**マージ済み**。328 は着手時に前提が消えていたため seal のみ。347 は [PR #418](https://github.com/aloekun/claude-code-hook-test/pull/418) で実装。**以下 3 節は「当初計画がどう変わったか」の記録**で、実装内容そのものは各 PR を参照する。
>
> 実行時間分布の実測は #417 の中で完了した (自身の成果物を書いた完了 run 140 件: 中央値 8.9 分 / p95 16.5 分 / 最大 23.3 分 / 25 分超 0 件、分布は対数正規)。**閾値 1500 秒は実測で裏付けられた**ため変更していない。詳細は `run_registry::running_runs` の doc を参照。
>
> 調査の副産物として、**PR 単位の `<pr>.md` を run 単位の成功証拠に使う誤り**が判明し #417 で是正した (feedback 再実行時に、先に死んだ run を後続 run の成果物で「成功」と誤判定する)。また **run が起動直後に死ぬ経路そのものは未解明**で、順位 468 として別途起票した。

### 順位 444 (PR #417 で実装済み — 記録): orphan reaper が meta.json を running のまま残す

- **不具合**: run がレポート書き込み後・meta 終端化前に死ぬと `status: "running"` のまま残り、以後すべての post-merge-feedback をブロックする。2026-08-13 に PR #396 マージで実発生。原因は `reap_orphans` が `if marker.exists() || success_report.exists() { continue; }` で成功 run を skip し、meta を終端化する経路が無いこと。
- **層 1 (reaper)**: reconcile 分岐を追加した。**当初計画からの変更 2 点** — (1) 成功判定の根拠を PR 単位の `<pr>.md` ではなく **run 単位の `<run dir>/reports/feedback-report.md`** にした (再実行された PR で、先に死んだ run を後続 run の成果物で誤判定するため)。(2) **`endTime` は書かない**ことにした。「レポートの mtime から導出」という当初案こそが 2026-08-13 の手動復旧で架空の 40.1 分を生んだ手口だったため。
- **層 2 (guard)**: fail 方向の非対称を解消した。**当初計画からの変更** — 「経過時間だけで判定してはならない、陽性シグナルと複合判定せよ」としていたが、その根拠だった「正常な 40 分 run」は**計測アーティファクトだった**。実測し直すと (自身の成果物を書いた完了 run 140 件) 最大 23.3 分・25 分超は 0 件で、経過時間のみの足切りで十分と確認できたため `ORPHAN_THRESHOLD_SECS` (1500s) を共有した。両者の根拠が別々に成立することは doc に明記した。進捗シグナルによる複合判定は順位 468 の観測結果を見てから再評価する。
- **回帰テスト**: 再実行シナリオ (1 本目=成果物ゼロ、2 本目=完走) で 1 本目が `failed` になること、`endTime` を捏造しないこと、status 確定の失敗を成功と報告しないこと、`reportDirectory` の `..` で別 run に到達できないこと。
- **完了基準**: 達成済み。reaper 通過後に run 単位の成果物に基づいて終端化されること、reaper が走らなくても stale running がブロックしないこと、閾値内 running のブロック非退行。

### 順位 328 (前提消滅 — 記録): 成功後の context.json 残存による誤 bail

- **不具合 (2026-07-19 #296 で実観測)**: 旧 guard は `context.json` の mtime を見て「1500 秒以内に書かれていれば進行中」と判定していた。#295 の feedback が正常完了しても `context.json` を掃除しないため、25 分以内の連続マージで #296 の feedback が誤 bail した。
- **着手時の確認で前提が消えていた**: 現行の `check_concurrent_run_guard` は `run_registry::running_runs` (= `meta.json` の `status` + `startTime`) **だけ**を読み、`context.json` を一切参照しない。判定根拠を run の状態へ移した**順位 398 (PR #388) の時点でこの結合は消えている**。`CONCURRENT_RUN_GUARD_SECS` も既に廃止済みで、コード上の残存は doc コメント内の言及のみ。
- **したがって当初の対処 (成功時に context.json を削除) は実装しなかった**。guard がもう読まない以上、削除で得られるのは後片付けの綺麗さだけである一方、timeout kill を生き延びた orphan takt が `context.json` を読み直す経路が実在するため (ADR-030 § Reconciliation)、消す側にわずかながら実害の芽がある。**利得がほぼ無く risk が非ゼロなので採らない。**
- **代わりに入れたもの**: 「書きたてで放置された `context.json` があっても guard を通る」ことを固定する回帰テスト (`a_leftover_context_file_does_not_block_the_next_feedback`)。同じ結合を将来再導入させないための seal。
- **完了基準**: 達成済み (上記テストで seal)。

### 順位 347 (実装済み): CodeRabbit findings が空でも fix commit が生成され abandon される

- **不具合**: findings 0 件でも fix commit 生成 → abandon の noise (PR #310 で実観測。2026-08-18 の PR #417 監視でも再現)。
- **該当コードパス (再調査の結果)**: `cli-merge-pipeline` ではなく **`src/cli-pr-monitor/src/stages/monitor.rs`** の `invoke_takt_into_outcome`。takt は「CodeRabbit がコメントを投稿した」だけでも起動する (`has_coderabbit_findings` は `new_comments` / `unresolved_threads` でも真になる) ため、findings 0 件のまま `create_fix_commit` が呼ばれていた。
- **対処**: findings が 0 件なら fix commit の事前作成を skip する。
- **「全件 non-actionable なら skip」は実装しなかった**: `extract_severity` の `"Info"` は `Critical` / `Major` / `Minor` / `High` / `Low` のどれにも一致しない場合の**受け皿**でもある。severity で絞ると、書式が変わって解析できなかった実指摘まで黙って skip する。判定根拠が確かな件数だけを条件にした。この契約はテストで固定している。
- **完了基準**: 達成済み。findings 0 件で作成されないこと、`Info` のみでも「対象なし」に倒さないことを回帰テストで seal。

**後始末**: todo22.md「順位 444」節 / todo17.md「post-merge feedback が成功後に…誤 bail させる」節 / todo14.md「CodeRabbit findings が空のとき fix commit 生成を skip」節を削除、todo-summary2.md の 444 / 328 / 347 行を削除。

---

## PR B: fix(merge-pipeline): 分析ソース選定を陽性照合ベースに統一 (順位 336 + 288(a) + 446)

**束ねる理由 (起案時)**: 3 件とも `src/cli-merge-pipeline/src/feedback/` の context.rs / transcript.rs が「時刻範囲だけで分析ソースを選ぶ」同一欠陥、と見立てていた。**446 は切り分けの結果この見立てから外れた** (下記)。

**着手順: 446 の切り分けを最初に行う** (実装方針に影響するため) — 実施済み。

> **PR B は 3 本に分割した (2026-08-18、ユーザー判断)。** 446 の切り分けで、実障害の原因と当初仮説が別物だと判明したため。**B-1 = 実障害の修正** (連結順序、下記)、**B-2 = 分析ソース選定の修正** (順位 336 + 288(a))、**B-3 = workspace 横断の可視性** (新規順位 469)。切り戻し単位を分けるための分割である。
>
> **なお B-3 は「将来リスクの予防」ではなかった (2026-08-18 実測)。** 分割時点では未発現の構造リスクと見ていたが、着手前の実測で **PR #417 が `improve` workspace で実装され、main からのマージで実装セッション (編集 31 件) が分析入力から欠落していた**ことが判明した。#417 の範囲で修正の効果を測ると improve 側 530 行が新たに拾え、main からの取りこぼしは 0 行だった。

### 順位 446 (切り分けで再定義 — B-1 で実装): transcript の連結順序が時系列でない

- **当初仮説は誤りだった (negative result)**: 「並列 workspace のセッションが不可視」が原因と想定していたが、PR #395 のブランチ名を両 project-id フォルダで grep したところ**メイン workspace 側にのみ出現**し、抽出対象フォルダの選択は正しかった。
- **真因**: `collect_jsonl_paths_in_deterministic_order` はファイルを `(mtime, path)` 順に読み、その順のまま連結する。**決定論的ではあるが時系列ではない。** #395 の範囲を再現した実測では、1189 行中 **11 箇所で時刻が逆行**し最大 **560 分**巻き戻っていた。先頭行は `15:18` だが真の最古は `15:02`。
- **なぜ気づきにくいか**: 抽出そのものは正しく、**行数は合っている** (session-analysis の報告 1189 行 = 実測 1189 行)。誤るのは範囲だけで、facet はこの非単調な列から「2.5 分しか無い」と判断し `session_data_unavailable` を報告した。報告された 2.5 分は 27 ファイル中 1 本の span と正確に一致していた。
- **対処**: 出力を timestamp 昇順にする。同一 timestamp は `(file_index, line_index)` で tie-break するため決定論は失わない。
- **完了基準**: 達成済み。実データ (#395 の範囲) で逆行 0 回・先頭が真の最古 `15:02:41Z` になることを確認。単体テストで「ファイル順に引きずられない」「同時刻は決定論的」「同一ファイル内は元の行順」を seal。

### 順位 336: pre-push run / transcript の選定を対象 PR の commit/bookmark 照合に変更

- **不具合**: `find_latest_prepush_reports_dir` と transcript 選定が時刻範囲のみのため、同日並行 push (#311/#312/#313) で他 PR の知見を誤帰属 (#311 の feedback に #313 のコードが混入、実地確認済み)。
- **対処**: context.json 生成で対象 PR の commit range / bookmark と突き合わせて選定。照合に外れた run/transcript は除外か unverified 表示 (fail-open な助言層)。
- **回帰テスト**: #311/#312 で観測した混入シナリオを固定。

### 順位 288(a): pre-push reports の全 run 集約 + `prepush_reports_dir` 配列化

- **背景**: 「最新 1 run」のみが分析ソースのため、複数回 push した PR で分析が偏る。上流の `[diff]` stage PR 範囲化は **PR #311/#313 で実装済み** — 残るのは集約側。
- **対処**: 対象 PR の pre-push run dir を全列挙する関数に拡張 (336 と同じ複数の識別根拠で判定)。context.json の `prepush_reports_dir` を配列化し、`.takt/facets/instructions/analyze-prepush-reports.md` を複数 dir 対応に更新。**スキーマ契約変更のため**: 全 reader の列挙 + 旧 string 形式との後方互換 (or schema versioning) + 空配列時の挙動を明記。
- **完了基準**: 複数 push した PR の feedback が、commit 範囲等の識別根拠に基づき全 pre-push run を分析対象にすること。

**後始末**: todo14.md「分析ソース選定を…修正」節 / todo22.md「順位 446」節を削除、todo-summary2.md の 336 / 446 行を削除。**順位 288 のエントリ (todo15.md) は削除しない** — 作業計画の該当項目にチェックを付け「(a) は PR で実装済み、残は (b) bookmark_check」と追記する。削除は PR I 完了時。

---

## PR C: fix(hooks): smoke suite の ETXTBSY 解消 (順位 396)

- **不具合**: `src/hooks-pre-tool-validate/tests/smoke.rs` の 2 テストが並列で exe を tempdir へ `fs::copy` → spawn するため、Linux で片方の copy 中の書き込み fd を fork した子が継承し、exec が `Text file busy` (os error 26) で落ちる。PR #376 の CI (ubuntu のみ) で実観測。flaky の放置は「また flake だろう」で実バグを見落とす経路になる (2026-08-10 ユーザー判断で Tier 1 格上げ)。
- **対処** (実装時に選択): (a) ci.yml の hooks smoke step を `--test-threads=1` (最小・即効)、(b) spawn を ETXTBSY でリトライ、(c) staging をやめて `built_exe()` 直接起動 (config staging 設計との整合要確認)。
- **手順**: **まず WSL Ubuntu-24.04 で並列実行を再現させる** — 3 案のどれを採るかは再現の観察 (どの段で落ちるか) に依存するため、再現前に実装へ入らない。環境は導入済み (`wsl -u root` はパスワード不要 / PowerShell から複数行 bash を渡すと壊れるのでスクリプトファイル経由)。再現したら直し、同じ手順で消えたことを確認する。他の smoke/E2E suite に「copy してから spawn」同型パターンが無いか棚卸しする。
  - **再現しなかった場合は実装に入らずユーザーに相談する。** ETXTBSY は fd 継承のタイミング依存で、ローカル WSL では CI の並列度・I/O 特性が再現しないことがある。測定できないまま 3 案から選ぶと「効いたかどうか確認できない修正」を入れることになり、flaky を Tier 1 に上げた趣旨 (「また flake だろう」で実バグを見落とす経路を塞ぐ) に反する。
- **完了基準**: Linux で ETXTBSY が出ないこと (再現手順付き)。同型パターンの棚卸し完了。
- **後始末**: todo21.md「hooks smoke suite の並列実行が…」節 + todo-summary2.md 396 行を削除。

> **実装済み (2026-08-19)。採ったのは 3 案のどれでもなく「copy と spawn の相互排除」だった** — 再現の観察から選び直した。以下は記録。
>
> **再現**: WSL Ubuntu-24.04 の **ext4 上** (`~/etxtbsy`) にリポジトリを複製してテストバイナリを 200 回回すと **43 回 ETXTBSY** (別測定で 30/200)。`--test-threads=1` と各テスト単独では 0/100 で、**2 テストの並列実行に固有**と確定した。`/mnt/c` (drvfs) 上では再現しない。
>
> **3 案を実測比較した** (各 200 run): (a) `--test-threads=1`、(b) spawn リトライ、(c) 共有 staging (`LazyLock`) — **どれも 0 件**に落ちた。決め手は副作用のほう:
>
> - **(a) は穴が残る**。smoke テストは専用 step (ci.yml:175) だけでなく `cargo test --workspace` (ci.yml:168) でも走るので、専用 step だけ直列化しても同じ race が残る。workspace 全体の直列化はコストが大きい。
> - **(c) は `LazyLock` の `TempDir` が drop されず、実測で 1 run あたり 37MB を `/tmp` に残した** (200 run で 7.2GB)。固定パス staging へ変えればリークは消えるが、「`target/debug` を汚さない」という smoke.rs の設計意図と衝突する。
> - **(b) は原因 (fd 継承) に触れず症状を待つ形**で、テストコードに retry ループが入る。
>
> **採った対処**: `static EXEC_STAGING_LOCK: Mutex<()>` で **copy と spawn (fork〜exec) を相互排除**する。`Command::spawn` は子の exec 完了まで親へ返らないため、spawn 呼び出しを囲めば fd 継承の窓が閉じる。テストごとの tempdir 分離と後始末はそのままで、リークも無い。
>
> **回帰 seal**: `concurrent_staging_and_spawn_survives_etxtbsy` (`#[ignore]`、8 スレッド × 16 ラウンド) を追加。**ロックを外すと Linux で 10/10 落ち、戻すと 0/10** (ADR-049 の「修正前に落ちることを確認」)。CI は `--ignored --test-threads=1` の leg で回す。所要 Linux 2.1s / Windows 9.7s。
>
> **同型パターンの棚卸し**: リポジトリ全体で「exe を copy してから spawn」は 2 ファイルのみ。`hooks-stop-quality/tests/t7_cwd_independence.rs` は **5 テスト全部が copy→spawn** で、現状 `#![cfg(windows)]` のため POSIX 経路は踏まないが、cfg を外した瞬間に smoke.rs 以上の危険度になるため**同じガードを入れた**。他 (`hooks-post-tool-linter/tests/incident_eval.rs`、`hooks-stop-tool-call-leak/tests/e2e.rs`) は exe をコピーせず直接 spawn するため対象外。

---

## PR D: fix(check-ci-coderabbit): rate-limit 第 3 format 対応 + 実レビュー有無の分離 (順位 318 + 320)

**束ねる理由**: 320 は 318 に依存 (台帳の依存欄に明記) し、同一 crate `check-ci-coderabbit` の連続した変更。

**⚠ 着手前の lane 調整**: [claude-code-web-tasks.md](claude-code-web-tasks.md) の順位 176 (`✅` auto lane) が同一ファイル `src/check-ci-coderabbit/src/rate_limit.rs` を触る。台帳の規律は「競合する割り当てをしない」— ユーザーに確認して 176 を `—` (human) へ移して本 PR に取り込むか、夜間 PR の land を待つ。

> **着手前の競合確認と実態調査の結果 (2026-08-19)**: **lane 調整は不要になり、実装対象もほぼ消えた。**
>
> **競合確認**: 開いている夜間 PR は 2 本 ([#422](https://github.com/aloekun/claude-code-hook-test/pull/422) 順位 228 / [#413](https://github.com/aloekun/claude-code-hook-test/pull/413) 順位 240) で、**どちらも `check-ci-coderabbit` を触らない**。順位 228 は名前が「rate-limit」で紛らわしいが `cli-pr-monitor` 側の別実装 (`src/cli-pr-monitor/src/stages/poll/rate_limit/tests.rs`)。順位 176 は auto lane 25 行中の **22 番目**で、夜間ループは 1 晩 1 件を**文書順**で選ぶ (open PR のある順位のみ skip、`lib_ledger::select`) ため当分回ってこない。実接触は `docs/todo-summary2.md` を両 PR が編集する点のみ (削除行が別なので軽微)。
>
> **順位 318 は 4 項目すべて実装済みだった** — ① `extract_next_review_format_wait_time` (rate_limit.rs:120、PR #309 / 2026-07-20)、② 本丸の silent 化解消 (`UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES = 30` + `warn_unknown_wait_time_format()` + `wait_time_parsed`)、③ 第 3 format fixture 3 本 + fallback 2 本、④ ADR-034 の format 一覧 table 行。**計画書が「案を検討」と書いた保守的既定 30 分の park がそのまま入っている。**
>
> **順位 320 も決定論層は実装済みだった** — [ADR-064](adr/adr-064-monitor-success-positive-evidence.md) の `has_review_evidence` が decide.rs の R1/R4 ゲートとして「check が pass でもレビュー実施の陽性証拠がなければ success に倒さない」を実現している (計画書の対処 (2))。
>
> **残っていたのは対処 (1) の facet 層だけで、影響は誤読リスクに限定**されていた。3 経路を追った結果: (a) 決定論層は倒れない、(b) ローカル takt は起動条件が `has_coderabbit_findings` なので「レビュー無し」では**そもそも到達せず**、verdict は re-push/fix 経路にしか流れない、(c) GHA Phase A は `Verdict: approved` を出しうるが**機械消費されない**コメント文字列 (`pr-monitor.yml` 全体で該当トークンの出現は 1 箇所のみ) で、監視役は approve/merge が禁止 (ADR-022)。
>
> **したがって本 PR は「facet gap 修正 + 台帳後始末」に縮小した** (ユーザー判断)。Rust コードは変更しない。

### 順位 318: rate-limit 第 3 format 未対応 + silent 化

- **不具合**: PR #287 で CR の wait-time 文言が第 3 format (`**Next review available in:** **32 minutes**`) に変わり、marker (`is_rate_limit_comment`) は一致するのに全 extract 関数が不一致 → `parse_rate_limit` が None で**静かに**「rate-limit 無し」扱い。監視が false-green を報告した。旧 → 新 → 第 3 と同一クラス 3 世代目。
- **対処**:
  1. `extract_next_review_format_wait_time` を追加し `extract_wait_time` の or_else 連鎖へ (ADR-034 § 検出 logic 更新手順 step 4)。
  2. **本丸 = silent 化の構造的解消**: marker 一致 & wait-time None の組合せを loud にする (warn ログ + 「rate-limit 検出・待ち時間不明」報告 + ADR-043 に従い保守的既定待ち時間〔例 30 分〕で park する案を検討)。これが入れば第 4 format が来ても silent regression にならない。
  3. fixture: 第 3 format の実 body 2-3 variant + silent ケース 1 本。**修正前に実際に落ちることを確認** (ADR-049)。
  4. ADR-034 の format 一覧 table に第 3 format 行を append し、症状記述に「marker 一致 / regex 不一致 (常時 None、silent)」を追記。
- **完了基準**: 第 3 format から待ち時間が抽出でき park 経路に乗ること。marker 一致 / 抽出失敗が silent に握り潰されないこと。

### 順位 320: CodeRabbit status check は実レビュー無しでも `pass`

- **不具合**: PR #287 で checks が一貫して pass だが実レビュー 0 件 (skip も rate-limit も pass、check summary 文字列は stale になる)。
- **対処**: (1) `.takt/facets/instructions/analyze-coderabbit.md` と `.github/workflows/pr-monitor.yml` prompt に「pass はレビュー実施の根拠にならない」を明記し、判定 source を reviews 件数 / walkthrough の `Configuration used` / 本文文言に固定。(2) `check-ci-coderabbit` に「レビュー実施有無」を `reviews` 件数 + walkthrough marker から判定する関数を追加し、`review_state: success` と分離して report。
- **完了基準**: 「check は pass だが実レビュー 0 件」が report で判別でき approved と誤報しないこと。

**後始末**: todo16.md 318 節 / todo17.md 320 節を削除、todo-summary2.md の 318 / 320 行を削除。

---

## PR E: fix(ci): 監視系 workflow の誤動作修正 (順位 319 + 431)

**束ねる理由**: どちらも `.github/workflows/` のみの変更で、完了判定が「マージ後の実走観測」という同じ性質。1 回のマージで両方の dogfood を開始できる。

### 順位 319: pr-monitor.yml バックストップの重複ガード — pull_request_review 経路

- **不具合**: 2026-07-20 の決定論ガード (#310) は issue_comment 経路のみで、dogfood 集計 (#347〜#390 の 29 PR) で **2 投稿以上が 69%** = 不合格。残原因は `pull_request_review` 経路の content フィルタ欠落 — (i) 1 回の walkthrough が両経路で起動 (2 投稿)、(ii) body 空の ack が review として通る (3 投稿目)。
- **対処**: 案 (a) body 空 / summarize マーカー無しの review を除外、**案 (b) が本命**: head SHA + walkthrough 単位の冪等キーで既投稿を判定する決定論ガード (event 条件だけでは同一 walkthrough の 2 経路を原理的に区別できない。ADR-042 の決定論層方針)。workflow 先頭設計メモ L82-83「追加は pull_request_review 経路が拾う」も改訂。
- **完了基準**: walkthrough 更新 1 回につき投稿が**両経路合算で高々 1 件**、ack / マージ後に投稿されないこと (**実 PR で確認** — エントリ削除は観測後)。

### 順位 431: review-request のレート制限拒否が success で終わる

- **不具合**: 2026-08-11 の夜間ループ実走 (PR #387) で CR が `Review limit reached` を返したのに、検証が「コメント 1 件以上付いたか」だけを見るため success 記録。未レビューの自律 PR が信号として残らない。
- **対処**: `.github/workflows/review-request.yml` で CR 応答を分類 (受理 / レート制限 / skip / エラー)。**文言依存の判別は脆い**ため、未知の変化は安全側 (未取得扱い) に倒す。success 判定を陽性証拠へ寄せるか warning 可視化に留めるかを決める。**リトライ機構は作らない** (ADR-019 § M5)。未レビュー PR を後から拾う経路 (weekly-review) との役割分担を決める。
- **完了基準**: レート制限で弾かれた自律 PR が run の色か棚卸しで**未レビューと分かる**こと。

**検証**: `pnpm lint:workflows`。**エントリ後始末は実走観測後** → [§ 残観測トラッキング](#残観測トラッキング)。

---

## PR F: fix(pr-monitor): cli-pr-monitor 小修正束 (順位 246 + 292 + 385)

**束ねる理由**: 3 件とも `cli-pr-monitor` 単一 crate の独立した小修正。292 と 385 は同じ `lock.rs`。

### 順位 246: CodeRabbit-only 構成で「幻の CI pending」

- **不具合**: 実 CI check が無い構成 (docs-only PR で共通) で poll が「CI: pending」を完了と判定できず recheck を上限まで繰り返す。PR #231/#232 で手動の GitHub API 確認 (`mergeStateStatus=CLEAN`) が 2 回必要になった。
- **対処**: poll の CI 完了判定に「実 check 不在 or CodeRabbit のみ」+「CR review 完了 (unresolved 0 / actionable 0)」+「mergeability CLEAN/MERGEABLE」で短絡する条件分岐を追加。**実 CI check が 1 件でも pending なら従来どおり待機** (誤短絡防止、regression test で固定)。
- **完了基準**: CodeRabbit-only 構成で無駄 recheck をせず merge-ready 判定。実 CI がある場合は非退行。
- **結果 (2026-08-20)**: **前提消滅 + 実装済み**。(i) `decide()` は `ci_pending = ci.overall == "pending" && !ci.runs.is_empty()` で空 runs の pending を待機理由にしない。(ii) `parse_ci_rollup` は CodeRabbit の commit status を除外するため、CodeRabbit-only 構成では `runs` が空になる。(iii) そもそも本リポジトリは `ci.yml` に paths フィルタが無く、docs-only PR でも実 CI check が付くため動機の構成自体が無い。**mergeability 短絡は不要** (幻の pending が発生しない)。ただし (i) と (ii) は個別にしか pin されておらず**連結が未固定**だったため、rollup JSON → `decide()` の end-to-end regression test 5 本を追加した (短絡が効く 2 ケース / 効きすぎない 3 ケース)。変異テストで検知を実測済み。

### 順位 292: lock.rs を token 方式の所有権検証へ統一

- **不具合**: `src/cli-pr-monitor/src/lock.rs` の `MonitorLock::Drop` (L41-50) が無条件 `remove_file` で、stale takeover 後に旧プロセスの Drop が新プロセスの lock を誤削除する (PR #271 で `pipeline_lock.rs` に修正済みの同型バグ)。
- **対処**: `src/lib-jj-helpers/src/pipeline_lock.rs` の token 方式を踏襲 — token フィールド追加 + Drop を token 一致確認付き削除に変更 + takeover 後の誤削除がないことの regression test。
- **結果 (2026-08-20)**: 実装済み。token 生成は複製せず `pipeline_lock::generate_lock_token()` を公開して共有した (同用途の識別子を作り直すと片側だけ前提が崩れても気づけない)。**旧 format の lock ファイル**は `#[serde(default)]` で読めるようにした — 必須にすると旧 lock が「破損 = stale」扱いになり、fresh な旧 lock を踏み越えて同時監視が起きる。regression test 3 本 (takeover 後の誤削除 / 旧 format fresh / 旧 format stale) を追加し、変異テストで検知を実測。あわせて `lock.rs` が 800 行を超えたため test module を `lock/tests.rs` `lock/proptests.rs` へ分離した (`stages/poll/rate_limit.rs` と同じ `#[path]` 方式)。

### 順位 385: lock の liveness check 要否判断

- **位置づけ**: **バグ報告ではなく判断タスク**。stale 判定は経過 1800s のみで pid 生存を見ない (module doc に既知トレードオフとして明記済み)。crash 時の復帰窓が最大 30 分になる。
- **判断**: pid 生存確認の要否を決める。**「不要」も正規の出口** — 影響は interactive セッションの監視遅延に限られ GitHub Actions 経路は無関係。不要なら根拠を lock.rs の module doc へ追記して閉じる。採用するなら pid 再利用対策 (start_time 併用) + OS 差の吸収を設計し、順位 301/303 の既存 TOCTOU 設計判断と衝突しないことを確認。
- **完了基準**: 採否いずれかが根拠つきで module doc (または ADR) に記録されていること。
- **結果 (2026-08-20、ユーザー判断)**: **不採用**。根拠を `lock.rs` の module doc に「pid の生存確認を入れない理由」として記録した (影響は interactive セッションの監視遅延のみ / pid 再利用の誤判定は fresh lock の takeover = 同時監視という**現状より悪い失敗**を招く / 本 lock は助言層で fail-open が正しい / 順位 301 の既存判断と整合)。**再検討の条件**も併記した。

**後始末**: todo13.md 246 節 / todo15.md 292 節 / todo21.md 385 節を削除、todo-summary2.md の 246 / 292 / 385 行を削除。

---

## PR G: fix(jj-helpers): bookmark 探索の深さ非依存化 + 自動 fix 後始末 (順位 386 + 387)

**束ねる理由**: 同じ「監視・自動 fix 経路が作るコミット」への対処の両面 (bookmark 探索が壊れる / ローカル副作用が残る)。387 のエントリ自身が 386 との同一 PR 化を検討事項として挙げている。

### 順位 386: 空コミットで bookmark が探索範囲外に出て merge-pr / push が失敗

- **不具合**: 計 9 回観測。`BOOKMARK_SEARCH_REVSETS = ["@", "@-", "@--"]` (`src/lib-jj-helpers/src/bookmarks.rs:25`) の 3 段しか遡らず、監視・自動 fix 経路が積む空コミットで bookmark が範囲外に出る。push 経路では「bookmark を @ に自動更新」が空コミットへ bookmark を移し `Won't push commit ... since it has no description` で失敗 (#370)。
- **対処 (本命)**: 探索を深さ非依存 revset (`heads(::@ & bookmarks())` 等) へ変更。`heads()` は複数 bookmark を返しうるため trunk 系除外 + 単一化の規律を維持する。**3 crate (push-runner / pr-monitor / merge-pipeline) すべてで回帰確認** (ADR-024)。push-runner の「@- 自動更新」も空コミットを飛ばす形へ揃える。
- **注意**: 2026-08-11 の追加観測で「子コミットに別 bookmark がある」ケースは*近い方を採る規則そのもの*が原因で、**深さ非依存化だけでは解決しない可能性**が指摘されている。検討して残る場合は挙動を明記する (順位 397 の `--pr` 逃げ道は両症状で機能済み)。
- **回帰テスト**: bookmark が @--- 以深にある構成での解決を固定。
- **完了基準**: 深い位置の bookmark で `pnpm merge-pr` / `pnpm push` が解決できること (またはその状態自体が発生しなくなること)。採った案の根拠を記録。

### 順位 387: 自動 fix 経路が push BLOCK 後もローカルを書き換えたまま残す

- **不具合**: #366 で scope guard (ADR-054) が push を BLOCK した後も、ローカルに fix コミットと working-copy 変更が残った (気づかなければ次作業に混入)。#369/#370 でも再発。
- **対処**: 自動 fix / 監視経路の終了パスを洗い、BLOCK・失敗時に (a) fix コミット・空コミットをロールバックする、または (b) 「未 push の自動生成コミットが残っている」と警告する。どちらも ADR-022 の「自動化コンポーネントは自分の副作用を後始末する」責務。
- **完了基準**: BLOCK / 失敗後にローカルへ未 push の自動生成コミットが残らない、または残ることが明示的に警告されること。

**後始末**: todo21.md 386 / 387 の両節を削除、todo-summary2.md の 386 / 387 行を削除。台帳の順位 412 / 426 (auto lane、同 crate 別ファイル) との衝突は低いが、着手時に `claude/nightly-412` / `nightly-426` ブランチの有無を確認する。

---

## PR H: fix(push-runner): push 経路 stage 修正束 (順位 376 + 254 + 322)

**束ねる理由**: 3 件とも `cli-push-runner` の stage 単位の独立修正で相互に無干渉。

### 順位 376: bookmark 自動前進がスタック境界を壊す

- **不具合**: 2026-08-06 実観測。スタック push 時に **@ の祖先にあたる非 trunk bookmark をすべて @ へ前進**させ、レビュー済み PR #361 の bookmark が #363 の tip を指した (gate が止めなければ silent 混入)。Severity High。
- **対処**: 自動前進の対象を絞る — 案 (a) 「@ と同一コミットを指す bookmark」のみ、案 (b) push 対象として解決した 1 本のみ。どちらも単一ブランチ運用の挙動は不変。実装は bookmark stage。
- **回帰テスト**: スタック構成で前進しないこと + 単一ブランチ構成で従来どおり前進すること (**両方向**)。

### 順位 254: pr_size_check の base をローカル master から remote tracking ref へ

- **不具合**: `[pr_size_check] default_branch = "master"` がローカル bookmark 基準のため、並列 workspace でローカル master が遅延すると merge 済み PR 分を合算 (実 160 行 → 1604 行と誤 block、実害あり)。ADR-013 の `sync_local` は `master@origin` 原則を test で固定済み — 同じ原則を適用する。
- **対処**: config を `master@origin` に変更するか、pr_size_check 側で remote tracking ref を優先解決する fallback を実装 (着手時判断)。`[file_length_gate] base` も同点検。ローカル master 遅延を模した revset 解決レベルの test を検討。

### 順位 322: scratch_file_warning が pattern 列挙 (deny-list) で新規命名をすり抜ける

- **不具合**: post-merge-feedback の takt run が repo root に `analyze_transcript.py` を残し、`[scratch_file_warning]` の `patterns = ["__*", "_tmp_*"]` に一致せず素通り (near-miss)。**AI が付ける名前を列挙で先回りするのは原理的に不可能**。
- **対処**: 先に再現確認 (post-merge-feedback 再実行で再現するか)。方式は (a) instruction facet への禁止明記 (助言層、**単独では不可**) + (b) repo root の追跡外新規ファイルを allow-list 以外すべて警告 (誤検知コスト見積もり要) または (c) root 未追跡 `*.py` 等の配置ベース判定 (本 repo は Rust + TS 構成なので高確度)。(a) + (b or c) の二層。
- **回帰テスト**: `analyze_transcript.py` (同名再作成でよい) を実 fixture として固定 (ADR-049 流儀)。deny-list の限界を `scratch_file_warning.rs` の module doc に記録。
- **完了基準**: takt run が root に残した一時ファイルが push 前に検出され、検出方式が pattern 列挙に依存しないこと。

**後始末**: todo20.md 376 節 / todo15.md 254 節 / todo17.md 322 節を削除、todo-summary2.md の 376 / 254 / 322 行を削除。

---

## PR I: fix(push-runner): bookmark_check の未レビュー祖先 fail-closed (順位 288(b))

- **背景**: 順位 288 の残タスク後半。`[diff]` stage の PR 範囲化 (実装済み) 後も、`src/cli-push-runner/src/stages/bookmark_check.rs` に「@ の非 trunk 祖先が未レビューのまま push される穴」(T8 / PR #280 と同クラス) の検証が残っている。
- **対処**: `<default_branch>..@` の各祖先コミットと pre-push review 証跡を対応付け、いずれかが未レビューなら **fail-closed で push を拒否** (ADR-043)。
- **回帰テスト**: レビュー済み祖先 / 未レビュー祖先の両ケースを seal。
- **PR H と分ける理由**: 同 crate だが、新しい fail-closed gate の追加は誤 block リスクがあり、切り戻し単位を独立させる。
- **後始末**: **ここで順位 288 のエントリ (todo15.md「post-merge feedback の pre-push reports を対象 PR の全 run 集約に拡張」節) を削除** (前半 (a) は PR B で完了済みの前提) + todo-summary2.md の 288 行を削除。

---

## PR J: fix(pr-monitor): post-pr-review の docs-only 判定を PR 全体基準に (順位 233)

- **不具合**: PR #227 で post-pr-review の analyze が `@` コミット (docs のみ) の diff を見て PR を docs-only と誤判定し、CodeRabbit が PR 全体で出した finding を ADR-035 filter で誤って適用外化。有効 finding の見逃しリスク。
- **診断から**: docs-only 判定に使う diff の生成箇所を特定する (pre-push の `review-diff.txt` 流用か、post-pr-review 独自の `@` 限定 diff 生成か)。起動箇所は `cli-pr-monitor` の `stages/takt.rs` 周辺。
- **対処** (どちらかを選択): (A) analyze に渡す diff を PR 全体 (base..head) に変更 — pre-push (ADR-027 = `@` 限定 simplicity) と diff 生成を共有しているなら post-pr-review 専用に分離する。(B) docs-only 分類を CodeRabbit findings の file path 基準に切替 (findings が code file を指すなら docs-only にしない)。
- **dogfood**: code + docs 混在 PR で誤判定しないことを確認。
- **完了基準**: code 変更を含む PR が docs-only 誤判定されず、code finding が誤フィルタされないこと。
- **後始末**: todo13.md 233 節 + todo-summary2.md 233 行を削除。

---

## PR K: fix(subprocess): timeout の孫プロセス穴を塞ぐ (順位 323)

- **不具合**: `lib-subprocess` の `run_cmd_shell_with` (capped / capped_reporting / unlimited の共通骨格) は timeout 後に `child.kill()` → reader thread join するが、`cmd /c` の**孫プロセス (実際の cargo / jj) は kill 対象外**で pipe を保持し続け、join が孫の自然終了までブロックする (実測: `timeout_secs = 1` のテストが 9.23s)。quality_gate / push / merge-pipeline のハング保護が実質無効 (ADR-043 の空洞化)。同根の実害: #286 の orphan takt による stale `.failed` marker。
- **手順**:
  1. **経過時間 assert 付きの再現テストを先に書く** (T6 = PR #283 の教訓: Err 内容だけの assert では素通りする)。
  2. (a) 失敗経路では join せず detach (実績あり。ただし `_capped` 系は表示用出力を捨てるトレードオフ) vs (b) 孫まで殺す (`taskkill /T /F` or Job Object。orphan 発生自体を止め stale marker 問題にも波及効果。Windows 実装コスト要見積) を評価して選択。選ばなかった側の理由を `run_cmd_shell_with` の doc に記録。
  3. (b) を採らない場合、`feedback::reconcile_takt_output` の「reconciliation が kill 直後 1 回のみ」の穴への緩和策を別途検討する。
  4. 3 variant + 呼び出し元 (cli-push-runner quality_gate / push、cli-merge-pipeline) で回帰確認。実機 E2E は `ping -t` 差し替え + before/after 経過時間比較。
- **完了基準**: `timeout_secs = 1` で孫が生存していても 1s + ε で制御が戻ること (経過時間 assert で seal)。ハングするコマンドが各 timeout で実際に打ち切られること。
- **後始末**: todo17.md 323 節 + todo-summary2.md 323 行を削除。

---

## PR L: fix(automation): 自動化経路の小穴・ノイズ修正束 (順位 467 + 181)

**束ねる理由**: どちらも自動化経路の出力品質の小修正で、単独 PR を立てる規模ではない。

### 順位 467: 夜間ループとレポート出力の小穴 3 点

- **D-1** (`.github/workflows/nightly-todo.yml`): 掃除ループの `git push --delete` 前に `git ls-remote` で ref の存在を確認し、既に消えていれば warning で skip (現状は `set -euo pipefail` で step 全体が中断)。**失敗の種別を潰さない** — 「ref が既に消えている」だけを warning + 継続にし、**ネットワーク / 認証エラーは従来どおり失敗させる** (ADR-072 決定 10)。TOCTOU で削除時に消えていた場合も種別判定で「既に消えている」として扱う。
- **D-2** (`src/lib-ledger/src/summary_gate.rs`): parse エラーに行番号と文脈を含める診断強化 + テスト。
- **F-2** (`src/cli-stale-branch-scan/`): `--repo` 明示時は GIT_DIR 導出失敗の警告を抑止 (夜間 workflow は jj リポジトリ外で走るため毎晩出る)。

### 順位 181: aggregate-weekly facet の findings.json が markdown fence で wrap される

- **不具合**: facet LLM が ` ```json ... ``` ` で wrap して write し、weekly-review skill が JSON parser に直接渡せない (2026-05-30 dogfood で実観測、skill 側は手動 strip の workaround 中)。
- **対処**: `.takt/facets/instructions/aggregate-weekly.md` の JSON 生成 section に「**raw JSON のみ、fence で囲まない。先頭 `{` 末尾 `}`**」を明示。(option) skill 側に defensive strip 手順を補足。**instruction 修正で habit を矯正できるかは未確定** — 次回 `/weekly-review` の dogfood で確認し、ダメなら skill 側 strip へ切替。

**検証**: `cargo test --workspace` + `pnpm lint:workflows`。**エントリ後始末は実走観測後** → [§ 残観測トラッキング](#残観測トラッキング)。

---

## 保留事項 (PR D 完了時に扱う) — すべて消化済み

ユーザー判断で PR D まで先送りしたもの。**先送りした時点では本計画の外に記録が無く、ここが唯一の記録だった。** 各項の消化にあたって記録先を本計画書の外へ移したため (下記)、現時点で本節の削除により失われる記録は無い — これが退役条件 4 の求める状態である。

> **`cwd_to_project_id` の Linux case 不一致は調査のうえ閉じた (2026-08-19、ユーザー判断)。**
>
> **課題としては残さない** — 記述されていた欠陥は [PR #421](https://github.com/aloekun/claude-code-hook-test/pull/421) で既に解消済みだった。`resolve_project_dir` が `read_dir` + 両側 lowercase 比較で解決し、`resolve_project_dir_matches_case_insensitively` ほか 2 本のテストで seal されている。**実 Linux (WSL Ubuntu-24.04 / ext4、case-sensitive であることを probe で確認) で 2 本とも pass** し、`cwd_to_project_id` の呼び出し元も `resolve_project_dir` のみと確認した。実障害の観測は無し。
>
> **解決の記録は本計画書の外にある** (だから本節の削除で記録は失われない) — 機構と理由は `resolve_project_dir` の doc コメントにあり、Linux で成立し続けることは [ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md) の ubuntu leg が毎 PR 実行して担保する。**一度きりの観測ではなく機械化された保証**である点が、退役条件 4 が守ろうとしている「本ファイルが唯一の記録である項目」との違い。
>
> **残る既知の穴 (実測済み・対応保留)**: case-sensitive filesystem では `Foo` と `foo` を同じ `projects_root` に置ける。両方が lowercase 比較に一致すると `resolve_project_dir` の `.find(...)` は **1 件だけ返し、もう一方を無言で除外する**。WSL Ubuntu-24.04 / ext4 で 5 回試行し、毎回 1 件のみ返ることを確認した (どちらが返るかは `read_dir` の順序依存で、契約上は未規定)。**対応を保留する根拠**は「今のところ発現経路を確認できていない」ことのみ — case-sensitive FS では同一ディレクトリの綴りが一意なので、同じ workspace root から 2 通りの綴りは通常生まれない。**ただしこれは実測ではなく推論であり、`~/.claude/projects` を OS 間で持ち込む等の経路は排除できていない。** 発現したら「複数一致は黙って選ばず loud にする」で塞ぐ ([ADR-043](adr/adr-043-security-gates-fail-closed.md))。**この穴自体の記録は本計画書の外に無かったため、[todo24.md](todo24.md) 順位 475 として起票し、本節の削除後も記録が残るようにした。**

### フィードバック採否 — 消化済み (2026-08-19)

7 PR 分 ([#417](https://github.com/aloekun/claude-code-hook-test/pull/417) / [#418](https://github.com/aloekun/claude-code-hook-test/pull/418) / [#419](https://github.com/aloekun/claude-code-hook-test/pull/419) / [#420](https://github.com/aloekun/claude-code-hook-test/pull/420) / [#421](https://github.com/aloekun/claude-code-hook-test/pull/421) / [#423](https://github.com/aloekun/claude-code-hook-test/pull/423) / [#424](https://github.com/aloekun/claude-code-hook-test/pull/424)) の
post-merge feedback **全 48 提案**を採否判定した。内訳は **採用候補 16 / 様子見 18 / 却下推奨 14**。

採用分を 7 系統に分類し、**[todo24.md](todo24.md) へ 5 タスク (順位 470-474) として起票した** (ユーザー判断: 全 7 系統を採用)。
統合の単位は「そのまま 1 PR になる粒度」。様子見・却下推奨は個別登録しない。

**起票前の実コード確認で 1 件が脱落した** — #417 の「`REPORT_FILE_NAME` / `RUN_REPORT_FILE_NAME` の pin テスト」は
既に両 crate に実装済みだった。同 PR の他 2 提案も大部分が実装済みで、残片だけを順位 471 に載せている。
**feedback レポートも台帳と同じく実装が動くほどずれる** — 本計画の「着手前に必ずやること」がそのまま当てはまる。

**これで § 保留事項は空になった** (退役条件 4 を充足)。

## 残観測トラッキング

完了基準に実走観測を含むタスク。マージ後に観測し、確認できたらエントリ後始末 (todoN.md 節 + summary 行の削除) を docs バッチで行う。

- [ ] **319** (PR E): マージ後の実 PR 数件で backstop 投稿が walkthrough 1 回につき両経路合算 ≤ 1 件であること → 確認後 todo17.md 319 節 + todo-summary2.md 319 行を削除
- [ ] **431** (PR E): 次にレート制限が起きた夜間 run で「未レビュー」が可視化されること → 確認後 todo22.md 431 節 (`review-request` の成功判定…) + todo-summary2.md 431 行を削除
- [ ] **467 D-1 / F-2** (PR L): 次回 dispatch or schedule 実走で、消えたブランチで job が落ちないこと + GIT_DIR 警告が出ないこと → 確認後 todo24.md 467 節 + todo-summary2.md 467 行を削除
- [ ] **181** (PR L): 次回 `/weekly-review` で findings.json が raw JSON で出力されること → 確認後 todo12.md 181 節 + **todo-summary.md** (順位 219 以下側) の 181 行を削除。矯正できなければ skill 側 strip へ切替してから完了

## 本計画書の退役手順

すべての作業の完了をもって本ファイルを削除する。条件と手順:

1. 進行表の 12 PR がすべてマージ済みであること
2. [§ 残観測トラッキング](#残観測トラッキング) の 4 項目がすべて消化され、対応するエントリ後始末が完了していること
3. 順位 288 のエントリ (todo15.md) が PR I 完了時に削除されていること
4. **[§ 保留事項](#保留事項-pr-d-完了時に扱う--すべて消化済み) が空であること** (2026-08-19 に充足) — 未処理のまま残っていれば、行き先を作ってから削除する (フィードバック採否は消化、`cwd_to_project_id` の case 問題は台帳エントリへ起票)。**本ファイルが唯一の記録である項目を、本ファイルの削除と一緒に消してはならない。** 順位 469 のエントリ削除で実際にこれをやり、記録を一度失った
5. **[§ 着手前に必ずやること](#着手前に必ずやること) の各項が、本ファイル外へ移送済みであること** — 台帳前提の実測・シグネチャ変更時の下流追跡・照合キーの一意性確認・doc 記述の実在確認はいずれも本計画に固有でない再発防止知見なので、[dev-conventions.md](dev-conventions.md) へ移す
6. `grep -rn "bugfix-batch-plan" .` で本ファイルへの参照が残っていないことを確認する (検索対象パス `.` を省くと標準入力待ちになるため必ず付ける)
7. 本ファイルを物理削除する (削除自体は残観測の最後のエントリ後始末と同じ docs バッチ PR に同乗してよい)

永続化すべき知見 (再発防止策・設計判断) は各 PR で ADR / module doc / dev-conventions に書き込む方針のため、**上記 4・5 を終えた後の**本ファイルに永続価値は残らない。
