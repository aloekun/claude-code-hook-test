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
| T8 | bookmark_check の空 `@` 誤誘導修正 | 不具合 | exit 7 誤案内の防止 | S | なし |
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

### T6: diff stage の timeout 欠落

- **現状**: `src/cli-push-runner/src/stages/diff.rs` (L20-23 付近) は `Command::output()` を
  無限待ち。他 stage は全て timeout 付き (jj 系 30s、gate 600s、push 300s)。
  ADR-045 の並列 workspace 運用で jj lock 競合時にパイプラインが無言ハングする。
- **方針**: 他 stage と同じ timeout 機構 (lib-subprocess) に載せ替え、timeout 時は
  明確なエラーで exit (fail-closed)。timeout 値は jj 系 30s に合わせるが、
  大 diff の書き出しを考慮して 60s 程度でも可。
- **テスト**: timeout 経路の unit test (長時間コマンドの fixture で Err になること)。
- **リスク**: 低。

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

### T8: 空 `@` 時の bookmark_check 誤誘導 (要再現確認)

- **現状** (コード監査の指摘、実装前に再現テストで確認すること):
  `advance_jj_bookmarks` は `@` が空なら bookmark を `@-` へ前進させる
  (`stages/push_jj_bookmark.rs:82-95` 付近) のに、`stages/bookmark_check.rs` (L44, L117-146 付近) は
  `jj bookmark list -r @` の厳密一致で検査するため、`jj new` 直後の正常な再 push 状態でも
  exit 7「bookmark を作成して再実行してください」で中断し、従うと bookmark を壊す方向に誘導する。
- **方針**: まず再現テストを書く (`@` 空 + bookmark が `@-` にある状態)。再現したら、
  検査を `@` 空時は `@-` を対象にする (advance と同じ規則) よう揃え、メッセージを
  「push すべき新変更がない」旨に修正。再現しなければ本タスクは却下として §8 の判定記録に残す。
- **リスク**: 中。jj 変更検出は ADR-021 の設計原則に従うこと (revset 合成の流儀)。

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
  `run_lint_screen_against_all_fixtures` (L746-769) を巻き込む。このテストは
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
8. `push_was_refused` の `contains` 誤爆厳格化 (T5 に含めなかった場合)。

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
