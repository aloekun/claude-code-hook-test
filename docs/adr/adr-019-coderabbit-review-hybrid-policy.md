# ADR-019: CodeRabbit レビュー運用のハイブリッド構成

## ステータス

承認済み (2026-04-16)

## コンテキスト

### 問題

ADR-018 で cli-pr-monitor を takt ベースに移行したが、Phase 1 は「分析のみ」で、CodeRabbit 指摘への対応は依然として Claude Code への「お願いベース」だった。また CodeRabbit は以下の特性を持つ:

1. **プラットフォーム非依存のレビュー**: 本プロジェクトは Windows 専用だが、`.exe` ハードコードなどを cross-platform 観点で Critical/Major 指摘する
2. **深刻度の過剰評価**: false positive や設計意図に反する提案を Critical として挙げることがある
3. **修正の粒度バラつき**: 1行置換で済むものから設計変更を伴うものまで混在

これらを無差別に自動修正しようとすると、ADR 違反や設計意図を破壊するリスクがある。一方で全指摘をユーザー判断に委ねると、takt 化の意義（deterministic な AI 連携）が薄れる。

### 検証で得られた知見

PR #41 (Phase 2 fix loop) 実装と CodeRabbit との相互作用で以下を確認:

- **project fitness filter が有効**: `CLAUDE.md` + ADR を参照して `not_applicable` をマークすることで、Windows 非対応指摘を除外できる
- **severity 再分類で精度向上**: CodeRabbit の severity をそのまま使うのではなく、takt の analyze ステップで再評価した方が自動修正の精度が上がる
- **ハイブリッド再 push**: Critical は自動 push、Medium 以下はユーザー確認、という設定分岐で安全性と自動化のバランスが取れる

## 決定

### 3 レイヤーのレビュー対応ポリシー

```text
[Layer 1] Project Fitness Filter (takt analyze ステップ)
  ├─ CLAUDE.md + ADR を読み、適用可能性を判定
  ├─ applicable / not_applicable にマーク
  └─ 不適合理由をレポートに明記

[Layer 2] Severity Classification (takt analyze ステップ)
  ├─ applicable な findings のみ対象
  ├─ Critical / High / Major → needs_fix (自動修正対象)
  ├─ Medium / Minor → user_decision (ユーザー判断)
  └─ Low / Info → approved (対応不要)

[Layer 3] Hybrid Re-push Policy (Rust cli-pr-monitor)
  ├─ auto_push_severity = "critical" → 常に自動 push
  ├─ auto_push_severity = "major"    → 常に自動 push
  ├─ auto_push_severity = "none"     → 常にユーザー確認
  └─ 未知値 → fail-closed (ユーザー確認)
```

### 設計原則

1. **AI の評価を Rust で二重判定しない**: Layer 2 の判定結果 (takt が fix を実行した事実) を信頼する。Rust 側は生 findings を severity 判定に使わない
2. **fail-closed をデフォルト**: 設定値が不正な場合は自動 push せず、ユーザーに判断を委ねる
3. **fitness filter は必須**: Layer 1 をスキップすると Windows 専用プロジェクトで意味のない修正が入る
4. **verdict 値の一貫性**: takt workflow YAML の `condition` 値 (`approved` / `needs_fix` / `user_decision`) と instruction の出力例を統一する。不整合は lint で検出する (ADR-020 関連)

### CodeRabbit Learning との連携

CodeRabbit は自身の Learning システムで「この repo/path では cross-platform 対応は不要」といったルールを記憶する。プロジェクト側からも以下を宣言する:

- `CLAUDE.md` に platform scope (Windows only) を明記
- ADR で意図的な設計決定を記録
- `.takt/facets/instructions/analyze-coderabbit.md` で fitness filter のチェック項目を明示

これにより CodeRabbit のレビュー自体が徐々に適合していく。

### CodeRabbit 無料枠の制約 (2026-04-19 追記)

本プロジェクトは CodeRabbit の無料枠を前提に運用しており、以下の制約を受け入れる:

| 制約 | 影響 |
|---|---|
| **1 時間 3 回のレビュー上限** | 連続 PR 作成時に 3 本目以降のレビューがスキップされる |
| **public リポジトリ限定** | 本リポジトリが public である前提が崩れると CodeRabbit が利用不能 |
| **アカウント単位の制約** | fork / 別アカウント運用すると制約が分離される (逆用可能性あり) |

### レビュアー可換性の方針 (2026-04-19 追記)

