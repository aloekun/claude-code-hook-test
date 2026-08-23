# 不具合修正バックログ消化計画 — 第 2 バッチ

> **状態**: 第 1 バッチ (PR A〜L、12 本) はすべてマージ済み。**第 2 バッチ PR M〜S (7 本) は起票済みで、7 本とも未着手**。加えて第 1 バッチの実走観測が 2 件残っている。
>
> **本ファイルは ephemeral な作業計画書**である。第 2 バッチのマージと後始末が済み、下記の観測も済んだら**本ファイル自身を削除する** (→ [§ 退役手順](#退役手順))。
>
> **第 1 バッチの経緯と知見の行き先** (2026-08-22 に縮小):
>
> - 12 PR の実施内容・設計判断は各 PR 本文と、各 PR が書いた module doc / ADR にある
> - 実施中に得た再発防止の知見 (台帳の記述を実測で確かめる / 複数ソースの一致は証拠にならない / シグネチャ変更時の下流追跡 / 照合キーの一意性 / doc の機構の実在確認) は [ADR-075](adr/adr-075-verify-premises-before-acting.md) へ移送済み
> - 作業手順の習慣 (PR 作成前の承認 / `--body-file` / `Set-Content` を使わない / `jj squash -u` / ビルドの成否は mtime で判断しない) は Claude の memory に記録済み
>
> **第 1 バッチについて本ファイルにしか無い記録は残っていない。** 削除して失われるのは、第 2 バッチの進行と、観測待ちタスクの追跡だけである。

## 第 2 バッチ 進行表 (2026-08-22 起票)

**選定基準は第 1 バッチと同じ** — (1) [docs/claude-code-web-tasks.md](claude-code-web-tasks.md) (夜間ループ台帳) に**未掲載**で、(2) **実観測された不具合の修正**にあたるもの。

**選定の経緯**: [todo-summary.md](todo-summary.md) / [todo-summary2.md](todo-summary2.md) の全 255 行から台帳掲載の 34 順位を機械的に除外し、**第 1 バッチ作成 (2026-08-17) 以降に起票された分**を対象に選別した (それ以前の順位は第 1 バッチ作成時に一度選別済みのため再検討しない)。対象は順位 468 / 470-485 の 16 件 + [todo25.md](todo25.md) の未採番 5 件。このうち予防 lint (470 / 474 / 482 / 483)、テスト追加 (471 / 478 / 485)、doc・再評価 (472 / 473 / 479)、docs サイズ是正 2 件は基準 (2) を満たさないため除外した。境界線上の 480 (観測性の改善) と 475 (発現経路が未確認) も今回は見送る。

| # | PR | 対象順位 | 状態 |
|---|---|---|---|
| Q | `fix(merge-pipeline): takt run の終了理由を記録する` | 468 | 未着手 |
| N | `fix(subprocess): 正常終了経路の join を上限付きにする` | 481 | 未着手 |
| M | `fix(hooks): 判定層の誤検知と見逃しを塞ぐ` | 476 + 477 | 未着手 |
| P | `fix(mtime): jj materialize で壊れる mtime 依存判定を是正する` | 488 (新規採番) | 未着手 |
| O | `fix(push-runner): bookmark_check の非空不変条件を seal する` | 484 | 未着手 |
| R | `feat(ledger): auto lane の対象ファイルが Guard 禁止パスに当たる行を弾く` | 486 | 未着手 |
| S | `fix(nightly-todo): master 参照を SHA で pin する` | 487 | 未着手 |

消化順は **Q → N → M → P → O → R → S** (表の順)。R / S は 2026-08-22 の夜間ループ停止調査から追加した。

**Q を先頭に置く理由**: Q は完了基準に実走観測を含み、発火率が 142 run 中 2 件 (1.4%) と低い。マージが遅いほど観測完了も遅れるため、観測窓を先に開ける。第 1 バッチの順位 431 が「能動的に起こせず待つしかない」状態で止まっている前例に倣った判断である。

**PR をスタックしない** — 各 PR は前の PR がマージされてから master 起点で作る。第 1 バッチでこの規則を置いた直接の理由 (順位 376 の bookmark 自動前進がスタック境界を壊す) は PR H で解消済みだが、push-runner のレビュー範囲が `<base>..@` の PR 全体である性質は変わらないため踏襲する。

**共通の運用ルールと着手前の心構えは [ADR-075](adr/adr-075-verify-premises-before-acting.md) にある。** 第 1 バッチでは 9 件中 7 件で台帳と実態がずれていた。第 2 バッチでも着手前に必ず実測で前提を確かめる。

### PR Q — takt run の終了理由を記録する (順位 468)

**単独にした理由**: 対象は [`src/cli-merge-pipeline/src/feedback/takt.rs`](../src/cli-merge-pipeline/src/feedback/takt.rs) の `run_takt_workflow` に閉じる。**この関数は `lib-subprocess` を経由せず生の `Command` + `Stdio::inherit()` を使っている** (2026-08-22 実装確認) ため、同じ「subprocess 実行経路の欠陥」でも PR N と実装面の接点が無い。

**実観測**: `.takt/runs` の post-merge-feedback run 全 142 件の走査で、2 件が analyze 起動から 34 秒以内に成果物ゼロで死亡。どちらも再実行は正常完走したため transient な起動失敗と見られるが、`status.success()` を bool に潰していて終了コード / シグナル / stderr がどこにも残らず、**なぜ止まったかを示す記録が無い**。

**完了基準の要点**: まず観測を足す (終了コード / シグナル / stderr の保存)。原因の特定と修正は観測結果を見てから判断する。**完了基準に実走観測を含む** → [§ 残観測トラッキング](#残観測トラッキング) へ追記して追う。

**後始末**: **観測後**に todo24.md の 468 節 + todo-summary2.md の 468 行を削除。

### PR N — 正常終了経路の join を上限付きにする (順位 481)

**単独にした理由**: 対象は [`src/lib-subprocess/src/lib.rs`](../src/lib-subprocess/src/lib.rs) の `run_cmd_shell_with` 内部に閉じる。

**実観測**: PR K ([#436](https://github.com/aloekun/claude-code-hook-test/pull/436)、順位 323) が塞いだ Major と**同型のギャップが実コードに現存**する。失敗経路 (timeout / wait 失敗) は `kill_process_tree` + `join_within_grace` で上限付きに回収するが、正常終了経路 (`Some(_)` 分岐) の reader thread join だけが素の `.join()` のまま残っている。

**完了基準の要点**: 子孫がパイプを握ったまま子が正常終了しても上限時間内に制御が戻ることを、**経過時間 assert 付きの決定論的テスト**で固定する。上限を外す変異でそのテストが落ちること。「理由を doc に書いて無制限のまま残す」は選択肢にしない (PR #439 CodeRabbit Major)。

**後始末**: todo25.md の 481 節 + todo-summary2.md の 481 行を削除 (実装と同じ PR に含める)。

### PR M — 判定層の誤検知と見逃しを塞ぐ (順位 476 + 477)

**束ねた理由**: crate は別だが**同一の障害クラス**である。どちらも Claude Code hook の判定関数で、476 は誤検知 (commit message 内の文言を実行と誤認)、477 は見逃し (複数行のインライン実行が silent no-op になるのを通す)。対処も完了基準も同型で、「判定を pure function に切り出す → 3 方向の unit test → 変異テストで検知を確認」がそのまま両方に当てはまる。Tier 1 が 2 件、いずれも本セッションでの実観測。

**実観測**: 476 は PR #429〜#432 で **4 回**発生し、毎回 `jj op log` での手動確認を強いられた。477 は PR #428 / #432 で **2 回**踏み、いずれも「修正を適用したつもりが実際には未適用のまま検証していた」状態を作った。

**対象**: [`src/hooks-post-tool-jj-op-verify/src/main.rs`](../src/hooks-post-tool-jj-op-verify/src/main.rs) の `detect_last_mutating_jj_op` を quote-aware にする。[`src/hooks-pre-tool-validate/`](../src/hooks-pre-tool-validate/) に「複数行を含む `-e` / `-c` インライン実行」の検出を追加してブロックする。

**前提**: 台帳の順位 283 の lane 操作が要る → [§ 着手前に片付ける 3 件](#着手前に片付ける-3-件)。

**後始末**: todo24.md の 476 / 477 節 + todo-summary2.md の 476 / 477 行を削除。あわせて台帳から 283 の行を削除する (476 へ統合するため)。

### PR P — jj materialize で壊れる mtime 依存判定を是正する (順位 488、新規採番)

**束ねた理由**: crate は別だが、**根因が「jj が working copy を materialize すると全ファイルの mtime が checkout 時刻へ書き換わる」の 1 つ**である。[todo25.md](todo25.md) のエントリ自身が「2 件は同一の根因クラスなので 1 タスクとして扱う」と宣言している。片方だけ直すと同じ罠がもう片方に残る。

**実観測**: 週次レビュー 2026-08-22 の WR-2026-08-22-J01 / J02 (severity=high、facet=jj-robustness、category=jj-mtime-staleness)。

**対象**: [`src/hooks-session-start/src/jj_helpers.rs`](../src/hooks-session-start/src/jj_helpers.rs) の `fetch_head_is_recent` (`.git/FETCH_HEAD` の mtime を「最後に fetch した時刻」として扱う) と、[`src/cli-pr-monitor/src/lock.rs`](../src/cli-pr-monitor/src/lock.rs) の `holder_still_writing` (空ロックファイルの「書き込み中」判定に mtime を使い、クラッシュ後に残った古い空ロックを「たった今作成された」と誤認する)。

**前提**: 順位の採番が要る → [§ 着手前に片付ける 3 件](#着手前に片付ける-3-件)。

**後始末**: todo25.md の該当節 + todo-summary2.md の 488 行を削除。

### PR O — bookmark_check の非空不変条件を seal する (順位 484)

**単独にした理由**: 対象は [`src/cli-push-runner/src/stages/push.rs`](../src/cli-push-runner/src/stages/push.rs) と [`src/cli-push-runner/src/stages/bookmark_check.rs`](../src/cli-push-runner/src/stages/bookmark_check.rs) に閉じる。

**実観測**: PR I ([#434](https://github.com/aloekun/claude-code-hook-test/pull/434)、順位 288(b)) の incident の**直接の根因** — fail-closed の判定結果 (空リスト) が上流の fallback logic に無視される execution-contract 違反。修正はしたが、`run_bookmark_check()` が `Some` を返すとき必ず 1 件以上、という不変条件が**テストで固定されていない**ため、将来 `Some(空)` を返す経路が復活しても気づけない。

**完了基準の要点**: `Some(空)` を返す変異を入れるといずれかのテストが落ちること。**非空を型で表現できるなら型を優先する** (非空 Vec 型にすればテスト無しで保証できる)。

**後始末**: todo25.md の 484 節 + todo-summary2.md の 484 行を削除。


### PR R — auto lane の対象ファイルが Guard 禁止パスに当たる行を弾く (順位 486)

**単独にした理由**: 台帳の選別層 (`lib-ledger` / `cli-ledger-candidates`) に閉じる。PR S と同じ調査由来だが、S は workflow の checkout 引数の話で実装面の接点が無い。

**実観測**: 2026-08-20 の run 87837551740 が順位 383 を選び、agent が `src/lib-ledger/src/lib.rs` を変更して `[NIGHTLY_DENY] 自律動作のガードレールを変更しているため push しません` で停止した。383 は**構造的に完了不能**である (実装すれば Guard が拒否、実装しなければ「変更がありません」で落ちる)。auto lane 22 行を deny リストと全件照合したところ **5 行が該当** (383 / 454 / 368 / 360 / 361)。383 は発火済み、残り 4 件は未発火。

**完了基準の要点**: Guard 禁止パスを成果物とする行に `✅` を付けると、**台帳を書き換えた時点で**決定論的に検出されること。[ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 6 がこの検査を「決定論だが未実装」と自認しており、今回その穴が発火した。

**注意**: 実装先が deny リスト配下にあるため、**本タスク自身を auto lane に載せてはいけない**。この規則の最初の適用対象が本タスク自身である。

**前提**: 台帳 lane の引き取りが要る → [§ 着手前に片付ける 3 件](#着手前に片付ける-3-件)。

**後始末**: todo25.md の 486 節 + todo-summary2.md の 486 行を削除。

### PR S — master 参照を SHA で pin する (順位 487)

**単独にした理由**: [`.github/workflows/nightly-todo.yml`](../.github/workflows/nightly-todo.yml) の checkout 引数に閉じる。

**実観測**: 2026-08-21 の run 88134039080 で `master-ref` (18:08:28Z、`7539551f`) と `work` (18:08:59Z、`868c9316`) が**別コミット**を読んだ。その 31 秒の間に順位 228 の実装 PR #422 がマージされ、選択は「228 未実装」の古い台帳から、実装は「228 実装済み」の新しい master で行われて変更 0 件になった。除外リストの推移も裏づけている (8/20 は `[228,324]`、8/21 は `[324,383]`)。

**完了基準の要点**: 台帳を読む step と agent が触る作業ツリーが**同一 SHA を見ている**ことがログから確認できること。**pin の向きは「work を master-ref に合わせる」** — [ADR-072](adr/adr-072-nightly-todo-loop.md) § 信頼境界の要が master-ref を正と定めているため。

**注意**: `.github/workflows/` は deny リスト該当。**本タスクも auto lane に載せてはいけない**。

**後始末**: todo25.md の 487 節 + todo-summary2.md の 487 行を削除。

## 着手前に片付ける 3 件

1 と 2 は該当 PR に同乗させる (単独の docs PR は立てない)。3 は即時の運用対処で、PR を待たない。

1. **順位 283 を auto lane から引き取る (`✅` → `—`)** — **PR M の前提**。順位 476 は台帳の順位 283 (`fix(jj-op-verify): verb 検出をコマンド境界に anchor する`) と同一内容で、`✅` のままだと夜間ループが同じファイルを並行実装する (順位 474 がまさに検知しようとしている競合)。476 側に実観測 4 回と quote-aware の具体的な設計案があるので、**283 を取り下げて 476 へ統合**し、台帳の lane 操作を PR M に含める。着手前に `claude/nightly-283` ブランチが存在しないことも確認する。
2. **todo25.md の未採番 5 件を順位 488-492 として起票** — **PR P の前提**。todo25.md は詳細エントリ 10 節に対し summary 行が 481-485 の 5 行しかなく、週次レビュー採用 3 件と docs サイズ是正 2 件が [ADR-033](adr/adr-033-todo-numbering-simplification.md) の言う「絶対番号は table のみに保持」を満たしていない。PR P が対象順位を書けないので、**5 件まとめての採番を PR P に同乗**させる (1 件だけ採番すると 1:1 対応のずれが残るため)。なお、このずれは順位 441「詳細エントリ ⇄ 台帳行の 1:1 対応検査」が検出すべきものである。
3. **台帳の lane 引き取りと handoff marker の後始末** — **即時の運用対処** (PR R の前提でもある)。夜間ループは毎晩 agent を 1 回まるごと走らせて最後に落ちる状態なので、PR を待たずに台帳を直す。
   - 順位 **383 / 454 / 368 / 360 / 361** の `無人可` を `✅` → `—` へ変更する (5 件とも成果物が Guard 禁止パス配下で、auto lane では完了不能)
   - `claude/nightly-383` (2026-08-20 の handoff marker、`bf206535` を指す空 ref) を削除する
   - `claude/nightly-228` は**削除しない** — 順位 467 D-1 の観測に使っている ([§ 残観測トラッキング](#残観測トラッキング))。なお現存するのは 2026-08-21 の handoff が作り直した空 marker (`868c9316` を指す) であり、PR #422 のブランチそのものではない。`cli-stale-branch-scan --deletable-only` が列挙することの確認は作り直し後 (2026-08-22) に行っているため観測は成立している

## 残観測トラッキング

完了基準に実走観測を含むタスク。観測できたらエントリ後始末 (todoN.md 節 + summary 行の削除) を docs バッチで行う。

下記 2 件は**第 1 バッチ (PR A〜L) 由来**である。第 2 バッチでは **PR Q (順位 468)** が観測待ちになるため、マージ時に本節へ 3 件目として追記する。

- [ ] **431** (PR E で実装、[PR #428](https://github.com/aloekun/claude-code-hook-test/pull/428))
  - **観測すること**: 次にレート制限が起きた夜間 run で「未レビュー」が可視化されること (rate-limit 拒否を red で落とす方式に変更済み)
  - **現状 (2026-08-22)**: **観測できていない** — #428 マージ後の `review-request` workflow は全 run が `skipped` で、CodeRabbit のレート制限が一度も発生していない。**能動的に起こせないため待つしかない**
  - **後始末**: todo22.md の 431 節 (`review-request` の成功判定…) + todo-summary2.md の 431 行を削除
- [ ] **467** (PR L で実装、[PR #437](https://github.com/aloekun/claude-code-hook-test/pull/437))
  - **F-2 は観測完了 (2026-08-22)**: 実 run の前後比較で `GIT_DIR 導出失敗` が 1 件 → 0 件 (2026-08-20 の run 32401510711 vs 2026-08-21 の run 32511788731)
  - **D-1 が未観測**: 掃除ループが「既に消えたブランチ」で job を落とさないこと + lease による compare-and-delete が働くこと。掃除対象が 1 件以上ある run が過去 40 回で 0 件だったため一度も実行されていない
  - **条件は整った**: [PR #422](https://github.com/aloekun/claude-code-hook-test/pull/422) のマージで `claude/nightly-228` が決着済み PR のブランチになり、`cli-stale-branch-scan --deletable-only` が対象として列挙することを確認済み。次の定時 run (毎日 18:00 UTC) で発火する見込み
  - **注意**: この観測のために `claude/nightly-228` を**意図的に残している**。週次レビューの残存ブランチ scan が削除候補として挙げるが、D-1 の観測が済むまで削除しないこと
  - **後始末**: **D-1 の観測後**に todo24.md の 467 節 + todo-summary2.md の 467 行を削除

## 退役手順

1. [§ 第 2 バッチ 進行表](#第-2-バッチ-進行表-2026-08-22-起票) の PR M〜S (7 本) がすべてマージされ、対応するエントリ後始末が完了していること
2. [§ 残観測トラッキング](#残観測トラッキング) の項目 (第 1 バッチの 431 / 467 + PR Q 由来の 468) がすべて観測され、対応するエントリ後始末が完了していること
3. `grep -rn --exclude=bugfix-batch-plan.md "bugfix-batch-plan" docs/ src/ .github/ CLAUDE.md` で**本ファイル以外**からの参照が残っていないことを確認する
   - **`--exclude` と検索対象パスの両方が要る。** 検索対象を `.` にすると `.takt/runs/` や `.claude/feedback-reports/` の過去ログが数百件当たって信号が埋もれ、`--exclude` が無いと**この手順の行自身**が当たって参照ゼロに決してならない (2026-08-23 に両方を実測)
   - 検索対象パスを省くと標準入力待ちになるため必ず付ける
4. 本ファイルを物理削除する (削除自体は残観測の最後のエントリ後始末と同じ docs バッチ PR に同乗してよい)

> **既に充足済みの条件** (2026-08-22 時点): 第 1 バッチ 12 PR のマージ / 順位 288 のエントリ削除 / 保留事項の消化 / 第 1 バッチの知見の本ファイル外への移送。
>
> **本ファイルが唯一の記録である項目を、本ファイルの削除と一緒に消してはならない。** 順位 469 のエントリ削除で実際にこれをやり、記録を一度失った。上記の移送はその再発を防ぐために行った。**第 2 バッチの選定経緯 (何を除外し、なぜ除外したか) は現時点で本ファイルにしか無い** — 退役時は除外理由の行き先を先に決めること。
