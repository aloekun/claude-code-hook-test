# TODO (Part 24)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo23.md がファイルサイズ 52690 B (2026-08-16 時点、50KB = 51200 B の安定読み取り閾値超過) に到達したため、新規エントリは本ファイルに記録する (2026-08-16 新設、週次レビュー 2026-08-15 実行セッションで検出)。**新規エントリの追加先は本ファイル**。todo.md / todo3.md 〜 todo23.md の既存エントリは引き続き有効、相互に独立。
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
