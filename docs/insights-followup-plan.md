# Claude Code Insights フォローアップ作業計画 (2026-08-12)

> **位置づけ**: 2026-08-11 の Claude Code Insights レポート (`<USERPROFILE>\.claude\usage-data\report-2026-08-11-214819.html`、ローカルのみ) の指摘を既存機構と突合し、ギャップ分のみを作業化した計画書。方針・PR 構成・台帳登録内容はユーザー承認済み (2026-08-12)。
>
> **最終目標**: 下記 PR 1 → PR 2 の完了後、**本ファイル自身を削除する** (完了タスクは仕組みに反映後に削除する todo.md 運用ルールと同じ扱い。経緯は git log に残る)。
>
> **PR 構成 (承認済み)**: PR 2 本、うち新規 2 本。PR 1 (.takt facet 変更) → PR 2 (docs バッチ) の順で実施する。

---

## 背景 (調査済み事実 — 再調査不要)

Insights レポートの主要指摘と、既存機構との突合結果:

| # | 指摘 | 既存カバー | ギャップ |
|---|---|---|---|
| 1 | 巨大 transcript (5.5MB+) の解析でスラッシング | ADR-030 の時刻 range filter (Rust 側) は実装済み | facet の前提「1.7 MB」に対し実測 **14.7 MB** (`.takt/post-merge-feedback-transcript.jsonl`、2026-08-12)。抽出先行手順の指示なし。25K token limit 衝突は `docs/todo14.md:149` で自己観測済み。避難措置スクリプト 2 本が残留 |
| 2 | レビューレポートが VCS に永続化されない | 採否のみ台帳 (todo-summary2.md + todo22.md) にコミットされる設計 | verdict の機械集計手段なし。committed docs → gitignored レポートへの参照が多数 (todo13/16/18/20 等 → `.claude/feedback-reports/<pr>.md`)。clone 先では辿れない |
| 3 | verdict 境界・rubric が暗黙的 | ADR-056 の `review-anomaly.md` policy で成熟 (blocking/non-blocking 境界表あり) | `security-review` / `supervisor-validation` の output-contract ファイルが未整備 (`format:` 名の宣言のみ)。feedback 4 軸 (Severity/Frequency/Effort/Adoption Risk) の値域未定義 |
| 4 | post-merge 知見の自動ルール化ループ (Horizon 提案) | 「台帳登録はユーザー承認必須」は ADR-072 の信頼境界として意図的な設計 (`aggregate-feedback.md:171`) | 蓄積レポート (100+ 件) の横断クラスタリング分析は未実施。承認境界を保ったまま提案生成まで自動化する余地 |

**対応不要と判断済みの指摘** (作業しない。記録のみ):

- 「41 時間で 0 commits」→ jj 運用 (コミットは jj + PR 経由で行われ、Insights の git commit 検出に載らない) による誤検出。実害なし。
- CLAUDE.md 追記 3 案 (Report Format / Large Transcripts / docs-only Review Scope) → それぞれ ADR-048 output-contract / 本計画 PR 1 (facet 側に置く方が適切) / ADR-035+ADR-057 で既カバー。
- **レビューテンプレート化 (Custom Skill 化) 提案** → 同じ役割を takt facet 群が既に担っている。判定基準は `.takt/facets/policies/review-anomaly.md` (ADR-056) に blocking/non-blocking 境界表まで明文化済み、出力形式は output-contract (ADR-048) で構造固定済み。**未テンプレート化の残りだけ**を本計画に取り込んだ (= 台帳登録 (3) security/supervisor の output-contract 整備、(4) feedback 4 軸の値域定義)。
- **ヘッドレス化提案** → 既に実態がヘッドレス。pre-push レビューは `pnpm push` から takt 経由で非対話実行 (ADR-015)、post-merge feedback はマージパイプラインが takt を同期起動 (ADR-030)、PR 監視は cli-pr-monitor が自律動作 (ADR-018)。Insights 自身の「セッションの大半が automated workflow 起点」観測がこの構成の証拠。残る人間関与 (PR 作成・マージ実行) はヘッドレス化の漏れではなく**意図的な信頼境界** (ADR-028/052: 外部可視成果物の生成操作のみ人間承認を挟む)。さらに無人化する経路も夜間 todo 消化ループ (ADR-072)・無人 fix push (ADR-067) として別途整備済み。
- **GitHub MCP 提案** → PR メタデータ取得は gh CLI ベースの決定論層 (cli-merge-pipeline / cli-pr-monitor) で実装済み。MCP 化は責務分離原則 (ADR-022) 上の利点がない。
- 自己修復 pipeline (Horizon 1) → ADR-036 (3 層レビュー)/ADR-058 (fix 後再ゲート)/ADR-067 (無人 fix push) で段階導入済みの方向性そのもの。

