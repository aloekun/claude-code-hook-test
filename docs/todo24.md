# TODO (Part 24)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo23.md がファイルサイズ 52690 B (2026-08-16 時点、50KB = 51200 B の安定読み取り閾値超過) に到達したため、新規エントリは本ファイルに記録していた (2026-08-16 新設、週次レビュー 2026-08-15 実行セッションで検出)。**本ファイルは既存タスクの編集・完了削除専用。新規エントリの追加先は [docs/todo25.md](todo25.md)** (50869B = 閾値まで残り331Bに到達したため、2026-08-22 以降移行)。todo.md / todo3.md 〜 todo23.md の既存エントリは引き続き有効、相互に独立。
>
> **サイズ表記について**: 各記載は**その時点の計測値**であり、現在値と一致しないことがある。現在値が必要なら計測すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 週次レビュー由来 (2026-08-15 実行セッション、findings 外の運用問題)

> **由来**: 2026-08-15 の週次レビュー ([ADR-031](adr/adr-031-weekly-review-pipeline.md)) 実行と、その後の調査セッションで判明した問題のうち、**夜間 todo ループ ([ADR-072](adr/adr-072-nightly-todo-loop.md)) の設計変更に依存しないもの**を登録する。
>
> **夜間ループ側の改善 (lane モデル移行) は 2026-08-17 に完了した。** 一時計画書 `docs/work-plan-nightly-lane-model.md` は完了に伴い削除済み。**削除直前の同計画書のチェックリストは PR-5・skill リポ・実走確認 1・2 の項目が `[ ]` のまま残っていたが、これは計画書側の更新漏れ（記帳漏れ）であり、各項目の実装自体は下表の証跡で個別に確認できる。** 実際の完了状況は次のとおり。
>
> | 項目 | 完了の証跡 |
> |---|---|
> | PR-1 (docs 整備) | [#409](https://github.com/aloekun/claude-code-hook-test/pull/409) |
> | PR-2 (facet 出力言語) | [#410](https://github.com/aloekun/claude-code-hook-test/pull/410) |
> | PR-3 (順位 table 存在照合ゲート) | [#411](https://github.com/aloekun/claude-code-hook-test/pull/411) |
> | PR-4 (失敗マーカー + ブランチ自動掃除) | [#412](https://github.com/aloekun/claude-code-hook-test/pull/412) |
> | PR-5 (台帳未掲載順位一覧の決定論出力) | [#414](https://github.com/aloekun/claude-code-hook-test/pull/414) で `src/cli-ledger-candidates` を新設し、weekly-review workflow の機械 step `ledger-candidates` (LLM 判断なし) に配線済み。facet `review-todo-whole` の Criterion 3-2 は本 exe の出力（未掲載順位の件数）を参照する記述に置き換え済み ([.takt/facets/instructions/review-todo-whole.md:97](../.takt/facets/instructions/review-todo-whole.md))。docs/todo23.md 側の rescope 済みエントリ（昇格候補集合の決定論化）も削除済み |
> | 実走確認 1 (facet 出力言語) | 2026-08-17 に weekly-review を 2 回実走。判定基準を「全 facet が日本語」から「最終成果物が日本語」へ改めたうえで合格 (→ [docs/dev-conventions.md](dev-conventions.md) § 契約は最終成果物に置く) |
> | 実走確認 2 (夜間ループの経路) | 2026-08-16 の `workflow_dispatch` (dry_run) で掃除 → 選択 → 停止の 5 観測点を照合。失敗マーカー経路も発火して確認済み |
> | skill リポ反映 | `$CLAUDE_SKILLS_REPO` の `weekly-review` skill に Phase 4 展開先変更と昇格フロー縮小を反映し commit 済み。`/skill-sync-check` は全 21 スキル同期済みを報告 |
>
> 恒久的な決定は [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 18〜20 / [ADR-052](adr/adr-052-autonomy-execution-boundary-classes.md) / [ADR-033](adr/adr-033-todo-numbering-simplification.md) § 改訂 / [docs/claude-code-web-tasks.md](claude-code-web-tasks.md) / [docs/dev-conventions.md](dev-conventions.md) にある。
>
> 同セッションで登録済みの 2 件 ([docs/todo23.md](todo23.md) § 週次レビュー由来) — facet 出力言語の明記 / 昇格候補集合の決定論化 — は本ファイルには重複させない。

### `review-todo-whole` facet の台帳読み取りが現物と食い違う — 報告の信頼性を担保する

> **動機**: 2026-08-15 の run で、`review-todo-whole` facet が台帳の `無人可` 列を**誤読した**。順位 284 を `✅` と報告したが、[docs/claude-code-web-tasks.md](claude-code-web-tasks.md) の現物は `—` であり、しかも同ファイルの § 無人可としなかった…理由 表に 284 が条件 3 違反として明記されている。facet は**自分が読んだはずのファイルに書いてある反証を見落として**逆の報告を出した。
>
> **なぜ問題か**: 本 facet の Criterion 3 は「台帳の外の状態 (未マージブランチ・進行中 PR) を見ないと判定できない」ことを唯一の存在理由にしている。その報告が台帳の現物とすら一致しないなら、外部状態の報告も同じ精度で疑う必要がある。実際 2026-08-15 の run は条件 3 を 3 件「unverified」と正しく報告しており (workflow が `network_access: false` のため `gh` に届かない)、**正しい部分と誤った部分が同じレポートに混在していた**。読み手が現物と突き合わせない限り区別できない。
>
> **同根の問題**: 同 facet は昇格候補チェックでも 251 件中 13 件しか判定せず、「token 予算不足」という**実測と矛盾する理由**を書いている (実行 2 分 37 秒 / iteration 1 回 / 予算制約の注入なし)。
>
> **本タスクの位置づけ**: facet は `model: haiku` で動いている (`.takt/workflows/weekly-review.yaml`)。台帳のセル値のような**機械的に読める事実**を LLM に読ませていること自体が設計の問題で、`lib-ledger` が既に台帳パーサを持っている以上、決定論層へ寄せられる。
>
> **参照**: `.claude/weekly-reviews/2026-08-15.md`、`.takt/runs/20260815-100604-weekly-review-2026-08-15/reports/review-todo-whole.md` (Criterion 3-3 の表)、`.takt/workflows/weekly-review.yaml` (`model: haiku`)、`src/lib-ledger/`、`src/cli-ledger-candidates/`
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Medium (毎週) / Effort S / Adoption Risk None。

#### rescope (2026-08-17): 残る範囲を確定した — 逆向き差集合の決定論化

lane モデルへの移行 ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 18) で facet の報告範囲を 2 度縮小し、そのうえで**まだ残る機械的読み取り**を確定させた。

| facet の検査 | 現状 | 決定論化の要否 |
|---|---|---|
| Criterion 3-2 (未掲載順位の列挙) | `cli-ledger-candidates` の出力への参照に置換済み | **完了** (LLM は数えなくなった) |
| Criterion 3-3 (未マージブランチ走査) | 撤去済み (条件 3 廃止) | **消滅** |
| Criterion 3-1 (台帳にあるが順位 table に無い行) | facet が台帳の順位列を読んで summary を grep する | **要** — `cli-ledger-candidates` の**逆向き差集合**そのもので、同じパーサで出せる |
| Criterion 3-3 (`✅` 行の `注意` 列の読み直し) | facet が `無人可` と `注意` のセルを読む | **一部要** — 判断留保の語を読むのは自然言語判断で LLM が適任だが、**どの行が `✅` か**は機械可読な事実で、2026-08-15 に誤読した (順位 284) のはまさにここ |

**したがって「不要」ではない。** ただし残りは当初より小さく、`cli-ledger-candidates` に逆向き出力 (台帳にあるが順位 table に無い順位) と `✅` 行の一覧を足すだけで足りる見込み。新しい経路の設計は要らない。

#### 作業計画

- [ ] `cli-ledger-candidates` に逆向き差集合 (台帳にあるが順位 table に無い順位) の出力を足す — Criterion 3-1 の置換先
- [ ] 同 exe に `無人可` 列の値つき行一覧を出す経路を足すか判断する (facet が `✅` 行を自分で読まなくて済むように)
- [ ] `review-todo-whole` の Criterion 3-1 / 3-3 を、その出力への参照に置き換える
- [ ] 次回 weekly-review の実走で、facet が報告する台帳の事実が現物と一致することを確認する

#### 完了基準

- facet が報告する台帳の機械可読な事実 (順位・`無人可`) が exe 由来になり、LLM の読み取り精度に依存しない
- 自然言語判断 (`注意` 欄の判断留保の読み取り) だけが facet に残る

#### 詰まっている箇所

- なし。当初のブロッカーだった PR-5 (`cli-ledger-candidates`) は [#414](https://github.com/aloekun/claude-code-hook-test/pull/414) で着地済み、その後の weekly-review 実走確認も完了した（本ファイル冒頭の完了報告テーブル参照）。本節の作業計画 (逆向き差集合の追加・Criterion 3-1/3-3 の置き換え) 自体は未着手のまま残る

## post-merge feedback 採用分 (#409-#414 の 5 PR 分、2026-08-17 採否確定)

> **由来**: lane モデル移行の 5 PR (#409 / #410 / #411 / #412 / #414) の post-merge feedback と、2026-08-16 の夜間ループ dispatch 実走で判明した事象。採用候補 14 件・却下推奨 13 件を系統別に精査し、**採用分を 3 タスクへ統合**した (却下推奨はそのまま却下、個別登録しない)。
>
> **F-1 (順位 228 の台帳パス修正) は本バッチで実施済み** — 台帳がガード対象で夜間ループが自分で直せず、放置すると毎晩同じ順位で停止するため先行対応した。

### docs 整合性と output-contract の drift を機械検証する (系統 A + B)

> **動機**: 同じクラスの drift を 2 つの経路で踏んだ。(1) **todo preamble の pointer 整合性** — 「現在の追加先」を指す記述が 3 ファイルで stale になっており、PR #409 で手作業修正した。skill 側にも同じ固定参照があり、そちらも修正した。(2) **facet 免除リストと workflow condition の不一致** — PR #410 で 19 instruction 全部に汎用リストを貼り、実在しない値を書き実在する値を落とした。どちらも「複数箇所に散った同じ事実が一致しているか」の検査で、機械的に照合できる。
>
> **なぜ 1 タスクか**: 検査対象は違うが**実装先と手法が同じ** (`cli-docs-lint` の validator 追加 + grep ベースの集合照合)。既存の順位 441 (詳細エントリ ⇄ 台帳行の 1:1 対応検査) とも実装先が同じで、3 つまとめて 1 つの validator 群にするのが自然。
>
> **参照**: `.claude/feedback-reports/409.md` Tier1 #1・Tier3 #1、`.claude/feedback-reports/410.md` Tier2 #1、`.claude/feedback-reports/414.md` Tier2 #2、順位 441、`docs/dev-conventions.md` § takt facet の出力言語
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Medium / Effort S-M / Adoption Risk None。

#### 設計決定 (案)

- **A-1**: `docs/todo*.md` の preamble にある「新規追加先」参照が全ファイルで一致するかを検査する。`docs/todo.md` の routing 表を正とし、他ファイルの記述との不一致を報告する
- **B-1**: 各 facet instruction の免除リストが、`.takt/workflows/*.yaml` の対応する `rules.condition` リテラルを網羅しているか照合する。対応表は `instruction:` と `- condition:` の対から機械的に作れる (`docs/dev-conventions.md` に手順を明記済み)
- **B-2**: `aggregate-weekly` の `findings=0` 分岐と `findings>0` 分岐が同じ見出し構造を出すことを固定する
- **B-3**: **最終レポートの言語を決定論的に検査する** — `weekly-review.md` と `findings.json` の自由記述 field が日本語かを機械判定する。[docs/dev-conventions.md](dev-conventions.md) § 契約は最終成果物に置く で契約点を 1 枚へ集約したが、**その 1 枚を見る機械がまだ無い**。閾値未満なら warning としてレポートへ明記する (助言層なので run は止めない = [ADR-043](adr/adr-043-security-gates-fail-closed.md))
- **A-2 は着手しない**: 提案された `crates/docs-parser` は本リポに存在せず、範囲記法の展開は facet instruction 側の話。A-1 に吸収する
- 実装先は `cli-docs-lint` の validator 追加を第一候補とし、順位 441 との統合可否を着手時に判断する

#### 作業計画

- [ ] 順位 441 (詳細エントリ ⇄ 台帳行の 1:1 検査) と統合するか、独立 validator にするかを決める
- [ ] A-1 (preamble pointer 整合) を実装 + fixture テスト
- [ ] B-1 (免除リスト ⇄ workflow condition) を実装 + fixture テスト
- [ ] B-2 (テンプレート分岐の見出し一致) を実装
- [ ] B-3 (最終レポートの言語検査) を実装 — 実装後、`docs/dev-conventions.md` の「best-effort であり検査は無い」旨の記述を更新する
- [ ] 既存違反 0 を確認して有効化する
- [ ] A-3 として範囲記法の展開規則を `docs/dev-conventions.md` に明記する

#### 完了基準

- preamble の stale pointer と、免除リストの過不足が push 前に決定論的に検出される
- 3 検査とも fixture テストを持ち、`cargo test` で回帰検出できる

#### 詰まっている箇所

- 順位 441 との統合可否 (実装先が同じなので分けると重複しうる)

### 出力先と検証設計の convention を明文化する (系統 C + E)

> **動機**: lane モデルの 5 PR で、同じ種類の設計ミスを 3 回踏んだ。(1) **出力先の見落とし** — 警告を stdout に出したが、workflow が stdout を捨てて許可リストの行だけ転送する設計だったため、最も見せたい経路 (exit 3) で警告が消えていた。(2) **fixture が実データを代表していない** — 順位 table 途中の空行という実データの癖を fixture が持たず、初版パーサが順位 193 以降を全部取りこぼした。実 exe を実ファイルに当てて初めて露見した。(3) **step outcome の組み合わせ** — `publish-tree` 失敗時に下流が `skipped` になり、`skipped != 'success'` で失敗マーカーが誤作成される経路を踏んだ。
>
> **なぜ 1 タスクか**: 3 件とも「**書く前に考える対象**」の明文化で、行き先が `docs/dev-conventions.md` と `ADR-072` に閉じる。実装を伴わない。
>
> **参照**: `.claude/feedback-reports/411.md` Tier3 #1・#2、`.claude/feedback-reports/412.md` Tier3 #2、ADR-043 / ADR-057 / ADR-064 (stdout/exit code の扱いを扱う既存 ADR 群)
>
> **実行優先度**: 💎 **Tier 3** — Severity Low-Medium / Frequency Medium / Effort S / Adoption Risk None。

#### 設計決定 (案)

- **C-1 出力の visible paths チェックリスト** (`docs/dev-conventions.md`): stdout / stderr / exit code / ファイル / ログ のそれぞれについて「誰が読むか」「誰が捨てるか」を実装前に確認する。呼び手が stdout をリダイレクトする経路では、診断は stderr へ出す
- **C-2 fixture と実データの対**  (`docs/dev-conventions.md`): parser / validator を実装したら、fixture テストに加えて**実 exe を実ファイルに当てる**。fixture は実データの癖を代表しないことがある (実例: 順位 table 途中の空行)
- **E-1 step outcome の組み合わせ** (ADR-072 へ追記): `skipped` は `success` ではない。上流が失敗して下流が skip された場合と、下流自身が deny した場合を条件式で区別する。現状は `nightly-todo.yml` のコードコメントに埋没している

#### 作業計画

- [ ] `docs/dev-conventions.md` に C-1 / C-2 を追記 (由来つき)
- [ ] ADR-072 に E-1 を追記 (決定 19 の実装節が適切)
- [ ] `pnpm lint:docs` + markdownlint clean

#### 完了基準

- 3 件とも恒久文書に記録され、コードコメントにしか無い状態が解消している

#### 詰まっている箇所

なし

### 夜間ループとレポート出力の小さな穴を塞ぐ (系統 D + F-2)

> **動機**: lane モデルの 5 PR で残った小さな欠陥。いずれも実害は限定的だが、放置すると無人経路のノイズか停止につながる。
>
> **参照**: `.claude/feedback-reports/412.md` Tier1 #2、`.claude/feedback-reports/411.md` Tier1 #1、2026-08-16 の dispatch 実走ログ
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Low-Medium / Effort S / Adoption Risk None。

#### 設計決定 (案)

- **D-1 ブランチ削除の事前存在確認** (`.github/workflows/nightly-todo.yml`): 掃除ループで `git push --delete` の前に `git ls-remote` で ref の存在を確認し、既に消えていれば warning で skip する。現状は `set -euo pipefail` により step 全体が中断する。**4 ソースが同一指摘**
  - **失敗の種別を潰さないこと。** 「対象 ref が既に消えている」だけを warning + 継続にし、**ネットワーク / 認証エラーは従来どおり失敗させる** ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 10 — インフラ障害は設計された結末ではないので red)。`git push --delete` の非ゼロを一律に握り潰すと、App token の失効やネットワーク断が「掃除対象なし」に化ける
  - 事前確認と削除の間に他経路がブランチを消す競合は残る (TOCTOU)。その場合も上記の種別判定で「既に消えている」として扱えれば実害はない
- **D-2 parse エラーの診断強化** (`src/lib-ledger/src/summary_gate.rs`): 行番号と文脈を含める。現状は「順位を整数として読めません」等で、どの行かは出るが周辺が分からない
- **F-2 `[env] GIT_DIR 導出失敗` の抑止** (`src/cli-stale-branch-scan/`): `--repo` を明示している場合は gh の repo 解決に GIT_DIR が不要なので警告を出さない。夜間 workflow は jj リポジトリ外で走るため毎晩出る

#### 作業計画

- [ ] D-1 を実装 (ls-remote 確認 + warning skip)
- [ ] D-2 を実装 + テスト
- [ ] F-2 を実装 (`--repo` 指定時は警告抑止)
- [ ] `cargo test --workspace` green / `pnpm lint:workflows` OK
- [ ] 次回 dispatch or schedule 実走で D-1 / F-2 の効果を確認

#### 完了基準

- 掃除ループが「既に消えたブランチ」で job を落とさない。**かつネットワーク / 認証エラーでは従来どおり落ちる** (両方をテストか実走で確認する)
- 夜間 run のログに実害の無い警告が出ない
- parse エラーから問題箇所が特定できる

#### 詰まっている箇所

- D-1 の効果確認は実走が要る (workflow 変更は実走でしか検証できない)

---

## PR #417 の調査で判明した残課題 (2026-08-18 起票)

### post-merge-feedback の takt run が起動直後に死ぬ経路 — 終了理由が記録されない

> **動機**: PR #417 (順位 444 = stale meta による恒久 block) の原因調査で、**block の原因になった run 自体がなぜ死んだのかは未解明**であることが判明した。順位 444 は「壊れた後に機構が止まらない」ようにする回復層の修正であり、この死亡そのものには触れていない。
>
> **実測した signature** (`.takt/runs` の post-merge-feedback run 全 142 件を走査):
>
> | | #281 の 1 本目 | #374 の 1 本目 | 正常な run (比較) |
> |---|---|---|---|
> | run ログの行数 | 5 | 5 | 30 |
> | 最終エントリ | `phase_start` ×3 | `phase_start` ×3 | `piece_complete` |
> | 最終エントリの時刻 | t+0.29 秒 | t+0.31 秒 | t+8.1 分 |
> | `phase_complete` | 0 件 | 0 件 | 12 件 |
> | `reports/` の中身 | 空 | 空 | 7 ファイル |
>
> 正常な run では最初の `phase_complete` が **t+33.9 秒**に出る。死んだ 2 本はどちらもそこへ到達しておらず、**analyze 第 1 フェーズの起動から 34 秒以内**に落ちている。どちらも再実行 (4.25 分後 / 32 分後) は正常時間 (10.5 分 / 8.1 分) で完走したため、タスク固有の難しさではなく **transient な起動失敗**と考えられる。
>
> **観測手段が無いことが問題の本体**: run ディレクトリには「いつ止まったか」しか残らず、**なぜ止まったか**を示す記録がどこにも無い。プロセスの終了コード / シグナル / stderr が保存されていないため、再現条件を絞り込めない。まず観測を足さないと原因調査に着手できない。
>
> **修正後も残る影響**: PR #417 の guard は経過時間で判定するため、この経路で死んだ run は**最大 25 分間、次の post-merge-feedback をブロックし続ける** (恒久 block ではなくなったが、ゼロではない)。#281 の再実行は 4.25 分後だったので、PR #417 適用後ならブロックされていた。
>
> **進捗シグナルとの関係**: 死んだ run のログ mtime は開始時刻で凍結する一方、正常な run は 30〜60 秒ごとに `phase_complete` を書き続ける。したがって「`logs/` の mtime が数分進んでいない」は死亡の明瞭な陽性シグナルであり、これを guard に足せば上記のブロック窓を 25 分から数分へ縮められる。ただし採否は本タスクの観測結果を見てから判断する ([ADR-064](adr/adr-064-monitor-success-positive-evidence.md) の陽性証拠要求)。
>
> **参照**: [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § L2 / § 並行起動 guard、[PR #417](https://github.com/aloekun/claude-code-hook-test/pull/417)、順位 323 (timeout が孫プロセスを縛れない — 同じ takt 実行経路の別欠陥)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (実害は最大 25 分のブロック + 再実行 1 回。データ破壊は無い) / Frequency Low (142 run 中 2 件 = 1.4%) / Effort S (まず観測の追加のみ) / Adoption Risk None。

#### 作業計画

- [ ] takt run の終了コード / シグナル / stderr 末尾を run ディレクトリへ残す (まず観測を足す)
- [ ] 観測が溜まったら死因を分類し、対処の要否を判断する (**再現しないまま塞ごうとしない**)
- [ ] 進捗シグナルによる guard のブロック窓短縮を、観測結果に照らして採否判断する

#### 完了基準

- 起動直後に死んだ run について、**なぜ止まったか**が run ディレクトリの記録だけで判別できること。
- 分類の結果として対処が不要と判断した場合は、その根拠を negative result として永続化して閉じること ([dev-conventions.md](dev-conventions.md) § spike / 実験タスクの見送り (negative result) 永続化 convention)。

---

## post-merge feedback 採用分 (#417 / #418 / #419 / #420 / #421 / #423 / #424 の 7 PR 分、2026-08-19 採否確定)

> **由来**: 不具合修正バックログ消化計画 (PR A〜D) の 7 PR の post-merge feedback。全 48 提案のうち
> **採用候補 16 / 様子見 18 / 却下推奨 14**。採用候補を 7 系統に分類し、**5 タスクへ統合**した
> (様子見・却下推奨はそのまま、個別登録しない)。統合の単位は「そのまま 1 PR になる粒度」。
>
> **起票前の実コード確認で 1 件が脱落した** — #417 の「`REPORT_FILE_NAME` / `RUN_REPORT_FILE_NAME` の
> pin テストを両 crate に追加」は既に両側に実装済みだった (`run_registry.rs` / `reaper/mod.rs` の
> tests、PR #418 で追加)。同 PR の他 2 提案も大部分が実装済みで、残片のみを順位 471 に載せている。
> **feedback レポートも台帳と同じく実装が動くほどずれる。**

### 誤帰属と副作用フラグ欠如を決定論ルールで弾く (系統 A)

> **動機**: 2 件とも**過去に実 incident を生んだパターン**の機械検出で、実装先も手法も同じ。
>
> - **設定文字列のパス相当フィールドへの `..` 混入**: `reportDirectory` の `..` で別 run の成果物を
>   成功証拠に誤用した (PR [#417](https://github.com/aloekun/claude-code-hook-test/pull/417) で修正)。
>   同型の誤帰属は 2026-08-09 #374 / 2026-07-16 #281 でも観測されている
> - **`jj workspace list` の `--ignore-working-copy` 欠如**: `list_workspace_roots` が `discover.rs` と
>   違ってフラグを欠いていた (PR [#421](https://github.com/aloekun/claude-code-hook-test/pull/421) で修正)。
>   並行 jj セッションの op-log divergence による commit 済み作業の silent revert に直結しうる
>
> **なぜ 1 タスクか**: 実装先が同じ `.claude/custom-lint-rules.toml` の正規表現層 ([ADR-007](adr/adr-007-custom-linter-layer-boundary.md))
> で、既存 13 ルールと同水準の単純パターン。fixture も同じ `tests/fixtures/incidents/{bad,good}/` ([ADR-049](adr/adr-049-incident-eval-regression-suite.md))。
>
> **将来の false positive に注意**: `--ignore-working-copy` は `update-stale` 等の意図的例外が増えると
> 誤検出しうる (現時点の例外呼び出しは 0 件)。逃がし方を決めてから有効化する。
>
> **参照**: `.claude/feedback-reports/417.md` Tier1 #2、`421.md` Tier1 #1、[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) (同型の trust boundary 検出ルール)
>
> **実行優先度**: 🚀 **Tier 1** — Severity High (両者とも実 incident 実績) / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `..` 混入検出ルールを正規表現層に追加 + bad/good fixture
- [ ] `jj workspace list` の `--ignore-working-copy` 欠如検出ルールを追加 + bad/good fixture
- [ ] **修正前に bad fixture が実際に落ちることを確認する** ([ADR-049](adr/adr-049-incident-eval-regression-suite.md))
- [ ] 既存コードで違反 0 を確認してから有効化する
- [ ] 意図的例外が出た場合の逃がし方を決めて doc に書く

#### 完了基準

- 両ルールが bad fixture で発火し、good fixture では発火しないこと
- 既存コードに違反が無いこと (有効化時点で赤にならない)

### cross-crate 定数 pin と reaper 回帰テストの残片を埋める (系統 B 実装 + C)

> **動機**: 起票前の実コード確認で**大部分が実装済み**と判明したため、**残っているのは次の 3 つだけ**。
>
> - `TASK_BOOKMARK_SEPARATOR` の直接 assert が `cli-merge-pipeline` 側に無い。定数は `context.rs` に
>   あるが assert が無く、`cli-push-runner` 側だけが pin している。**doc コメントの「両 crate の
>   unit test が pin する」という主張と実装が食い違っている** (doc の嘘)
> - reaper の「**同一 run の `reportDirectory` は受理される**」の明示 assert (拒否側は実装済み)
> - `settle_meta_status` の **I/O 失敗**ケース (JSON malformed と「失敗を成功と報告しない」は実装済み)
>
> **なぜ 1 タスクか**: いずれも既存テストファイルへの小片追加で、production コードの変更を伴わない。
>
> **参照**: `.claude/feedback-reports/417.md` Tier2 #2/#3、`420.md` Tier2 #1
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Low / Effort XS-S / Adoption Risk None。

#### 作業計画

- [ ] **着手時に再度実コードを確認する** — 本エントリの元になった 3 提案のうち 1 件は起票時点で既に実装済みだった
- [ ] `context.rs` の tests に `TASK_BOOKMARK_SEPARATOR` の literal pin assert を追加し、doc コメントの主張と一致させる
- [ ] `reaper/tests.rs` に「同一 run の `reportDirectory` は受理される」assert を追加
- [ ] `settle_meta_status` の I/O 失敗 (読み取り不能 / 書き込み不能) ケースを追加

#### 完了基準

- 共有定数が片側だけ変わったとき、両 crate のテストが落ちること
- reaper の受理/拒否が両方向 assert されていること
- `settle_meta_status` の I/O 失敗 (読み取り不能 / 書き込み不能) が `Err` として扱われ、**成功と報告されない**ことがテストで固定されていること

### 語彙・テスト作法・判断規律の convention を明文化する (系統 B 規約 + D + E + F 規約)

> **動機**: 8 項目、すべて行き先が [dev-conventions.md](dev-conventions.md) で実装を伴わない。
>
> **語義の分離** (同じに見える別物を区別する):
>
> - 「コメント投稿の有無 (presence)」と「actionable findings の有無」を混同しない (#418 の根本原因)
> - ソートキーが「決定論的」なのか「時系列」なのかを doc comment で明示する (#419 の真因。
>   決定論的ではあるが時系列でない順序が `session_data_unavailable` の誤報を生んだ)
>
> **テスト作法**:
>
> - cross-crate で共有する canonical constant は producer/consumer 双方で literal pin test を書く (#418)
> - filesystem の mtime 分解能に依存しないテスト (`filetime::set_file_mtime`) を既定パターンにする (#419)
>
> **判断の規律**:
>
> - 再利用されるキーを一意識別子として使わない / evidence の scope を実際より狭く見積もらない (#420)
> - 「将来リスク」分類は着手前の実測で裏付けてから PR 分割・優先順位を決める (#421)
> - fail-open でも黙って飛ばさない + todo 削除前に完了基準の全項目が実コードに現れているか確認する (#421)
> - 効きが確認できない修正を入れない / 計画案を実測で棄却し観測ベースで解を選ぶ (#423)
>
> **なぜ 1 タスクか**: 行き先が 1 ファイルに閉じ、実装を伴わない。docs 変更はマイルストーンでまとめる運用。
> 分量が大きくなるなら語義 / テスト作法 / 判断規律の 3 セクションで PR を分けてよい。
>
> **様子見だった #424 の「意味的に異なる状態が同じ見え方になるバグクラス」も語義セクションに畳める。**
> 着手時に採否を判断する。
>
> **参照**: `.claude/feedback-reports/418.md` Tier3 #1/#2、`419.md` Tier2 #3・Tier3 #1、`420.md` Tier3 #2、`421.md` Tier3 #1/#2、`423.md` Tier3 #1
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Medium / Effort S-M / Adoption Risk None。

#### 作業計画

- [ ] 語義セクション (2 項目) を追記
- [ ] テスト作法セクション (2 項目) を追記
- [ ] 判断規律セクション (4 項目) を追記
- [ ] **各項に実例 (PR 番号と何が起きたか) を必ず添える** — 一般論だけの規約は守られない
- [ ] #424 の様子見項目を畳むか判断する

#### 完了基準

- 8 項目が [dev-conventions.md](dev-conventions.md) に順位付きセクションとして載っていること
- 各項が「なぜ」と実例を持つこと
- #424 の様子見項目 (「意味的に異なる状態が同じ見え方になるバグクラス」) を**採用 / 保留 / 却下のいずれかに決定し、理由を記録**していること

### テスト用 staging ロックの 2 crate 重複を共有化するか再評価する (系統 F 実装)

> **動機**: PR [#423](https://github.com/aloekun/claude-code-hook-test/pull/423) で `EXEC_STAGING_LOCK` /
> `exec_staging_guard()` を `smoke.rs` と `t7_cwd_independence.rs` に**意図的に複製した**
> ([ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md) 層 1「2 crate 重複は extract 必須ではなく要 dogfood」)。
> pre-push レビューと post-merge feedback の双方が DRY 違反として指摘しており、**判断の是非を一度見直す**。
>
> **判断が要る点**: 共有化すると **test 専用の同期プリミティブを `lib-subprocess` の production surface に載せる**
> ことになる。ロックはプロセス内でしか意味を持たず、テストバイナリは別プロセスなので共有しても保証は増えない。
> #423 時点の決定は「3 つ目の copy→spawn テストが現れた時点で extract を再評価する」。
>
> **なぜ独立タスクか**: 前回判断の巻き戻しを含むため、切り戻し単位を分ける。
>
> **参照**: `.claude/feedback-reports/423.md` Tier2 #1、`t7_cwd_independence.rs` の `EXEC_STAGING_LOCK` doc
>
> **実行優先度**: 💎 **Tier 3** — Severity Low / Frequency Low / Effort S / Adoption Risk Low。

#### 作業計画

- [ ] 3 つ目の copy→spawn テストが出現したかを確認する (#423 の再評価トリガ)
- [ ] 出ていなければ現状維持と結論し、本エントリを閉じる (negative result の残し方を判断する)
- [ ] 共有するなら置き場 (`lib-subprocess` の test-support か、新規 dev-dependency crate か) を決める

#### 完了基準

- 複製を残すか共有するかが根拠付きで決まり、コード doc に反映されていること

### 夜間 auto lane とユーザー割当 PR の同一ファイル競合を自動検知する (系統 G)

> **動機**: PR D の着手前に、台帳・作業計画書・実装の 3 箇所を人手で横断確認する必要が生じた
> (2026-08-19)。台帳の順位 176 (auto lane) が PR D と同じ `src/check-ci-coderabbit/src/rate_limit.rs` を
> 触る想定だったため。結果的に PR D はそのファイルを触らず競合しなかったが、**それを判定するのに
> 手作業の横断確認が要った**。
>
> [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) は lane 割当の判断基準を定めるが、
> **同一ファイルの並行競合検知は範囲外**で重複しない。
>
> **参照**: `.claude/feedback-reports/424.md` Tier2 #1、`lib_ledger::select` (文書順で 1 件選択)
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium (手戻りリスク) / Frequency Low / Effort S / Adoption Risk None。

#### 作業計画

- [ ] 検知の入力を決める (台帳の対象ファイル欄 × open PR の changed files)
- [ ] 実装先を決める (`nightly-todo.yml` の step か `cli-nightly-task-select` の拡張か)
- [ ] 競合時の挙動を決める (選定から除外するか、warning に留めるか)

#### 完了基準

- auto lane の選定が、ユーザー割当 PR と同一ファイルを触る順位を**機械的に検知し、選定から除外するか警告として報告する**こと (どちらを採るかは作業計画で決める。warning に留める場合も、検知結果が run log から読めること)

---

## pre-push review 由来 (2026-08-19、bugfix-batch-plan.md 退役準備中に発見)

### `resolve_project_dir` の case-sensitive FS 複数一致が無言で 1 件に縮退する

> **動機**: 不具合修正バックログ消化計画 (PR A〜L) での `cwd_to_project_id` Linux case 不一致調査 (順位 469 の完了確認、2026-08-19) で、**別の未対応の穴**が実測で見つかった。case-sensitive filesystem (WSL Ubuntu-24.04 / ext4 で確認) では `Foo` と `foo` を同じ `projects_root` に置ける。両方が `cwd_to_project_id` の lowercase 比較に一致すると、`resolve_project_dir` (`src/cli-merge-pipeline/src/feedback/transcript.rs:37`) の `.find(...)` は **1 件だけ返し、もう一方を無言で除外する** (どちらが返るかは `read_dir` の順序依存で契約上未規定)。5 回試行して毎回 1 件のみ返ることを確認済み。
>
> **発見時点で発現経路は未確認** — case-sensitive FS では同一ディレクトリの綴りが通常一意なため、同じ workspace root から 2 通りの綴りは生まれにくい。ただしこれは推論であり、`~/.claude/projects` を OS 間で持ち込む等の経路は排除できていない。**bugfix-batch-plan.md は退役予定で削除されるため、この観測の記録先を本エントリに移した。**
>
> **参照**: `resolve_project_dir` の doc コメント (`transcript.rs:28-36`)、[ADR-043](adr/adr-043-security-gates-fail-closed.md) (fail-closed 原則)
>
> **実行優先度**: 💎 **Tier 3** — Severity Low (発現経路未確認) / Frequency Low / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `resolve_project_dir` の doc コメントに、複数一致時は 1 件のみ返し他方を無言で除外すること (順序は `read_dir` 依存で未規定) を明記する
- [ ] `~/.claude/projects` を OS 間で持ち込む等、発現経路が実在するかを再評価する
- [ ] 発現しうると判断したら、ADR-043 に従い複数一致を loud に検出する実装を追加する。発現しないと判断したら、その根拠を negative result として本エントリに記録して閉じる ([dev-conventions.md](dev-conventions.md) § spike / 実験タスクの見送り (negative result) 永続化 convention)

#### 完了基準

- `resolve_project_dir` の doc コメントが複数一致時の挙動を明記していること
- 発現経路の評価結果 (対応要否とその根拠) が記録されていること
---

### jj-op-verify が commit message 内の文言を実行と誤認して警告する (本セッション 4 回実観測)

> **動機**: `jj describe -m "... jj abandon ..."` のように **commit message の本文にコマンド名を書いただけ**で、`hooks-post-tool-jj-op-verify` が「直前に `jj abandon` を実行した」と誤認し `operation not recorded` 警告を出す。本セッション (PR #429〜#432) で **4 回**発生し、毎回 `jj op log` での手動確認を強いられた。
>
> **原因 (実装確認済み)**: [`detect_last_mutating_jj_op`](../src/hooks-post-tool-jj-op-verify/src/main.rs#L61) が `command.split_whitespace()` で**コマンド文字列全体をトークン化**し、その中に変更系 jj サブコマンド名が現れるかだけを見ている。`-m` の引数 (quote 内) を除外していないため、message 本文の文言が実行と区別されない。
>
> **実害**: 警告そのものは助言層 (block しない) だが、ADR-045 の op-log divergence は**実在する重大事故クラス**であり、狼少年化すると本物の divergence を見逃す。実際、本セッションでは 4 回とも「hook 警告 → `jj op log` 確認 → 正常」の往復が発生した。
>
> **対処案**: quote 内を除外してからトークン化する (`-m` / `--message` の引数、および `"..."` / `'...'` で囲まれた範囲をスキップ)。**false negative 側のトレードオフは軽微** — quote 外に現れる変更系コマンドは従来どおり検出できる。
>
> **回帰テスト**: `detect_last_mutating_jj_op` は pure function なので unit test で固定できる。(a) `jj describe -m "fix: jj abandon について"` → None、(b) `jj abandon -r x` → Some(abandon)、(c) `jj describe -m "msg" && jj abandon -r x` → Some(abandon) の 3 方向。
>
> **参照**: [`src/hooks-post-tool-jj-op-verify/src/main.rs`](../src/hooks-post-tool-jj-op-verify/src/main.rs)、[ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) § Known operational risks、PR #431/#432 post-merge feedback (Tier1 #4)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium (誤検知による狼少年化。本物の divergence を見逃すリスク) / Frequency **High** (本セッションだけで 4 回) / Effort S / Adoption Risk Low (quote 除外は決定論的で pure function に閉じる)。

#### 作業計画

- [ ] `detect_last_mutating_jj_op` を quote-aware にする (`-m` / `--message` の引数と quote 範囲を除外)
- [ ] pure function の unit test を 3 方向 (誤検知しない / 正しく検出する / 複合コマンドで最後の操作を採る) で追加
- [ ] 変異テストで検知を確認する (quote 除外を外すと誤検知テストが FAILED になること)

#### 完了基準

- commit message にコマンド名を含む `jj describe` で警告が出ないこと。quote 外の変更系コマンドは従来どおり検出されること。両方向が unit test で固定されていること。

---

### Git Bash 経由の複数行 `node -e` が silent no-op になる (本セッション 2 回実観測)

> **動機**: Windows の Git Bash から複数行の `node -e '...'` を渡すと、**終了コード 0・出力なしで何も実行されない**。本セッション (PR #428 / PR #432) で **2 回**踏み、いずれも「修正を適用したつもりが実際には未適用のまま検証していた」状態を作った。1 回目は workflow の抽出スクリプト、2 回目はテスト初期化子の一括置換で、どちらも**失敗が silent なため気づくまでに時間を要した**。
>
> **既知の同型**: memory `powershell-set-content-crlf` (PowerShell から複数行 bash を渡すと壊れる) と同じクラス。MSYS の argv 変換が複数行引数を壊すことが原因と推測されるが、**silent に失敗する**点が本件の危険性の本体。
>
> **現在の回避策**: スクリプトをファイルに書いて `node script.mjs` で渡す。本セッションではこの形に統一して解決した。
>
> **対処案**: (a) 回避策を convention として明文化する (助言層、**単独では不可** — 本セッションでは convention 化した直後に再発した)、(b) **決定論層**: PreToolUse hook で「複数行を含む `node -e` / `python -c` 等」を検出してブロックし、ファイル経由を案内する。(a) は既に `docs/dev-conventions.md` に記載済みだが再発したため、(b) が本命。
>
> **回帰テスト**: hook の判定関数を pure function として切り出し、(a) 複数行 `node -e` → block、(b) 単一行 `node -e` → 許可、(c) `node script.mjs` → 許可 の 3 方向で固定する。
>
> **参照**: [`docs/dev-conventions.md`](dev-conventions.md) § GitHub Actions の `run:` は常に `-e` 付きで起動する (末尾の注意書き)、`src/hooks-pre-tool-validate/`、PR #431 post-merge feedback (Tier2 #4)。
>
> **実行優先度**: 🚀 Tier 1 — Severity **High** (silent failure により「検証したつもり」を作る。実際に誤った検証結果を報告しかけた) / Frequency Medium (複数行スクリプトを渡すたび) / Effort S / Adoption Risk Low (既存 hook への判定追加)。

#### 作業計画

- [ ] `hooks-pre-tool-validate` に「複数行を含む `-e` / `-c` インライン実行」の検出を追加し、ファイル経由を案内してブロックする
- [ ] 判定を pure function に切り出し、3 方向の unit test で固定する
- [ ] 変異テストで検知を確認する

#### 完了基準

- 複数行の `node -e` がブロックされ、ファイル経由の案内が出ること。単一行および `node script.mjs` は従来どおり通ること。両方向が unit test で固定されていること。

---

### jj 出力の path separator を前提にするコードのテスト整備 (Windows は `\` 区切り)

> **動機**: PR #432 の CodeRabbit 指摘で「`jj file list` は POSIX の `/` を使うので `is_root_level` の `\` 判定は不要 / 誤判定を生む」と提案されたが、**実測すると Windows の jj 0.42 は `\` 区切りで出力する** (`sub\f.py`)。`\` 判定を外すと Windows でサブディレクトリのファイルを root 直下と誤判定して誤検知が出る。指摘は前提が誤っていた。
>
> **問題**: この「両区切りを見る」という判断が**テストで固定されていない**。現在は module doc に理由を書いてあるだけで、将来「POSIX 準拠」を理由に再び外される余地が残る。同型の path 判定は [`extract_basename`](../src/cli-push-runner/src/stages/scratch_file_warning.rs) にもあり、そちらも同じ前提に依存している。
>
> **対処案**: jj 出力を parse する箇所の path separator 前提を regression test で固定する。`is_root_level` / `extract_basename` の両方について、(a) `\` 区切りのサブディレクトリパスを root 扱いしない、(b) `/` 区切りも同様に扱う、の 2 方向。可能なら実 jj を使った統合テストで「実際にどちらの区切りが出るか」も固定する (ADR-065 の CI matrix で両 OS を回しているため、OS 差が出れば CI が検出する)。
>
> **参照**: [`src/cli-push-runner/src/stages/scratch_file_warning.rs`](../src/cli-push-runner/src/stages/scratch_file_warning.rs) (`is_root_level` の module doc に実測根拠を記録済み)、PR #432 CodeRabbit 指摘、PR #432 post-merge feedback (Tier2 #1 / #3)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (誤検知 or 検出漏れ。scratch guard は最終防衛層) / Frequency Low (path 判定を触るときだけ) / Effort S / Adoption Risk None (テスト追加のみ)。

#### 作業計画

- [ ] `is_root_level` / `extract_basename` の separator 前提を両方向の unit test で固定する
- [ ] 実 jj の出力 separator を統合テストで固定できるか検討する (CI matrix で OS 差が出れば検出される形)
- [ ] 変異テストで検知を確認する (`\` 判定を外すと該当テストが FAILED になること)

#### 完了基準

- `\` 判定を外すと FAILED になる regression test が存在すること。両 OS の CI で green であること。

---

### 手書きの「公開 API 一覧」doc が re-export とずれる — 決定論的な一致検査を入れる

> **動機**: [`src/lib-jj-helpers/src/lib.rs`](../src/lib-jj-helpers/src/lib.rs) の `# 公開 API` doc 一覧が実際の `pub use` とずれていた。機械的に突き合わせた実測 (2026-08-21):
>
> ```text
> re-export されている公開 API: 22 件
> doc 一覧に載っている:         15 件
> doc に未記載 (陳腐化):         7 件
> doc にあるが re-export されていない: 0 件
> ```
>
> **単発の記載漏れではない**: 未記載 7 件のうち 4 件は PR #431 で追加したものだが、残り 3 件 (`GitDirResolution` / `is_inside_workspace` / `list_workspace_roots`) は**それ以前から漏れていた**。つまりこの doc は継続的にずれる構造で、**手で直しても同じことが繰り返される**。
>
> **方針 (2026-08-21 ユーザー判断)**: doc を手で埋めるのではなく **lint 化して決定論的に固定する**。「ドキュメントを増やしても確実さは保証されない。改善は常に決定論的で積み上げる形で実現されるべき」という方針に基づく。
>
> **対処案**: `pub use` 文から re-export 名を抽出し、doc の `[\`Name\`]` 参照と突き合わせる決定論的検査を追加する。検査ロジックは検証時に実装済み (両集合の差分を双方向に報告する形)。配置先は `scripts/` の既存 lint 群 (`lint-workflows.mjs` と同じ層) が候補。あわせて現在の未記載 7 件を埋める。
>
> **注意**: `cargo doc` が生成する API 一覧と二重管理になる面がある。lint を入れるか、手書き一覧そのものを廃止するかは実装時に再評価してよい (どちらも「ずれる doc を無くす」点で方針に合致する)。
>
> **参照**: [`src/lib-jj-helpers/src/lib.rs`](../src/lib-jj-helpers/src/lib.rs) `# 公開 API` 節、PR #431 post-merge feedback (Tier1 #2)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Low (doc のずれ。実行時の挙動には影響しない) / Frequency Medium (公開 API を足すたびにずれる。実測で 2 世代分の漏れを確認) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `pub use` と doc の `[\`Name\`]` を突き合わせる決定論的検査を実装する (双方向の差分を報告)
- [ ] 現在の未記載 7 件を doc に反映する
- [ ] 検査を壊して落ちることを確認する (未記載を 1 件作ると FAILED になること)
- [ ] 手書き一覧の維持コストが見合わなければ、一覧廃止案も比較して判断を記録する

#### 完了基準

- 公開 API を追加して doc を更新し忘れると、決定論的検査が落ちること。現在の 7 件のずれが解消されていること。

---

### `owns()` が false を返す原因をログで区別する (lock の debug 可能性)

> **動機**: [`src/cli-pr-monitor/src/lock.rs`](../src/cli-pr-monitor/src/lock.rs) の `MonitorLock::Drop` は `owns()` が false のとき「lock は既に別インスタンスへ takeover 済み」と 1 種類のログしか出さないが、false になる原因は **(a) 別インスタンスが takeover 済み / (b) lock 内容が parse 不能 (破損・書き込み途中) / (c) token 不一致**の 3 通りある。原因が区別できないと、同時監視の競合 (#364 型) を調査するときにログから経路を再構成できない。
>
> **対処案**: false の原因を enum で区別し、ログ文言に反映する。判定は pure function に切り出せるため unit test で固定できる。
>
> **参照**: [`src/cli-pr-monitor/src/lock.rs`](../src/cli-pr-monitor/src/lock.rs) (`owns` / `Drop`)、PR #430 post-merge feedback (Tier2 #3)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (調査可能性。lock 競合は #364 で実発生済み) / Frequency Medium / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `owns()` の戻り値を 3 状態 (Owned / TakenOver / Unreadable) に分け、ログ文言を分岐させる
- [ ] pure function の unit test で 3 状態を固定する
- [ ] 変異テストで検知を確認する

#### 完了基準

- Drop 時のログから false の原因 3 種が区別できること。3 状態が unit test で固定されていること。
