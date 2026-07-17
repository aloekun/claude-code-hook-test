# push パイプライン不具合修正・改善 作業計画

> **このファイルの寿命**: 本計画の全タスクが完了 (実装 or 明示的却下) した時点で、
> 本ファイル自体を削除する (§8 削除条件)。恒久ドキュメントではない。
> 先行例: `file-length-enforcement-plan.md` の削除条件パターン。

## 1. 背景 (2026-07-16 調査の要約)

`pnpm push` (= `cli-push-runner.exe` → takt `pre-push-review` → `jj git push` → `cli-pr-monitor.exe --monitor-only`) の
遅延と不具合を調査した。実測と根拠は以下の通り。

### 実測ベースライン (2026-07-16 時点、`.takt/runs/` 直近 20 run)

| 指標 | 値 |
|------|-----|
| takt 部分 全体 | 中央値 3.8 分 / 平均 5.6 分 / 最大 14.6 分 |
| fix なし run (11/20) | 1.1〜3.8 分 |
| fix あり run (9/20) | 5.5〜14.6 分 |
| simplicity-review execute | 平均 203s / 最大 399s (毎回発生・並列の律速) |
| security-review execute | 平均 91s (simplicity と並列、wall-clock 寄与小) |
| fix execute | 平均 296s / 最大 487s |
| report+judge フェーズ (takt 固定費) | 15〜25s/step、1 iteration 合計 1 分弱 |
| quality_gate rust-lint-test (takt 前) | clippy + cargo test + `--ignored` (269s 実測記録 = `push-runner-config.toml` step_timeout コメント) |
| REJECT 頻度 (全 56 run) | simplicity 9 回 / security 2 回 |

計測方法 (after 計測で再現すること): 各 run の `meta.json` の startTime/endTime、
`trace.md` の `- Started:` / `- Completed:` 行を集計する。takt 外は `cli-push-runner` の
`パイプライン完了 (Xs)` ログ行と、T0 で追加した stage 別ログ行
`[push-runner] stage=<name> elapsed=<秒>s` (name = pre_checks / quality_gate / diff /
takt / push) を使う。書式の定義元は `src/cli-push-runner/src/log.rs` の
`format_stage_elapsed()`。

### 結論

- 12 分超の主犯は takt のオーケストレーション固定費 (~1 分/iter) ではなく、
  (1) quality_gate 内の **assert ゼロの Ollama eval テスト** (毎 push 実行)、
  (2) **takt builtin の 8KB checklist policy** が ADR-036 の anomaly-only 設計を上書きして
  REJECT → 5〜8 分の fix iteration を誘発、
  (3) fix step 内での **workspace ビルド+テストの重複再実行**。

> **(1) は T1 の実測で下方修正済み (2026-07-16)**: eval は 269s ではなく 41s だった
> (GPU 更新により推論が高速化。§5 T1 実施結果)。(1) は「主犯」ではなく小口の無駄であり、
> 12 分超の主因は (2)(3) = **takt の execute/fix 時間**に絞られる。以降のタスクの
> 期待効果を §1 の数値から見積もる場合、同様に stale な前提が無いか実測で確認すること。
- 追加実装レベルで「コード変更 push 12 分超 → 5〜7 分、docs-only push → 1 分弱」が見込める。
- 根本再設計 (takt 離脱) は本計画のスコープ外。本計画完了後に ADR-055 telemetry と
  CodeRabbit findings 突合の実測を見て別途判断する (§7)。

## 2. 進め方の原則 (実施セッションへの制約)

1. **PR 分割**: `pr_size_check` は warning 800 / block 1500 行 (insertions+deletions)。
   タスクごとに小 PR で進める。T2 (crate 削除) のみ大量削除になるため
   `PR_SIZE_CHECK_OVERRIDE=1` の使用を PR 説明に明記して bypass する。
2. **regression test 必須**: レビュー policy は「Bug fix without a regression test」を
   無条件 REJECT する。不具合修正 (T5〜T8) は必ず再現テストを先に書く。
   incident 由来のものは ADR-049 (incident→eval 回帰スイート) の流儀に従う。
3. **実験的機能は ADR-039 の 3 点セット** (config opt-in / kill-switch / bounded lifetime) を守る。
   gate 系の変更は ADR-043 (fail-closed) に従う: 判定不能時はフル実行に倒す。
4. **dogfood のブートストラップ注意**: 各 PR の push は「修正対象のパイプライン自身」を通る。
   quality_gate や takt 設定を壊すと自分の push が通らなくなる。1 PR 1 変更を徹底し、
   失敗時は直前の設定へ revert できる粒度を保つ。
5. **exe の再ビルドを忘れない**: `.claude/*.exe` が実際に実行される配布物。
   Rust 変更後は対応する `pnpm build:<name>` (または `pnpm build:all`) を実行しないと
   古い exe をテストすることになる。Windows 注意: build script の `cp` は
   Git の `usr/bin` が PATH に必要。派生プロジェクトへの配布は `pnpm deploy:hooks`。
6. **ADR 更新**: 各タスクの「ADR」欄に従い、新規 ADR または既存 ADR への追記を PR に含める。
   docs は日本語で書く。

## 3. タスク一覧 (推奨実施順)

| # | タスク | 種別 | 期待効果 | 規模 | 依存 |
|---|--------|------|----------|------|------|
| T0 | stage 別計測ログ + before 記録 | 計測 | 効果検証の基盤 | XS | なし |
| T1 | Ollama eval を gate から除外 | 改善 | **-42s/push** (実測後修正、当初見積 -2〜4.5 分) | S | T0 |
| T4 | refute facet dogfood 開始 | 改善 | FP 起因 fix iteration の削減 | XS | なし |
| T5 | push 拒否検知の truncate 依存修正 | 不具合 | silent-failure push の防止 | S | なし |
| T6 | diff stage の timeout 追加 | 不具合 | 無限ハング防止 | XS | なし |
| T7 | Stop hook file-length step の cwd 依存修正 | 不具合 | quality gate 誤失敗の防止 | S | なし |
| T8 | bookmark_check の空 `@` 誤誘導修正 | 不具合 | exit 7 誤案内の防止 (**再現確認済**) | S | なし |
| T3 | `pnpm build` 形骸ゲートの実体化 or 削除 | 不具合 | 見せかけゲートの解消 | XS | なし |
| T2 | 旧 cli-push-pipeline の workspace 除去 | 改善 | clippy/test 対象の純減 | S (大量削除) | なし |
| T10 | takt builtin review policy の shadow | 改善 | **-1.5〜3 分/iter + 無駄 fix 削減** | M | T0 |
| T11 | docs-only / 空 diff の決定論 routing | 改善 | docs-only push **-6〜8 分** | M | T1 |
| T12 | fix 後の決定論再ゲート + fix 検証義務の縮小 | 改善+安全 | -1〜3 分/fix iter + 自己申告依存の解消 | M | T1, T11 |
| T13 | backlog 小物 (§6) | 任意 | 各 -数秒〜1 分 | XS×n | 任意 |
| T99 | after 計測 + 本ファイル削除 | 完了 | - | XS | 全タスク |

> T9 は欠番 (採番時の意図的な予約なし。実施セッションが新規タスクを追加する場合の
> 空き番号として使ってよい)。

T1 を最優先とする理由: 以降の全 PR の dogfood push が速くなり、作業全体が複利で加速する。

## 4. 不具合修正タスク詳細

### T5: push 拒否検知が 40 行 truncate 済み出力に依存

- **現状**: `src/cli-push-runner/src/stages/push.rs` の `push_was_refused()` (L118-120 付近) は
  `run_stage_cmd` (= `run_cmd_shell_capped`、MAX_LINES=40 silent truncate、`runner.rs`) の
  出力に `refusing to` を検索して push 成否を判定する。
  `src/lib-subprocess/src/lib.rs:233-237` の doc が「control flow 判定に出力を使う callsite では
  capped を使うな」と明記しており、その契約違反。出力 40 行超で拒否行が落ちると
  **リモート未反映のまま exit 0** → pr-monitor が旧 head を監視する。
- **方針**: push コマンドの実行だけ非 truncate に切り替える。lib-subprocess には
  `drain_pipe_unlimited` と `run_cmd_shell_capped_reporting` (truncate 明示 variant、
  cli-merge-pipeline が MAX_LINES=200 で使用) が既にある。判定用には unlimited 相当を使い、
  ログ表示用にのみ cap する。あわせて `contains("refusing to")` の誤爆
  (成功出力に偶然含まれるケース) を行頭マッチ等に厳格化するか検討。
- **テスト**: 41 行以上の出力の末尾に `Refusing to ...` を含む fixture で
  「拒否が検知されること」の回帰テスト。既存の push stage テスト群に追加。