CodeRabbit は便利だが、無料枠制約と将来的な仕様変更リスクを踏まえて「CodeRabbit 依存を固定化しない」ことを設計方針とする。

#### 「ハイブリッド」の定義 (再定義)

当初 ADR-019 起草時の「ハイブリッド」は「takt 分析 + CodeRabbit review」の意味合いが強かったが、本追記で以下に再定義する:

> **ハイブリッド = takt 内製レビュー + 外部 AI レビュー (plugin 可換)**

外部 AI レビューは CodeRabbit に限定せず、以下を交換可能な plugin として扱う:

- CodeRabbit (現行)
- GitHub Copilot Reviews
- Greptile
- その他 future 候補

#### Layer 構成は外部 AI 可換を前提に保つ

ADR-019 の 3 レイヤー構成は外部 AI の種類に依存しない形で設計されている:

- **Layer 1 (fitness filter)**: `.takt/facets/instructions/analyze-<tool>.md` を tool 別に用意する
- **Layer 2 (severity classification)**: `needs_fix` / `user_decision` / `approved` の 3-way verdict は tool 共通
- **Layer 3 (hybrid re-push)**: `auto_push_severity` の設定は tool 非依存

切り替え時の実装コストは「Layer 1 の analyze instruction を新 tool 用に書き起こす」程度に抑える設計を維持する。

#### 具体的な可換性確保策

- **CodeRabbit 固有の成果物に依存しない命名**: `pr-review` / `post-pr-monitor` 等、tool 名を含まない workflow 名を優先
- **analyze instruction は tool 別ファイル**: `analyze-coderabbit.md` / (将来) `analyze-copilot.md` のように分離
- **Rust 側 (cli-pr-monitor) は tool 固有 API に依存しない**: PR comments API を通じて取得できる汎用フォーマットに閉じる

### M5 (rate limit 耐性作り込み) を不採用とする論拠 (2026-04-19 追記)

無料枠制約に対する耐性機能 (例: "3 回超過後の自動 retry"、"レビュー失敗時の claude -p fallback") を作り込まない理由:

1. **レビュアーロックインの温床**: rate limit 耐性は CodeRabbit 固有挙動への依存を深め、可換性の方針と矛盾する
2. **投資対効果が薄い**: 1h 3 回制限は日常運用でまず引っかからない。連続 PR 作成の局面は設計上避けるべきケース (CodeRabbit rate limit 対策として PR-B/PR-C の push 間に 1 時間インターバル = 運用でカバー)
3. **無料枠に高度機能を期待しない**: 有償プラン契約の判断は別 ADR で行う。現時点では「制約を受け入れて公式経路を使う」のが最小コスト

代わりに以下で運用カバーする:

- PR 作成間隔の調整 (運用ルール、自動化なし)
- レビュー空振りを検出したら「そもそも push しない」「手動で claude -p review に切り替える」 (interactive 判断)
- ロックインが問題化した時点で plugin 可換設計の具体実装に着手 (本 ADR の方針に従う)

### WP-03 (2026-07-04 追記): CodeRabbit クォータ設計 — レビュー消費削減による rate-limit 緩和

#### 運用実態の変化 (M5 不採用の前提が崩れた)

2026-04-19 の「M5 (rate limit 耐性作り込み) を不採用」判断は「1h 3 回制限は日常運用でまず引っかからない」を前提としていた。しかしその後、監視 (cli-pr-monitor) の auto-push + takt fix loop 自動化が進み、**fix push ごとの自動増分レビューでレビュー消費が増加**、解除待ち (3〜4 回/時上限に対し体感で毎日頻発、1 回あたり 20〜40 分) が運用上の最大ボトルネックになった (2026-07-04 ユーザー確認)。本リポジトリは public だが、CodeRabbit の public 特典 (無償レビュー) は本アカウントの rate-limit を撤廃しておらず、無料枠の時間あたり上限が実際に効いている。

#### M5 不採用との整合 (耐性ではなく消費削減)

WP-03 は 2026-04-19 で却下した「rate-limit 耐性 (超過後の auto-retry / 失敗時 fallback)」= CodeRabbit 固有挙動への依存を深めるロックインの温床、とは**別のアプローチ**を採る:

- **却下したもの (耐性)**: 上限に当たった後に自動リトライ・迂回する機構。CodeRabbit 固有挙動依存を深める。
- **WP-03 (消費削減)**: そもそも消費するレビュー回数を減らす。標準の `.coderabbit.yaml` 設定 + 運用調整であり、可換性を損なわない (別ツール移行時は当該 config を捨てるだけ)。

#### 決定 (balanced 構成)