**ユーザー決定 (2026-08-12 AskUserQuestion で確定)**:

1. 永続化 = verdict の telemetry 化 (台帳登録) + 参照規律の明文化 (順位 358 消化)。レポート本体の docs/ コミットはしない。
2. transcript = 二段構え。facet 指示の即時修正 + スクリプト掃除のみ実施し、Rust summary index 化 (順位 335) は Tier 2 のまま台帳に残す。
3. rubric 残件 = 台帳へ新規登録 (今すぐは実装しない)。
4. 自動ルール化 = 承認付き mining として Tier 3 で台帳登録 (完全自動化は不採用)。

---

## PR 1: analyze-session facet の抽出先行化 + 残留スクリプト掃除

**変更対象**: `.takt/facets/instructions/analyze-session.md`、削除 2 ファイル。

### 1-1. `.takt/facets/instructions/analyze-session.md` の修正

**(a) 冒頭方針 (現 5-7 行目) の文言明確化**。現行の「本 facet は filter 済 jsonl を読むだけ。生 file を直接 grep しない」は「フィルタ前の生 transcript (`~/.claude/projects/...`) を直接読まない」という意味。filter 済み jsonl への Grep は次項の正規手段になるため、誤読されないよう書き換える:

```markdown
- 本 facet は filter 済 jsonl のみを入力とする。フィルタ前の生 transcript (~/.claude/projects/ 配下) を直接読まない
```

**(b) Phase 1 (現 41-43 行目付近) を抽出先行手順に置換**。現行の「`transcript_path` を Read で読む」を以下に差し替える (キーワードは正規手順へ昇格する `.takt/analyze_transcript.py` の分類ヒューリスティック + 本 facet の抽出観点から採る):

```markdown
## Phase 1: Transcript の読み取り (抽出先行)

filter 済み jsonl でも実測で 10 MB を超えることがある (2026-08-12 実測 14.7 MB)。
**丸読みしない**こと。以下の手順で読む:

1. まずファイルサイズを確認する (例: `(Get-Item <path>).Length` / `wc -c`)
2. 約 1 MB 以下なら従来通り Read で全体を読んでよい
3. 超過する場合は Grep (filter 済み jsonl に対して) で候補行を先に絞る:
   - tool エラー: `"is_error":true`、`error`、`exit code`
   - ユーザー修正指示: `そうじゃない`、`こうして`、`やり直し`、`違う`、`incorrect`、`need to change`
   - ワークアラウンド: `workaround`、`回避`、`instead of`
   - 実装の困難: `difficult`、`challenge`、`試行錯誤`、`retry`
4. ヒット行の前後のみ Read (offset/limit 指定) で読み、文脈を確認する
5. 先頭・末尾の各数十行も読む (セッションの開始意図と終了状態の把握)
6. 抽出結果が知見ゼロでも、手順 3-5 を実施済みなら「知見なし」報告してよい
   (丸読みによる再確認はしない)
```

**(c) 注意書き (現 66 行目) の実測値更新**。「1.7 MB / 数百行になり得る。重要な箇所だけ要約する」を以下へ:

```markdown
- 実測で 14.7 MB / 数千行に達し得る (2026-08-12)。Phase 1 の抽出先行手順に従う
```

### 1-2. 残留スクリプトの削除

過去セッションの避難措置がそのまま残留したもの (順位 322 で観測済みの scratch 残留 near-miss と同一事象)。1-1(b) への昇格をもって削除する:

- `__parse_transcript.ps1` (repo root)
- `.takt/analyze_transcript.py`

### 1-3. スコープ外 (やらないこと)

- **順位 335** (cli-merge-pipeline での summary index 生成、`docs/todo14.md:147-161`) は実装しない。台帳のまま nightly-todo または手動着手に委ねる。本 PR は 335 が land するまでの暫定対処という位置づけ。

### 1-4. 検証

