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
- **stage 別 (gate/push 等) は T0 ログが stderr のみで非永続** — R3 (順位 325) が解消する。
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

### R1: gate 失敗時出力の truncate 改善 (T13 項目 1 採用分) — XS **【実装済み・未 push, 2026-07-18】**

- **内容**: quality_gate の step 失敗時、cargo test の失敗一覧が 40 行 truncate で消え
  診断できない問題の解消。`run_cmd_shell_capped_reporting` (truncate 明示 variant) + cap
  引き上げ、または失敗経路のみ全量表示 (T5 の「失敗経路は診断を落とさない」原則の残り半分)。
- **対象**: `src/cli-push-runner/src/stages/quality_gate.rs` + `lib-subprocess` の必要 variant
- **受け入れ基準**: 失敗 step の出力が truncate されず表示されることの回帰テスト。
  成功経路の表示は現状維持 (cap あり) で退行なし。
- **実施結果 (2026-07-18, 実装済み / 未 push)**:
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
  - **§1 表への行追加と PR 番号 backfill は push/マージ時に実施** (§1 は「全 PR マージ済み」
    のスナップショットのため、未 push の本タスクは §3 の本欄で完了記録とする)。

### R2: loop_monitor judge の haiku 化 (T13 項目 3 採用分) — XS

- **内容**: loop_monitor の `judge.model: sonnet` → `haiku` (2 択判定のみ。post-pr-review.yaml に前例)。
- **対象**: `pre-push-review.yaml` と `pre-push-review-refute.yaml` の**両方** (原則 6 参照。
  片方だけ変えると効果ゼロで気付けない = T10 で実際に起きた罠)。
- **受け入れ基準**: 実 push で judge が haiku で完走。fix iteration 発生 run での遷移時間を
  記録できれば尚可 (R3 未実装の間は push ログの手動保存)。

### R3: push per-run メトリクスの JSONL 永続化 (todo 順位 325) — S

- **内容**: run 終了時に stage 別 elapsed / docs_only 判定 / post_takt_regate 判定 /
  pr_size 行数 / takt run slug / total / exit code / os を 1 行 JSONL で `.claude/telemetry/` へ
  append。lib-telemetry (ADR-055) を再利用。**詳細仕様と作業計画は docs/todo13.md 順位 325 が canonical**。
- **位置付け**: R5/R6 の計測基盤。harness-improvement-plan セクション 3 (Linux 対応) 着手前に
  入れると、同作業の push (8〜15 回見込み) が自動的に after 計測コーパスになる。
- **完了時**: todo13.md エントリ + todo-summary.md 順位 325 行を削除 (todo 側の運用に従う)。

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