1. **`.coderabbit.yaml` 新設** (リポジトリルート)。初回 PR は自動レビューを維持しつつ、fix push ごとの自動増分レビューを抑止する:
   - `reviews.auto_review.enabled = true` (初回 PR open は自動レビュー)
   - `reviews.auto_review.drafts = false` (draft は対象外、既定だが明示)
   - `reviews.auto_review.auto_incremental_review = false` (**消費の主因である push 毎の自動再レビューを停止**)
   - `reviews.auto_review.auto_pause_after_reviewed_commits = 5` (暴走ガード)
   - `language = "ja-JP"` (`.coderabbit.yaml` はダッシュボード設定を上書きし未指定キーは既定 en-US に戻るため、日本語レビューを固定)
2. **明示再レビュートリガー (決定論層、監視側)**: `auto_incremental_review = false` は fix push 後の自動再レビューを止めるため、監視の auto-push 成功後に `@coderabbitai review` を 1 回だけ明示投稿して再レビューを発火する。`pr-monitor-config.toml [fix] trigger_review_after_push = true` で opt-in し、`src/cli-pr-monitor/src/stages/review_trigger.rs` が担う。この経路は**助言層 = fail-open** (state 不在 / PR 番号未確定 / gh 投稿失敗は log を残して続行。ADR-043 の fail-closed はゲート層にのみ適用)。
3. **運用 (1 push 束ね)**: fix step は全 finding を 1 iteration で修正 → 検証 → auto-push 1 回、が既存の挙動 (fix.md facet は変更不要)。トリガーが監視側に入ったことで「fix 束ね 1 回 = 明示レビュー 1 回」に確定する。

#### 消費モデルの変化

- **従来**: 初回レビュー 1 + fix push ごとの自動増分レビュー N = `1 + N` 回。
- **WP-03**: 初回レビュー 1 + fix 束ねごとの明示レビュー 1 = `1 + (iteration 数)` 回。push 毎ではなく「レビューしてほしい確定タイミング」のみ消費し、中間 push (rebase / cleanup 等) が誤ってレビューを消費しない。

#### 既知の制約

- **手動 fix push は手動トリガーが必要**: 明示トリガーは監視の auto-push 経路 (`auto_push_severity` = critical/major) のみ。ユーザー手動 push (severity=none / minor) 後は `@coderabbitai review` を手動投稿する (fail-open のログが誘導する)。
- **設定の二重管理**: `.coderabbit.yaml` の `auto_incremental_review` と `pr-monitor-config.toml` の `trigger_review_after_push` は必ず揃える (前者 false ⇔ 後者 true)。揃わないと再レビュー欠落 (両 off) or 二重レビュー (両誤設定) になる。派生プロジェクトの template では default false + コメント例で明示。

#### 受け入れ基準 (dogfood)

rate 解除待ちの発生が 1 回/日未満になること。導入後の実績で確認する。未達なら `auto_pause` 値 / トリガー条件を調整、または `enabled = false` (フル手動トリガー) への切替を再検討する。

## 影響

### 採用される構成要素

- `.takt/facets/instructions/analyze-coderabbit.md` (Layer 1 + Layer 2)
- `.takt/workflows/post-pr-review.yaml` の `analyze` ステップ (3-way verdict 分岐)
- `pr-monitor-config.toml` の `[fix]` セクション (`auto_push_severity`)
- `src/cli-pr-monitor/src/stages/monitor.rs` の `should_auto_push()` 純粋関数 (Layer 3)

### 避けるべきアンチパターン

- **生 findings ベースの auto push 判定**: Layer 1 の filter を通っていない findings を severity 判定に使うと、`not_applicable` な Critical が自動 push を誤発動させる (PR #41 CodeRabbit Major 指摘)
- **byte-position slicing**: レビュー文は日本語を含むため `str[..N]` は panic する。`truncate_safe` または `chars().take(N)` を使う (ADR-007 のカスタムリンター層 custom-lint-rules.toml に検出ルールを追加)
- **お願いベースの通知**: Claude Code に「CronCreate してください」と stdout で指示するのではなく、takt の完了を Bash tool の `run_in_background` で待つ (ADR-018 で決定済み)

## 次ステップ (スコープ外)

- **analyze instruction の強化**: ADR を自動検索して filter ルールを動的に抽出
- **Learning と ADR の双方向同期**: ADR を更新したら CodeRabbit Learning にも通知
- **他ツールのレビュー統合**: Copilot review, Greptile などの別 AI レビューも同じ Layer 構成で処理