- 通常の `pnpm push` → PR 作成フロー (ADR-028 ゲートに従いユーザー確認を経る)。
- 実効検証は **本 PR マージ直後の post-merge feedback 実走そのもの**が兼ねる (analyze-session facet が新手順で走る)。25K token limit 衝突なしに `Session Analysis Report` が生成されれば成功。失敗時は `.failed` marker → L2 recovery (ADR-030) で観測できる。

---

## PR 2: docs バッチ (convention 消化 1 件 + 台帳登録 5 件 + 注記 + 本計画書削除)

docs-only PR。ユーザーの運用方針 (doc 変更はマイルストーンで束ねる) に従い 1 本にまとめる。**PR 1 マージ後・post-merge feedback の完走確認後**に実施し、本計画書の削除もここに含める。

### 2-1. 順位 358 の消化: Cross-File Reference Lifecycle を dev-conventions.md へ明文化

`docs/dev-conventions.md` へ新節を追加する。趣旨 (文面は整えてよい):

```markdown
## committed docs から ephemeral 成果物への参照規律 (順位358)

**規律**: 永続文書 (docs/、ADR、CLAUDE.md) から gitignored / 揮発性の成果物
(.claude/feedback-reports/*、.takt/runs/*、.claude/weekly-reviews/* 等) への参照だけで
根拠を成立させてはならない。clone 先・CI・クラウド環境にはその参照先が存在しない。

**手順**: 根拠の要旨 (finding の内容・判断理由) を committed 側の文書に転記した上で、
出典表記として ephemeral パスを添えるのは可 (「参照のみ」が禁止、「転記 + 出典」は推奨)。

**由来**: Claude Code Insights (2026-08-11) の「レビュー履歴が監査不能」指摘。
既存文書の違反棚卸しは別項目 (台帳参照) で扱う。
```

消化に伴う台帳運用: 順位 358 の詳細エントリ (どの todo ファイルかは `grep -rn "Cross-File Reference Lifecycle" docs/todo*.md` で特定) を削除し、`docs/todo-summary2.md:115` 付近の該当行も削除する。

### 2-2. 台帳への新規登録 5 件

**登録手順** (docs/todo.md の運用ルール準拠): 詳細エントリは `docs/todo22.md` 末尾へ、索引行は `docs/todo-summary2.md` の table 末尾へ追加。順位番号は**登録時点の最大順位 + 1 からの連番** (本計画書作成時点の最大は 432 だが、実行時に必ず todo-summary2.md 末尾で確認する)。todo22.md には新セクション `## Claude Code Insights フォローアップ採用分 (2026-08-12 採否確定)` を立て、由来として本計画書 (削除済みになるので「docs/insights-followup-plan.md、git log 参照」) を記す。

以下、各エントリのドラフト (todo22.md の既存エントリの体裁に合わせて整形する):

**(1) pre-push review verdict の telemetry 化** — 🔧 Tier 2 / Effort S-M

- 動機: pre-push review の APPROVE/REJECT・warning 件数が `.takt/runs/*/reports/*.md` に散在し機械集計不能 (Insights「レビュー履歴が監査不能」指摘)。順位 392 (push パイプライン terminal outcome の telemetry 化) と同一基盤 (ADR-055 firings JSONL) の隣接項目。
- 対処案: cli-push-runner または takt 後処理で、workflow 名・PR/change id・verdict・warning 件数を firings JSONL へ記録。実装時に順位 392 との統合を検討 (同時着手なら 1 PR で両方)。
- 参照: ADR-055、ADR-062 (月次 ROI レビューの入力になる)、順位 392。
- summary2 行の注記: 「なし (2026-08-12 Insights 採用。順位 392 と同一基盤、実装時に統合検討)」

**(2) committed docs → ephemeral 参照の既存違反棚卸し** — 💎 Tier 3 / Effort M

- 動機: 2-1 の規律制定時点で、todo13/16/18/20 等に `.claude/feedback-reports/<pr>.md Tier N #M` 形式の参照が多数残存。一括修正は範囲が大きく規律制定と分離した (2026-08-12 ユーザー判断)。
- 対処案: `grep -rn "feedback-reports\|\.takt/runs\|weekly-reviews" docs/` で全違反を列挙 → 要旨転記 or 参照削除を機械的に適用。件数が多ければ複数バッチに分割可。
- 参照: dev-conventions.md の新節 (2-1)、ADR-035。

**(3) security-review / supervisor-validation の output-contract 整備** — 🔧 Tier 2 / Effort S