- **リスク**: 低。出力保持量が増えるだけ。
- **実施結果 (2026-07-17, 実装済み / PR #282)**:
  - **実装**: 方針どおり「判定は全量・cap は表示側のみ」。
    - `lib-subprocess` に `run_cmd_shell_unlimited` を追加した。既存 asset のうち
      `drain_pipe_unlimited` は pipe 単体、`run_cmd_shell_capped_reporting` は truncate を
      明示するだけで**判定用には依然不足** (cap は残る) のため、`run_cmd_shell` family に
      欠けていた unlimited variant を足す形にした。3 variant の共通骨格
      (spawn → drain → wait → combine) は 3 つ目の copy が出た時点で `run_cmd_shell_with` に
      集約し、各 variant は drain 戦略の違いだけを表す。境界判定は ADR-044 §「後続の
      variant 追加」に記録した。
    - push stage は `run_push_cmd` (unlimited) で全量取得し、`push_was_refused` は
      **全量出力**に対して判定する。表示は成功時のみ `cap_for_log` (先頭 40 行 +
      `... (N lines truncated)`) を通し、**失敗経路 (拒否 / Err) は全量表示**する
      — 失敗時こそ診断情報を落としてはならないため (§6 backlog 1 と同じ理由)。
      成功時のログ量は従来どおり 40 行で、増えない。
    - **副産物: `runner::run_stage_cmd` を削除**した。push stage が唯一の呼び出し元だったため
      未使用になり clippy が検出。dead code を残さない方針 (T2 と同じ) に加え、
      「capped 経路で control flow 判定する」罠を構造的に排除する意味がある。
      `MAX_LINES` は表示用として残置し (quality_gate / scratch_file_warning / lint_screen が使用)、
      doc に「判定に使う出力を本値で cap してはならない」を明記した。
  - **`contains` 厳格化は不採用 (ユーザー承認済み)**: 方針の「行頭マッチ等に厳格化するか検討」は
    **見送り**、`contains("refusing to")` を維持した。理由はリスクの非対称性: 誤検知
    (push 成功を失敗と報告) は出力もそのまま表示されるため気付いて再実行できるのに対し、
    検知漏れは**リモート未反映のまま exit 0** = T5 が直そうとしている事故そのもの。
    jj のメッセージ書式変更で検知漏れ側に倒れる厳格化は ADR-043 (fail-closed) に反する。
    判断根拠はコード doc (`push_was_refused`) にも残した。§6 backlog 8 は却下扱い。
  - **回帰テスト (ADR-049 の流儀)**: `mod t5_truncated_refusal_detection` に 6 本
    (cli-push-runner 206 passed。`run_stage_cmd` の 2 本を削除したため 208 → 206)、
    `lib-subprocess` に unlimited variant の 4 本 (31 passed)。
    bad = 41 行目の拒否行を検知すること、good = 40 行超の正常出力を誤検知しないこと、
    および「表示 cap は判定に影響しない」ことを固定した。
    **修正前の挙動に対して失敗することを確認済み**: `run_push_cmd` を capped 版に戻すと
    上記 3 本が fail する (「run_push_cmd が 40 行に切り詰めている = T5 の不具合」)。
    回帰テストが素通りしないことの実証。
  - **サンドボックス実機検証 (before/after)**: 短いパス (`C:\t5\repo`) に jj repo を張り、
    `[push] command` を「40 行の正常出力 + 末尾に拒否行」の fake command に、
    `[diff] command` を空出力にして takt を skip、quality_gate を noop にして
    push stage まで到達させ、配布 exe (修正前) と修正後 exe を比較した。

    | | before (現行配布 exe) | after (修正後) |
    |---|---|---|
    | 41 行目の拒否行 | 見逃し → `[push] 成功` | `[push] 失敗: リモートに反映されませんでした (jj が push を拒否)` |
    | exit code | **0 (silent failure が再現)** | 3 (EXIT_PUSH_FAILURE) |
    | 成功時 (50 行・拒否なし) の表示 | 40 行で silent truncate | 40 行 + `... (10 lines truncated)`、exit 0 |

    before が**「リモート未反映のまま exit 0」を逐語で再現**することを確認した上で修正を当てている。
    この状態で本番なら pr-monitor が旧 head を監視し始める。
  - **発見 (本タスク外)**: `cli-pr-monitor` の `push_to_remote` は exit code のみを見ており
    **拒否検知が無い**。post-PR の re-push で同型の silent failure が起き得る。
    §2 原則 4 (1 PR 1 変更) に従い PR #282 では触れず、§6 backlog 9 に追加した。
  - **post-PR レビュー指摘の採用 (3 件、ユーザー承認済み)**:
    - **CodeRabbit Minor**: `run_cmd_shell_capped` の doc が「Err 経路で child を kill しない
      basic semantics」と書いていたが、実際は Err 経路を `kill_and_join_err` が受けて
      child を kill + reap し reader thread も join する。`wait_with_timeout_basic` 単体の
      性質としては正しい記述が、`kill_and_join_err` 導入 (PR #208) 以降 stale になっていた
      **pre-existing の不整合**。child の lifecycle は 3 variant 共通なので、記述を共通骨格
      (`run_cmd_shell_with`) の doc に集約し、variant 側は参照のみにした。
    - **pre-push warning (書式重複)**: `cap_for_log` の `... (N lines truncated)` 書式が
      `drain_pipe_capped_reporting` と重複していた。切り詰めの実装自体は共有できない
      (streaming vs materialize 済み文字列) が、**書式片は共有できる**という指摘は妥当なので
      `lib_subprocess::truncation_notice` として切り出し、両者から使う形にした。
    - **pre-push warning (表記)**: 本節と §8 の T5 行の「本 PR」を PR 番号採番後に backfill。
      T4 行が「本 PR」のまま放置され本 PR で backfill する羽目になった負債を、同じ形で
      繰り返さないため。

### T6: diff stage の timeout 欠落

- **現状**: `src/cli-push-runner/src/stages/diff.rs` (L20-23 付近) は `Command::output()` を
  無限待ち。他 stage は全て timeout 付き (jj 系 30s、gate 600s、push 300s)。
  ADR-045 の並列 workspace 運用で jj lock 競合時にパイプラインが無言ハングする。
- **方針**: 他 stage と同じ timeout 機構 (lib-subprocess) に載せ替え、timeout 時は
  明確なエラーで exit (fail-closed)。timeout 値は jj 系 30s に合わせるが、
  大 diff の書き出しを考慮して 60s 程度でも可。
- **テスト**: timeout 経路の unit test (長時間コマンドの fixture で Err になること)。
- **リスク**: 低。
- **実施結果 (2026-07-17, 実装済み / PR #283)**:
  - **timeout 値は 60s + `[diff] timeout` で上書き可 (ユーザー承認済み)**。方針が
    「jj 系 30s に合わせるが 60s 程度でも可」と両論併記だったため確認した。60s の根拠:
    diff は working copy の snapshot + 大 diff の書き出しを伴い、読み取りのみの
    `jj bookmark list` (30s) より重い。timeout の目的は**ハング検知**であって latency
    制限ではなく、誤 timeout は pipeline 全体の中断 (exit 5) を招くため余裕側に倒す。
    config 化は `[push] timeout` と同形 (`Option<u64>` + 既定値定数) で、誤 timeout する
    環境の escape hatch。本リポジトリの config は未指定 = 既定 60s。
  - **実装**: `run_diff_cmd` を `Command::output()` (無限待ち) から
    spawn → `drain_pipe_unlimited` × 2 → `wait_with_timeout_safe` に載せ替えた。
    - **`run_cmd_shell_unlimited` (T5 で追加) は使えない**: `run_cmd_shell_*` は全 variant が
      `combine_output` で stdout と stderr を結合するが、diff の stdout は reviewers が読む
      レビュー対象そのものとしてファイルに書かれる。jj が stderr に出す警告
      (並列 workspace 運用時の `Concurrent modification detected` 等 = **まさに本タスクが
      想定する状況**) が混入するとレビュー対象を汚す。よって分離を維持した。
    - 同型の「全量 + 分離 + timeout」は `bookmark_check::run_jj_bookmark_list` にもあるが、
      そちらは direct args で signature が非互換のため共通化しない (**ADR-044 層 1** の
      「shell vs direct args は各 crate 残置」に該当)。判定は ADR-044 に追記した。
    - `wait_with_timeout_basic` でなく `_safe` を選んだのは、try_wait 失敗時に早期 return する
      callsite で child を残さないため (ADR-044 層 2 の「Err 経路で kill するか」)。
  - **⚠ 初版実装の欠陥を回帰テストが検出した (本タスク最大の学び)**: 「timeout 後に
    reader thread を join する」初版は、`[diff] timeout = 1s` に対し制御が戻るまで
    **9.6s** 掛かった。原因は `cmd /c <command>` の構造で、`child.kill()` が殺すのは
    cmd.exe だけで**孫 (実際の `jj`) は生き残る**。孫は pipe の書き込み端を継承したままなので
    EOF が来ず、join が孫の自然終了までブロックする = **timeout が意味を成さない**
    (T6 が直そうとしているハングの再生産)。よって失敗経路では thread を join せず
    detach して即座に返す (push-runner は直後に exit 5 で終了するため thread は道連れ)。
    出力は timeout 時に不要 (診断は timeout メッセージ自身が持つ)。
    **教訓**: timeout の回帰テストは「Err が返ること」だけでなく**経過時間を assert する**
    こと。Err の内容だけ見る初版テストなら、この欠陥は素通りしていた。
  - **回帰テスト (ADR-049 の流儀)**: `mod t6_diff_timeout` に 7 本 + config 2 本
    (cli-push-runner 206 → **215 passed**)。由来 (コード監査。T5 と同じく in the wild の
    発火記録は無く「他 stage は全て timeout 付き = diff だけが穴」という非対称として特定)
    を module doc に明記した。bad = 応答しないコマンドを timeout で打ち切り、かつ
    **5s 以内に制御を返す**こと (上記欠陥を固定)、good = timeout 内に終わるコマンドを
    誤って打ち切らないこと。あわせて「stderr を diff に混ぜない」契約も seal した
    (`run_cmd_shell_*` に載せ替えると落ちるテスト)。
    **修正前の挙動に対して失敗することを確認済み**: cli-push-runner のテスト全体が
    9.66s → **1.55s** に短縮 = timeout が実際に効いている証跡。
  - **サンドボックス実機検証 (before/after)**: 短いパス (`C:\t6\repo`、MAX_PATH 対策) に
    jj repo を張り、`[diff] command` を `ping -t 127.0.0.1` (永久に応答し続ける =
    返らない `jj diff` の代役) に、他 stage を noop にして diff stage まで到達させ、
    `@-` のソースから build した修正前 exe と修正後 exe を比較した。

    | | before (修正前 exe) | after (修正後) |
    |---|---|---|
    | 外側 kill 25s | `stage=diff elapsed=24.4s` / exit 124 (**自力で返らない**) | — |
    | 外側 kill 10s | `stage=diff elapsed=9.4s` / exit 124 | — |
    | 外側 kill なし | **無限ハング** (上記 2 点が実証) | `stage=diff elapsed=3.0s` / exit 5 |
    | 診断 | なし (無言で停止) | `diff コマンドがタイムアウトしました (3s): ping -t 127.0.0.1` + jj lock 競合を疑う旨 |

    before は **diff stage の所要時間が外側 kill の時刻にそのまま追随**する
    (25s→24.4s / 10s→9.4s) = **内部に上限が一切無い**ことの実証で、放置すれば
    無限に待つ。ユーザーは診断も無いまま手動 kill するしかない。
    あわせて (a) 実 `jj diff -r @` (既定 60s) が誤 timeout せず 0.1s で 28 行を書き出し
    takt へ進むこと (good 側) も実機で確認した。
    **副次的実証**: before の run 後に `ping.exe` が残存していた —
    孫プロセスが cmd.exe の kill を生き延びる (= join がブロックする) ことの実機裏付け。
  - **発見 (本タスク外)**: `lib-subprocess` の `run_cmd_shell_*` **3 variant すべてが同じ穴**を
    持つ。timeout 後に reader thread を join するため、孫プロセスが pipe を保持していると
    timeout が wall-clock を縛れない。実測: `run_cmd_shell_capped` に `timeout_secs = 1` を
    指定したテストが返るまで **9.23s** (既存テストは経過時間を assert しないため素通り)。
    影響先は quality_gate (`step_timeout = 300`) と push (`timeout = 300`)、cli-merge-pipeline で、
    ハングした `cargo test` / `jj git push` に対して timeout が効かない可能性がある。
    §2 原則 4 (1 PR 1 変更) に従い PR #283 では触れず、§6 backlog 10 に追加した。

### T7: Stop hook file-length step の cwd 依存 (2026-07-16 に実際に発火した incident)

- **現状**: `.claude/hooks-config.toml` の `[[stop_quality.steps]] file-length` の cmd が
  相対パス `'.\.claude\hooks-post-tool-comment-lint-rust.exe --check-modified-files'`。
  hooks-stop-quality.exe は継承 cwd のままステップを実行するため、セッションの cwd が
  リポジトリルート以外 (例: `.takt/runs` に `cd` したまま Stop) だと
  「指定されたパスが見つかりません」で gate が誤失敗する。`pnpm` 系ステップは
  pnpm が package.json を上方探索するため偶然通る。副症状として cmd.exe の
  CP932 エラー出力がそのまま流れて文字化けする。
- **方針** (機構で直す、ADR-042 の方向):
  1. hooks-stop-quality.exe がステップ実行前に作業ディレクトリをプロジェクトルートへ
     正規化する。ルートの導出は (a) hook 起動時に Claude Code が渡す
     `CLAUDE_PROJECT_DIR` env、または (b) 自 exe パス (`<root>/.claude/*.exe`) の
     親の親、のいずれか。実装時にどちらが確実か確認して選ぶ。
  2. 文字化け対策: 子プロセス出力が UTF-8 として不正な場合に CP932 として
     デコードするフォールバックを lib-subprocess (または hook 側) に追加。
     既存の encoding 処理の有無を先に確認すること。
- **テスト**: ADR-049 流の incident 再現テスト —「リポジトリルート以外の cwd から
  hooks-stop-quality を起動しても file-length step が成功する」。
- **リスク**: 低〜中。cwd 正規化が takt subsession 判定
  (`main.rs:306` の `current_dir()` 使用箇所) に影響しないか確認が必要。
  正規化は「ステップ実行の子プロセス」にのみ適用し、判定ロジックは元 cwd を使う形が安全。
- **実施結果 (2026-07-17, 実装済み / PR #284)**:
  - **ルート導出は (b) 自 exe パス (ユーザー承認不要 = 実測で確定)**。方針が (a)
    `CLAUDE_PROJECT_DIR` env と両論併記で「実装時にどちらが確実か確認して選ぶ」と
    していたため実測した: VSCode 拡張環境 (Claude Code 2.1.212) で
    **`CLAUDE_PROJECT_DIR` は空**であり、ADR-005 が 2026-03-17 に記録した不安定性が
    現在も再現する。(a) は死んでいる。(b) は `config_path()` /
    `pipeline_lock::exe_claude_dir()` / `lib_telemetry::exe_dir()` が既に採る規約
    (順位 287 / ADR-010) と同形。判断根拠は **ADR-005 に追記**して恒久化した
    (本計画は削除予定のファイルなので、残すべき知見は ADR 側に置く)。
  - **⚠ 方針の前提が 1 つ誤っていた — takt subsession 判定も同じ原因で壊れていた**:
    リスク欄は「正規化が takt 判定に**影響しないか**確認が必要」「判定ロジックは元 cwd を
    使う形が安全」としていたが、調査の結果 **`takt_subsession_active` は cwd 依存で
    既に壊れていた**。cwd = `.takt/runs` のとき `<cwd>/.takt/runs`
    (= `.takt/runs/.takt/runs`) を探して空振りし、active run を検出できない。つまり
    ADR-004 § takt subsession skip が効かず、**edit: false の subsession に「直せ」指示を
    返す**事故 (PR #221 で実観測済みのもの) が起こり得る状態だった。元 cwd を維持するのは
    「安全」ではなく既知の不具合の温存にあたる。回帰テストで**修正前に実際に失敗すること**を
    確認済み (推測ではない、下記)。
  - **採用した方針 (ユーザー承認済み)**: **`main` 冒頭で 1 度だけ正規化**し、
    takt 判定・step 実行の**両方**に効かせる。両者は同一ファイル・同一根本原因の 2 症状で、
    片方だけ直すと壊れた検出が残る。以降のコードは「cwd = プロジェクトルート」を前提にできる。
  - **実装** (`src/hooks-stop-quality/`、runtime の変更は 1 exe に閉じる):
    - `project_root_from_exe(exe)` — `<root>/.claude/<hook>.exe` の規約 (ADR-010) を
      満たすときのみ `Some(root)`。親ディレクトリ名が `.claude` でなければ `None` を返し
      **正規化しない**。cwd 書き換えは後続の全 step の実行位置を変える操作なので、
      `cargo test` / `cargo run` 直下の `target/debug/` を推測で「ルート」扱いするより
      継承 cwd のまま (= 従来挙動) に倒す。
    - `normalize_cwd_to_project_root()` — ルート特定不能 / `set_current_dir` 失敗は
      **警告のみで継続** (fail-open)。Stop 時点のゲートは助言層で本物のゲートは push
      pipeline 側の quality_gate にある、という既存の線引き (`pipeline_is_running` の doc、
      ADR-043) に揃えた。
    - **lib-subprocess は変更なし**: cwd をプロセス単位で正規化するため、`cmd /c` の子は
      正規化後の cwd を継承する。全 pipeline 共有の `run_cmd_shell_*` に cwd 引数を
      足す案 (variant 増殖) を採らずに済んだ。
  - **ファイル分割 (T7 に付随して発生、T1 と同型)**: `main.rs` が 800 行上限に触れた
    (712 行 → 追記で 804 行)。file-length linter は touch-trigger ratchet のため、
    責務が最も独立していてテスト量が最大の takt 判定を `takt_subsession.rs` へ切り出した
    (main 532 行 / takt_subsession 290 行)。**T7 自身が直している file-length gate に
    T7 が引っ掛かった**形で、gate が機能していることの副次的実証でもある。
  - **回帰テスト (ADR-049 の流儀)**: `tests/t7_cwd_independence.rs` に E2E 5 本 +
    `project_root_from_exe` の unit 2 本 (hooks-stop-quality 全体 26 → **33 passed**)。
    由来 incident と再現状態を module doc に明記した。
    - **exe を `<root>/.claude/` に staging して spawn する**のが要点。`target/debug/` の
      exe を直接起動すると **exe-relative のルート導出を素通りしてしまい**、
      テストが実配置を検証しない。ADR-010 の実レイアウトを temp に組んで走らせる。
    - bad = 「cwd = `.takt/runs` でルート相対 step が解決できること」(症状 1)、
      「cwd = `.takt/runs` で active takt run を検知して skip すること」(症状 2)。
      good = 「cwd = root の正常経路が壊れていないこと」「**実失敗する step は cwd に
      依らず block すること**」(= 正規化がゲートを骨抜きにした、という最悪の退行ガード)、
      「completed run では skip しないこと」(過剰 skip ガード)。
    - **修正前の挙動に対して失敗することを確認済み**: `normalize_cwd_to_project_root()` の
      呼び出しを外すと **bad 2 本がちょうど失敗し good 3 本は通る**。失敗内容は
      incident の逐語再現 (`**file-length** failed:` + CP932 の文字化け) だった。
      good が before/after 両方で通ることも合わせて、テストが素通りしない証跡になる。
  - **実機検証 (before/after、本リポジトリの実 config で実施)**: サンドボックスではなく
    **本リポジトリの `.claude/hooks-config.toml` そのもの**で、cwd = `.takt/runs` から
    配布 exe を起動して比較した (回帰テストが temp fixture なので、実 config での確認を別途行う)。

    | | before (現行配布 exe) | after (修正後) |
    |---|---|---|
    | cwd = `.takt/runs` | `{"decision":"block", ... **file-length** failed:` + 文字化け | **出力なし (= block せず通過)** |
    | cwd = repo root | 通過 | 通過 (維持) |
    | cwd = `src/lib-subprocess/src` | (同型で失敗するはず) | 通過 |

    **方針の記述「pnpm 系ステップは pnpm が package.json を上方探索するため偶然通る」も
    実機で裏付けられた**: before で失敗したのは `file-length` step **のみ**で、
    pnpm 系 5 step と `cargo clippy` は cwd = `.takt/runs` でも通っていた
    (cargo も Cargo.toml を上方探索する)。**ルート相対パスを書いた step だけが壊れる**
    という非対称が、症状を step ごとにまだらにして発見を遅らせていた。
  - **発見 (本タスク外)**: T7 は **cwd drift による silent 故障の 3 例目**で、
    先行 2 例は既に todo 化されている — 順位 281 (🚀 Tier 1「config 読み hook の
    `current_dir()` 解決を検出する lint rule」、PR #267 で jj-op-verify が同型の実装をして
    pre-push REJECT された実例) と順位 287 (💎 Tier 3「config 読み hook は exe-relative 解決必須」
    convention の明文化、281 と同一 PR bundle 推奨)。T7 は「config 解決」ではなく
    「**step 実行の cwd**」という別カテゴリなので既存 lint rule 案では捕捉できない。
    281 着手時に検出対象を「`hooks-*` の `current_dir()` 使用全般」へ広げるか検討する価値がある
    (§2 原則 4 に従い本 PR では触れない。281/287 は本計画のスコープ外 = docs/todo13.md 管理)。

### T8: 空 `@` 時の bookmark_check 誤誘導 (**再現確認済み** 2026-07-16)

- **現状** (当初はコード監査による推測。2026-07-16 に実機で再現し確定 — 下記「再現記録」):
  `advance_jj_bookmarks` は `@` が空なら bookmark を `@-` へ前進させる
  (`stages/push_jj_bookmark.rs:82-95` 付近) のに、`stages/bookmark_check.rs` (L44, L117-146 付近) は
  `jj bookmark list -r @` の厳密一致で検査するため、`jj new` 直後の正常な再 push 状態でも
  exit 7「bookmark を作成して再実行してください」で中断し、従うと bookmark を壊す方向に誘導する。
- **再現記録 (2026-07-16、T1 = PR #279 の dogfood push で実際に発火)**:
  「要再現確認」だった本タスクは **in the wild で再現した**。よって却下条件
  (「再現しなければ却下」) は解消し、実施対象として確定。

  再現状態 (T1 セッションで自然発生したもの。作為的に作った状態ではない):

  ```text
  @   zxxkpomz (empty) "WIP: next work"      ← 空の working copy
  @-  nvmysvqk perf/lint-screen-evals-opt-in ← bookmark はここ
  ```

  `pnpm push` の実際の出力 (抜粋):

  ```text
  [push-runner] [push] bookmark 'perf/lint-screen-evals-opt-in' を @- に自動更新
  [push-runner] [bookmark] ローカル bookmark (非 trunk) が見つかりません
  [push-runner]   push 不可: `jj git push` は bookmark が必要です。
    対処: `jj bookmark create <name> -r @` で bookmark を作成して再実行してください
  [push-runner] パイプライン中断: 非 trunk bookmark が見つかりません。
  [push-runner] stage=pre_checks elapsed=0.5s
  ```

  観測できた事実:
  1. **同一 run 内で 2 つの stage が矛盾している**: `advance_jj_bookmarks` は
     「bookmark を `@-` に自動更新」と報告済み (= `@` が空である前提を正しく扱っている)
     のに、直後の `bookmark_check` が「bookmark が見つからない」と報告する。
     矛盾する 2 行が連続して出るため、ログだけでも異常と判る。
  2. **誤誘導が確定**: 案内される `jj bookmark create <name> -r @` に従うと、
     **空の WIP コミットに bookmark が付く**。計画時の推測どおり「bookmark を壊す方向」。
  3. **exit code は 7** (計画の推測どおり)。stage は `pre_checks` で中断するため、
     quality_gate 以降は一切走らない。
  4. **回避策** (T1 セッションで実際に採った手段): 誤誘導に従わず、
     `jj edit @-` で `@` を bookmark のあるコミットへ移動 + 空 WIP コミットを abandon。
     これで `bookmark_check` を通過し push 成功。
  5. **前段の別症状**: bookmark が 1 つも無い状態でも同じ exit 7 + 同じ文面が出る
     (「ローカル bookmark が見つかりません (新規ブランチ等)」)。こちらは案内が正しい
     (実際に作成が必要) ため、**修正時に 2 ケースを取り違えないこと** —
     「bookmark が皆無」と「bookmark が `@-` にあるが `@` が空」を区別してメッセージを
     出し分ける必要がある。現状は両者が同じ文面に潰れており、これが誤誘導の実体。
- **方針** (再現済みのため却下判定は不要。当初の「再現しなければ却下」条件は解消):
  1. 再現テストを書く (`@` 空 + bookmark が `@-` にある状態)。上記「再現状態」がそのまま
     fixture の仕様になる。incident 由来なので ADR-049 の流儀に従う。
  2. 検査を `@` 空時は `@-` を対象にする (advance と同じ規則) よう揃える。
  3. メッセージを分岐させる: 「bookmark が `@-` にあるが `@` が空」= 正常な再 push 状態なので
     「push すべき新変更がない」旨に修正。「bookmark が皆無」= 既存の案内が正しいので維持
     (上記 5.)。**現状は両者が同じ文面に潰れており、これが誤誘導の実体**。
- **リスク**: 中。jj 変更検出は ADR-021 の設計原則に従うこと (revset 合成の流儀)。
- **実施結果 (2026-07-17, 実装済み / PR #280)**:
  - **方針 2・3 の矛盾を実施前に解消**: 着手時に方針 2 (「検査を `@` 空時は `@-` を対象にする」) と
    方針 3 (「`@` 空 + bookmark が `@-` は正常な再 push 状態なので "push すべき新変更がない" 旨に修正」)、
    および再現記録の事実 4 (T1 セッションは `jj edit @-` 後に **push 成功** = push すべき変更はあった)
    の 3 者が矛盾していることが判明した。さらに方針 2 を文字通り実装すると
    **AI レビューを無言でバイパスする**ことが判明 — `[diff] command = "jj diff -r @"`
    (`push-runner-config.toml`) のため `@` が空のまま続行すると diff が空になり、
    `main.rs` の「diff が空のためレビューをスキップして push に進みます」経路で
    takt が skip されたまま `@-` の変更が push される。誤誘導バグを
    レビューバイパスに置き換えることになるため、方針 2 の文字通りの実装は採らない。
  - **採用した方針 (ユーザー承認済み)**: **exit 7 による中断は維持し、案内文のみ正す**。
    `@` に bookmark が無い状態を 2 ケースに切り分け、T8 の状態には
    T1 セッションで実証済みの回避策 (`jj edit @-` + 空 WIP の abandon) を案内する。
    レビュー範囲は無傷、変更は bookmark_check に閉じる。
  - **実装**:
    - `push_jj_bookmark.rs` の `determine_target_revision()` から `working_copy_is_empty()` を
      切り出して `bookmark_check` と共有した。「`@` が空なら `@-`」の規則を
      advance と検査で**二重定義しない**ことが再発防止の核心 (矛盾の実体がこれだった)。
    - `bookmark_check.rs` に判定 enum `BookmarkCheckOutcome`
      (`Proceed` / `EmptyWorkingCopy` / `NoBookmarks`) と pure function
      `decide_bookmark_check(bookmarks_at_head, head_is_empty, parent_bookmarks_fn)` を追加。
      jj 呼び出しは closure 注入で外に出した (**ADR-021 原則 3** に準拠。既存の
      `dispatch_bookmark_advance` と同じ流儀で、本 repo には実 jj repo を張る
      test 前例が無いためこの形を踏襲)。
    - 判定順は **`@` の空判定を最優先**する。当初は「`@` に bookmark があれば従来どおり続行」を
      先に置き、bookmark が空の `@` にある既存経路を温存していたが、**PR #280 の CodeRabbit
      Major 指摘で反転**した: その経路は `jj diff -r @` が空になり、祖先の未 push 変更が
      AI レビューを経ずに push される (本タスクが方針 2 を却下した理由と同じ穴が、
      bookmark の位置違いで残っていた)。`advance_jj_bookmarks` は非 trunk bookmark が
      2 つ以上あると fallback 更新を skip するため、この状態は実在する。
      「レビュー範囲 = `@` だから `@` は非空でなければならない」という本タスクの不変条件に
      判定順を揃えた (§8 判定記録の post-PR 修正欄)。
    - `main.rs:76-79` にも同じ `jj bookmark create -r @` 案内が**重複**しており、
      これも誤誘導の出力元だったため撤去し、ケース別案内を出す bookmark_check に一本化した。
  - **ADR-021 原則 5 との関係**: 原則 5 の標準 (`@`/`@-`/`@--` の優先度付き revset) に対し、
    bookmark_check は PR #271 (CodeRabbit Major: 他 workspace の bookmark 混入) の対策として
    意図的に `@` 厳密一致へ狭めた経緯がある (`OWN_WORKSPACE_BOOKMARKS_REVSET` の doc)。
    本修正の `@-` 照会は**案内文の出し分け (診断) 専用**で、push 対象
    (`-b <name>` の組み立て) は `@` のままとし、PR #271 の対策を維持する。
  - **回帰テスト (ADR-049 の流儀)**: `mod t8_empty_head_misdirection` に 12 本追加
    (cli-push-runner 全体 186 → 198 passed。初版 7 本 + post-PR 修正で 5 本)。
    由来 incident (PR #279 の dogfood push) と
    再現状態を module doc に明記し、追跡鎖 incident → 修正 → test を残した。
    bad = 「`@` 空 + bookmark が `@-`」が `NoBookmarks` に潰れないこと、
    good = 「bookmark 皆無かつ `@` が非空」が `NoBookmarks` のままであること
    (**記録 5. の取り違え防止をテストで固定**)、および `@` に bookmark がある場合に
    `@-` を照会しないこと (panic で固定)。
  - **サンドボックス実機検証 (before/after)**: 記録と同型の状態
    (`@` = 空 WIP / `@-` = `perf/lint-screen-evals-opt-in`) を張った jj repo で
    配布 exe と修正後 exe を実行して比較した。

    | | before (現行配布 exe) | after (修正後) |
    |---|---|---|
    | bookmark 所在の報告 | 「ローカル bookmark (非 trunk) が見つかりません」(矛盾) | 「`@` が空です (bookmark は @- にあります: perf/...)」 |
    | 案内 | `jj bookmark create <name> -r @` (**空コミットに bookmark = 破壊的**) | `jj edit @-` + 空 WIP の `jj abandon` |
    | exit code | 7 | 7 (維持) |

    before が記録の出力を**逐語で再現**することを確認した上で修正を当てている。
    あわせて (a) 案内どおり `jj edit @-` + abandon した後の再実行で bookmark_check を
    通過し scratch/quality_gate へ進むこと、(b)「`@` 非空 + bookmark 皆無」では
    従来の作成案内が**そのまま出る**ことも実機で確認した。
    Windows 注意: jj の index segment 名が長く `MAX_PATH` に掛かるため、
    サンドボックスは深い scratchpad 配下ではなく短いパスに作る必要がある。

### T3: `pnpm build` の形骸化

- **現状**: `package.json:11` の `"build": "npx tsc --noEmit --pretty || true"`。
  **typescript が devDependencies に無いため `npx tsc` は常に失敗**し、`|| true` で
  握りつぶされる。つまり型チェックは一度も機能しておらず、quality_gate と Stop hook の
  build step は時間だけ消費する見せかけゲート (2026-07-16 に `npx tsc` 単体実行で確認済み)。
- **方針**: どちらかを選ぶ。
  - (a) typescript を devDependency に追加し `|| true` を外して実体化する。
    既存 ts (src/logger.ts, src/sample.ts) が型エラーなら先に修正。
  - (b) TS 資産が実質サンプルのみと判断するなら、quality_gate group と
    stop_quality step から build を削除する。
  推奨は (a)。ADR-043 (fail-closed) に整合するのは実体化の方向。
- **リスク**: 低。(a) の場合 tsconfig.json の有無・内容を確認。

## 5. 改善タスク詳細

### T0: stage 別計測ログ + before 記録

- **方針**: cli-push-runner の各 stage (pre-checks / quality_gate / diff / takt / push) に
  経過秒のログ行を追加する (`log_info` に統一書式、例: `stage=quality_gate elapsed=312s`)。
  既存の「パイプライン完了 (Xs)」に加える形。§1 のベースライン表を before 値として使う。
- **受け入れ基準**: `pnpm push` 1 回で全 stage の所要秒が確認できる。
- **実施結果 (2026-07-16, 実装済み)**:
  - `log.rs` に `timed()` を追加し、`main.rs` の 5 stage (pre_checks / quality_gate /
    diff / takt / push) を包んだ。書式は `stage=<name> elapsed=<秒>s` (小数第 1 位)。
    小数を残すのは「一瞬で終わった stage」と「未計測」をログ上で区別するため。
  - 記録は stage の成否を見ずに行う。中断で終わった run でも、その stage に
    かかった時間が after 計測に残る。
  - 空 diff で takt を skip した run では `stage=takt` 行が出ない。skip 自体は
    既存の「diff が空のため…」行で判別できるため、skip 用の行は追加していない。
  - before 値は §1 のベースライン表をそのまま使う (再計測はしない)。
  - 検証: サンドボックス jj リポジトリで配布 exe を 2 経路 (空 diff → push /
    diff あり → takt 失敗) 実行し、5 stage 全ての行が出ることを確認済み。
  - **stage log 初回実測 (PR #278 = 本タスク自身の dogfood push、コード変更あり・fix なし)**:

    | stage | 実測 |
    |---|---|
    | pre_checks | 1.2s |
    | quality_gate | 93.9s |
    | diff | 0.1s |
    | takt | 149.4s (simplicity / security とも APPROVE、fix iteration 0) |
    | push | 2.5s |
    | 合計 | 247s |

    この 1 run は「fix なし run」に該当し、§1 の該当帯 (1.1〜3.8 分) の上端付近。
    ただし **quality_gate 93.9s は §1 が T1 の根拠に引く 269s と大きく乖離する** (下記 T1 参照)。

### T1: Ollama eval を quality_gate から除外 (最優先)

- **現状**: `push-runner-config.toml` の rust-lint-test group 3 本目
  `cargo test -- --ignored --test-threads=1` が、
  `src/cli-finding-classifier/tests/lint_screen_evals.rs` の
  `run_lint_screen_against_all_fixtures` (L746-769) を巻き込む
  (**パスと行番号は T1 着手前の記述。T1 で当該ファイルは
  `tests/lint_screen_evals/{main.rs,e2e.rs}` に分割済みで、この関数は現在
  `e2e.rs` にある** — 下記「実施結果」参照)。このテストは
  `#[ignore]` 付き・**assert ゼロ** (`report_summary` は println のみ)・mistral:7b を
  15 fixture 分実呼出する計測専用テストで、doc コメント自体が名前フィルタ付き
  手動起動を想定している。269s の実測記録あり (GPU 更新後は短縮の可能性)。
  さらに takt fix step の `--ignored` 義務 (`.takt/facets/instructions/fix.md`) でも
  再実行され、1 push で 2 回走り得る。なお lint_screen 機能自体は
  `push-runner-config.toml` で `enabled = false`。
- **方針**: テスト側に env opt-in ガードを入れる (呼出箇所が gate / fix / 手動と
  複数あるため、コマンド側でなくテスト側で塞ぐのが漏れがない):
  冒頭で `LINT_SCREEN_EVALS` が truthy でなければ skip メッセージを出して return。
  `OllamaClient` を実呼出する他の `#[ignore]` テストが無いか
  cli-finding-classifier / lib-ollama-client を grep し、あれば同じガードを適用。
  手動起動手順 (env 設定込み) を該当テストの doc コメントと ADR-038 に追記。
- **付随**: 除外後に `--ignored` スイートの実時間を再計測し、
  `step_timeout = 600` (`push-runner-config.toml`) を実測+マージンに right-size する
  (コメントの履歴欄に経緯を追記)。
- **受け入れ基準**: Ollama 停止状態で `pnpm push` の quality_gate が通る。
  gate の `--ignored` 実行時間が計測で大幅減 (目安 269s → 90s 未満)。
- **ADR**: ADR-038 に「eval は env opt-in の手動実行に変更」を追記。
- **⚠ 着手前に検証すること (T0 セッションからの申し送り、2026-07-16)**:
  1. **期待効果の前提が崩れている可能性がある**。T0 の dogfood push (PR #278) で
     計測した **quality_gate 全体が 93.9s** で、本タスクが根拠に引く 269s
     (`push-runner-config.toml` の `step_timeout` コメントの実測記録) の約 1/3 だった。
     quality_gate は rust-lint-test group (clippy + `cargo test` + `--ignored`) を含み、
     他 group と並列実行される 93.9s なので、`--ignored` 単体はさらに短いはず。
     つまり **`run_lint_screen_against_all_fixtures` は既に 269s も掛かっていない**公算が高く、
     本タスクの期待効果「-2〜4.5 分/push」および受け入れ基準「269s → 90s 未満」は
     そのままでは使えない。
  2. **想定原因**: ローカル LLM 環境が ADR-040 記録時 (RTX 3070 8GB) から
     **RTX PRO 5000 48GB** に更新済み。mistral:7b の推論が当時より大幅に速い可能性がある。
     ADR-040 の resource 数値は stale なので、その前提で書かれた見積りは疑うこと。
  3. **最初にやること** (方針を決める前に実測する):

     ```sh
     # (a) --ignored スイート全体
     cargo test --workspace -- --ignored --test-threads=1
     # (b) eval テスト単体の寄与 (これが除外対象)
     cargo test -p cli-finding-classifier run_lint_screen_against_all_fixtures -- --ignored --exact
     ```

     (b) が (a) の大半を占めるなら本タスクの前提は生きている。占めないなら
     **期待効果を実測値で書き直してから**着手するか、優先度を下げて T10/T11
     (execute 短縮の本丸) を先に回す判断もあり得る。判断根拠は §8 判定記録に残すこと。
  4. Ollama が停止中だと (b) は失敗/長時間化する可能性がある。受け入れ基準の
     「Ollama 停止状態で gate が通る」は本タスクの成果物なので、着手前の計測時は
     Ollama を起動した状態で測る (= 現状の実力値を取る)。
- **実施結果 (2026-07-16, 実装済み / PR #279)**:
  - **着手前の実測** (申し送りに従い Ollama 起動状態で計測):

    | 対象 | 実測 |
    |---|---|
    | (a) `cargo test --workspace -- --ignored --test-threads=1` | 63s |
    | (b) `run_lint_screen_against_all_fixtures` 単体 | 41.3s (= (a) の 65%) |

    (b) が (a) の大半を占めるため、申し送り 3. の判定基準「前提は生きている」に該当し着手した。
    ただし **絶対値は根拠の 269s に対し約 1/4** で、想定原因 2. (GPU 更新) が裏付けられた。
    期待効果は **-2〜4.5 分/push → -42s/push** に下方修正 (§3 表も修正済み)。
    fix 発生 run では `fix.md` の `--ignored` 義務でも走るため、その分の削減も乗る。
  - **実装**: `LINT_SCREEN_EVALS` が truthy (`1`/`true`/`yes`/`on`、trim + 大小無視 =
    push-runner の `parse_override_env` に語彙を合わせた) でなければ skip して return。
    コマンド側でなくテスト側で塞いだのは申し送りの方針通り (呼出箇所が gate / fix / 手動と複数)。
    `OllamaClient` を実呼出する `#[ignore]` テストは他に無いことを grep で確認済み
    (cli-finding-classifier / lib-ollama-client)。
  - **after 実測**: `--ignored` スイート全体 **63s → 21s** (-42s)。eval 単体は 41.3s → 0s (skip)。
    opt-in 経路 (`LINT_SCREEN_EVALS=1`) は 15 fixture が正常実行され agreement 86.7% (GO) を確認。
  - **step_timeout の right-size**: 600 → **300**。実測してから縮小する方針を採り、
    `cargo clean` 後に gate の全コマンドを計測した (32 core / target cold・registry warm):
    最遅は `cargo test` の **28s** (clippy 8s / `--ignored` 19s / pnpm 系は全て 1-2s)。
    `step_timeout` は group 単位でなく **コマンド単位**の適用 (`stages/quality_gate.rs` の
    `run_group`) なので、最遅 1 コマンドが下限を決める。28s に約 10 倍のマージンを取った。
    経緯は `push-runner-config.toml` のコメント履歴に記載。
  - **ファイル分割 (T1 に付随して発生)**: `tests/lint_screen_evals.rs` が変更前から 799 行
    (上限 800) で、ガード追加分が入らなかった。file-length linter は touch-trigger ratchet
    のため、`tests/lint_screen_evals/{main.rs,e2e.rs}` に分割した (main = schema/metrics の
    常時実行テスト 608 行 / e2e = env ガード + 実 Ollama 呼出 + レポート)。
    Cargo が `tests/<name>/main.rs` を test target として自動認識するため、
    target 名 `lint_screen_evals` と既存の起動コマンドは不変。
  - **受け入れ基準の達成状況**:
    - 「Ollama 停止状態で gate が通る」: **達成 (ただし before から満たされていた)**。
      `screen_diff` は Ollama 不達時に fallback するため panic せず、かつ assert ゼロなので
      元から pass していた。T1 の実質的な成果は時間削減と、gate から
      「何も検証しないのに実 LLM を呼ぶテスト」を外した設計面の整理。
    - 「gate の `--ignored` が大幅減 (目安 269s → 90s 未満)」: 前提が stale だったため
      **実測値ベースで読み替えて達成** (63s → 21s)。

### T4: refute facet の dogfood 開始

- **現状**: ADR-047 の反証 (refute) facet は実装済みだが
  `push-runner-config.toml` の `[pre_push_review] refute_enabled = false` のまま未運用。
  reviewer (sonnet) の false positive finding を fix 前に haiku で却下し、
  無駄な fix iteration (5〜8 分) を削る仕組み。誤 reject は post-PR CodeRabbit 層で
  回収される安全網構造が前提。
- **方針**: `refute_enabled = true` に変更するのみ。ADR-047 の bounded lifetime
  (2 週間 dogfood → 採否判定を ADR-047 に記録) はこの計画とは独立に進行させる。
  本計画上は「有効化 + 初回 push で verify step が動くことの確認」で完了とする。
- **リスク**: findings 発生時に haiku verify ~1 分が追加されるが、fix iteration 削減が上回る想定。
- **実施結果 (2026-07-17, 実装済み)**:
  - **変更は方針どおり 1 行**: `push-runner-config.toml` の
    `[pre_push_review] refute_enabled = false → true`。`templates/push-runner-config.toml` は
    `false` のまま据え置き (派生プロジェクトは現行 `pre-push-review` を継承。dogfood は
    本リポジトリに閉じる = ADR-047 の config opt-in 設計どおり)。
  - **dogfood 開始日を同じ PR で固定した**: ADR-039 の bounded lifetime は「有効化から
    2 週間」を起点に持つため、起点日が記録されないと期限が判定不能になる。
    **開始 2026-07-17 → 判定期限 2026-07-31** を ADR-047 (ステータス行 / Config opt-in /
    Bounded lifetime の 3 箇所) と `push-runner-config.toml` の `[pre_push_review]`
    コメントに明記した。あわせて ADR-047 の「本 PR (導入 PR) は OFF とする」という
    導入 PR 時点の記述を、dogfood 開始済みの現状に合わせて過去形へ更新した。
  - **有効化前に確認したこと** (dogfood のブートストラップ注意 = §2 原則 4 の適用。
    有効化した瞬間から PR #281 自身の push が refute workflow を通るため、事前に静的確認した):
    - refute 側の資産が揃っている: `.takt/workflows/pre-push-review-refute.yaml` /
      `.takt/facets/instructions/refute-finding.md` /
      `.takt/facets/output-contracts/refutation-report.md`。
    - workflow 切替ロジックの unit test 4 本が通る
      (`cargo test -p cli-push-runner resolve_workflow`)。切替は
      `config/mod.rs` の `resolve_takt_workflow` に単一集約済み。
    - **exe 再ビルド不要** (§2 原則 5 の例外): Rust 変更ゼロで、config は実行時に
      cwd の `push-runner-config.toml` から読まれる (`config_path()`)。
    - 同じ config を読む他 exe への波及なし: `cli-pr-monitor` の
      `stages/gate.rs` は `[quality_gate]` のみ参照し `[pre_push_review]` を見ない。
  - **初回 dogfood push の実測 (PR #281 自身の push、2026-07-17)**:

    | stage | 実測 |
    |---|---|
    | pre_checks | 1.3s |
    | quality_gate | 49.7s (最遅 group = rust-lint-test 49.7s) |
    | diff | 0.1s |
    | takt | 97.8s (`pre-push-review-refute`、1 iteration、reviewers 2 本とも APPROVE) |
    | push | 2.2s |
    | 合計 | 151s |

    - **切替は確認できた**: 起動ログが
      `パイプライン開始: ... takt (pre-push-review-refute) → push`
      (`main.rs` が `resolve_takt_workflow` の結果を出力) を出し、takt も
      `ワークフロー 'pre-push-review-refute' を起動` → `Workflow completed` で完走した。
      **有効化は効いている**。
    - **verify は予告どおり発火しなかった**: simplicity / security とも APPROVE で
      `all("approved") → COMPLETE` に抜けたため、`any("needs_fix") → verify` の経路に
      入らなかった。よって **verify 実動の観測は次に findings が出る run に持ち越す**
      (完了条件の「verify step が動くことの確認」は「有効化が効いていることの確認」までを
      PR #281 の成果として読む。ユーザー承認済みの範囲)。
    - 参考: T0 の PR #278 (コード変更・fix なし) は quality_gate 93.9s / takt 149.4s /
      合計 247s。本 run は docs+config の小 diff かつ T1 適用後のため単純比較はできないが、
      同じ「fix なし run」帯に収まっている。
  - **⚠ 計測手順の誤りを dogfood 初回で発見・修正した (T4 の副産物)**:
    ADR-047 の §dogfood 計測項目は refute run を
    `.takt/runs/*-pre-push-review-refute/trace.md` で辿ると書いていたが、
    **この glob は 1 件もマッチしない**。takt の run ディレクトリ名は
    **workflow 名ではなく task 名**から作られるため
    (`runSlug` = `<UTC timestamp>-pre-push-review`)、refute run でも
    ディレクトリ名は `20260716-182505-pre-push-review` になる (本 run で実測)。
    さらに timestamp は **UTC** なので JST 2026-07-17 の run が `20260716-*` になる。
    放置すると 2026-07-31 の採否判定で **run が 0 件ヒットし「データなし」と誤読する**恐れが
    あったため、ADR-047 と本節の計測手順を `meta.json` の `piece` フィールド基準に修正した:

    ```sh
    grep -l '"piece": "pre-push-review-refute"' .takt/runs/*/meta.json
    ```

    (`trace.md` 冒頭の `# Execution Trace: pre-push-review-refute` でも同定可能。
    非 refute run の `piece` は `pre-push-review` で、両者はこのフィールドで判別できる。)
    設計時に書いた計測手順が実運用開始まで検証されていなかった例であり、
    **dogfood 開始 PR で計測手順まで実地確認する**価値がここに出た。
  - **dogfood 中の計測項目** (ADR-047 §dogfood 計測項目、判定期限 2026-07-31 まで):
    上記 `piece` 基準で特定した run の fix iteration 数 (`trace.md` の reviewers↔fix cycle 数) と、
    reject 率 / reject 誤り率 (`refutation-report.md` の rejected findings を後続 PR の
    CodeRabbit 再指摘と照合 = 安全網の実証)。

### T2: 旧 cli-push-pipeline の workspace 除去

- **現状**: ADR-015 で置換済みの旧実装 `src/cli-push-pipeline/` が
  `Cargo.toml:27` の workspace member に残存し、毎 push の clippy / test / `--ignored` の
  ビルド・実行対象になっている。pnpm scripts / `build:all` からは未参照。
- **方針**: crate ディレクトリごと削除し、workspace members からも除去する
  (dead code を残さない。履歴は git にある)。ADR-015 に削除完了の追記。
  削除 PR は diff 行数が block 閾値を超えるため `PR_SIZE_CHECK_OVERRIDE=1` を使い、
  PR 本文に理由を明記する。
- **受け入れ基準**: `cargo clippy --workspace` / `cargo test` が通り、
  ビルド対象 crate 数が 1 減っている。他 crate から参照が無いことを事前 grep で確認。

### T10: takt builtin review policy の shadow (execute 短縮の本丸)

- **現状**: takt builtin の 8KB `policy: review`
  (`node_modules/takt/builtins/en/facets/policies/review.md`) が pre-push の全 reviewer に
  注入されている。内容は「DRY / TODO / テスト無し新規挙動 等は無条件 REJECT」
  「Boy Scout」「全件 Fact-check」というチェックリスト型で、ADR-027/036 が確立した
  anomaly-only 設計と矛盾する。実害の実例: run `20260715-185649` の simplicity REJECT は
  この builtin の「DRY 違反 = 無条件 REJECT」を直接根拠にしており、~7 分の fix iteration を
  誘発した。docs-only の 9 行差分でも execute 95s を要した一因。
  facet 解決順はプロジェクト `.takt/facets/{kind}/` → `~/.takt` → builtin のため、
  プロジェクト側ファイルで shadow 可能 (ADR-048 が output-contracts で実証済みの機構)。
- **方針**:
  1. 新名称 policy (例 `review-anomaly`) を `.takt/facets/policies/` に作成し、
     `pre-push-review.yaml` の reviewer 2 step の `policy: review` を差し替える
     (blast radius を pre-push に限定。post-pr-review / weekly-review は当面現状維持)。
  2. 内容: 「事実確認・file:line 特定・実装可能な修正提案」の原則は維持しつつ、
     無条件 REJECT チェックリストと Boy Scout 強制を撤去し、REJECT 判断を
     instruction 側 (review-simplicity.md / review-security.md) の anomaly 基準に委譲する。
     Scope Determination (diff 起因 = blocking / 既存問題 = non-blocking) は簡約して残す。
  3. あわせて `review-simplicity.md` の lint-screen 参照セクション
     (`.takt/facets/instructions/review-simplicity.md` 冒頭付近) を削除する
     — `[lint_screen] enabled = false` なので恒常デッドウェイト。
  4. security 側の builtin persona / knowledge の slim 化は効果測定後の
     フォローアップとする (wall-clock 律速でないため優先度低)。
- **受け入れ基準**: 変更後 5 run 程度で simplicity execute の平均が短縮
  (目安 203s → 150s 以下) し、checklist 型 REJECT (anomaly 基準に該当しない
  DRY/TODO 単独指摘) が発生しない。
- **ADR**: 新規 ADR (試験運用) として「policy 層の anomaly 設計整合」を記録。
  ADR-036 との整合を本文で参照。takt は 0.35.3 pin (ADR-017) のため
  builtin 側の将来変更には影響されない。

### T11: docs-only / 空 diff の決定論 routing

- **現状**: 空 diff の takt skip は `main.rs` (`run_diff_and_lint_screen` の戻り値) に
  実装済み。しかし判定が quality_gate の後にあり、docs-only 判定は pre-push に存在しない。
  docs-only push でも rust-lint-test (数分) + takt (~2 分) を毎回払っている。
  post-PR 側には `is_docs_only_summary` の先行実装がある
  (`src/cli-pr-monitor/src/stages/gate.rs` L89-127 付近)。
- **方針**:
  1. stage 3 (pr_size_check) で取得済みの `jj diff --stat` (または `jj diff --summary`) から
     変更ファイル一覧を quality_gate 前に判定。
  2. **docs-only 判定は ADR-035 の path 基準に厳密準拠**: `docs/**` 等のみで構成され、
     除外パス (`.takt/facets/instructions/**`, `.claude/**`, `.takt/workflows/**` 等
     code-equivalent なもの) を 1 つも含まない場合に限り docs-only とする。
  3. docs-only の場合: rust-lint-test group を skip (JS 系 lint/test は軽いので実行維持)、
     takt は skip または将来の軽量 workflow へ (MVP は skip で可 — post-PR の CodeRabbit
     層が残るため安全網は維持される)。
  4. **fail-closed (ADR-043)**: 判定不能・パース失敗時はフル実行に倒す。
  5. **ADR-039 3 点セット**: `push-runner-config.toml` に config opt-in section
     (default OFF、本リポジトリで enabled = true で dogfood)、env kill-switch、
     bounded lifetime (3-5 PR で誤 skip が無いか判定) を備える。
- **受け入れ基準**: docs-only PR の `pnpm push` が 1 分台で完走し、
  除外パスを含む diff ではフルパイプラインが走る (両方テストで固定)。
- **ADR**: 新規 ADR (試験運用)。ADR-035 の「instruction 規約 → 決定論機構への昇格」
  (ADR-042 の方向) として位置づける。

### T12: fix 後の決定論再ゲート + fix step 検証義務の縮小

- **現状**: `main.rs` の `run_pipeline` は quality_gate → takt → push の順で、
  **takt の fix がコードを書き換えた後に決定論検証が無い**。fix の検証は
  `.takt/facets/instructions/fix.md` が fix agent に義務付ける
  `cargo build -p` → `cargo build --workspace` → `cargo test -p` → `cargo test --workspace` →
  `cargo test -- --ignored --test-threads=1` の自己申告のみ (run `20260715-185649` で
  上位集合を含む 5 連直列実行を確認 = fix execute 平均 296s の主因)。
  同型の穴は post-PR 経路で PR #224 の実害後に決定論 gate
  (`cli-pr-monitor/src/stages/gate.rs`) で塞がれたが、pre-push 経路は未対応。
- **方針**:
  1. push-runner に Stage「post-takt re-gate」を追加: takt 実行後、作業コピーが
     takt 起動前と変化した場合のみ quality_gate を再実行する
     (変化検出は diff 取得時に記録した snapshot / `jj diff` の比較。ADR-021 の原則に従う)。
     T1/T11 適用後の gate は数十秒〜2 分程度なので再実行コストは許容範囲。
  2. `fix.md` の検証義務を縮小: 「影響 crate の `cargo build -p` + `cargo test -p` のみ実行、
     workspace 全体と `--ignored` は post-takt re-gate に委譲」に書き換える。
     `--ignored` 統合テスト gate の記述 (PR #224 由来) は「re-gate が `--ignored` を
     含むため fix step では不要」と根拠ごと更新する。
  3. **注意**: `fix.md` は post-pr 経路と共有 (ADR-020)。post-pr 側は既に決定論 gate が
     あるため縮小して問題ないが、変更時に post-pr の gate が `--ignored` を含むかを
     確認し、含まないなら含める。
- **受け入れ基準**: fix 発生 run で fix execute が短縮 (目安 296s → 150s 以下) し、
  re-gate が fix の破壊的変更 (故意にテストを壊す fixture) を検出して push を block する
  統合テストが通る。
- **ADR**: ADR-037 (fix-trust shortcut) に「honesty constraint の機械的 backstop を
  pre-push 経路にも拡張」を追記。ADR-043 整合。

## 6. T13: backlog (任意、各 XS)

効果が小さい・優先度が低いもの。実施しない場合は docs/todo.md へ順位付きで移すか、
却下理由を §8 の判定記録に残す。

1. quality_gate 失敗時の出力 truncate 改善: `run_cmd_shell_capped_reporting`
   (truncate 明示 variant) + cap 引き上げで cargo test の失敗一覧が消えないようにする。
2. gate グループ失敗時の early-abort: いずれかのグループが失敗したら他グループを
   打ち切り即失敗表示 (`stages/quality_gate.rs` の join 待ち改善)。
3. `pre-push-review.yaml` の loop_monitor `judge.model: sonnet` → `haiku`
   (2 択判定のみ。post-pr-review.yaml に haiku 前例あり)。
4. `fix.md` の過去レポート参照 (Glob + 2 ファイル読み) を Step Iteration 1 では skip する追記。
5. pr-monitor の gh 直列 4-5 呼び出し削減 (初回 push では PR 不在が自明のケース)。
6. `advance_jj_bookmarks` の二重実行 (stage 1 と stage 8) の統合検討。
7. 同一 checkout での `pnpm push` 並走ガード (pipeline lock は advisory のまま、
   push 同士のみ相互排他にするか検討。ADR-025/ADR-045 との整合を確認)。
8. ~~`push_was_refused` の `contains` 誤爆厳格化 (T5 に含めなかった場合)。~~
   **却下 (2026-07-17, T5 で判定)**: リスクが非対称なため厳格化しない。誤検知は出力表示で
   気付いて再実行できるが、検知漏れは「リモート未反映のまま exit 0」= T5 が防ぐ事故そのもの。
   ADR-043 (fail-closed) に従い `contains` を維持する。判断根拠は `push_was_refused` の
   doc コメントに恒久化済み (§4 T5 実施結果)。
9. `cli-pr-monitor` の `push_to_remote` (`src/cli-pr-monitor/src/stages/push.rs`) に
   拒否検知を追加する (T5 の調査で発見、2026-07-17)。jj は新規 bookmark 拒否時に exit 0 を
   返すが、同関数は exit code のみを見ているため post-PR の re-push が無言で失敗し得る。
   出力取得は `run_cmd_direct` (unlimited) なので**判定の追加だけ**で済む
   (T5 と違い truncate 問題は無い)。規模 XS。
10. `lib-subprocess` の `run_cmd_shell_*` 3 variant で **timeout が wall-clock を縛れない**
    (T6 の実装中に発見、2026-07-17)。`run_cmd_shell_with` は timeout 検知後に reader thread を
    join するが、`cmd /c <command>` の孫プロセス (実際の `cargo` / `jj`) は `child.kill()` の
    対象外で pipe の書き込み端を保持し続けるため EOF が来ず、join が**孫の自然終了まで
    ブロック**する。実測: `run_cmd_shell_capped` に `timeout_secs = 1` を指定したテストが
    返るまで 9.23s (`ping -n 10` の自然終了待ち)。既存テストは経過時間を assert しないため
    素通りしている。影響: quality_gate (`step_timeout = 300`) と push (`timeout = 300`)、
    cli-merge-pipeline — ハングした `cargo test` / `jj git push` を timeout で打ち切れない。
    対処案は T6 と同じ「失敗経路では join せず detach」だが、`_capped` 系は表示用出力を
    捨てることになるためトレードオフの判断が要る (T6 の diff は timeout 時に出力不要だった)。
    孫まで殺す (`taskkill /T`) 案もある。規模 S。**テストには経過時間 assert を必ず入れること**
    (無いと本件は再び素通りする)。
11. 子プロセス出力の **CP932 デコードフォールバック** (T7 の方針 2 から分離、2026-07-17。
    **ユーザー承認済みの分離**)。`lib-subprocess` の `drain_pipe_*` は `from_utf8_lossy` 固定で、
    **repo 全体に encoding 処理が存在しない** (grep 済み) ため、cmd.exe が返す日本語エラーが
    文字化けする (CP932 の各バイトが U+FFFD replacement character に潰れる)。
    T7 の incident では `指定されたパスが見つかりません。` が判読不能な状態で表示されていた。
    T7 の cwd 修正でこの**特定の**エラーは出なくなったが、**経路自体は残る** — 例えば
    `pnpm build:all` 未実行で step の exe が欠落する場合 (ADR-005 Negative に既知として記載、
    クローン直後・派生プロジェクトで現実的) は cwd と無関係に同じ文字化けが出る。
    失敗時こそ診断情報を落とすべきでない (T5 §4 / backlog 1 と同じ理由) が、
    影響先が push-runner / merge-pipeline を含む共有 lib のため §2 原則 4 (1 PR 1 変更) に従い分離した。
    方針案: `drain_pipe_*` は行単位で読むので「UTF-8 として不正な行のみ CP932 で再デコード」の
    フォールバックが素直 (正常な UTF-8 出力 = cargo/pnpm は不変)。`encoding_rs` 依存の追加要否を
    判断すること。規模 S。

## 7. スコープ外 (本計画では実施しない)

- **takt 離脱 (Rust 直オーケストレーション + `claude -p` 直呼び)**: 本計画完了後、
  ADR-055 telemetry と `check-ci-coderabbit --list-findings` による
  「pre-push APPROVE 後に CodeRabbit が何を出したか」の突合データを取ってから判断する。
- **pre-push AI レビューの廃止 (CodeRabbit 全面依存)**: CodeRabbit rate-limit
  (ADR-019 記録: 解除待ち 20-40 分が頻発) のため、push は速くなっても
  PR マージまでの総時間が悪化する公算が大きく非推奨と判断済み。
- **review+fix の単一エージェント統合**: ADR-036 が特定した self-review 盲点
  (6-iter アウトライアの根因) を再導入するため非推奨と判断済み。

## 8. 完了条件とファイル削除 (最終目標)

以下がすべて満たされた時点で、**本ファイルを削除する PR を出す** (T99):

1. T0〜T12 の各タスクが「実装・マージ済み」または「却下 (理由を本ファイル末尾の
   判定記録に追記した上で docs/todo.md か ADR に転記)」のいずれかになっている。
2. T13 backlog の各項目が「実施済み」「todo.md へ移管」「却下記録」のいずれかになっている。
3. after 計測を実施し、§1 のベースラインとの比較 (takt 中央値 / fix 発生時 /
   docs-only push の 3 点) を T99 の PR 本文または関連 ADR に記録している。
   目標値: コード変更 push (fix あり) 12 分超 → 7 分以下、docs-only push → 1 分台。
4. 長期継続する判断 (refute 採否 = ADR-047、docs-only routing 採否 = T11 の新 ADR) は
   各 ADR の bounded lifetime に引き継がれており、本ファイルに残る未決事項がない。

削除 PR には after 計測結果と「本計画の全タスク処置済み」の対応表を含めること。

### 判定記録 (実施セッションが追記する)

| タスク | 判定 | 日付 | 備考 (却下理由 / 移管先) |
|--------|------|------|--------------------------|
| T0 | 実装・マージ済 (PR #278) | 2026-07-16 | stage 別ログ `stage=<name> elapsed=<秒>s` を追加 (§5 T0 実施結果)。before 値は §1 表を使用。初回実測で T1 の前提に疑義 → §5 T1 の申し送り参照 |
| T1 | 実装済 (PR #279) | 2026-07-16 | `LINT_SCREEN_EVALS` env opt-in で eval を gate から除外。`--ignored` 63s → 21s。step_timeout 600 → 300 (実測 right-size)。**判断根拠**: 着手前実測で (b) 41.3s が (a) 63s の 65% を占め、申し送りの判定基準「前提は生きている」に該当したため実施。ただし絶対値が 269s の約 1/4 だったため期待効果を -2〜4.5 分 → -42s に下方修正し、§1 結論の (1)「主犯」認定も修正した。実行の本丸は (2)(3) = T10/T12 |
| T8 | 実装済 (PR #280) | 2026-07-17 | 「`@` 空 + bookmark が `@-`」を `NoBookmarks` から切り分け、`jj edit @-` を案内するよう修正。`working_copy_is_empty()` を advance と共有して規則の二重定義を解消。`main.rs` 側の重複案内も撤去。回帰テスト 12 本 + サンドボックス実機で before/after 比較 (§4 T8 実施結果)。**post-PR 修正 (CodeRabbit Major 2 件 + simplicity 警告 2 件を採用)**: (a) 判定順を反転し、bookmark が空の `@` にある場合も中断する — 続行すると `jj diff -r @` が空になり祖先の未 push 変更が AI レビューを経ずに push される (方針 2 を却下した理由と同じ穴が bookmark の位置違いで残っていた)。(b) `@-` 照会の失敗を `unwrap_or_default()` で「親はあるが bookmark 無し」に潰していたのを `ParentState::Unavailable` として保持し、親を確認できない場合は実行不能な `jj edit @-` を案内しない (T8 が直したはずの誤誘導の再生産だった)。あわせて simplicity-review の非ブロッキング警告 2 件も採用: (c) `query_parent_state()` の jj 失敗を log する (他の jj 失敗処理の慣習と揃える)。(d) `@-` に bookmark が無い場合は `jj edit @-` だけでは次に `NoBookmarks` で止まるため、bookmark 作成まで含めて案内する。Minor 1 件のうち日付指摘は CodeRabbit が UTC 基準のため不採用 (本 repo の記録は JST 基準で 2026-07-17 が正)。**dogfood 実証**: 本修正の push 作業中に、私自身が `jj new` で空コミットを作ってしまい T8 の incident 状態を再現したが、修正後の bookmark_check が破壊的な `jj bookmark create -r @` ではなく正しい `jj edit @-` を案内し、案内どおりの操作で復旧できた (修正が in the wild で機能することの実証)。**方針変更**: 方針 2 (`@-` を検査対象にして続行) は `[diff] command = "jj diff -r @"` により **takt レビューを無言 skip して push する**ことが判明したため不採用。方針 3 の「push すべき新変更がない」も再現記録の事実 4 (実際に push 成功 = 変更はあった) と矛盾するため不採用。exit 7 は維持し**案内文のみ正す**方針をユーザー承認のうえ採用した。**実施順**: T4-T7 を飛ばして T1 の次に実施 (T1 の dogfood push で再現が取れたタイミングを優先。T8 は他タスクと独立のため順序入替は無害) |
| T5 | 実装済 (PR #282) | 2026-07-17 | push 拒否検知を 40 行 truncate 済み出力から**全量出力**に切替。`lib-subprocess` に `run_cmd_shell_unlimited` を追加 (`drain_pipe_unlimited` は pipe 単体、`_capped_reporting` は cap が残るためどちらも判定用には不足だった) し、3 variant の共通骨格を `run_cmd_shell_with` に集約。境界判定は ADR-044 §「後続の variant 追加」に記録。表示は成功時のみ `cap_for_log` で 40 行 + 超過明示に絞り、**失敗経路は全量表示** (診断情報を落とさない)。**副産物**: 唯一の呼び出し元が消えた `runner::run_stage_cmd` を削除 — dead code 除去に加え「capped 経路で control flow 判定する」罠の構造的排除。`MAX_LINES` は表示用として残置し doc に判定禁止を明記。**厳格化は不採用 (ユーザー承認済み)**: `contains` 誤爆の厳格化はリスクが非対称 (誤検知は出力表示で気付けるが、検知漏れは「リモート未反映のまま exit 0」= 本タスクが防ぐ事故そのもの) で ADR-043 (fail-closed) に反するため見送り、§6 backlog 8 を却下記録に変更。**回帰テスト**: `mod t5_truncated_refusal_detection` 6 本 + lib-subprocess 4 本。`run_push_cmd` を capped に戻すと 3 本が fail することを確認済み (回帰テストが素通りしないことの実証)。**サンドボックス実機で before/after 比較**: 拒否行を 41 行目に置いた fake push command で、before = `[push] 成功` + **exit 0 (silent failure 再現)** / after = 拒否検知 + exit 3。成功経路 (50 行) は 40 行 + `... (10 lines truncated)` 表示で exit 0 を維持。**発見 (本タスク外)**: `cli-pr-monitor` の `push_to_remote` は拒否検知が無く同型の穴 → §6 backlog 9 に追加 (1 PR 1 変更のため別 PR)。**post-PR 修正 (CodeRabbit Minor 1 件 + pre-push 非ブロッキング警告 2 件を採用)**: (a) `run_cmd_shell_capped` の doc「Err 経路で child を kill しない」は **pre-existing の stale 記述** (`kill_and_join_err` 導入 = PR #208 以降、実際は kill + reap + reader thread join している) だったため、child lifecycle の記述を 3 variant 共通の骨格 `run_cmd_shell_with` に集約し variant 側は参照のみにした。(b) `cap_for_log` の truncate 書式重複を `lib_subprocess::truncation_notice` として切り出し (実装は streaming vs materialize で共有できないが書式片は共有できる、という指摘は妥当)。(c) T5 行 / §4 の「本 PR」を PR #282 に backfill (T4 行が放置され本 PR で backfill する羽目になった負債を繰り返さないため)。**実施順**: 計画の推奨順どおり T4 の次に実施 |
| T4 | 実装済 (PR #281) | 2026-07-17 | `push-runner-config.toml` の `refute_enabled = false → true` で dogfood 開始 (変更は方針どおり 1 行、templates は OFF 据え置き)。**dogfood 開始日を同 PR で固定**: ADR-039 bounded lifetime の起点が無いと 2 週間の期限が判定不能になるため、開始 2026-07-17 → **判定期限 2026-07-31** を ADR-047 (ステータス行 / Config opt-in / Bounded lifetime の 3 箇所) + config コメントに明記した。採否判定自体は本計画と独立に ADR-047 で進行する (§8 完了条件 4. の引き継ぎ先)。**初回 dogfood push で切替を実証** (PR #281 自身の push): 起動ログ `takt (pre-push-review-refute)` + takt の `ワークフロー 'pre-push-review-refute' を起動` → 完走を確認。合計 151s (pre_checks 1.3s / quality_gate 49.7s / diff 0.1s / takt 97.8s / push 2.2s)。**verify は予告どおり未発火**: reviewers 2 本とも APPROVE で `all("approved") → COMPLETE` に抜けたため `any("needs_fix") → verify` に入らず、verify 実動の観測は次の findings 発生 run に持ち越し (完了条件は「有効化が効いていることの確認」までで読む)。**副産物: 計測手順の誤りを発見・修正**。ADR-047 §dogfood 計測項目の `.takt/runs/*-pre-push-review-refute/trace.md` は **1 件もマッチしない** — run ディレクトリ名は workflow 名でなく task 名から作られ (`runSlug` = `<UTC timestamp>-pre-push-review`)、refute run でも `20260716-182505-pre-push-review` になる (timestamp も UTC で JST の日付と 1 日ずれ得る)。放置すると 2026-07-31 の採否判定で run 0 件 →「データなし」誤読の恐れがあったため、ADR-047 と §5 T4 実施結果を `meta.json` の `piece` フィールド基準 (`grep -l '"piece": "pre-push-review-refute"' .takt/runs/*/meta.json`) に修正した。設計時の計測手順が実運用開始まで未検証だった例。有効化前の静的確認 (refute workflow / facet 群の存在、`resolve_takt_workflow` の unit test 4 本、`cli-pr-monitor` への波及なし、Rust 変更ゼロのため exe 再ビルド不要) は §5 T4 実施結果に記載。**実施順**: T5-T7 を飛ばして T8 の次に実施 (T4 は他タスクと独立の XS で、依存なし) |
| T6 | 実装済 (PR #283) | 2026-07-17 | `run_diff_cmd` を `Command::output()` (無限待ち) から spawn + `drain_pipe_unlimited` × 2 + `wait_with_timeout_safe` に載せ替え、timeout 時は `DiffResult::Error` = exit 5 で中断 (fail-closed / ADR-043)。**timeout 値 60s + `[diff] timeout` で上書き可 (ユーザー承認済み)**: 方針が「30s に合わせるが 60s でも可」と両論併記だったため確認した。60s の根拠は diff が snapshot + 大 diff 書き出しを伴い `jj bookmark list` (30s) より重いこと、および timeout の目的がハング検知であって latency 制限ではなく誤 timeout のコスト (pipeline 全体が exit 5) が高いこと。config 化は `[push] timeout` と同形の escape hatch。**T5 の `run_cmd_shell_unlimited` は使えない**: `run_cmd_shell_*` は全 variant が stdout と stderr を結合するが、diff の stdout は reviewers が読むレビュー対象そのものとしてファイルに書かれるため、jj の stderr 警告 (並列 workspace 時の `Concurrent modification detected` = **まさに本タスクが想定する状況**) が混入する。分離を維持し、同型の `bookmark_check::run_jj_bookmark_list` とは direct args で signature 非互換のため共通化しない (ADR-044 層 1 に判定を追記)。**⚠ 初版実装の欠陥を回帰テストが検出した (本タスク最大の学び)**: 「timeout 後に reader thread を join する」初版は timeout 1s に対し制御が戻るまで **9.6s** 掛かった。`cmd /c` の child は cmd.exe で**孫 (実際の jj) は kill 対象外**、孫が pipe を保持するため EOF が来ず join がブロックする = timeout が意味を成さない (T6 が直すハングの再生産)。失敗経路では join せず detach する形に修正。**教訓**: timeout の回帰テストは Err の内容だけでなく**経過時間を assert する** (しないと素通りする)。**回帰テスト**: `mod t6_diff_timeout` 7 本 + config 2 本 (206 → 215 passed)。cli-push-runner のテスト全体が 9.66s → 1.55s に短縮 = timeout が効いている証跡。「stderr を diff に混ぜない」契約も seal (`run_cmd_shell_*` に載せ替えると落ちる)。**サンドボックス実機で before/after 比較**: `[diff] command` を `ping -t` (永久応答 = 返らない jj diff の代役) にし、`@-` から build した修正前 exe と比較。before は **diff stage の所要時間が外側 kill に追随** (25s→24.4s / 10s→9.4s) = 内部に上限が無く放置すれば無限待ち・診断なし。after は 3.0s で exit 5 + 「jj lock 競合を疑え」の診断。実 `jj diff` (既定 60s) が誤 timeout しないことも確認。before の run 後に `ping.exe` が残存し、孫が kill を生き延びる実機裏付けも取れた。**発見 (本タスク外)**: `lib-subprocess` の `run_cmd_shell_*` 3 variant が**同じ穴**を持ち timeout が wall-clock を縛れない (実測 9.23s)。影響は quality_gate `step_timeout` / push `timeout` / cli-merge-pipeline → §6 backlog 10 に追加 (1 PR 1 変更のため別 PR)。**実施順**: 計画の推奨順どおり T5 の次に実施 |
| T7 | 実装済 (PR #284) | 2026-07-17 | `hooks-stop-quality` の `main` 冒頭で cwd をプロジェクトルートへ正規化 (`normalize_cwd_to_project_root`)。**ルート導出は (b) exe パス**: 方針が両論併記だったため実測し、VSCode 拡張環境で **`CLAUDE_PROJECT_DIR` が空** = ADR-005 (2026-03-17) の不安定性が現在も再現することを確認して (a) を却下。既存規約 (順位 287 / ADR-010、`config_path()` / `pipeline_lock::exe_claude_dir()` / `lib_telemetry::exe_dir()`) と同形。判断根拠は**本計画が削除予定のため ADR-005 に追記**して恒久化した。**⚠ 方針の前提が 1 つ誤っていた (ユーザー承認のうえ逸脱)**: リスク欄は「正規化が takt subsession 判定に影響しないか確認が必要」「判定ロジックは元 cwd を使う形が安全」としていたが、`takt_subsession_active` は **cwd 依存で既に壊れていた** (cwd = `.takt/runs` だと `.takt/runs/.takt/runs` を探して空振り → active run 未検出 → ADR-004 § takt subsession skip が効かず edit: false の subsession に「直せ」を返す = PR #221 の事故が再発しうる)。元 cwd 維持は「安全」ではなく既知不具合の温存のため、**両症状に効く main 冒頭 1 回の正規化**を採用。回帰テストで修正前に実際に失敗することを確認済み (推測ではない)。**実装**: `project_root_from_exe` は ADR-010 の配置 (`<root>/.claude/<hook>.exe`) を満たすときのみ `Some` を返し、`target/debug/` 等では正規化を skip (cwd 書き換えは全 step の実行位置を変えるため、推測でルート扱いせず従来挙動に倒す)。ルート特定不能・`set_current_dir` 失敗は警告のみで継続 = fail-open (`pipeline_is_running` と同じ線引き、ADR-043: Stop 時点は助言層で本物のゲートは push pipeline 側)。**lib-subprocess は無変更** — プロセス単位の正規化で `cmd /c` の子が継承するため、共有 `run_cmd_shell_*` への cwd 引数追加 (variant 増殖) を回避できた。**ファイル分割 (T7 に付随、T1 と同型)**: `main.rs` が 712 行 → 追記で 804 行となり 800 行上限に触れたため takt 判定を `takt_subsession.rs` へ切り出し (main 532 / takt_subsession 290)。**T7 が直している file-length gate に T7 自身が引っ掛かった** = gate が機能していることの副次的実証。**回帰テスト**: `tests/t7_cwd_independence.rs` E2E 5 本 + unit 2 本 (26 → 33 passed)。**exe を `<root>/.claude/` に staging して spawn** するのが要点 — `target/debug/` の exe を直接起動すると exe-relative のルート導出を素通りして実配置を検証しない。`normalize_cwd_to_project_root()` の呼び出しを外すと **bad 2 本がちょうど失敗し good 3 本は通る**ことを確認済み (failure は incident の逐語再現)。good 側に「実失敗する step は cwd に依らず block する」= 正規化がゲートを骨抜きにする最悪の退行ガードを含む。**実機検証 (before/after、本リポジトリの実 config)**: cwd = `.takt/runs` で before = block + `**file-length** failed:` + 文字化け / after = 出力なし (通過)。root cwd と深い cwd も通過。**方針の記述も実機で裏付け**: before で失敗したのは `file-length` step のみで pnpm 系 5 step + `cargo clippy` は通っていた (pnpm/cargo は設定ファイルを上方探索する) = **ルート相対パスを書いた step だけが壊れる**非対称が症状をまだらにし発見を遅らせていた。**CP932 デコードフォールバック (方針 2) は §6 backlog 11 へ分離 (ユーザー承認済み)**: 影響先が共有 lib (push-runner / merge-pipeline) のため §2 原則 4 に従う。cwd 修正で incident の文字化けは消えるが、exe 欠落時 (ADR-005 Negative の既知事象) 等で経路自体は残るため却下ではなく backlog。**発見 (本タスク外)**: T7 は cwd drift silent 故障の 3 例目で、順位 281 (Tier 1、lint rule) / 順位 287 (Tier 3、convention 明文化) が先行 todo 化済み。ただし T7 は「config 解決」でなく「**step 実行の cwd**」の別カテゴリのため既存 lint rule 案では捕捉できず、281 着手時に検出対象拡大を検討する価値がある (本計画スコープ外 = todo13.md 管理)。**実施順**: 計画の推奨順どおり T6 の次に実施 |
