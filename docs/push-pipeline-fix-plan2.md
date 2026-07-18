# push パイプライン改善 残作業計画 (push-pipeline-fix-plan.md の後継)

> **本ファイルの位置付け**: [push-pipeline-fix-plan.md](push-pipeline-fix-plan.md) (以下「旧計画」、
> 約 1300 行) のサイズ超過に伴い、**残作業のみ**を分離した後継ファイル (2026-07-18 分離)。
> 実装済みタスクの詳細記録 (方針・実施結果・逸脱の経緯) は旧計画 §4/§5/§8 に残るが、
> **本ファイルのみで「何を実装してきたか / 何が残っているか」を把握できる**ことを編集方針とする。
> 残タスクは 1 PR 1 タスクで進める。
>
> **最終目標: 全残作業完了時に旧計画と本ファイルを同一 PR で削除する** (R6)。

## 1. これまでに実装した内容 (2026-07-16〜07-18、全 PR マージ済み)

背景: `pnpm push` (= cli-push-runner → quality_gate → takt pre-push-review → jj git push →
cli-pr-monitor) の遅延 (コード変更 push 最大 14.6 分) と不具合の一掃。2026-07-16 調査の結論は
「主因は (1) gate 内の Ollama eval 毎 push 実行、(2) takt builtin policy の過剰 REJECT →
5〜8 分の fix iteration 誘発、(3) fix step 内の workspace 検証重複」。(1) は実測で小口
(-42s) と判明し、(2)(3) が本丸だった。

| タスク | PR | 実装内容 (1 行) |
|---|---|---|
| 計画 doc | #277 | 旧計画ファイルの追加 |
| T0 | #278 | stage 別所要時間ログ `stage=<name> elapsed=<秒>s` を追加 (**stderr のみ・非永続** → R3 で解消) |
| T1 | #279 | Ollama eval (assert ゼロ・計測専用) を env `LINT_SCREEN_EVALS` opt-in 化し gate から除外 (-42s/push) |
| T8 | #280 | 空 `@` 時の bookmark_check 誤誘導を修正 (`jj edit @-` 案内 + 空 `@` でも中断維持) |
| T4 | #281 | refute facet の dogfood 開始 (`refute_enabled = true`。ADR-047、判定期限 7/31) |
| T5 | #282 | push 拒否検知を 40 行 truncate 出力から全量出力に切替 (silent-failure push 防止、`run_cmd_shell_unlimited` 追加) |
| T6 | #283 | diff stage の timeout 追加 (60s、失敗経路は join せず detach。「timeout テストは経過時間を assert する」教訓) |
| T7 | #284 | Stop hook file-length step の cwd 依存修正 (exe パス基準でプロジェクトルートへ正規化) |
| T3 | #285 | `pnpm build` 形骸ゲートを `npx --no-install tsc --noEmit` で実体化 (`\|\| true` 除去、fail-closed 実測) |
| T2 | #286 | 旧 cli-push-pipeline crate (dead code 316 行) を削除、workspace 22 → 21 crate |
| T10 | #287 | takt builtin 8KB checklist policy を anomaly 設計の project policy `review-anomaly` (-37%) で shadow (ADR-056、判定期限 7/31) |
| T11 | #288 | docs-only PR (ADR-035 path 基準) の rust gate 決定論 skip。`lib-docs-policy` 新設 (ADR-057、判定期限 8/15) |
| T12 | #289 | fix 後の決定論再ゲート `post_takt_regate` + fix.md 自己検証義務の縮小 (ADR-058、判定期限 8/15) |
| T13 | #290 + 判定 | backlog 13 項目の処置確定: **採用 2 (→ R1/R2)**、todo 移管 3 (項目 9→順位 324 / 項目 10→順位 323 / 項目 12→既存順位 16)、却下 6 (項目 2/4/5/6/8/11)、条件付き却下 2 (項目 7/13、再評価トリガー付き)。判断根拠は旧計画 §6/§8 |
| R1 | #292 | quality_gate 失敗 step の出力を全量表示 (40 行 truncate 除去。成功経路は cap 維持 = T5「失敗経路は診断を落とさない」の残り半分。ADR-049 流の回帰テスト 2 本追加) |
| R2 | #293 | loop_monitor stall-detection judge を sonnet → haiku 化 (2 択 routing のみ、post-pr-review.yaml に前例)。pre-push-review + refute 両 yaml を同期変更 (片方だけは効果ゼロ = T10 の罠、原則 6)。ADR-047 配下 |