- 動機: `.takt/facets/output-contracts/` には `simplicity-review.md` しかなく、`security-review` / `supervisor-validation` は `.takt/workflows/pre-push-review.yaml` (79 / 153 行目付近) で `format:` 名だけ宣言され契約ファイル不在。verdict 欄の書式が facet の自由記述に委ねられている (Insights「rubric 暗黙」指摘の残件)。
- 対処案: `simplicity-review.md` を雛形に 2 ファイルを新設。作成時チェックリスト (memory `takt-output-contract-checklist` より転記): ① builtin security-review の列構造・casing をミラー (snake_case field id + Title Case ラベルの混在は意図的)、② 全 finding セクションで列セット統一、③ finding_id は new/persists/resolved/reopened を通じて不変と明記、④ 追加 PR 自身の pre-push で dogfood。
- 参照: ADR-048、ADR-056。

**(4) feedback 4 軸 (Severity/Frequency/Effort/Adoption Risk) の値域定義** — 💎 Tier 3 / Effort XS-S

- 動機: `.takt/facets/instructions/aggregate-feedback.md` (155-175 行目付近) の Recommendation 表は 4 軸の列を持つが各軸の値域・境界が未定義 (「Medium とは何か」が書かれていない)。判定の一貫性が analyzer 依存。
- 対処案: 各軸に 3〜4 段階の値域と 1 行判定基準を同ファイルへ追記 (例: Frequency = High: 直近 1 か月で 2 回以上再発 / Medium: 過去に同型あり / Low: 初出)。
- 参照: ADR-030、ADR-062。

**(5) 蓄積 feedback レポートの横断 mining (承認付き)** — 💎 Tier 3 / Effort M

- 動機: `.claude/feedback-reports/` に 100+ 件が蓄積済みだが分析は per-PR のみ。横断クラスタリングで recurring anti-pattern (同型指摘の反復、却下理由の傾向等) を抽出できる (Insights Horizon 3 の承認境界維持版)。
- 対処案: ローカル実行の分析 (skill or takt facet) で系統別クラスタと防止策案を**提案レポートまで**生成。台帳登録・ルール化は従来通りユーザー承認必須 (`aggregate-feedback.md` の承認規約と ADR-072 信頼境界を変更しない)。完全自動化 (承認なしの rule 生成) は不採用と決定済み (2026-08-12)。
- 参照: ADR-072、ADR-038 (ローカル LLM 分類の先行事例)、順位 403 (feedback レポートの主張は実測で二重検証 — todo22.md 却下記録 #386 T1-2 参照)。

### 2-3. 順位 335 への注記追加

`docs/todo14.md:147-161` (post-merge-feedback の transcript 分析を summary index に置換) の本体エントリへ 1 行注記: 「2026-08-12 暫定対処済み: analyze-session facet に抽出先行手順を追加 (Insights フォローアップ PR 1)。本項は根本対処として有効なまま」。

### 2-4. 本計画書の削除

`docs/insights-followup-plan.md` (本ファイル) を PR 2 の diff に含めて削除する。

---

## 完了基準

- [ ] PR 1 マージ済み (facet 修正 + スクリプト 2 本削除)
- [ ] PR 1 マージ直後の post-merge feedback が token limit 衝突なしに完走 (`.claude/feedback-reports/<pr>.md` 生成 or 「知見なし」報告。`.failed` marker が残らないこと)
- [ ] PR 2 マージ済み (dev-conventions 新節 + 順位 358 エントリ/索引行削除 + 台帳 5 件登録 + 順位 335 注記 + **本計画書削除**)
- [ ] `docs/todo-summary2.md` の索引と todo22.md の詳細エントリが 1:1 対応 (cli-docs-lint が通ること)

## 実施時の運用注意

- ファイル編集前に `jj new` (dev-conventions.md 規約)。push は `pnpm push`、PR 作成・マージは ADR-028/052 のゲートに従いユーザー確認を経る。
- PR タイトル・本文の粒度提示規約 (PR N 本、うち新規 M 本) — 本計画は承認済みなので変更が生じた場合のみ再提示。
- `docs/todo22.md` が 50KB に近づいた場合は todo23.md 新設の運用 (todo.md 冒頭ルール) に従う。
- 本計画書が PR 1 の diff に混入するのは許容 (jj の自動 snapshot 下で分離コストをかけない)。PR 2 で削除されるため一時的なコミットで問題ない。
