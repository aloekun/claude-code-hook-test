# auto-push gate dogfood — B1-loop GO/NO-GO 判定基準 (ephemeral)

> **本ファイルの位置付け**: PR #224 で顕在化した auto-push gate-bypass の是正 (A1 + B1) の dogfood 観測記録と、B1-loop (convergence ループ差し戻し) の要否判定基準・設計案を保持する **ephemeral 文書**。恒久文書 (CLAUDE.md / ADR) から本ファイルへ参照を張らないこと。
>
> **削除条件**: B1-loop の GO/NO-GO 判定が確定した時点で、判定結果を永続文書へ移管した上で本ファイルを削除する (§6 の retirement checklist 参照)。
>
> **判定期限**: 次のいずれか早い方 — (a) PR-1 merge 日 (§3 に記入) から **6 週間経過**、(b) **gate FAIL 2 件** 到達、(c) **auto-push 発火 10 回** 到達。

## 1. 背景 — 一連の不具合修正 (PR #224 gate-bypass)

PR #224 (PR-W2) で、CodeRabbit Major finding を takt が auto-fix した際、`create_fix_commit` の変更が `#[ignore]` の repush 統合テスト 2 件を破壊したまま PR に到達した。原因は二重のゲート迂回:

1. **fix 時の穴**: takt fix facet の coder は `cargo test` (非 `--ignored`) のみで検証し `convergence_verdict: fully_resolved` を宣言した。`#[ignore]` 統合テストは plain `cargo test` では実行されない。
2. **push 時の穴**: 監視 (cli-pr-monitor) の auto-push は `jj git push` 直 push で、cli-push-runner の quality_gate (`cargo test -- --ignored` を回す唯一の自動経路、`push-runner-config.toml` の `rust-lint-test` group) をバイパスしていた。

補足: pre-push pipeline (ADR-015) にも同型の穴がある。ステージ順が Stage 1 quality_gate → Stage 2 takt review+fix → Stage 3 push のため、Stage 2 の takt fix が `#[ignore]` テストを壊しても push 前に gate は再実行されない。A1 はこちらの穴も塞ぐ唯一の層。

## 2. land した防御層 (PR-1)

| 層 | 内容 | 実装 |
|---|---|---|
| A1 (fix 時) | test ファイル変更 or `pub`/`pub(crate)` 関数の挙動・signature 変更時、`cargo test -- --ignored --test-threads=1` の PASS を `fully_resolved` の前提条件化 | `.takt/facets/instructions/fix.md` (pre-push-review / post-pr-review 共有) |
| B1 (push 時) | auto-push 前に push-runner-config.toml の quality_gate group を実行。FAIL なら push せず `action_required` (fail-closed、ADR-043)。fix diff が docs-only (ADR-035 path 基準) なら gate skip | `src/cli-pr-monitor/src/stages/gate.rs` + `repush.rs` |

B1 は **即 escalation 方式** (ループなし)。gate FAIL 時は人間が `pnpm push` で復旧する。この escalation コストが実際にどの程度発生するかを dogfood で観測し、B1-loop (自動修復ループ) の要否を判定する。

## 3. dogfood 運用

- **観測開始日 (PR-1 merge 日)**: (merge 時に記入)
- **観測対象イベント**: auto-push 発火 (cli-pr-monitor の `[decision] gate:` ログが出た監視ターン)。
- **記録方法**: イベント発生ごとに下表へ 1 行追記する。gate の実行経路・結果は `[gate]` プレフィックスの stdout ログから転記できる。

### 観測ログ

| # | 日時 | PR | gate 経路 (docs-only skip / PASS / FAIL / disabled) | FAIL 時の原因 | 機械修復可能だったか | 人間対応 |
|---|---|---|---|---|---|---|
| - | - | - | - | - | - | - |

「機械修復可能」= sibling テストの更新忘れなど、takt fix facet が自力で直せる類の回帰。flaky・インフラ起因 (cargo/jj 環境問題等) は「不可」に分類する。

## 4. B1-loop 要否の判断基準 (GO/NO-GO)

- **GO (B1-loop を実装する)**: 判定期限までに gate FAIL が **2 件以上**発生し、かつその**過半が機械修復可能**な類だった場合。人間 escalation コストが実在し、ループで回収できると判断する。
- **NO-GO (B1-loop を見送る)**: gate FAIL が 0-1 件、または FAIL の主因が flaky・インフラ起因 (人間判断が必要な類) だった場合。ループの複雑さ (専用 workflow + 反復制御) に見合わないため、B1 の即 escalation 運用を恒久化する。

## 5. B1-loop 設計案 (GO 判定時の実装用に保存)

PR #224 セッション合意 (2026-06-29) と PR-1 計画時 (2026-07-03) の設計判断を保存する。時間を空けて着手しても再導出が不要なように、不採用案とその理由も残す。

- **差し戻し方式**: 専用の最小 takt workflow `gate-fix.yaml` を新設する (fix step → convergence_verdict routing → COMPLETE / supervise)。gate 失敗出力 (失敗コマンド + 出力 tail) を `.takt/gate-failure.txt` 等に書き出して fix step の入力にする。
  - **不採用案**: 既存 post-pr-review workflow に合成 findings を流す方式。`analyze-coderabbit` は CodeRabbit 入力前提の適合性フィルタ (ADR-035 docs-only filter 含む) を通すため、gate 失敗が誤って `not_applicable` 化されるリスクがある (PR #227 で docs-only 誤判定の実観測もある)。
  - fix facet は共有 (ADR-020) のため、A1 の `--ignored` 条件付き必須がループ内でも効き、収束が速くなる。
- **ループ制御**: monitor プロセス内で同期実行。`gate FAIL → gate-fix takt → gate 再実行` を最大 **N=2 回** (post-pr-review.yaml の `loop_monitors threshold: 2`、`rate_limit.max_retries=3` と整合)。config key は `[fix.gate] max_retries = 2` を想定。
- **空振り検知**: 差し戻し後に takt が実質変更なし (既存 `decide_repush` 部品 = pre/post commit id + diff 空判定を再利用) なら即 escalation (無限ループ・空打ち防止)。
- **上限到達**: N 回超過で `action_required` (fail-closed)。push は絶対にしない。
- **時間予算**: gate ~5 分 + fix 数分 × 2 回で最悪 ~20-30 分。`monitor.max_duration_secs` との関係を実装時に明記する。

## 6. 判定結果の反映先 (retirement checklist)

判定確定後、以下を**同一 commit** で実施する (todo エントリ削除時の land 確認手順に従う):

1. **GO の場合**: PR-2 (B1-loop) を §5 の設計案で実装。land と同時に本ファイル削除 + `docs/todo13.md` の該当エントリ削除 + `docs/todo-summary.md` の該当行削除。
2. **NO-GO の場合**: ADR-043 (Security/Quality Gate Fail-Closed) に「auto-push gate は即 escalation 運用を恒久化 (dogfood 観測結果 N 件を根拠)」の amendment を追加する docs-only PR を作成。同 PR で本ファイル削除 + todo エントリ削除 + summary 行削除。
3. いずれの場合も、観測ログから得た知見のうち永続価値のあるもの (例: gate FAIL の頻度・原因分類) は amendment / PR body に移管してから削除する (①永続先作成 → ②参照付替 → ③参照元削除 の順)。
