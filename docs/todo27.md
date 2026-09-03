# TODO (Part 27)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: `docs/todo14.md` (61KB) と `docs/todo22.md` (59KB) が 50KB の安定読み取り閾値を超えたため、2026-09-03 に**両ファイルの大きいエントリ 10 件をここへ移した** (順位 512)。**新規エントリの追加先ではない** — 新規は `docs/todo26.md` に記録する。本ファイルは移送したエントリの編集・完了削除専用。
>
> **移送の基準**: サイズ上位から選んだ。移動した各エントリは順位 table (`docs/todo-summary*.md`) の「ファイル」列も本ファイル名へ更新してあり、`cli-docs-lint` の entry-pairing 検査が 1:1 対応を強制する。件数がそのまま編集量になるため、**同じバイト数を動かすのに最も件数が少ない選び方**を採った。
>
> **移送元の内訳**: `todo14.md` から順位 345 / 354 / 357 / 359 / 364 / 433、`todo22.md` から順位 414 / 431 / 432 / 445。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---
### 順位 433: cli-telemetry-report コード堅牢化 + 回帰テスト (月次 ROI レビュー PR #336/#337 post-merge feedback 採用)

> **動機**: 月次ハーネス ROI レビュー実装 (PR #335-338) の post-merge feedback で cli-telemetry-report に決定論的な堅牢化余地が挙がった。特に Phase C (snapshot 保持) の根本シナリオ = 月中に無効化→翌月再有効化される機構が既存 3 テストで未カバー (High)。Phase D の `confirmed_streak = zero_streak - partial` は前提 (`partial ⇒ zero_streak≥1`) が別関数依存で局所防御が無く、将来のリファクタで release build の silent underflow wrap により誤った deactivation 判定を招き得る。加えて `zero_firing_list` の MD/JSON 二重計算は出力乖離の温床。
>
> **対処案**: 下記 作業計画。いずれも既存テストモジュール / 局所コードへの追加で Effort 小。
>
> **参照**: PR #337 (Phase C+D) / PR #336 (Phase A) / PR #338 (Phase E)、`.claude/feedback-reports/337.md` (Tier1 #1 / Tier2 #1,#2)、`.claude/feedback-reports/336.md` (Tier1 #1 / Tier2 #1)、`.claude/feedback-reports/338.md` (Tier2 #2)、[ADR-062](adr/adr-062-monthly-harness-roi-review.md)、`src/cli-telemetry-report/src/{aggregate,verdict,report,registry}.rs`。
>
> **実行優先度**: 🔧 Tier 2 — Severity High〜Low (越境テストのみ High) / Frequency Low〜Medium / Effort S〜XS / Adoption Risk None。

#### 作業計画

- [ ] [High] aggregate.rs: 月中 無効化→翌月 再有効化トグルが `resolve_snapshot` を正しく通過する月間境界越境テストを追加 (#337 Tier2-1)
- [ ] verdict.rs: `confirmed_streak` 計算を `checked_sub` ベースの明示処理にし release build でも underflow を防止、`debug_assert!` (`partial ⇒ zero_streak≥1`) は診断用に併設 (#337 Tier1-1 + CodeRabbit PR #339)
- [ ] verdict.rs tests: `current_month_partial==0` の境界を明示テスト化し `confirmed_streak==zero_streak` 等価を保証 (#337 Tier2-2)
- [ ] report.rs: `zero_firing_list` の共有計算を `render()` で一度だけ実行し MD/JSON フォーマッタへ結果を渡す構造に抽出 (#336 Tier1-1)
- [ ] report.rs tests: MD/JSON 両出力で zero_firing (id集合 / provenance / last_fired_month) + source_failures 一致の回帰テスト (#336 Tier2-1)
- [ ] registry.rs tests: rule / preset / hook の 3 供給源が空・欠落時に同一 fail-open 挙動 (source_failures 計上) をすることを 1 テストで統一検証 (#338 Tier2-2、実装本体は #336 で対応済)

#### 完了基準

- 月中トグルシナリオ・streak 不変条件 (release-mode での判定保証を検証する回帰テストを含む)・MD/JSON 一致・3 供給源 fail-open が回帰テストで固定され、`cargo test --workspace` / clippy を通過すること。

---

### 順位 345: deploy 時の exe/config feature 互換性診断 (内容ベース、mtime 不使用)

> **動機**: deployed `.claude/*.exe` が古く、tracked config (`.claude/hooks-config.toml`) が要求する新 feature (例: `{{CLAUDE_DIR}}` プレースホルダー展開) を満たさないと、silent `command not found` で quality gate が誤 block する。本セッションで 2 回実観測 (PR #307 の `{{CLAUDE_DIR}}` 機能追加時、2026-07-20 WP-15 rebase 時、いずれも MEMORY.md 記録済)。PR #310 post-merge feedback Tier1 #1 で採用。
>
> **対処案**: deploy step で exe 埋め込みバージョン文字列と config 側 `min_exe_version` フィールドを**内容ベースで比較**する診断チェックを追加する。**mtime 比較は使わない** — jj tracked config は `jj workspace add`/checkout で mtime がリセットされ偽陽性/偽陰性を生む (既知の mtime-staleness 問題と同型)。将来的に [ADR-051](adr/adr-051-cross-system-config-coupling.md) の隣接領域として ADR 化も検討可。
>
> **参照**: `.claude/feedback-reports/310.md` Tier1 #1、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、`.claude/hooks-config.toml`、`scripts/deploy-artifacts.mjs`。
>
> **実行優先度**: 🚀 Tier 1 — Severity High (silent command-not-found で quality gate 誤 block) / Frequency Medium (2 回実観測) / Effort M / Adoption Risk None (mtime 回避設計であれば)。

#### 作業計画

- [ ] exe にビルドバージョン文字列を埋め込み、`.claude/hooks-config.toml` に `min_exe_version` フィールドを追加
- [ ] deploy step (`scripts/deploy-artifacts.mjs` or cli-merge-pipeline の deploy 処理) で内容ベースの互換性チェックを実装 (mtime 不使用)
- [ ] 互換性違反時に silent でなく明確なエラーで停止することを確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- config が要求する feature を満たさない古い exe が deploy されている場合、内容ベース比較で検出され silent `command not found` にならないこと。

---

### 順位 354: todo ファイル削除・更新時のチェックリストを dev-conventions.md に追加

> **動機**: PR #332 で todo16.md の複数セクション削除時に lint:md を 3 回以上再実行する非効率を観測した。todo ファイルの段階的削除と都度 lint:md 実行の手順が明文化されておらず、削除漏れ・lint 崩れ・summary 行との不整合が起きやすい。#332 post-merge feedback Tier3 #8 で採用。専用スクリプト化 (#332 Tier2 #1) は ADR-033 効果待ちで様子見だが、チェックリスト明記自体は Effort XS の無リスク即応策として独立採用可能。
>
> **対処案**: `docs/dev-conventions.md` に「todo ファイルの削除・更新時は (1) 詳細エントリ (todoNN.md) と summary 行 (**該当順位を収める `docs/todo-summary.md` または `docs/todo-summary2.md`**。順位 220 未満は前者) を対で更新、(2) 段階的に削除し都度 lint:md で整合確認、(3) 削除する順位を指す本文参照を残さない ([ADR-033](adr/adr-033-todo-numbering-simplification.md) § アンチパターン)」のチェックリストを追加する。
>
> **2026-08-16 更新**: 当初の対処案 (3) は「順位番号を本文に書かない」だったが、[ADR-033](adr/adr-033-todo-numbering-simplification.md) § 改訂 が本文参照の禁止を緩和したため、**削除済み順位への参照を残さない**へ置き換えた。相補関係にあった順位番号 lint rule のタスクは同日 retire している。
>
> **参照**: `.claude/feedback-reports/332.md` Tier3 #8、`docs/dev-conventions.md`、[ADR-033](adr/adr-033-todo-numbering-simplification.md)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low / Frequency Medium / Effort XS / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` に todo ファイル削除・更新チェックリストを追加 (詳細/summary の対更新・段階削除+都度 lint:md・削除済み順位への本文参照を残さない)
- [ ] 本エントリ削除 + 該当順位を収める summary index (`docs/todo-summary.md` または `docs/todo-summary2.md`) の行削除

#### 完了基準

- todo ファイルの削除・更新時に段階削除と整合確認の手順が checklist 化され、削除漏れ・lint 崩れ・summary 不整合が防止されること。

---

### 順位 357: CLAUDE.md の ADR index ステータスタグと ADR 本体ステータスの整合チェックを追加

> **動機**: PR #340 で CLAUDE.md の ADR-047 index タグが `*(試験運用)*` のまま、ADR-047 本体のステータス「却下 (2026-07-19 確定)」と乖離して残存していることを、pre-push simplicity review と post-merge 分析が独立に指摘した (実害継続を Read で確認済み)。index タグと本体ステータスの整合は手動更新に依存しており、ステータス遷移 (試験運用 → 採用/却下) のたびに再発しうる。#340 post-merge feedback Tier1 #1 で採用。
>
> **対処案**: CLAUDE.md の ADR index 行のステータスタグと、対応 ADR ファイル本体の「ステータス」見出しの一致を検証する doc-consistency チェックを pre-push 経路に追加する。[ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の正規表現層/AST 層はいずれも単一ファイル起点設計のため、`custom-lint-rules.toml` への追加ではなく独立チェック (cli-docs-lint 拡張 or 専用スクリプト/test) として実装する。**責務分界 (PR #341 CodeRabbit 指摘で明文化)**: 本 entry はステータスタグ整合のみを扱い、採番重複/索引存在/番号一致は順位 272 の責務。実装は同一 validator module への同居が可能で相補。
>
> **参照**: `.claude/feedback-reports/340.md` Tier1 #1、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md)、[ADR-047](adr/adr-047-prepush-refute-facet.md)、順位 272 (同居実装候補)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium / Frequency Medium / Effort M / Adoption Risk None。

#### 作業計画

- [ ] CLAUDE.md の ADR-047 タグを `*(試験運用)*` → `*(却下)*` に修正 (実残存の不整合解消、着手時の即修正)
- [ ] ADR index タグと ADR 本体「ステータス」見出しの整合チェックを実装 (cli-docs-lint 拡張 or 独立スクリプト、順位 272 と同居検討)
- [ ] pre-push 経路 (lint:docs) への組込みと、不整合 fixture での検知確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- CLAUDE.md の ADR index タグと ADR 本体ステータスの乖離が pre-push で機械検知されること (採番/索引存在/番号一致の検知は順位 272 の完了基準で扱い、本 entry の対象外)。

---

### 順位 359: WP-16 系 post-merge feedback 文書系 10 件の docs バッチ (dev-conventions 集中)

> **動機**: PR #342 (CI matrix / ADR-065)・#343 (監視 CI 観測修正)・#344 (pipeline_lock レース修正) の post-merge feedback で採用確定した文書系 10 件を、1 本の docs バッチ PR に集約する (2026-08-02 方針決定。per-PR の細切れ doc PR を避け milestone でまとめる運用)。全件 `docs/dev-conventions.md` 中心の追記で、GitHub 仕様の gotcha など Severity High 2 件を含む。
>
> **内容 (10 件)**:
>
> 1. `[workspace] default-members` 不在で `cargo test` = `cargo test --workspace` という暗黙不変条件の明記 + `push-runner-config.toml` への inline comment (#342 T3-1。順位 360 のテストと対)
> 2. ADR-051 (cross-system config coupling) をチェックリストに登録 — 3 系統以上のインフラ設定を跨ぐ変更時の確認項目 (#342 T3-3)
> 3. ambient/auto-detect 環境状態 (git ブランチ名・`GH_REPO`・cwd) に依存せず明示引数を使う設計原則 — PR #238/#247/#343 の 3 例目で systemic (#343 T3-1)
> 4. 並行バグ調査の標準テストパターン — 決定論再現テスト + stress の aggregate 計測 + low-core CI でのみ再現するレースの扱い (#344 T2-1)
> 5. TOCTOU + 2 層防御 (verify-before-destroy + deferred cleanup、最終 fallback は `create_new` 排他へ収束) の設計原則 (#344 T3-1)
> 6. CI 失敗の introduced-by-this-change / pre-existing を diff で切り分けるチェックリスト (#344 T3-2)
> 7. `paths:` フィルタ付き check を required 化すると skip が pending 扱いで PR が永久ブロックされる GitHub gotcha + early-success 代替 (#342 T3-2、Severity High)
> 8. exe-spawn テストは exe + deploy 済 config を temp dir へ staging する規約 — 既存 bounded wait 規約と対 (#342 T3-4、Severity High)
> 9. cross-platform matrix の `fail-fast: false` 既定 (#342 T3-5)
> 10. shell 抽象化確認済み cfg ガードの除去可否ガイダンス + レビュー層がコード内コメント根拠で false positive 判定する際の前提再検証手順 (#342 T3-7)
>
> **参照**: `.claude/feedback-reports/342.md` / `343.md` / `344.md` (各 Rationale)、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、[ADR-063](adr/adr-063-linux-portability-release-binaries.md)。
>
> **実行優先度**: 💎 Tier 3 — 各件 Effort XS〜S・合計 M / Adoption Risk None。Severity High 2 件 (#7, #8) を含むため docs バッチとしては早めの実施が望ましい。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ 10 件を追記 (既存 convention の書式に合わせる)
- [ ] `push-runner-config.toml` へ #1 対応の inline comment を追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 10 件すべてが dev-conventions.md (+ inline comment 1 箇所) に反映され、`pnpm lint:docs` / markdownlint が clean であること。

---

### 順位 364: ADR-054 scope guard の pre-push 展開 — fix diff の allowlist 照合 (ADR-068 残課題)

> **動機**: pre-push の takt fix step には「finding 由来 allowlist との fix diff 照合」の決定論層が無く、instruction (fix.md の scope allowlist) 頼み。2026-08-02 の WP-17 PR 2a incident (fix が finding 対象外の lib crate 2 つを丸ごと削除し gate 全 PASS で push) で顕在化した。[ADR-068](adr/adr-068-fix-step-authority-boundary.md) の後退検知 backstop は削除系 (ファイル脱落 / 追加行削減) のみ検知する 80/20 の暫定で、**追加系の injection (finding 対象外ファイルへの書き込み・config 書き換え) は検知できない**。PR #348 security review の non-blocking 注記 (fix step が push-runner-config.toml の `max_added_line_shrink_pct` / `enabled` を書き換えて backstop 自体を自己弱体化できる経路が instruction 頼み) もこれで閉じる。
>
> **対処案**: `cli-push-runner` の post_takt_regate 段 (または直前の専用 stage) で、`.takt/runs/` の最新 findings レポートから `Location` 列を抽出して allowlist を導出し、takt 前後の diff 差分の変更ファイルを照合する。判定コアは `lib-scope-guard` (WP-17 再分割 PR で land 予定) を再利用し、cli-pr-monitor の post-pr 経路と判定の同一性を保つ (ADR-054 の drift 防止)。violation は ADR-068 の `[FIX_REGRESSION]` と同様の loud block + 独立 kill-switch。findings レポートのパース失敗は fail-closed。
>
> **参照**: [ADR-068](adr/adr-068-fix-step-authority-boundary.md) § 決定 3 / 残課題、[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) § 欠点 (pre-push 展開の予告元)、`src/cli-pr-monitor/src/stages/scope_guard.rs` (post-pr 側の先行実装)、PR #348 security review 注記。依存: WP-17 再分割 PR (lib-scope-guard の land) 後が効率的。
>
> **実行優先度**: 🔧 Tier 2 — Severity High (injection 防御の穴) / Frequency Low (fix 発生時のみ) / Effort M / Adoption Risk Low (既存 stage への追加、kill-switch つき)。

#### 作業計画

- [ ] findings レポート (.takt/runs/ 最新 run) から Location 列を抽出する parser (fail-closed)
- [ ] lib-scope-guard で allowlist 照合、violation は loud block + 独立 kill-switch
- [ ] incident 再現テスト: (a) finding 対象外ファイルへの変更が**変更種別 3 種 (追加 = 新規ファイル作成 / 書き換え = 既存ファイル編集、config 自己弱体化含む / 削除) のいずれでも** block されること、(b) `ALWAYS_ALLOWED` 対象ファイル (`.takt/review-diff.txt` 等) への変更は finding allowlist 外でも block されないこと、の両方を固定する (完了基準の「追加・書き換え・削除いずれも」に対応)
- [ ] fix.md / fix-supervisor.md の「pre-push は後退検知のみ」記述を更新
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- fix step が finding 対象外ファイルを変更 (追加・書き換え・削除いずれも) した push が、決定論的に block されること。ADR-068 の後退検知では通ってしまう「追加系 injection」ケースがテストで固定されていること。
- `ALWAYS_ALLOWED` (post-pr 側の先行実装 `src/cli-pr-monitor/src/stages/scope_guard.rs` で定義済み、現状 `.takt/review-diff.txt` のみ) は、fix step が本 instruction (fix.md 「Pre-completion diff refresh」) に従って正当に書き換える中間ファイルの例外リストである。pre-push 側の実装も finding allowlist に加えてこのリストを常に許可し、post-pr 側と同一のリストを共有すること (ADR-054 の drift 防止)。この例外により (a) の block 判定が誤って `.takt/review-diff.txt` 自体の正当な refresh まで block しないことをテストで固定する。

### 順位 414: 系統 A-1: 「各出力面は新しい perimeter」原則と screening 関数の出口別分離を明文化する

> **動機**: PR [#389](https://github.com/aloekun/claude-code-hook-test/pull/389) で PR タイトルという 3 つ目の公開面が増えた際、既存の `screen_for_public_output` を流用できないことが判明した。あちらの無害化は**「workflow がコードスパンで囲む」ことが前提**で `@mention` と markdown を verbatim に残す設計だったが、**タイトルはコードスパンにできない**。
>
> **systemic pattern である**: 3 ソース (PR diff / セッション / pre-push security review) が独立に同じ原則を指摘した。過去にも同型が起きている — [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 14 の初版は「公開面 = PR 本文」と狭く見て **step ログを見落とし**、`tee` 経由の露出を後から塞いだ。**公開面は塞ぐたびに次が見つかる**。
>
> **対処案**: (a) 「新しい出力面を足すときは、既存 screening を流用してよいかを**囲いの有無**から判断する」を convention として明文化、(b) [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) へ **output surface × wrapping context の対応表**を追記する (どの出口がどんな囲いを持ち、それゆえ何を追加処理すべきか)。
>
> **参照**: [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 14 § 3 つ目の公開面、[screening.rs](../src/lib-ledger/src/screening.rs) (2 関数の対照が実装済み)。
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Frequency **High** (出力面は増え続ける) / Effort S / Adoption Risk None。

#### 作業計画

- [ ] `docs/dev-conventions.md` へ「出力面ごとの screening」節を追加する (囲いの有無から必要処理を導く判断手順)
- [ ] ADR-054 へ output surface × wrapping context の対応表を追記する
- [ ] 既存の出力面 (PR 本文 / step ログ / PR タイトル / marker 本文) を棚卸しし、表の初期値を埋める

#### 完了基準

- 新しい出力面を足す人が、既存 screening を流用してよいかを**表を見るだけで判断できる**こと。

### 順位 431: `review-request` の成功判定を初回レビュー取得まで遅らせる

> **動機**: 2026-08-11 の夜間ループ実走 (PR [#387](https://github.com/aloekun/claude-code-hook-test/pull/387)) で、CodeRabbit がレート制限により `Review limit reached` を返した。`review-request` の検証は**要求後に CodeRabbit のコメントが 1 件以上付いたか**だけを見るため、**拒否も success として記録**され、run は緑で終わった。
>
> **契約どおりではある**: この検証は決定 11 の失敗 (10 時間の無反応に気づけなかった) への対策で、workflow のコメントにも「投稿の成否ではなく CodeRabbit の反応を待つ」と明記されている。**設計の欠陥ではない。**
>
> **問題は残る**: 結果として「レビューが付かないまま success で終わった自律 PR」がどこにも信号として残らない。[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § M5 の方針でリトライ機構を持たないため、解除後に自動で再要求されることもない。翌朝レビューする人間は、PR を開くまで未レビューだと分からない。
>
> **対処案**: (a) 反応の**中身**を判別し、レート制限による拒否は success としない (run を warning か failure にする)、(b) 未レビューの自律 PR を検出する信号を別に持つ (weekly-review の自律アクション棚卸しに載せる = WP-19 ステップ 3)、(c) 解除見込み時刻を run log に出して人間が再要求できるようにする。**リトライ機構の作り込みは § M5 の方針に反する**ため、検出と可視化に留めること。
>
> **参照**: [review-request.yml](../.github/workflows/review-request.yml)、[ADR-072](adr/adr-072-nightly-todo-loop.md) § 定常運用 2 巡目の実走観測、[ADR-019](adr/adr-019-coderabbit-review-hybrid-policy.md) § 無料枠の窓は固定時刻ではなく直近の消費に追随する / § M5、[ADR-064](adr/adr-064-monitor-success-positive-evidence.md) (陽性証拠の要求)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (未レビューの自律 PR が可視化されない) / Frequency Medium (人間が 2 本マージした夜) / Effort S-M / Adoption Risk Low (判別を厳しくしすぎると正常な反応まで failure にする)。

#### 作業計画

- [x] CodeRabbit の応答パターンを分類する (2026-08-20 実装)。`REVIEWED` (walkthrough marker) / `RATE_LIMITED` / `OTHER` の 3 分類とし、`OTHER` (ack・skip 通知・未知 format) は陽性証拠に数えず deadline 到達で red = 安全側 (未取得扱い) へ倒す。
  - **台帳の前提がずれていた**: 拒否の実体は walkthrough の `Review limit reached` placeholder ではなく **command ack** (`<!-- This is an auto-generated reply by CodeRabbit -->` + `⚠️ Action not completed` + `Review rate limited.`) で、`markers.rs` の `RATE_LIMIT_MARKERS` の**どちらにも一致しない** (PR #387 の実 body を `gh api` で確認)。本 workflow の marker は markers.rs の上位集合とし、`Review rate limited.` を workflow 固有 marker として追加した。
  - rate-limit の判定は陽性証拠より**先**に行う。#387 では拒否 ack と summarize marker 付き placeholder が 3 秒差で並んでおり、先に陽性証拠を探すと placeholder をレビュー実体と読んで silent success に戻る。
- [x] success 判定を**陽性証拠へ寄せる**方針に決定 (2026-08-20、ユーザー判断)。レート制限検出時は `[REVIEW_REQUEST_RATE_LIMITED]` を出して **red で落とす**。リトライは作らない (ADR-019 § M5) ため、run の色が「この PR は未レビューのまま残った」の即時信号になる。成功条件を厳しくしたぶん待機上限を 10 分 → 15 分へ延長 (job の `timeout-minutes` も 15 → 20)。
- [x] weekly-review との役割分担を workflow 先頭に明記 (2026-08-20)。本 workflow は**即時信号のみ**で再要求はせず、未レビュー PR を全体で拾い直すのは weekly-review の自律アクション棚卸し (WP-19 ステップ 3) 側とする。両者は独立に動く。
- [ ] **実走観測**: 次にレート制限が起きた夜間 run で red + `[REVIEW_REQUEST_RATE_LIMITED]` が出ることを確認してから本エントリを削除する。

#### 完了基準

- レート制限で弾かれた自律 PR が、run の色か後続の棚卸しのいずれかで**未レビューと分かる**こと。

> **現在地 (2026-08-20)**: 実装済み。分類 jq は workflow ファイルから切り出して実 PR (#387 / #426 / #421 / #419) のコメント列へ適用し、**#387 (本エントリの由来 incident) が `RATE_LIMITED` = red、正常レビュー済みの 3 件が `REVIEWED` = green** になることを実測した。marker が複数層に分散する構造なので `scripts/lint-workflows.mjs` に同期検査を追加している (片方だけ変えると silent success に戻るため)。**完了基準の判定は実走観測待ち。**

### 順位 432: `check_concurrent_run_guard` の `.takt/runs` 全走査コストと保持ポリシー

> **動機**: PR [#388](https://github.com/aloekun/claude-code-hook-test/pull/388) の pre-push review (simplicity、non-blocking) の指摘。`check_concurrent_run_guard` は呼ばれるたびに `.takt/runs/*` を**全ディレクトリ走査し各 `meta.json` を JSON パース**する。旧実装は `context.json` の mtime を 1 回読む O(1) だった。
>
> **実測 (2026-08-11)**:
>
> | 項目 | 値 |
> |---|---|
> | run ディレクトリ数 | **538** |
> | 内訳 | pre-push-review 268 / post-pr-review 147 / その他 |
> | 最古の run | 2026-06-26 (46 日分が蓄積) |
> | `.takt/runs` の総容量 | **174 MB** |
> | クリーンアップ機構 | **無し** |
>
> **現時点で実害は無い** (538 ファイルのパースは 1 秒未満、マージは 1 日数回)。問題は**増加が単調で削除する仕組みが無い**こと。
>
> **対処案** (独立に進められる 2 方向):
>
> 1. **走査を絞る** — dir 名に workflow 名が含まれる規約 ([ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § task labeling convention) を使い、`meta.json` を読む前に名前でフィルタする。数行で定数が 1/4 以下になる。**局所改善で低リスク**
> 2. **保持ポリシーを作る** — 週次レビュー等で古い run を畳む。174MB の削減にもなるが、run log は障害調査の資料でもあるため**保持期間の判断が要る** (どこまで遡って調査するかの実績を先に見るべき)
>
> **参照**: [markers.rs](../src/cli-merge-pipeline/src/feedback/markers.rs) (`check_concurrent_run_guard`)、[run_registry.rs](../src/cli-merge-pipeline/src/feedback/run_registry.rs) (`collect_feedback_runs`)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § task labeling convention、[ADR-031](adr/adr-031-weekly-review-pipeline.md) (週次の棚卸し先)。
>
> **実行優先度**: 💎 Tier 3 — Severity Low (現時点で実害なし) / Frequency Medium (単調増加) / Effort S (案 1) 〜 M (案 2) / Adoption Risk Low。

#### 作業計画

- [ ] 案 1 (名前フィルタ) を先に入れる。実測で効果を確認する (パース回数の削減)
- [ ] 案 2 の保持ポリシーは、run log を実際に何日前まで遡ったかの実績を確認してから期間を決める
- [ ] `.takt/runs` の容量を定期的に見る仕組みが要るかを判断する (週次レビューへ載せるか)

#### 完了基準

- 次のいずれかが達成され、どちらを採ったかが根拠つきで記録されていること。
  - **案 1**: `meta.json` の**パース件数**が post-merge-feedback の run 数まで減っていること (実測で確認)。ディレクトリの列挙自体は残るため **O(n) は解消しない** — 削減できるのは読み取り / パース回数である
  - **案 2**: 保持ポリシーにより `.takt/runs` の run 数に上限が定まっていること

---

## 台帳整理バッチ (2026-08-12): todo2.md 退役に伴う移送 2 件 + docs 棚卸しの新規起票 7 件

> **由来**: docs/ 直下の一時作業ドキュメント全件棚卸し (2026-08-12) の採否確定分。旧 todo2.md の ADR-032 ブロック (docs-only 高速パス) は [ADR-057](adr/adr-057-docs-only-deterministic-routing.md) が別設計で実現したため退役し、独立価値の残る 2 タスクのみ本ファイルへ移送した。加えて棚卸しが発見した構造問題 7 件を起票した。

### 順位 445: todo preamble と facet routing 記述の整合を lint で機械検証する

> **動機**: `docs/todo.md` preamble が列挙する todo ファイル群 (新規追加先 / 編集専用 / 列挙範囲) と、それを参照する `.takt/facets/instructions/review-todo-whole.md` の routing 記述が**独立に手で維持されており、片方だけ古くなる**。
>
> 2026-08-13 の PR #395 で実際に発生した: preamble の新規追加先が更新される一方、facet 側には `todo6.md` / `todo2-7.md` という**旧世代の固定値**が残り、whole-tree review が古い送付先を案内していた。`cli-docs-lint` は preamble の数詞は見るが**列挙範囲と実ファイル群の集合一致は検証しない**ため、この class は機械層に穴がある。weekly-review が 50KB 超過のたびに todo ファイルを増やす構造上、再発は継続的に起こる。
>
> **参照**: [review-todo-whole.md](../.takt/facets/instructions/review-todo-whole.md) (routing 記述)、[docs/todo.md](todo.md) preamble、`src/cli-docs-lint/`、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md) (正規表現層/AST 層の線引き)、[dev-conventions.md](dev-conventions.md) § 同一事実が複数箇所に分散する場合の変更手順 (本タスクが入るまでの暫定 convention)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (誤誘導であり実行時破壊ではない) / Frequency **Medium** (todo ファイルは継続的に増える) / Effort S / Adoption Risk None。

#### 設計決定

`cli-docs-lint` に検査を追加する (custom lint rule ではなく docs-lint 側。preamble 解析は既に同 exe が持っているため)。

**集合の作り方を先に固定する。** ここを曖昧にすると誤検出か検査漏れのどちらかが必ず出る:

- **対象は番号付きの詳細ファイルのみ** — `docs/todo*.md` の素の glob は `docs/todo-summary.md` / `docs/todo-summary2.md` も拾う。これらは順位 table であって詳細エントリの追加先ではないので、`todo<数字>.md` に限定する (`todo.md` 本体の扱いも明示的に決める)。
- **範囲表記は展開してから比較する** — preamble と facet instruction はどちらも `todo3.md 〜 todo23.md` / `todo3-23.md` のような範囲表記を使う。文字列のまま集合比較すると常に不一致になるため、範囲を展開して要素の集合へ落とす。
- **数詞と列挙範囲は別の検査** — 既存 `cli-docs-lint` は数詞 (「24 つ」) を見ているが、列挙範囲が実ファイル集合と一致するかは見ていない。本タスクで足すのは後者。

- [ ] 集合抽出規則を実装する (番号付き詳細ファイルのみ / 範囲表記の展開)
- [ ] preamble の列挙集合と `docs/todo<数字>.md` の実ファイル集合を比較する
- [ ] facet instruction 側の routing 記述に含まれる `todoN.md` 参照を抽出し、preamble の集合と矛盾しないか検査する
- [ ] fixture テスト (good / bad) を追加する。**bad 側に「summary ファイルを誤って含む」「範囲表記が未展開」の 2 ケースを必ず入れる** (本タスクの取りこぼし要因そのもの)
- [ ] 本タスク land 後、[dev-conventions.md](dev-conventions.md) § 同一事実が複数箇所に分散する場合の変更手順 の routing 該当部分を撤去する (ADR-042 のルール vs 仕組み化)

#### 完了基準

- preamble と実ファイル群、preamble と facet routing 記述の不一致が `pnpm lint:docs` で検出されること。
- 検出が fixture テストで固定され、`cargo test --workspace` が green であること。

---