### 実測の現在地 (2026-07-18 時点)

| 指標 | before (2026-07-16 baseline、直近 20 run) | 現在 |
|---|---|---|
| takt 部分 中央値 | 3.8 分 | T10 後 4.9 分 (n=4・全て fix なし。diff サイズの交絡があり判定保留) |
| fix iteration 発生率 | ~45% (9/20) | T4 後 19% (3/16) → T10 後 0% (0/4)。n 不足のため ADR-056/058 判定に持ち越し |
| fix あり run 所要 | 5.5〜14.6 分 | T10 後は fix 発生なし (データ待ち) |
| quality_gate (rust 系) | 269s (T1 前) → ~50s (T1 後) | docs-only push は 3.0s (T11 skip、PR #290 push で実測) |
| docs-only push 総計 | - | 168s / 231s (takt 160〜225s が支配項) |

- **計測方法 (永続データ)**: `.takt/runs/<slug>/meta.json` の startTime/endTime と `trace.md` の
  iteration ヘッダ (takt 部分・fix 発生の判定)。refute run の抽出は `meta.json` の
  `"piece": "pre-push-review-refute"` 基準 (run ディレクトリ名では判別不可、ADR-047 に記載)。
- **stage 別 (gate/push 等)** は T0 ログが stderr のみで非永続だったが、**R3 で per-run JSONL
  (`.claude/telemetry/push-runs-*.jsonl`) に永続化**した (実装済み・未 push)。次回 push 以降が
  自動的に集計コーパスになる。
- ⚠ **旧計画 §8 の目標「docs-only push 1 分台」は総時間としては達成不能の見込み**:
  T11 で「docs-only でも takt レビューは skip しない」(docs の事実誤り検出実績があるため) を
  ユーザー承認済みであり、takt が支配項として残る。R6 の after 計測で目標を gate 部分
  (実測 3.0s) に再解釈するか、未達理由を記録すること (旧計画の学び「見積も目標も実測で見直す」の適用)。

## 2. 進め方の原則 (旧計画 §2 の要約継承)

1. **1 PR 1 変更** (`pr_size_check` warning 800 / block 1500 行)
2. **不具合修正は再現テスト先行**。timeout 系の回帰テストは **経過時間を assert する** (T6 教訓)
3. **実験的機能は ADR-039 3 点セット** (config opt-in / kill-switch / bounded lifetime)、
   gate 系は ADR-043 fail-closed (判定不能→フル実行)
4. **Rust 変更時は exe 再ビルド** (`pnpm build:<name>`。`.claude/*.exe` が実行される配布物)
5. **dogfood 注意**: 各 PR の push は修正対象のパイプライン自身を通る。壊すと自分の push が通らない
6. **workflow 設定を触るときは「今どの経路が実際に走るか」を config から再確認**
   (T10 教訓: `refute_enabled = true` のため実際に走るのは `pre-push-review-refute.yaml`)

## 3. 残タスク (1 PR 1 タスク、推奨順)

### R1: gate 失敗時出力の truncate 改善 (T13 項目 1 採用分) — XS **【マージ済み #292, 2026-07-18】**

- **内容**: quality_gate の step 失敗時、cargo test の失敗一覧が 40 行 truncate で消え
  診断できない問題の解消。`run_cmd_shell_capped_reporting` (truncate 明示 variant) + cap
  引き上げ、または失敗経路のみ全量表示 (T5 の「失敗経路は診断を落とさない」原則の残り半分)。
- **対象**: `src/cli-push-runner/src/stages/quality_gate.rs` + `lib-subprocess` の必要 variant
- **受け入れ基準**: 失敗 step の出力が truncate されず表示されることの回帰テスト。
  成功経路の表示は現状維持 (cap あり) で退行なし。
- **実施結果 (2026-07-18, マージ済み / PR #292)**:
  - **方針**: 受け入れ基準「truncate されず表示」に従い**失敗経路の全量表示**を採用
    (`capped_reporting` + cap 引き上げ案は truncate を明示するだけで基準を満たさないため
    不採用)。T5 (§4/PR #282) が push stage で確立した「判定は exit status ベース・失敗経路は
    診断を落とさない・成功経路は cap」を quality_gate に横展開した = **T5 の残り半分**。
  - **実装 (`stages/quality_gate.rs` に閉じる)**: step 実行を `run_step` に集約し、
    `run_cmd_shell_capped` (`MAX_LINES` = 40 行の silent truncate) から
    `run_cmd_shell_unlimited` へ切替。失敗 step は全量を `eprintln!`、成功 step は
    従来どおり出力を表示しない (quiet = 退行なし)。判定 (`ok`) は exit status 由来で
    出力量に依存しないため、全量保持のコストは失敗時の診断のためだけに払う旨を
    `run_step` の doc に明記した。**`run_cmd_shell_unlimited` は T5 で追加済みのため
    lib-subprocess の変更は不要** (対象欄の「必要 variant」は充足済み)。top-level の
    `MAX_LINES` import は capped 不使用に伴い除去 (const 自体は push.rs 等が使用のため残置)。
  - **回帰テスト (ADR-049 の流儀)**: `mod r1_failure_output_not_truncated` **2 本追加**
    (cli-push-runner crate 250 → 252 passed。quality_gate module は 9 → 11 本)。由来
    (2026-07-16 調査でコード監査により backlog 化。
    in the wild の発火記録なし) を module doc に明記。bad = 60 行出力 + exit 1 の step で
    cap (40 行) の外にある診断行 (60 行目) が残ること、good = 成功 step が退行しないこと。
    **修正前の挙動で失敗することを確認済み**: `run_step` を capped 版に戻すと bad が
    「40 行に切り詰めている = R1 の不具合」で fail し good は通る (回帰テストが素通りしない証跡)。
  - **サンドボックス実機 before/after は不要と判断**: T5〜T7 は exit code / 制御フローを
    変える変更のため配布 exe 比較を行ったが、R1 は**失敗経路の表示量のみ**を変え判定
    (`ok` / exit code) は不変。回帰テストの capped↔unlimited 差替えで before/after は
    実証済みのため、XS 相応の検証に留める。
  - **exe 再ビルド済み** (`pnpm build:cli-push-runner`)。`cargo clippy -p cli-push-runner
    --all-targets` warning 0 / `cargo test -p cli-push-runner` 252 passed。
  - **§1 表に R1 行 (#292) を追加済み** (2026-07-18 マージ完了に伴い backfill)。未 push
    だった間は §1 (「全 PR マージ済み」スナップショット) に載せず §3 本欄を完了記録としていた。

### R2: loop_monitor judge の haiku 化 (T13 項目 3 採用分) — XS **【マージ済み #293, 2026-07-18】**

- **内容**: loop_monitor の `judge.model: sonnet` → `haiku` (2 択判定のみ。post-pr-review.yaml に前例)。
- **対象**: `pre-push-review.yaml` と `pre-push-review-refute.yaml` の**両方** (原則 6 参照。
  片方だけ変えると効果ゼロで気付けない = T10 で実際に起きた罠)。
- **受け入れ基準**: 実 push で judge が haiku で完走。fix iteration 発生 run での遷移時間を
  記録できれば尚可 (遷移時間は R3 (#294) が per-run JSONL で永続化)。
- **実施結果 (2026-07-18, マージ済み #293)**:
  - **方針**: loop_monitor の stall-detection judge を `sonnet` → `haiku`。judge は cycle が
    threshold (2) 回反復した時に `Healthy → reviewers(refute 側も同じ) / Unproductive → supervise`
    の **2 択 routing** を返すだけで、コード読解や修正判断を伴わない。haiku で十分という前例は
    post-pr-review.yaml の analyze step (haiku で approved/needs_fix/user_decision の **3 分類**を
    担当) = より複雑な分類を既に haiku が捌いている実績。
  - **対象 (両方変更した)**: `pre-push-review.yaml` (judge L30) と `pre-push-review-refute.yaml`
    (judge L39) の 2 ファイル。`refute_enabled = true` / `refute_workflow = "pre-push-review-refute"`
    (`push-runner-config.toml` L213-214) のため**実走は refute 側**だが、kill-switch
    (`refute_enabled = false`) で非 refute 側へ即戻せる設計 (ADR-047) のため、片方だけ変えると
    戻した瞬間に効果が消え気付けない (原則 6 / T10 で実際に起きた罠)。両 yaml の judge に
    「もう片方と揃えよ」inline コメント (原則 6 / T10 参照付き) を追加し、同期義務を人可読に明記した。
  - **ADR-039 の観点**: これは experimental feature の**新設ではなく既存 judge の model 変更のみ**。
    opt-in / kill-switch / bounded-lifetime は refute facet 自体 (ADR-047、判定期限 7/31) が
    既に保持しており、haiku judge はその配下に入る。非 refute 側の judge も同値 (haiku) に
    揃えたのは、refute 廃止 (kill-switch or ADR-047 却下) 時の着地先として効果を維持するため。
  - **exe 再ビルド不要**: yaml 設定変更のみで Rust 変更なし (原則 4 は非該当)。takt (0.35.3、
    ADR-017 で固定) が実行時に yaml を読む配布物であり、`.claude/*.exe` は無関係。
  - **検証**: `takt prompt pre-push-review` / `takt prompt pre-push-review-refute` で両 workflow が
    step 4 まで正常にパース・レンダリングされることを確認 (judge step preview の
    `reportContent is required` は実 report 本文を要さない dry preview 固有の制約で、yaml 不正
    ではない)。両ファイルの `loop_monitors[0].judge.model` が `haiku` であることも確認済み。
  - **受け入れ基準の充足度 (未検証事項として記録)**: 基準「実 push で judge が haiku で完走」は
    judge が **loop_monitor の cycle 停滞検出時のみ fire** する = fix iteration が 1 回以上発生する
    run でしか起動しないため、fix なし run (§1 実測では T10 後 0/4 が fix なし) では judge 自体が
    呼ばれない。マージ (#293) 後もまだ fix 発生 run が出ておらず「haiku 完走」の実証は次の fix
    発生 run 待ち。遷移時間の記録 (尚可) は R3 (#294) が per-run stage timing を永続化したため
    コンソール手動保存に依存しなくなった (judge 発火の詳細は従来どおり `.takt/runs/<slug>/trace.md`)。
  - **§1 表に R2 行 (#293) を追加済み** (2026-07-18 マージ完了に伴い backfill)。未 push だった間は
    §1 (「全 PR マージ済み」スナップショット) に載せず §3 本欄を完了記録としていた。

### R3: push per-run メトリクスの JSONL 永続化 (todo 順位 325) — S **【実装済み・未 push, 2026-07-18】**

- **内容 (当初案の全フィールド)**: run 終了時に stage 別 elapsed / docs_only 判定 /
  post_takt_regate 判定 / pr_size 行数 / takt run slug / total / exit code / os を 1 行 JSONL で
  `.claude/telemetry/` へ append。lib-telemetry (ADR-055) を再利用。**詳細仕様と作業計画は
  docs/todo13.md 順位 325 が canonical**。⚠ **このうち `pr_size 行数` と `takt run slug` は実装で
  deferred** (下記実施結果 / ADR-055 amendment 参照)。実際に永続化される schema は実施結果の
  「記録フィールド」を正とすること (集計利用者が存在しないフィールドを期待しないため)。
- **位置付け**: R5/R6 の計測基盤。harness-improvement-plan セクション 3 (Linux 対応) 着手前に
  入れると、同作業の push (8〜15 回見込み) が自動的に after 計測コーパスになる。
- **実施結果 (2026-07-18, 実装済み / 未 push)**:
  - **スキーマ判定 (canonical の「ADR-055 amendment か別 record kind か」)**: **別 record kind =
    別ファイル `push-runs-<YYYY-MM-DD>-<pid>.jsonl`** を採用。firing (`firings-*.jsonl`) は
    WP-12 step 2 の集計が glob 走査する前提で shape 固定のため、shape の異なる push-run 行を
    混ぜると firing 集計を壊す。opt-in (`[telemetry] enabled`) / kill-switch
    (`CLAUDE_TELEMETRY_DISABLE`) / fail-open / per-pid×日次 partition / `WRITE_LOCK` の既存原則
    には相乗り (グローバル telemetry スイッチ 1 つで両 record kind を統べる)。ADR-055 に
    amendment section を追記した。
  - **責務分離**: lib-telemetry には**ドメイン中立の汎用 writer** (`record_metric` /
    `record_metric_to` / `record_metric_gated_to`。任意 `Serialize` を prefix 付き partition へ
    書き `ts` を差し込む) のみ追加し、push-run 固有スキーマ `RunRecord` は消費側
    `cli-push-runner/src/metrics.rs` が保持。ADR-055 §欠点 の「観測層を特定ドメインに結合させない」
    思想を保つ (UTC ヘルパーの消費者も lib-telemetry 内に留まり、lib-time 抽出トリガに未到達)。
  - **記録フィールド**: os / exit_code / total_secs / docs_only (bool) / skipped_groups /
    post_takt_regate 判定 (**skip / run-pass / block を区別**: disabled・override_skipped・
    no_change・changed_pass・changed_block・indeterminate_pass・indeterminate_block) /
    takt_workflow / bookmarks / stage 別 elapsed (JSON object)。プライバシーはメタデータのみ
    (ADR-055 §プライバシー)。bookmark 名は branch 識別子 (session_id と同性質) のため run 識別鍵
    として採用。
  - **deferred (canonical フィールド案のうち本 PR 見送り、完了基準の必須外)**: **takt run slug**
    (`run_takt` の `run_cmd_inherit` が takt 出力を捕捉しないため `.takt/runs/` との join 鍵を
    clean に取れない)・**pr_size 行数** (`run_pr_size_check` が総行数を返さない)。いずれも
    別 PR 候補。
  - **変更範囲の確認・明記 (完了基準の「main.rs と .claude/telemetry のみが対象」主張の検証)**:
    canonical の想定より広く、以下も変更が必要だった。**主張は不正確**と判明:
    - `metrics.rs` (新規): 収集 struct `RunMetrics` + `RunRecord` serde。
    - `main.rs`: `run_pipeline` を「メトリクス所有 + 全終了経路で 1 回 write」の薄い wrapper と、
      stage を回す `run_stages` に分割。各 `timed()` を `metrics.timed()` に置換。
    - `log.rs`: 計測を `RunMetrics::timed` に一元化するため free `timed()` を撤去し、stderr
      contract 出力を `log_stage_elapsed` に抽出 (T0 の `stage=... elapsed=...s` 書式は不変)。
      → canonical の「log.rs 変更不要」は不成立。bool 戻り値の `timed()` からは elapsed を蓄積
      できず、計測点の一元化 = `RunMetrics::timed` への移設が最小の clean 解だった。
    - `stages/post_takt_regate.rs`: `run_post_takt_regate` の戻り値を bool → `RegateOutcome`
      (`decision` + `proceed`) に変更し `RegateDecision` を surface。→ canonical の
      「post_takt_regate 呼び出し元 変更不要」も不成立。bool 単独では「無変更 skip」と
      「変更あり pass」を区別できず、**完了基準が要求する post_takt_regate 判定 (ADR-058 の
      skip vs run-pass vs block 信号)** を満たせないため必須の逸脱。
    - `lib-telemetry`: 上記の汎用 writer 追加 (器の再利用の実体)。
  - **テスト**: cli-push-runner 252→256 passed (差引 +4 = metrics module 5 本新規 [timed 戻り値+stage
    記録 / 完了 run の全フィールド / 中断 run exit7 でも書かれる / kill-switch OFF で書かれない /
    takt skip 時 workflow 省略] − log.rs の `timed` テスト 1 本撤去。252 は R1/#292 後の基準)。
    lib-telemetry 14→20 passed (+6 = 汎用 writer テスト 4 [ts 差し込み / firing と別ファイル /
    gate ON ×1・OFF ×1] + file_prefix path-traversal 検証 2 [CodeRabbit #294 対応])。
    post_takt_regate は既存テストに verdict assert を追加 (本数不変)。
    `cargo clippy --workspace --all-targets --all-features -- -D warnings` warning 0。
  - **実機 end-to-end 検証 (配布 exe)**: `pnpm build:cli-push-runner` で `.claude/` に再配布後、
    config 不在の一時 cwd から起動し **config error (exit 4) の中断経路でも** `os=windows` /
    `exit_code=4` / `ts` 付き `push-runs-*.jsonl` 行が `firings-*` とは別ファイルに 1 行書かれること、
    `CLAUDE_TELEMETRY_DISABLE=1` では書かれないことを実測 (検証で出た合成行は削除済)。
  - **受け入れ基準の充足度**: 完了基準「stage 別 elapsed / docs_only / post_takt_regate 判定 /
    total_secs が事後集計できる」✓、「ADR-057/058 の効果検証がコンソール手動保存に依存しない」✓
    (機械集計可能な JSONL 化)。実 push コーパスは**次回 push 以降**に自動蓄積される (本 work unit は
    commit までのため実データ 0 件。R5/R6 が消費)。
  - **todo 側**: docs/todo13.md 順位 325 エントリ + todo-summary.md 順位 325 行を削除済 (canonical
    の「本エントリ削除」)。exe 再ビルド済 (原則 4)。§1 表への行追加と PR 番号 backfill は
    push/マージ時に実施 (R1/R2 と同じ扱い)。
  - **CodeRabbit レビュー対応 (PR #294)**: 未解決 3 件を対応 — (a) `lib-telemetry` の公開 API
    `record_metric*` 由来 `file_prefix` に path-traversal 検証 (`is_safe_file_prefix`、fail-open
    skip) + 回帰テスト 2 本を追加 (Major、現 exploit 無しだが `pub` API の defense-in-depth)、
    (b) 本 R2 セクションのマージ済み表記の不統一を解消 (Minor)、(c) 本 R3「内容」欄の deferred
    フィールド (pr_size 行数 / takt run slug) を明示 (Minor)。CI pass / CodeRabbit review 完了。

### R4: ADR-047 / ADR-056 の採否判定 — 判定期限 2026-07-31 (doc PR)

- **ADR-047 (refute facet)**: `meta.json` の `piece` 基準で refute run を集計し、FP 起因
  fix iteration の削減効果と verify step の実動を評価 → 採用/廃止/延長をステータス更新。
- **ADR-056 (policy shadow)**: 受け入れ基準「simplicity execute 203s → 150s 以下 (5 run)」と
  REJECT 率で評価。T4 refute と期間が重なるため効果の帰属に注意 (ADR-056 に記載済み)。
- 2 判定を 1 doc PR にまとめてよい (どちらも ADR ステータス更新のみ)。

### R5: ADR-057 / ADR-058 の採否判定 — 判定期限 2026-08-15 (doc PR)

- **ADR-057 (docs-only routing)**: 誤 skip 0 の確認 + docs-only push の gate 実測 (すでに
  3.0s の実測 1 件あり = PR #290)。
- **ADR-058 (post-takt re-gate)**: fix 発生 run での block/pass 実績 (fix 無変更 skip の
  実測は PR #290 で 1 件あり)。
- R3 実装済みならメトリクス JSONL で機械集計。未実装なら `.takt/runs` + push ログの手動記録。

### R6 (旧 T99): after 計測 + 両計画ファイル削除 PR — 最終タスク

- **after 計測**: §1「実測の現在地」を最終更新し、baseline との 3 点比較
  (takt 中央値 / fix あり run / docs-only push) を PR 本文と関連 ADR に記録する。
  docs-only 目標の再解釈 (§1 の ⚠) もここで確定する。
- **全タスク対応表**: §1 の表を最終更新して PR 本文に転記 (旧計画 §8 の削除 PR 要件を継承)。
- **削除**: `docs/push-pipeline-fix-plan.md` と `docs/push-pipeline-fix-plan2.md` を**両方削除**。
  他ドキュメントから両ファイルへの参照が残っていないことを `pnpm lint:docs` / grep で確認する。
- **前提**: R1〜R5 完了。

## 4. 完了条件

1. R1〜R5 がすべて「マージ済み」または「判定記録済み」(R4/R5 は ADR ステータス更新をもって完了)。
2. R6 の削除 PR が after 計測結果と全タスク対応表を含み、両計画ファイルを削除している。
3. 順位 323 (lib-subprocess timeout) / 順位 324 (pr-monitor push 拒否検知) は **todo 系列で継続管理**
   であり本計画の完了条件ではない (旧計画 §8 条件 2 は「移管」で充足済み)。ただし R3 (= 順位 325)
   のみ R5/R6 の計測品質に直結するため本計画の残タスクに含めている。
4. 本計画から見て未決の長期判断が残っていない (refute / policy shadow / docs-only routing /
   re-gate の採否はすべて各 ADR の bounded lifetime に引き継ぎ済み → R4/R5 で消化)。
