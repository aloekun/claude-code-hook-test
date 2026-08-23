# TODO (Part 15)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo13.md がファイルサイズ約 171KB (50KB 安定読み取り閾値の約 3.4 倍) に達したため、順位 248〜296 のエントリを本ファイルに分離した (2026-07-20 docs 50KB 超過解消の物理分割)。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo3.md 〜 todo19.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---
### Gate Function Design Checklist を新規 guide として追加 (fail-closed パターン集) (PR #234 post-merge-feedback T3-1 採用)

> **動機**: fail-closed 実装の失敗パターンと推奨パターンが複数の ADR / memory に分散しており、新規 gate 実装者が再発させるリスクが高い。PR #234 で `collect_oversize_files` の初版が `.ok()?` で読み取り失敗を握り潰す fail-open bug を含み CodeRabbit Major #234-1 で指摘された。gate 実装の失敗/推奨パターンを 1 箇所に集約する。
>
> **本タスクの位置づけ**: PR #234 post-merge-feedback Tier 3 #1 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。同 feedback の T1-1 (`filter_map + .ok()?` の linter 化) / T1-2 (TOCTOU linter 化) は false positive 多発リスクで却下推奨となったため、その補完としてドキュメント化が必須。
>
> **参照**: `.claude/feedback-reports/234.md` Tier 3 #1、`docs/adr/adr-043-security-gates-fail-closed.md` (fail-closed 原則)、順位 249 (ADR-043 コード例追記、相補)、custom lint ⑫ `no-hardcoded-jj-revset-range`。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。

#### 作業計画

- [ ] Gate Function Design Checklist を `CLAUDE.md` patterns section または `docs/guides/gate-functions.md` に新設: (1) 判定不能状態は fail-closed、(2) gate 関数内で `filter_map + .ok()?` 禁止、(3) single-pass file access で TOCTOU 回避、(4) iterator chain + `Result::?` idiom で nesting depth 抑制、(5) エラーパスを明示的にテスト
- [ ] ADR-043 (順位 249) との相互リンク
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- fail-closed gate の失敗/推奨パターンが 1 箇所に集約され、新規 gate 実装者が参照して再発を防げる。

---

### ADR-043 に fail-open vs fail-closed の具体コード例を追記 (PR #234 post-merge-feedback T3-2 採用)

> **動機**: ADR-043 は security-critical だが具体的なコード例が未記載で、解釈の分散が PR #234 の `.ok()?` fail-open bug を生んだ。`.ok()?` anti-pattern / single-read + `ErrorKind` inspection idiom / multi-step vs 単一操作の比較を ADR 本文に追記し、レビュー時の一貫した判断基準を提供する。
>
> **本タスクの位置づけ**: PR #234 post-merge-feedback Tier 3 #2 採用 (Severity Medium / Frequency Medium / Effort S / Adoption Risk None)。順位 248 (運用チェックリスト) と相補的な決定記録。
>
> **参照**: `.claude/feedback-reports/234.md` Tier 3 #2、`docs/adr/adr-043-security-gates-fail-closed.md` (追記先)、順位 248 (Gate Function Design Checklist)。
>
> **実行優先度**: 💎 **Tier 3** — Effort S。

#### 作業計画

- [ ] ADR-043 に具体コード例 section を追加 (`.ok()?` anti-pattern / single-read + `ErrorKind` idiom / TOCTOU 回避の単一操作)
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- レビュー時に fail-open / fail-closed の判断基準が具体コードで参照でき、解釈の分散が解消される。

---

### ADR-021 に「jj revset の base branch は config/arg 化 (hardcode 禁止)」を明文化 (PR #234 post-merge-feedback T3-3 採用)

> **動機**: PR #234 で `[file_length_gate] base` を config 引数化する ADR-021 準拠パターンを実装した (default `master`、`format!("{}..@", base)`)。custom lint ⑫ `no-hardcoded-jj-revset-range` は `.rs` の `master..@` literal を捕捉するが、TOML config / docs / 他ツールへの原則適用は明文化されていない。base branch hardcode 禁止の原則を明文化する。
>
> **本タスクの位置づけ**: PR #234 post-merge-feedback Tier 3 #3 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。jj change detection は複数ツールで多用されるため原則の明文化価値がある。
>
> **参照**: `.claude/feedback-reports/234.md` Tier 3 #3、`docs/adr/adr-021-jj-change-detection-principles.md` (追記先)、custom lint ⑫ `no-hardcoded-jj-revset-range`。
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。

