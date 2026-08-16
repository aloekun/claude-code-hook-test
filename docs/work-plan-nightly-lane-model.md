# 作業計画書: 夜間 todo ループの lane モデル移行 (一時文書)

> **本ファイルは一時文書である。** 下記「完了チェックリスト」の全項目が完了した時点で、**本ファイル自身を削除する**こと (最後の実装 PR に同梱してよい)。恒久的な決定はすべて ADR / 台帳 / dev-conventions に記録してから消す — 本ファイルにしか書かれていない決定を残したまま削除してはならない。
>
> **読者**: 実作業を行う Claude (Opus)。本ファイルだけで作業できるように書いてある。前提知識は [ADR-072](adr/adr-072-nightly-todo-loop.md) (夜間ループ)、[ADR-031](adr/adr-031-weekly-review-pipeline.md) (週次レビュー)、[docs/claude-code-web-tasks.md](claude-code-web-tasks.md) (台帳)。
>
> **作成**: 2026-08-16。2026-08-15 の週次レビュー実行で発覚した問題群 (下記 § 背景) に対し、ユーザーが方針を確定した。

---

## 背景 — なぜこの作業をするか

### 発覚した問題 (2026-08-15〜16 のセッションで実測により確定)

1. **週次レビューの昇格検査が 2 週連続で機能しなかった**。`review-todo-whole` facet (haiku) に 251 件の全件判定を指示文だけで強制していたが、13 件しか判定せず「候補 0 件」と報告 (前週は 164 件中約 50 件サンプリング)。instruction は完全に届いていた (実行ログで確認、20,145 字) が無視された。instruction 再強化 (#399/#400) は同じ層への対処で 2 回とも失敗。
2. **facet が台帳を誤読** (順位 284 の 無人可 を `✅` と報告、現物は `—`)。
3. **facet の出力言語が不定** (2026-08-15 の run で `review-todo-whole` がほぼ全文ハングル、他 4 facet は英語、日本語ゼロ)。原因は言語指定の不在 (`.takt/config.yaml` が無く en builtin にフォールバック。instruction にも contract にも言語指定なし)。
4. **台帳の条件 3 (重複の恐れがない) と ADR-072 決定 3 が同じ状態を逆に解釈**。決定 3 は `claude/nightly-<順位>` ブランチを「着手済みマーカー」として扱い再選択を防ぐが、条件 3 は同じブランチを「無人可マークの無効化理由」として降格を要求する。この矛盾から誤った週次 finding (WR-2026-08-13-T01/T02) が採用された。T01 は台帳の明文規定 (「削除するのはマージした順位だけ」) に矛盾し、実行すると未完了タスク 3 件が台帳から消える。
5. **人間が台帳タスクを手で実装・マージした場合の後始末が漏れる** (実績 4 件中 2 件失敗)。台帳の行が残ると夜間ループが再実装しうる。
6. **verify 段で失敗した run はブランチも PR も残さないため、翌晩同じタスクが再選択される** (先頭独占ループ)。

### ユーザーが確定した設計方針 — lane モデル (担当権管理)

問題の根本は「タスク割り振り」の問題を「分散システムの状態管理」として解いていたこと。GitHub 上の副作用 (ブランチ・PR・マージ履歴) から担当と進捗を推測するのをやめ、**台帳が担当を直接表現する**:

```text
台帳 = 担当割り当て表
  無人可列 ✅ = auto  (夜間ループに割り当て)
  無人可列 —  = human (ユーザー + Claude Code に割り当て)
  ※ 台帳の表形式は変更しない。意味論の再定義のみ

ブランチ (claude/nightly-<順位>) = 作業中マーカー (完了マーカーではない)
完了 = PR に同梱された台帳削除がマージされること (実装済み: cli-ledger-cleanup --apply)
失敗 = マーカーを残して人間確認へ。再投入は人間の明示操作
```

**核心となる運用原則** (ADR 改訂に反映すること):

- 無人可 (= lane) の判断は**人間だけ**が行う。夜間 worker は auto の先頭 1 件を取って実行するだけで、「本当に簡単か」「重複しないか」を再審査しない
- **競合は検出するのではなく、競合する割り当てをしない**。人間が auto を付けたタスクは夜間ループの所有物。人間が引き取るなら lane を human に変える
- **失敗したタスクを無変更で自動再投入しない**。agent が完遂できなかったタスクは人間確認へ戻す。再投入 (lane を auto のまま維持しブランチ/マーカーを削除) は人間の意思表示
- インフラ障害 (network / gh / clone) は implement 前に red で落ちマーカーを作らない → 翌晩自然に再試行。**transient / タスク不適合の分類は run の構造で解決し、分類器は作らない**

### 確定済みの個別決定 (ユーザー承認済み、再ヒアリング不要)

| 決定 | 内容 |
|---|---|
| verify 失敗の扱い | 人間確認へ戻す (空 ref マーカー方式、下記 PR-4) |
| 台帳条件 3 | 廃止 (代替ゲート不要 — 割り当て規律で防ぐ) |
| Phase 4 展開先 | weekly-review skill を「preamble が指す現在の新規追加先 todoN.md」へ展開する形に変更 |
| facet 出力言語 | 各 output contract に**直書き 1 行** (参照形はプロンプトに載らず効かないため不採用)。由来を dev-conventions.md に記録 |
| 選出システム | **既存実装を流用** (lib-ledger / cli-nightly-task-select)。作り直さない |

### 検討して捨てた案 (negative result として ADR-072 改訂に記録すること)

| 案 | 捨てた理由 |
|---|---|
| `claude/nightly-state` ブランチ + 最終試行日順選択 | 自動再投入をやめたため試行履歴・順序変更が不要になった |
| 対象ファイル重なり走査 (他経路重複の検出ゲート) | 「競合する割り当てをしない」が原則。検出は割り当て規律の代替にならない |
| Ledger-Rank trailer の未マージブランチ走査 | trailer は任意記入で、実例 (順位 284 の `claude/select-next-task-a9aiam`) を検出できない |
| 失敗理由の分類器 | インフラ障害は構造上マーカーを作らないため、分類は run の構造で足りる |
| 台帳への選択時直 push (試行日列) | ガード対象ファイルへの自律書き込み経路の新設。ADR-070 で同型案が却下済み |
| 再挑戦上限 N 回 | 再投入が人間操作になったため上限管理が不要 |

---

## 共通ルール (本計画書の PR-1〜PR-5 に適用)

> **適用範囲は本計画書が指示する PR (人間 + Claude Code の interactive セッションが作るもの) に限る。** 夜間ループが `claude/nightly-*` から自動作成する PR は [ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md) 原則 1・2 と [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 15 に従い、**承認はマージ 1 点**である。本節の承認要件を夜間ループへ適用すると lane が止まる ([#409](https://github.com/aloekun/claude-code-hook-test/pull/409) の CodeRabbit 指摘。ADR-028 原則 1 が「interactive セッションは人間ゲート / 自律 actor は事前分類」と分けているとおり)。

- **PR 作成前にタイトル・ボディをユーザーへ提示し明示承認を得る** (ADR-028、interactive セッション限定)。push は `pnpm push` (takt レビュー経由)、PR 作成は承認後に `pnpm prepare-pr-body` → `pnpm create-pr` (複数行 body は `--body-file` 経路)
- 各 PR の完了時、対応する `docs/todo*.md` のエントリを削除する (運用ルール「完了タスクは削除」)
- docs 変更は `pnpm lint:docs` と markdownlint が clean であること。Rust 変更は `cargo test --workspace` green + 該当 crate の `cargo clippy`
- workflow (`nightly-todo.yml`) の変更は実走でしか検証できない ([dev-conventions](dev-conventions.md))。`workflow_dispatch` (可能なら dry_run) で観測してから完了と報告する
- 本ファイルの下記チェックリストは、各 PR の作業に同梱して更新してよい

---

## PR-1 (docs): lane モデルの決定化 + 撤回 + 登録整理

**注意**: 2026-08-16 時点の working copy に未コミットの docs 変更が既にある (todo24.md 新設、todo23.md への 2 件登録、todo.md preamble 更新、todo3〜11/22/23 の数詞・ポインタ整合修正)。これらは本 PR に同梱する。`jj st` で現状を確認してから着手すること。

### 1-a. ADR-072 の改訂

新しい決定 section (番号は既存の続き) として以下を記録する:

- **台帳 = 担当割り当て (lane モデル)**: 無人可列の意味論を「auto / human の担当割り当て」と再定義。無人可の判定条件から条件 3 を削除 (条件 1・2 は人間が割り当て時に使う判断基準として残る)。夜間 worker はマークの再審査をしない
- **ブランチ = 作業中マーカー**: 完了マーカーではないことを明記。完了は台帳削除の PR 同梱 (既存決定) が表現する
- **agent 実行後の停止はすべて人間確認へ**: implement 完了後に publish へ到達しなかった run (verify 失敗 / ledger-completion 未完了 / guard deny / 空 diff) は、空 ref マーカー `claude/nightly-<順位>` (base commit を指す) を残して停止する。マーカーがある間その順位は再選択されない (既存の決定 3 の除外がそのまま効く)。implement **前**の停止 (kill-switch / 背圧 / タスク無し / インフラ障害) はマーカーを作らない → 自然再試行。この境界が transient / タスク不適合の分類を構造で代替する
- **再投入は人間の明示操作**: (a) 人間が引き取る → 台帳の `✅` を `—` へ変更しマーカー/ブランチを削除、(b) 仕様を直して再投入 → `✅` のままマーカー/ブランチを削除。close した nightly PR のブランチは自動掃除 (PR-4) が削除し、lane が auto のままなら再選択される — **lane を auto のまま残す = 再投入の意思表示**
- **決着済み PR ブランチの自動掃除**: 夜間ループが起動時に、決着済み (closed / merged) の PR に紐づく `claude/nightly-*` ブランチを削除する。PR の無いブランチ (= 失敗マーカー) は対象外
- § 検討して捨てた案 (上記表) を negative result として記録
- § 残課題から「失敗した run の学習が無い」を削除 (仕様化されたため)

### 1-b. ADR-052 の追記

自律 actor の操作分類に「自ブランチ (claude/nightly-\*) の削除」「空 ref の作成 (失敗マーカー)」を追加。どちらも claude/\*\* 空間内で、PR が紐づくブランチは成果物が PR 上に残り GitHub の Restore branch で復元可能 = commitment 点の侵犯に当たらない旨を記す。

### 1-c. 台帳 (docs/claude-code-web-tasks.md) の改訂

- § 自律実行可否の 2 段階分類: 条件 3 を削除し 2 条件にする。lane モデルの意味論 (✅ = 夜間ループの所有物、人間が触るなら lane を変える) を追記
- § 夜間ループでマージしたタスクの後始末 の近くに close 時の運用を追記: 「nightly PR を close するとき、引き取るなら `✅` → `—` に変更する。`✅` のまま残す = ブランチ掃除後に自動再投入される意思表示」
- § ライフサイクル 定期更新（週次） の項 2 (昇格候補の全件判定) と項 3 (条件 3 の再確認) を lane モデルに合わせて縮小: 昇格 = 人間の割り当て判断であり、週次はその材料 (台帳未掲載の順位一覧、PR-5 で決定論化) を提供するのみ。§ 昇格検査履歴 は廃止する (表が空のまま。収束機構は不要になった旨を記して section を削除するか、廃止注記を残す — 実行者判断)

### 1-d. facet instruction の改訂 (.takt/facets/instructions/review-todo-whole.md)

- Criterion 3-3 (無人可マークの条件 3 再検査 = 未マージブランチ/PR の走査) を撤去
- Criterion 3-2 (昇格候補の全件判定 + 検査履歴記帳の義務) を縮小: 全件判定・両経路理由・記帳の義務を撤去し、「台帳未掲載の順位の存在を件数レベルで報告する (判定はしない。割り当ては人間の判断)」程度に留める。PR-5 で決定論出力に置き換わる予定と注記

### 1-e. 誤採用 finding の撤回

[docs/todo.md](todo.md) § 週次レビュー採用 (2026-08-13) から以下 2 エントリを削除する:

- 「台帳の ✅無人可 5 行を condition 3 違反により — へ降格 (WR-2026-08-13-T02)」— 前提消滅 (対象ブランチは 2026-08-15 に削除済み、順位 216/239 は完了済み) + 条件 3 自体が本 PR で廃止
- 「台帳 Batch 1 の closed-without-merge 行を棚卸し履歴へ移動し、in-flight を明示 (WR-2026-08-13-T01)」— 台帳の明文規定「削除するのはマージした順位だけ」と矛盾。実行すると未完了 3 タスクが台帳から消える

撤回の理由を PR description に記す (採用済み finding を実行せず消したことが後から追える必要がある)。

### 1-f. todo エントリの整理

- [docs/todo24.md](todo24.md) の「週次レビューが誤って採用した台帳 finding 2 件を撤回する」→ 本 PR で実施するため削除
- [docs/todo24.md](todo24.md) の「review-todo-whole facet の台帳読み取りが現物と食い違う」→ lane モデルで Criterion 3 が縮小されるため、「PR-5 の facet 改訂後に残る報告範囲を見て要否を再判定する」旨に rescope
- [docs/todo23.md](todo23.md) の「昇格候補集合の構築を決定論層へ移す」→ rescope: LLM 全件判定・収束機構 (昇格検査履歴) は廃止。残るのは「台帳未掲載の順位一覧を決定論的に出す」のみ (PR-5)
- 夜間ループ改善 (PR-3/PR-4 相当) のタスクエントリは登録しない — 本計画書が直接の作業指示となるため二重管理しない

### PR-1 完了基準

- ADR-072 / ADR-052 / 台帳 / facet instruction / todo.md が上記どおり改訂され、`pnpm lint:docs` + markdownlint clean
- 本計画書 (このファイル) が同 PR に含まれてリポジトリに入る

---

## PR-2 (takt): facet 出力言語の直書き

### 作業内容

- `.takt/facets/instructions/` 配下の全 instruction (19 ファイル) の Output contract 節 (無ければ末尾) に 1 行追加: 「**レポート本文は日本語で書く。コード識別子・ファイルパス・ADR 番号・コマンドは原文のまま。**」
- `aggregate-weekly` は findings.json の `description` / `proposal` も日本語とする旨を明記 (skill が todo へ展開する際の翻訳工程が減る)
- [docs/dev-conventions.md](dev-conventions.md) に規約の由来を 1 箇所記録: 「takt facet の出力言語は各 instruction に直書きする (2026-08-15 の run で言語指定不在によりハングル出力が発生。参照形はプロンプトに載らず効かないため直書き。変更時は grep で全箇所を更新する)」

### PR-2 完了基準

- 全 instruction に言語指定行がある (`grep -L "日本語" .takt/facets/instructions/*.md` が空)
- [docs/todo23.md](todo23.md) の「weekly-review facet の出力言語を output contract に明記する」エントリを削除
- マージ後、次回 weekly-review 実走で全レポートが日本語であることを確認 (完了チェックリストの実走確認項目)

---

## PR-3 (実装・小): 順位 table 存在照合ゲート

**目的**: 人間が台帳タスクを手で実装・マージし後始末が漏れた場合 (実績 4 件中 2 件)、台帳に残った行を夜間ループが再実装するのを防ぐ。着手フローでは完了時に `docs/todo-summary*.md` の順位行が削除されるため、**順位 table からの消失 = 完了済み (または取り下げ) の機械的シグナル**になる。

### 作業内容

- `src/lib-ledger/` に「指定順位が summary markdown 群のいずれかの順位 table に存在するか」を返す関数を追加。既存の `removal.rs` (remove_summary_row) が summary table のパースを既に行っているので流用する。セル形 `| <順位> |` で照合 (裸の数字は行数等に誤マッチする — 既存実装の照合方針に合わせる)
- `cli-nightly-task-select` に summary ファイルパスを渡す引数を追加 (**省略不可**。フラグ欠落 = 数えられなかった、は exit 2。決定 3 の `--exclude-ranks` と同じ設計)
- 選択ループでの扱い: 候補順位が summary に**無い**場合、その順位を**選択せずスキップし、警告を stdout に出す** (「順位 N は台帳に残っているが順位 table に無い。後始末漏れか再採番。台帳の行を人間が確認すること」)。他の候補があれば続行する。summary ファイルが**読めない**場合は exit 2 (fail-closed、ADR-072 決定 2「曖昧さはすべて停止側へ」)
- `nightly-todo.yml` の Select step に `master-ref/docs/todo-summary.md` と `master-ref/docs/todo-summary2.md` を渡す配線を追加
- unit test: 存在する / 両ファイルに無い / ファイル不読 / 順位 table 以外の表 (棚卸し履歴等) にだけ数字がある場合に誤検知しない

### 留意点

- 順位の**再採番**でも同じ警告が出る (台帳が旧番号を指したまま)。誤検知ではなく台帳の staleness なので、警告文言はその可能性に触れること

### PR-3 完了基準

- `cargo test --workspace` green、`workflow_dispatch` (dry_run) で警告経路とスキップ経路が動くことを観測

---

## PR-4 (実装・workflow): ブランチのライフサイクル 2 種

**対象ファイル**: [.github/workflows/nightly-todo.yml](../.github/workflows/nightly-todo.yml)、必要なら `src/cli-stale-branch-scan/`

### 4-a. 失敗マーカー (agent 実行後の停止 → 人間確認)

- implement 完了後に publish へ到達しない停止 (verify 失敗 / ledger-completion 未完了 / guard deny / 空 diff) が起きたら、**空 ref `claude/nightly-<順位>` を base commit (agent 起動前に記録済みの `git -C work rev-parse HEAD`) に作成**する。実装は `gh api -X POST /repos/{owner}/{repo}/git/refs` 1 回 (App token 使用。コードは push しない)
- run の色: 決定 10 の分類に従い green のまま、ただし `[NIGHTLY_SKIP]` とは**別のマーカー** (例: `[NIGHTLY_HANDOFF]`) で Report outcome に出す。報告には順位と次の操作 (「引き取る → 台帳 ✅→— + ブランチ削除 / 再投入 → ブランチ削除のみ」) を含める
- マーカーがある限り決定 3 の除外 (`git ls-remote` による `claude/nightly-<順位>` の存在確認) がそのまま効くため、**selector 側の変更は不要**

### 4-b. 決着済み PR ブランチの自動掃除

- タスク選択の**前**に、`claude/nightly-*` ブランチのうち「紐づく PR がすべて決着済み (**closed または merged**)」のものを削除する step を追加
- **merged を除外しない** (2026-08-16 修正、[#409](https://github.com/aloekun/claude-code-hook-test/pull/409) の CodeRabbit 指摘)。初版は「merged はブランチ削除済みが通常」として対象外にしていたが、マージ時のブランチ消し忘れと台帳行の残存が重なると、その順位が決定 3 の除外に永久に掛かり続ける
- **PR の無いブランチは削除しない** (失敗マーカーを消さないため)。**境界は「PR があるか」の 1 点だけ**にする。この判定は `cli-stale-branch-scan` が既に持っている (「PR が 1 件も無いブランチは提案対象外」)。同 crate に機械可読出力 (削除可能ブランチを 1 行 1 件、`claude/nightly-*` に限定するフィルタ付き) を追加して workflow から消費するのを推奨 — shell での PR 状態パースは回帰テストの場が無い (ADR-072 決定 1 の rationale と同じ)
- **回帰テスト**: 「merged だがブランチが残っている」ケースが削除対象になること、「PR が 1 件も無いブランチ」が対象外のままであることの 2 件を unit test で固定する
- 削除は App token で `git push origin --delete` (job の `GITHUB_TOKEN` は read-only のため不可)

### 4-c. App token の発行タイミング

- 現在は publish 直前に 1 回 mint (寿命 1h、agent 最大 60 ターンのため)。掃除 (選択前) と失敗マーカー (verify 後) にも token が要る
- 推奨: **掃除用に job 冒頭で 1 回、publish/マーカー用に implement 後で 1 回の計 2 回 mint**。失敗マーカーの作成は publish 用 mint の位置 (verify の直後) で間に合う

### 4-d. PR body への close 時案内

- nightly PR の body テンプレートに 1 行追加: 「close する場合: 引き取るなら台帳の `✅` を `—` へ。`✅` のまま close するとブランチ掃除後に自動で再投入されます」

### PR-4 完了基準

- `workflow_dispatch` で (a) 掃除 step が対象なし時に無害に通過、(b) 失敗マーカー経路 (可能なら意図的に verify を落とす dry-run 相当) の動作を観測。フル経路の確認は次回 schedule 実走に委ねてよい (完了チェックリストに実走確認項目あり)

---

## PR-5 (実装・rescope 済み): 週次レビューの割り当て補助

**旧計画 (LLM 全件判定 + 収束機構) は廃止済み。** 残る作業は小さい。

### 作業内容

- 決定論 exe: `docs/todo-summary.md` + `docs/todo-summary2.md` の全順位から台帳の現行タスク表に載っている順位を引いた**差集合 (台帳未掲載の順位一覧)** を出力する。`lib-ledger` の既存パーサ + PR-3 で足した summary パーサを流用。出力は markdown (順位・Tier・タイトル程度)
- 配置: weekly-review workflow の純機械 step (file-length-watchlist / workspace-hygiene-scan と同型、LLM 判断ゼロ) を推奨。skill 決定論層 (stale-branch-scan 方式) でも可 — ネットワーク不要なので workflow 内で成立する
- 用途は**人間の割り当て判断の材料**。skill はこの一覧を提示するだけで、判定も記帳もしない (lane の付与は人間)
- facet `review-todo-whole` の Criterion 3-2 相当をこの出力への参照に置き換える
- 完了後、[docs/todo23.md](todo23.md) の rescope 済みエントリ (昇格候補集合の決定論化) を削除。[docs/todo24.md](todo24.md) の構造化データ entry は、この時点の facet の報告範囲を見て要否を再判定し、不要なら理由を付して削除

### PR-5 完了基準

- `cargo test --workspace` green。次回 weekly-review 実走で一覧が出力されることを確認

---

## skill リポ作業 (本リポジトリ外)

weekly-review skill (`~/.claude/skills/weekly-review/` — 実体は `$CLAUDE_SKILLS_REPO` で管理) に対して:

1. **Phase 4 の展開先変更**: 採用 findings の展開先を `docs/todo.md` 固定から「`docs/todo.md` preamble が指す**現在の新規追加先** (2026-08-16 時点では todo24.md)」に変更する。todo.md は 47.8KB で編集専用と定義されており、固定のままだと次回採用時に 50KB ゲートが skill 実行中に発火する。展開先ファイルが 50KB 超過している場合の挙動 (次ファイル新設 or ユーザーへ報告) も定める
2. **台帳昇格フローの縮小**: Phase 3 の「台帳昇格候補の扱い (必須ステップ)」と Phase 4 の「§ 昇格検査履歴へ記帳する (必須ステップ)」を lane モデルに合わせて縮小 — PR-5 の決定論一覧を提示し、台帳への行追加はユーザーが承認した場合のみ (無人可=`—` 固定は維持)。全件判定の収支検査・両経路理由・記帳の手順を削除
3. 反映後 `/skill-sync-check` で同期状態を確認

---

## 実施順序と依存

```text
PR-1 (docs) → PR-2 (言語) → PR-3 / PR-4 (相互に独立、並行可) → PR-5
skill リポ作業は PR-1 マージ後いつでも (PR-5 と独立)
```

- PR-1 が全決定の記録なので必ず先頭
- PR-3 と PR-4 に順序依存はない (旧計画の state ブランチ依存は消滅)
- PR-5 は PR-3 の summary パーサを流用するため PR-3 の後が楽

---

## 完了チェックリスト

作業の進捗に応じて本節を更新すること (各 PR に同梱してよい)。

- [x] PR-1: docs (ADR-072/052 改訂、台帳条件 3 廃止、facet Criterion 3-2/3-3 縮小、T01/T02 撤回、todo rescope、本計画書の同梱) — 2026-08-16 実施。計画外の追加 (ユーザー承認済み): (a) 順位 449「昇格不適格判定の『両経路記載』を決定論化」を削除 (検査対象の § 昇格検査履歴 廃止で前提消滅)、(b) 台帳 § 無人可としなかった理由 の順位 284 行を lane 表記へ更新 + 見出しから件数を除去、(c) 未採番だった詳細エントリ 3 件に順位 462-464 を採番、(d) **ADR-033 改訂** — 「順位 = 追記型 ID・優先度は Tier 列・再採番はしない・細粒度の順序は行の並びで表す」を明文化し、本文の順位参照禁止を緩和。帰結として順位 334 (本文順位番号 lint) を retire
- [ ] PR-2: facet 出力言語の直書き + dev-conventions 記録 + todo23 エントリ削除
- [ ] PR-3: 順位 table 存在照合ゲート (lib-ledger + selector + workflow 配線)
- [ ] PR-4: 失敗マーカー + ブランチ自動掃除 + token 2 段化 + close 時案内
- [ ] PR-5: 台帳未掲載順位一覧の決定論出力 + facet 参照置き換え + todo23/24 エントリ整理
- [ ] skill リポ: Phase 4 展開先変更 + 昇格フロー縮小 + skill-sync-check
- [ ] 実走確認 1: weekly-review 実走で全 facet レポートが日本語 (PR-2 の効果)
- [ ] 実走確認 2: nightly schedule 実走 (または dispatch) で掃除 → 選択 → (成功なら PR / 失敗ならマーカー) の経路が設計どおり動く (PR-3/PR-4 の効果)
- [ ] **本ファイル (docs/work-plan-nightly-lane-model.md) を削除する** — 上記すべて完了後。恒久的な決定がすべて ADR / 台帳 / dev-conventions に反映済みであることを確認してから消すこと