#### 作業計画

- [ ] ADR-021 (または `CLAUDE.md`) に「jj revset の base branch は config / arg 化し hardcode 禁止」の原則を明文化 (`.rs` / TOML config / docs / 他ツール横断)
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- base branch hardcode 禁止の原則が明文化され、新規 jj 変更検出実装で参照できる。

---

### ADR-NNN (採番未確定、land 時に確定): 部分効果 env var anti-pattern の文書化 (PR #239 post-merge-feedback T3-1 採用)

> **動機**: サブコマンドによって効果が異なる env var は「一部のコマンドが成功する」ことで全体が動いていると誤認させ、silent 部分故障を招く。実例 = `GH_REPO` は `gh pr create/merge` には効くが引数なし `gh repo view` には効かず、PR #238 で「マージ成功 / post-merge feedback silent 消失」の部分故障が発生した。`gh-repo-env-guard` preset (PR #239) が GH_REPO 個別には機械防御するが、「なぜ partial coverage が危険か」の原則が未文書化で、同型のショートカット提案 (例: `GH_HOST` 系 env var) を reviewer / implementer が即認識できない。
>
> **参照**: PR #238 (実害) / PR #239 (preset 実装 + feedback 提案 #1)、ADR-045 § PR 運用時の追加設定、`.claude/hooks-config.toml` gh-repo-env-guard preset。
>
> **実行優先度**: 💎 Tier 3 — Effort S。Severity Medium + Frequency Medium + Adoption Risk None (PR #239 post-merge-feedback T3-1、ユーザー採用 2026-07-03)。

#### 作業計画

- [ ] 新 ADR (順位 135 placeholder policy 適用) に「部分効果 env var」anti-pattern を codify: 定義 / PR #238 実例 / 判定基準 (env var による回避策採用時は対象コマンド全系統でのカバレッジ確認を必須化) / 推奨代替 (全系統に効く機構 = GIT_DIR 自動注入型、または明示フラグ)
- [ ] CLAUDE.md の ADR 一覧にリンク追加 (ADR-022 の「CLAUDE.md はリンクに留める」方針に従い本文は ADR 側へ)
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- 将来の env var ベースの回避策提案に対し、reviewer / implementer がカバレッジ確認を求める根拠文書が ADR として存在し、CLAUDE.md から辿れること。

#### 詰まっている箇所

- なし。

---

### ADR-030 に PR #239 の feedback silent skip 実装記録を追記 (PR #239 post-merge-feedback T3-2 採用)

> **動機**: 非 colocated jj workspace での owner_repo 検出失敗 → `.failed` marker 未書込 → L2 recovery 未発動という silent skip シナリオ (PR #238 実観測、feedback が recovery 不能なまま消失) と、その対処 (`AiStepContext` enum 化 + `SkipWithMarker` variant + `skip_with_failed_marker()`) を ADR-030 に実装記録として残し、次回同類問題の参照点にする。
>
> **参照**: PR #238 (実害) / PR #239 (`src/cli-merge-pipeline/src/pipeline.rs` の `AiStepContext::SkipWithMarker`)、ADR-030 (失敗マーカーによる recovery)。
>
> **実行優先度**: 💎 Tier 3 — Effort XS。Severity Low (既修正) + Frequency Low (PR #239 post-merge-feedback T3-2、ユーザー採用 2026-07-03)。次回 ADR-030 を参照・編集する PR への同乗で消化可。

#### 作業計画

- [ ] ADR-030 に「owner_repo 検出失敗などの実行前 skip も marker 付き skip とし L2 recovery 対象にする (`AiStepContext::SkipWithMarker`)」の実装記録 sub-section を数行追記 (PR #238 シナリオを inline cite)
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- ADR-030 を読んだ実装者が「feedback の skip 経路はすべて marker を残す」規約を実装記録から把握できること。

#### 詰まっている箇所

- なし。

---


### ADR-040 の実測値を新 GPU (RTX PRO 5000 48GB) で再 calibration (ADR-046 WP-01 スパイクで陳腐化を観測)

> **動機**: ADR-038 / ADR-040 は Local LLM の実行環境を **RTX 3070 8GB** として実測値を固定しているが、実機は **NVIDIA RTX PRO 5000 Blackwell 48GB** に更新済み (2026-07-04 に `nvidia-smi` で確認)。この結果、(1) ADR-040 の VRAM/latency trade-off 表 (例: mistral:7b ~2GB at 32K ctx) と (2)「VRAM scarcity → 同時起動不可 / model swap 制約 / KV cache budgeting」という framing が陳腐化した。27-31B Q4 モデルが 100% GPU で動き (qwen3-coder:30b ~21.8GB / gemma4:31b ~20.9GB / gemma4:26b ~17.6GB at num_ctx 32768)、VRAM ではなく latency が実効制約になった。
>
> **参照**: ADR-040 (Local LLM Context Size、実測値元)、ADR-046 (WP-01 スパイク、4 モデルの VRAM・latency 実測を保持)、ADR-038 (現行 classifier、RTX 3070 前提の記述)、memory `gpu-upgrade-rtx-pro-5000`。
>
> **実行優先度**: 💎 Tier 3 — Effort S。実装変更を伴わず ADR amendment 中心。分類層 (ADR-038) の運用に直接の不具合はないが、num_ctx 再選定や派生プロジェクト porting 時に誤った RTX 3070 前提を引き継ぐリスクを解消する。

#### 作業計画

- [ ] ADR-040 に amendment: RTX 3070 8GB の実測表は「旧環境 (historical)」と明示し、新 GPU での再測定値 (ADR-046 の VRAM 実測 + 代表 diff の latency) を追記
- [ ] 「Context 選定の判断 flow」の memory 軸 (同時起動可否 / swap) を latency 軸へ再重み付け
- [ ] ADR-038 の RTX 3070 前提記述 (§コンテキスト / §帰結の VRAM 8GB 制約) に更新環境への参照を付す
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- ADR-040 を読んだ実装者が、現行 GPU では VRAM が制約でなく latency が実効制約であることを把握でき、RTX 3070 8GB の数値を現行前提と誤認しないこと。

#### 詰まっている箇所

- なし (GPU 更新の事実・ADR-046 の実測値あり)。

---

### classifier FP 検出強化プロンプトで格上げ候補を再評価 (WP-04 見送りの follow-up、ADR-038 amendment 由来)

> **動機**: WP-04 (classifier モデル格上げ) の実測で、mistral:7b からの格上げ候補 (gemma4:12b/26b/31b, qwen3-coder:30b) は **いずれも `false_positive_likely` 検出を改善しなかった** (gold FP 6 件中、正検出は最良 qwen3-coder でも 1 件、全モデルが 3〜4 件を有害な auto_fix に誤分類)。ただし eval で使った `classify.txt` は mistral:7b 向けに tune 済みのため、「FP 検出が能力限界なのか、プロンプト不適合なのか」が未分離。FP 検出を明示的に強化したプロンプト版で候補を再測し、切り分ける。
>
> **参照**: ADR-038 § classify モデル格上げの評価と見送り (2026-07-05 追記、WP-04)、`src/cli-finding-classifier/prompts/classify.txt`、`src/cli-finding-classifier/src/main.rs` (`--prompt-file` で差し替え可)、WP-04 scratchpad の eval セット (Opus gold 35 件) + ハーネス。ADR-019 § 既知 CodeRabbit FP パターン (キュレート FP 例の出典)。
>
> **実行優先度**: ⏳ Tier 5 — Effort M。現行 mistral:7b は安全軸完璧・最軽量で運用に支障なく、優先度は低い。materially better な新 local モデル出現時も再評価トリガー。

#### 作業計画

- [ ] FP 検出強化版 `classify.txt` を作成 (false_positive_likely の positive signal をより明示、Windows 専用/test mock/合成 fixture 等の既知 FP パターンを few-shot 化)
- [ ] WP-04 の Opus gold eval セット (35 件) で qwen3-coder:30b 等を再測、FP recall と human_review 安全軸を確認
- [ ] 能力限界と確認できれば恒久見送りとして本 entry 削除。プロンプト不適合なら該当モデル + 専用プロンプトで格上げ (ADR-038 の model default 変更 + amendment)
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- FP 検出が「モデル能力限界」か「プロンプト不適合」かが実測で切り分けられ、格上げ採否が結論付けられていること。

#### 詰まっている箇所

- なし (WP-04 の eval 資産・gold セットあり、プロンプト改訂のみ)。

---

### push pipeline の `cargo test` を cargo-nextest 化 (WP-05 で Stop hook には無効と判明、push 側 follow-up)

> **動機**: WP-05 (Stop hook 高速化) の実測で、当初計画の nextest 案は **Stop hook には無効**と判明した (Stop hook は cargo test を実行せず、真因は 7 ステップの逐次実行 → 並列化で解決済、ADR-004 amendment)。一方、**push pipeline (cli-push-runner の quality_gate) は `cargo test -- --ignored --test-threads=1` を実行**しており、実測で ~80s を要する (WP-03 push で観測)。ここは nextest による並列テスト実行で高速化の余地がある。push は Stop hook より低頻度だが、fix→push サイクルの待ち時間に直結する。
>
> **参照**: `push-runner-config.toml` の `[[quality_gate.groups]]` name=`rust-lint-test`、`src/cli-push-runner` の quality_gate stage、ADR-004 § ステップ並列実行による高速化 (2026-07-05 追記) の scope 外 note、ADR-017 (takt バージョン固定哲学 = nextest 固定の根拠)。
>
> **実行優先度**: ⏳ Tier 5 — Effort S-M。現行 push は機能上支障なく、優先度は低い。ツール依存追加の費用対効果を要評価。

#### 作業計画

- [ ] cargo-nextest の導入判断: ツール依存追加 (ADR-017 pinning + `pnpm deploy:hooks` 派生プロジェクト配布) のコスト vs push 高速化の便益を評価
- [ ] 採用時: `push-runner-config.toml` の `cargo test` step を `cargo nextest run` に置換。**nextest は doctest を実行しないため `cargo test --doc` を併走**させる (doctest 有無を確認: `///` の ` ``` ` を持つ crate)
- [ ] `--ignored` 統合テスト (repush 等) が nextest で正しく実行されることを確認 (nextest の `--run-ignored` フラグ)
- [ ] before/after 実測で push pipeline 時間短縮を確認
- [ ] 本 entry 削除 + todo-summary2.md 行削除

#### 完了基準

- push pipeline の test 実行時間が短縮され、doctest / `--ignored` 統合テストの網羅性が維持されていること。または費用対効果が見合わないと判断し見送りが記録されていること。

#### 詰まっている箇所

- なし (WP-05 で Stop 側は完了、push 側の nextest 適用余地とコスト構造は明確)。

---

### cli-docs-lint に ADR 重複採番 + CLAUDE.md 索引整合チェック追加 (PR #261 post-merge-feedback T1-#2 採用)

> **動機**: PR #261 で当方が ADR-052 として起草した ADR が、並行 land した PR #260 の ADR-052 (自律実行境界) と採番衝突し、rebase 時にファイル名 + 本文タイトル + ソース内参照 10+ 箇所の置換が発生した実例。ADR は既に 53 件、並行 PR 開発が常態化しており再発頻度 Medium。現状この衝突を機械検知する層が存在しない (発見は rebase 時の CLAUDE.md conflict 頼み)。
>
> **チェック内容 (案)**: (a) `docs/adr/adr-NNN-*.md` の同一 NNN 重複検出、(b) CLAUDE.md 索引 ⇔ 実ファイルの対応検証 (索引にあるファイルの存在 / 実ファイルの索引掲載)、(c) ファイル名の NNN ⇔ 本文 H1 タイトル番号の一致。
>
> **参照**: `.claude/feedback-reports/261.md` Tier 1 #2、`src/cli-docs-lint/src/main.rs` (CheckMode 拡張、preamble / cross-ref / priority-inversion の既存 check-mode dispatch と kill-switch 骨格を流用)、ADR-007 (層の線引き)、ADR-039。
>
> **関連 (重複ではない)**: 順位 135 (todo8.md、ADR-NNN placeholder policy) は todo entry 側の採番 hardcode を防ぐ「ルール」であり、本 entry は land 済みファイル群の衝突を検知する「仕組み」(ADR-042 の役割分担で相補)。feedback report Tier 2 #2 (ADR sanity テスト新設) は本 entry と目的重複のため却下済み。順位 357 (todo14.md、index ステータスタグ ⇔ ADR 本体ステータスの整合) はチェック対象が異なる別 entry — 本 entry の責務は a/b/c (採番重複/索引存在/番号一致) のみでステータスタグは扱わない。実装は同一 validator module への同居が可能 (PR #341 CodeRabbit 指摘で責務分界を明文化)。
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。既存 cli-docs-lint 骨格の流用で新規 module 1 つ + fixture テスト。

#### 作業計画

- [ ] `src/cli-docs-lint/src/` に adr_consistency validator module を新設 (check 内容 a/b/c)
- [ ] 既存 CheckMode dispatch / kill-switch 設定に統合 (ADR-039 パターン)
- [ ] fixture テスト: 重複採番 / 索引欠落 / 番号不一致の bad fixture + clean fixture
- [ ] push-runner quality_gate (`pnpm lint:docs`) 経由で発火することを確認
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- ADR 採番衝突・索引不整合・ファイル名/タイトル番号不一致が push 前に決定論的に検出され、PR #261 型の rebase 時大量置換が再発しない構造になっていること (ステータスタグ整合は順位 357 の完了基準で扱い、本 entry の対象外)。

---

### 層別テストテンプレート (StubOllama パターン・integration 独立性) の共有化 (PR #265 post-merge-feedback T2-1 採用)

> **動機**: WP-11 (PR #265、ADR-054) の多層防御実装で、層別テスト戦略の設計に時間を要した。具体的には (a) 空 responses の `StubOllama` で「LLM が呼ばれていないこと」を証明する短絡検証パターン、(b) tempdir + `jj git init` + CwdRestore で実 jj repo を立てる integration テストの独立性パターン、の 2 つを都度設計した。WP-17 (自律化) で classifier / scope guard 層を拡張する際に同種の設計判断が再発する見込み。
>
> **参照**: `.claude/feedback-reports/265.md` Tier 2 #1、`src/cli-finding-classifier/src/lib.rs` (StubOllama)、`src/cli-pr-monitor/src/stages/scope_guard.rs` (integration パターン)、ADR-041 (test isolation patterns)、ADR-025 (CwdRestore)、ADR-044 (共通化と分離の線引き — shared crate 化の境界判定に適用)
>
> **実行優先度**: 🔧 Tier 2 — Effort M。WP-17 着手前の実施が効果的。

#### 作業計画

- [ ] 対象パターンの棚卸し (StubOllama / tempdir+jj init+CwdRestore / 層別テストの構成方針)
- [ ] ADR-044 の境界判定で shared test crate 化 or fixture + doc 化を判断
- [ ] 切り出し + 既存呼び出し側 (cli-finding-classifier / cli-pr-monitor) の移行
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 新しい LLM 系 / jj 統合系のテストが共有テンプレートを参照して層別テストを組める状態になっていること。

---

### ADR-007 に「コメント配置の意思決定フロー」を追加 (PR #265 post-merge-feedback T3-2 採用)

> **動機**: PR #265 実装中に `classify_one` (cli-finding-classifier) と `config.rs` (cli-pr-monitor) の 2 箇所で非 doc コメントを書き、Bundle Z comment-lint (#B-α) に block された (同種ミス 2 回 = パターン化の価値あり)。「この説明は doc コメント (`///`) に書くべきか、識別子名 / 関数分割で表現して削除すべきか」の判断フローが未文書化。linter 自動化 (feedback Tier 1 #2) は意味論的判定 = NLP が必要なため却下済みで、本エントリは人間 / AI の判断補助ドキュメントとしての補完。
>
> **参照**: `.claude/feedback-reports/265.md` Tier 3 #2、`docs/adr/adr-007-custom-linter-layer-boundary.md` (既存 Q1-Q3 判断フロー形式で拡張)、`src/hooks-post-tool-comment-lint-rust` (Bundle Z #B-α)
>
> **実行優先度**: 💎 Tier 3 — Effort S。doc のみ、バッチ PR で消化可。

#### 作業計画

- [ ] ADR-007 に Q 形式の「コメントを書きたくなったときの配置判断フロー」を追記 (doc コメント / 識別子名 / マーカー付き Why コメントの 3 分岐)
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- コメント配置の判断が ADR-007 の判断フローで一意に決まり、Bundle Z block の手戻りが減ること。

---

### PR body 配置タイミング規約を dev-conventions に明記 (PR #265 post-merge-feedback T3-3 採用)

> **動機**: PR #265 の push パイプライン実行中に working copy へ `__pr-body.md` を作成し、jj snapshot 直前の退避で commit 混入をかろうじて回避したヒヤリハットが実発生。混入すると repo 履歴に残る。「PR body は push 完了後に scratchpad で準備し、`pnpm create-pr -- --body-file` に絶対パスで渡す (push 実行中の working copy に置かない)」というタイミング規約が未文書化。
>
> **参照**: `.claude/feedback-reports/265.md` Tier 3 #3、`docs/dev-conventions.md` (追記先)、ADR-028 (external-output 実行フロー)、`src/cli-pr-monitor/src/stages/create_pr.rs` (--body-file パススルー実装)
>
> **実行優先度**: 💎 Tier 3 — Effort XS。doc のみ、バッチ PR で消化可 (並列安全化 PR の docs への相乗りも可)。

#### 作業計画

- [ ] dev-conventions.md に PR body 配置タイミング規約を追記 (scratchpad + 絶対パス推奨 / repo 直下 `__` ファイルは push 完了後のみ)
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- PR body ファイルが push パイプラインの snapshot に混入しない手順が規約として参照可能になっていること。

---


### config-reading hook の `current_dir()` 解決を検出する lint rule (PR #267 post-merge-feedback T1-1 採用)

> **動機**: PR #267 で新規 hook (jj-op-verify) が既存 3 hook と異なる `current_dir()` ベースの config 解決を実装し、pre-push simplicity-review が REJECT (`SIM-NEW-jjopverify-cwd-config-L179`、High) → fix step が `current_exe().parent()` へ修正した実例。Bash の cwd drift による silent fail-open (`enabled=false` 扱い) は新規 hook 追加のたびに再発しうる。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 1 #1、`.claude/custom-lint-rules.toml` (新規ルール)、順位 287 (convention 明文化、同一 PR bundle 推奨)
>
> **実行優先度**: 🚀 Tier 1 — Severity High / Effort S。

#### 作業計画

- [ ] custom-lint-rules.toml に「hooks-* の .rs で `current_dir()` + `hooks-config.toml` の組合せ」を検出するルール追加 (bad/good fixture + incident 構造)
- [ ] 順位 287 (convention 明文化) を同一 PR で bundle
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- config を cwd 基準で解決する新規 hook が push 前に決定論的に検出されること。

---

### jj-op-verify の変更系 verb 網羅拡大 (PR #267 post-merge-feedback T1-2 採用)

> **動機**: 現行の検出対象 (new/describe/abandon/rebase/squash/bookmark 変更系) に `undo` / `restore` / `split` / `bookmark move` / `bookmark track` / `bookmark untrack` が含まれない。特に `jj undo` の検出漏れは lost-update 再発リスクが高く、Operation Verification Checklist 自動化の対象を狭める。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 1 #2、`src/hooks-post-tool-jj-op-verify/src/main.rs` (match 文)。**拡張時は `expected_op_keyword` を実際の `jj op log` 出力と要照合**
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Effort M。

#### 作業計画

- [ ] 各 verb の実際の op description を jj 0.42 実機で確認し keyword map に追加 + テスト
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 変更系 jj 操作の検出網羅率が上がり、`jj undo` 等の op 記録検証が機能すること。

---

### jj-op-verify の verb 検出を command-boundary に anchor (PR #267 post-merge-feedback T1-3 採用)

> **動機**: `split_whitespace()` の非 anchored 検出は、commit message 引用符内の `"jj new"` 等で false positive「operation not recorded」を誘発しうる。実装時に accepted risk として一度見送った経緯あり (実害観測 0 件)。採用は「advisory 層の UX 劣化」防止目的で、着手時に実観測状況を再確認すること。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 1 #3、`src/hooks-post-tool-jj-op-verify/src/main.rs:detect_last_mutating_jj_op`、順位 285 (edge-case テスト、表裏の関係)
>
> **実行優先度**: 🚀 Tier 1 — Severity Medium / Effort S。

#### 作業計画

- [ ] verb 検出をコマンド境界 (`&&` / `;` / `|` / 文頭) anchor に変更 + 引用符内の誤検出テスト
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- commit message 内の jj キーワードで警告が誤発火しないこと。

---

### config path 解決の cwd 跨ぎ integration test (PR #267 post-merge-feedback T2-3 採用)

> **動機**: PR #267 で FIXED 済の `SIM-NEW-jjopverify-cwd-config-L179` は、既存テストが pure parser のみで file-lookup 経路を未カバーだったため混入した。非 repo-root cwd から hook を起動して config が読み込まれることを検証する統合テストは、cwd drift シナリオ (ADR-045 の核心リスク) の re-incident 検知網になる。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 2 #3、`hooks-post-tool-jj-op-verify` test suite。Adoption Risk: OS 依存 (temp dir / path 形式)
>
> **実行優先度**: 🔧 Tier 2 — Severity High / Effort M。

#### 作業計画

- [ ] 実 exe spawn + 非 repo-root cwd で config 読込を assert する `#[ignore]` integration test
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- exe-relative 解決の退行が統合テストで検出されること。

---

### 「config 読み hook は exe-relative 解決必須」convention の明文化 (PR #267 post-merge-feedback T3-1 採用)

> **動機**: 順位 281 (lint rule) の文書層の補完。ADR-045 (または dev-conventions) と該当 hook の inline comment に規約として明文化する。
>
> **参照**: `.claude/feedback-reports/267.md` Tier 3 #1。**順位 281 と同一 PR での bundle 実装を推奨** (別作業に切り出す価値は低い)
>
> **実行優先度**: 💎 Tier 3 — Effort XS。

#### 作業計画

- [ ] 順位 281 の PR に同乗して convention を明文化
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 新規 hook 作成時に参照できる規約が存在し、lint rule (281) と 2 層で防御されていること。

---

### push-runner の stack push モード (opt-in、YAGNI につき見送り継続)

> **動機**: `bookmark_check.rs` の `OWN_WORKSPACE_BOOKMARKS_REVSET = "@"` (厳密一致) は、stacked bookmark 運用 (`feature/base` → `feature/api` → `feature/ui` を `@` 先頭で一括 push) では `@` の bookmark だけでは不足するというトレードオフを持つ。現状その運用実績はなく、必要になった時点で明示オプトインの stack push モード (`[push] stack_push` 等) を追加する拡張余地として記録する。
>
> **参照**: `src/cli-push-runner/src/stages/bookmark_check.rs:39-43` (トレードオフの記述箇所、本エントリを指して「todo 登録済み」と既に言及している)
>
> **実行優先度**: ⏳ Tier 5 (YAGNI、実運用実績なし) — Effort M。

#### 作業計画

- [ ] stacked bookmark 運用が実際に必要になった時点で `[push] stack_push` config を設計
- [ ] 実績が出ないまま長期化する場合は close 判断も検討
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- (着手判断待ち) 次のいずれかに至ること: (a) stacked bookmark 運用の実需が生じ opt-in モードが設計・実装される、または (b) 実績が出ないまま長期化し close 判断がなされる。

---

### jj-op-verify hook の位置づけ再整理 — 並列 workspace 安全化ではなく混線緩和層として再分類

> **動機**: `hooks-post-tool-jj-op-verify` は PR #267 / ADR-045 上「並列 workspace 安全化」の一部として位置づけられているが、検知対象 (「op が記録されない」症状) は jj の公式並行モデルでは説明できない (並列操作なら stale working copy エラーか divergent operation heads として op log に残るはず)。実体は出力混線 (Opus 4.8 / Fable 5 モデル起源のシリアライズ不具合、ADR-053 が上流バグと断定済み) の症状検出器であり、並列 workspace 運用の有無とは独立に価値を持つ。「並列対策が完了したので撤去可能」という将来の誤判断を防ぐため、ADR-045 ではなく ADR-053 の枠組みに紐付け直す。
>
> **参照**: `docs/adr/adr-045-jj-workspace-parallel-sessions.md` § Known operational risks、`docs/adr/adr-053-stop-tool-call-leak-detection.md`、`src/hooks-post-tool-jj-op-verify/src/main.rs`
>
> **実行優先度**: 💎 Tier 3 — Effort S (ドキュメント再整理のみ、hook 実装は変更不要)。

#### 作業計画

- [ ] ADR-053 に「jj-op-verify hook は tool 実行はされたが結果表示の信頼性が疑わしい型の混線を検知する」旨を追記し、当該 hook への参照を追加
- [ ] ADR-045 の該当 hook の記述を「並列 workspace 対策」から「混線検知 (副次的に並列 workspace 由来の stale 検出にも有効)」に改める
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- 将来のセッションが「並列運用をやめたので jj-op-verify は不要」と誤判断しないよう、ADR 上の位置づけが混線緩和層として明記されていること。

---

### ADR-045 にコミット消失事故の「並列原因」診断が未検証である旨の注記追加

> **動機**: WP-11 作業中に発生した「コミット 2 つ消失」事故は「並列 jj workspace の同時操作が原因」と診断され ADR-045 に記録されたが、この診断は当時の一次証拠 (`jj op log` の実データ) ではなく、post-merge-feedback の `analyze-session` facet による事後の自己分析 (未検証) に依拠している。「op が一切記録されない」という症状は jj の公式並行モデルでは説明できず、混線 (モデル起源のシリアライズ不具合) による状態誤認が真因である可能性の方が技術的に整合する。confirmation bias の記録として、この診断の不確実性を ADR-045 に注記する。
>
> **参照**: `docs/adr/adr-045-jj-workspace-parallel-sessions.md` § Known operational risks、本セッションの調査 (transcript `ed897a3e-85b5-44d1-a78c-ff23973f207e.jsonl` 系列、独立 subagent 検証)
>
> **実行優先度**: 💎 Tier 3 — Effort XS。

#### 作業計画

- [ ] ADR-045 の該当事故記述に「並列 workspace 原因説は事後分析による推定であり、一次証拠 (当時の jj op log) には未到達。混線 (モデル起源) が真因である可能性も残る」旨を注記
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- ADR-045 を読む将来のセッションが、この診断を「確定事実」ではなく「未検証の有力仮説」として扱えること。

---

### Lock stale takeover + Drop の concurrency scenario 拡張テスト (271.md T2-2 採用)

> **動機**: PR #271 が導入した token-based ownership Drop の前提 (「fresh lock は takeover されない」) を、既存 `concurrent_stale_takeover_only_one_wins` に加え、takeover 後の旧 guard drop までの full cycle を長い operation chain で検証する価値が高い。PR #267 (concurrent checkout 事故) の再発防止網としても機能する。
>
> **参照**: `.claude/feedback-reports/271.md` Tier 2 #2、`src/lib-jj-helpers/src/pipeline_lock.rs` の tests モジュール
>
> **実行優先度**: 🔧 Tier 2 — Effort M。

#### 作業計画

- [ ] takeover → 旧 guard drop → 新 guard drop の full cycle を検証するテストを既存テストファイルに追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- takeover 後の旧 guard drop が新 lock を誤削除しないことが、長い operation chain のシナリオでも保証されていること。

