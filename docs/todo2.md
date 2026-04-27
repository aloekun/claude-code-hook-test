# TODO (Part 2)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo.md がファイルサイズ約 40KB に達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する。todo.md の既存エントリは引き続き有効、相互に独立。新セッションでは両方を確認すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo.md](todo.md#推奨実行順序サマリー-2026-04-27-更新) を参照。本ファイルに記録する ADR-032 は sub-phase ごとに Tier が分散するため、各 Phase の冒頭に個別の優先度を記載。

---

## 現在進行中

### docs-only PR 高速パスの導入 (ADR-032 起案 + 実装)

> **動機**: PR #81 (ADR-031 起案、docs only) の実所要時間は約 15 分 (pre-push-review ~3min + post-pr-review ~1.5min + post-merge-feedback ~10min)。`*.md` のみの変更で 3 つの takt workflow が全走するが、simplicity / security / architecture 観点での検出対象がほぼゼロで signal/noise が極めて低い。日々のドキュメント反映を阻害している。
>
> **本タスクの位置づけ**: ロジック変更やアーキテクチャの置き換えなど **意味のある実装** に対するレビューは残しつつ、**ドキュメント単独の変更** は high-velocity で反映できる balance を取る。skip の代償は「即時 broken-link-check」「CodeRabbit 非ブロッキングコメント」「週次レビュー (ADR-031) の docs 整合性観点追加」の三層で補償する。試験運用フラグで導入し、4-6 週の dogfood 観測後に本採用判断。
>
> **計画ファイル参照**: `~/.claude/plans/1-docs-todo-md-askuserquestion-validated-orbit.md` (本タスク策定時の plan、新セッションでも同じ判断を再現可能)
>
> **実行優先度**: タスク全体は **🚀 Tier 1 〜 💎 Tier 3 に分散** (Phase ごとに優先度が異なる)。
> - Phase pre (branch protection): **Tier 1 (順位 3/10)** — 設定のみ、A/B と並列可
> - Phase α: 既存 todo.md「週次レビュー (ADR-031)」エントリ参照 — **Tier 2 (順位 4/10)**
> - Phase broken-link: **Tier 2 (順位 5/10)** — 順位 2 (markdownlint) の clean baseline 後が望ましい
> - Phase β (実装、enabled=false): **Tier 3 (順位 6/10)** — 全前提揃ってから
> - Phase γ (enablement): **Tier 3 (順位 7/10)** — 順位 4 dogfood 後の 1 行 flip
> - Phase δ (dogfood): **Tier 3 (順位 8/10)** — 実 docs PR で検証
>
> **最大 payoff**: Phase γ enable 後、docs PR 所要時間 ~15min → ~30sec (30 倍速)。daily efficiency への貢献は本リポジトリ随一だが、**前提依存が多いため近道はない**。

#### 背景

##### 既存 3 パイプラインの空白

| 既存パイプライン | レビュー対象 | docs only での価値 |
|---|---|---|
| pre-push-review (ADR-015, ADR-027) | push 前の diff | 低 (simplicity/security は docs にほぼ無効) |
| post-pr-review (ADR-018, ADR-019) | CodeRabbit findings 自動 fix | 低 (Nitpick 中心) |
| post-merge-feedback (ADR-030) | PR 知見抽出 + Tier 提案 | 中 (PR #81 で markdownlint 提案を生成) |

##### docs only ≠ 安全 という認識

「docs only」変更でもシステムを壊しうる:
- README 手順ミス → 本番運用事故
- API ドキュメントの誤記 → 利用者側バグ誘発
- ADR の誤り → 設計判断の誤誘導
- broken link → ナビゲーション破綻

→ **「壊れる変更」は CI で止め、「良くない変更」は CodeRabbit コメントで気付く** 二段構えで補償する。週次レビューだけに依存しない。

##### GitHub のマージ制御の正しい分離 (個人開発 + AI エージェント前提)

- **Required status checks (CI)**: lint/test/build/markdownlint/broken-link-check (壊れる変更を止める。**唯一のブロッキング**)
- **Required reviewers**: ❌ **使わない** — 個人開発で実装/テスト/PR が AI 自動化される中、人間レビュー必須は唯一の同期処理として律速になり anti-pattern
- **CodeRabbit (非ブロッキング)**: センサー / 異常検知役。ゲートキーパーではない (コメントは届くが merge をブロックしない)
- **人間レビュー (event-driven)**: バグ発生時 / 大きい変更 / 設計変更 (ADR 絡み) など特定トリガー時のみ。常時必須化しない
- **週次レビュー (ADR-031)**: 遅延・包括的に cross-PR drift を補完。定常的な「整合性チェック」はここに集約

#### 設計決定 (確定済)

##### 用語と挙動

「takt workflow を skip」は **AI による自動処理 (analysis / fix loop / aggregation) を skip** することを意味する。**CodeRabbit 自体は GitHub 側で動作し続け、PR にコメントを投稿する**。違いは「コメントが届くかどうか」ではなく「コメントが届いた後に Claude/takt が自動で fix を試みるかどうか」。

| 機構 | docs-only PR での挙動 | normal PR での挙動 |
|---|---|---|
| pre-push-review takt | skip | simplicity/security review 走る |
| CodeRabbit (GitHub 側) | **コメント投稿される (非ブロッキング)** | コメント投稿される |
| post-pr-review takt (CodeRabbit findings 自動 fix) | **skip** | findings 自動修正 loop |
| post-merge-feedback takt | skip | 知見抽出 + Tier 提案 |
| quality_gate (lint/test/build/markdownlint/broken-link-check) | **走る (broken-link 含む)** | 走る |
| GitHub Required status checks | 緑必須 (CI = 壊れる変更を止める) | 緑必須 |

##### 3 exe 独立検出 (label / marker は使わない)

3 つの exe は異なる lifecycle で異なる source-of-truth を見ているため、共通の skip 判定を共有するより **各 exe が自分の diff source から独立に判定** するのが安全:

| exe | lifecycle | source-of-truth | 取得方法 |
|---|---|---|---|
| cli-push-runner | push 前、PR 未存在 | `@` の diff | `jj diff -r @ --name-only` |
| cli-pr-monitor | PR 存在中 (amend 追従) | PR head | `gh pr diff <pr> --name-only` |
| cli-merge-pipeline | マージ時 | PR final state | `gh pr view --json files` |

却下案:
- **label 案**: amend 時に push-runner が label を clear する round-trip が必要、ADR-022「意図表現を含む既存 artifact 改変」リスクに触れる
- **marker file 案**: `*.md` のみの初回 push → 後続 amend で `.rs` 追加のケースで stale 化し silent loss を生む

採用案 (独立検出): 上記ケースでも pr-monitor / merge-pipeline が新しい head を見て自動で full review に戻る。補償ロジック不要。

##### 共通 classifier

ファイルリスト → 判定の **純関数のみ** を共通化:

```rust
// src/lib-jj-helpers/src/lib.rs に追加
pub fn classify_docs_only(files: &[String]) -> DocsOnlyClassification {
    // → AllMd | Mixed { non_md: Vec<String> } | Empty
}
pub fn should_force_full_review(files: &[String], threshold: usize) -> bool {
    // PR サイズ guard
}
```

ADR-024 の YAGNI 原則に従い、起点は lib-jj-helpers 拡張。2 例目以降出現で `lib-docs-detection` crate 切り出し検討。

##### override 設計

- `--full-review` CLI フラグ: cli-push-runner にのみ追加 (人間がコマンド入力する場所)
- `TAKT_NO_SKIP=1` 環境変数: 3 exe すべてが起動毎にチェック (CI rerun で override 忘れ防止のためキャッシュしない)
- pr-monitor / merge-pipeline には CLI フラグを生やさない (起動経路が pnpm script chain で人間が直接渡しにくいため env var が自然)
- ログ出力: override 検出時は `override 有効 (TAKT_NO_SKIP)` を明示

##### PR サイズ guard (保険)

strict `*.md` 判定の更に上位の guard として、以下に該当する場合は **docs-only でも full review に降格**:
- 変更ファイル数 ≥ 20 (大規模 docs reorganization は俯瞰レビューを要する)

config: `[docs_only] full_review_threshold_files = 20`、運用観測で調整。

##### 観測メトリクス + 誤検出検知ログ

skip 判定時に `.claude/docs-only-metrics.jsonl` (gitignore) に append:

```json
{
  "timestamp": "2026-04-27T...",
  "exe": "cli-push-runner | cli-pr-monitor | cli-merge-pipeline",
  "pr": 81,
  "decision": "skip | run | override | size-guard",
  "files": ["docs/foo.md", "docs/bar.md"],
  "ext_distribution": {"md": 2},
  "override_source": "TAKT_NO_SKIP | --full-review | null",
  "duration_saved_estimate_secs": 600
}
```

集計指標: skip 率、平均処理時間短縮、override 発動回数、誤検出疑い (override 頻発)。

##### kill switch

`push-runner-config.toml` / `pr-monitor-config.toml` / `hooks-config.toml` に `[docs_only] enabled = true|false` を追加。デフォルトは **false** で merge → ADR-031 Phase B dogfood 成功後に enablement-only PR で `true` に切替 (compensating check 不在状態での bypass を防止)。

##### 補償 1: 即時 broken-link-check (quality_gate に追加)

ユーザー指摘の「docs は壊れないがシステムは壊れる (README 手順ミス等)」への対応として、quality_gate にリンク切れ検査を即時実行:

- ツール候補: `markdown-link-check` (npm) または `lychee` (Rust、高速)。実装時に選定
- 適用: 全 markdown ファイルまたは変更ファイル + その incoming/outgoing link
- 実行時間目標: 1 分以内
- 統合先: push-runner-config.toml の `quality_gate.lint` group。markdownlint task と並列
- markdownlint との関係: markdownlint は文法 (MD040 等)、broken-link-check は **リンク先実在性**。両者は補完関係

##### 補償 2: ADR-031 への docs 整合性観点追加 (週次)

ADR-031 の `architecture-reviewer` facet (whole-tree) の rubric に以下の観点を追加:

- **ADR/symbol drift**: ADR 本文のコードシンボル参照が実コードに存在するか
- **terminology drift**: 同概念が複数の用語で書かれていないか
- **docs-code 整合**: コード側の API 名と docs の説明が一致するか
- **docs 間の重複/不整合**: 同じ事実が複数箇所で異なる記述になっていないか

新 facet は作らず **architecture facet 内の sub-criteria** として組み込む (4 facet 構成の保守コスト増を回避)。broken-link 自体は補償 1 で拾うため、補償 2 は **意味論的 drift** に集中する。

#### ユーザー判断記録 (本タスク策定時に合意済 — 2026-04-27)

| 質問 | 回答 |
|---|---|
| 「docs only」定義 | **厳密** (`*.md` のみ、1 バイトでも非 .md 入れば normal review)。将来的に `docs/**` の assets 含める段階的緩和を検討 |
| バイパス範囲 | **3 takt workflow すべて skip + CodeRabbit 解析もスキップ**。quality_gate は維持し broken-link-check 追加 |
| CodeRabbit 扱い | **非同期ブロッキング (コメントは残す)**: takt 自動 fix loop だけ skip、コメント自体は届く |
| 補償 | **週次レビュー (ADR-031) に docs 整合性観点追加** + 即時 broken-link-check |
| 検出方式 | **自動 + `--full-review` override + PR サイズ guard (保険)** |
| 観測 | **メトリクスログ + 誤検出検知ログ** を組み込む |
| Branch protection | 整備を ADR-032 の前提条件として扱う。**Required status checks + 直接 push 禁止 のみ**。**Required reviewers は使わない** (個人開発 + AI エージェント前提では人間レビュー必須は anti-pattern、律速になる)。CodeRabbit はセンサー役、人間レビューは event-driven (2026-04-27 改訂) |
| PR 分割 | **sequential**: PR-pre → PR-α → PR-broken-link → PR-β → PR-γ → PR-δ |
| アンチパターン | review-simplicity と共通化しない、must-run 化しない、Reminder 強制起動しない、CodeRabbit 完全停止しない、broken-link-check を CI から外さない、PR サイズ guard を削らない |

#### 作業計画

##### Phase pre: GitHub Branch Protection 整備 (PR-pre、コード変更なし)

> **設計方針** (2026-04-27 改訂): 個人開発 + コーディングエージェント前提では、**Required reviewers (人間レビュー必須) は anti-pattern**。実装/テスト/PR 作成が AI で自動化される一方、人間レビューだけが同期処理として律速になるため。Required reviewers を外し、ブロックは **CI (Required status checks) に集約** する。
>
> **役割分担**:
> | フェーズ | チェック | 性質 | 役割 |
> |---|---|---|---|
> | push 時 (即時) | CI / lint / test / build | ブロッキング (Required status checks) | 「壊れる変更」を止める |
> | PR 時 | CodeRabbit | 非ブロッキング | センサー (異常検知のみ、ゲートキーパーではない) |
> | 週次 (遅延) | ADR-031 全体レビュー | 包括 | cross-PR drift / semantic drift 補完 |
> | 任意 | 人間レビュー | event-driven | バグ発生時 / 大きい変更 / 設計変更 (ADR 絡み) のみ |
>
> **個人開発で許容するリスク** (チーム開発と前提が異なる):
> - 一時的な変なコード混入 → **修正コスト低 + コンテキスト保持 + ロールバック容易** で許容可能
> - 設計の歪みが遅れて発見 → 週次レビューで補完、修正コストは全体俯瞰時の方が低い
>
> **設計の本質**: 「全変更をレビューする」ではなく **「リスクの高い変更だけ止める」**。機械で止められるものは全部機械に寄せる。

- [ ] main branch protection 設定:
  - **Required status checks**: lint / test / build / rust-test / markdownlint / broken-link-check (markdownlint と broken-link-check は後続 PR で追加されるが先に rule を予約しておく)
  - **直接 push 禁止** (PR 必須)
  - ❌ **Required reviewers は設定しない** (人間レビューは event-driven で運用、必須化しない)
- [ ] CodeRabbit を非ブロッキング化 (センサー役):
  - Required status checks に含めない
  - Required reviewers 機構自体を使わないため、CodeRabbit の review approval も自動的に non-required となる
- [ ] 設定変更を確認 (`gh api repos/aloekun/claude-code-hook-test/branches/master/protection`)
- [ ] 運用方針を README または CLAUDE.md に短く明示 (例: 「人間レビューは event-driven (バグ / 大きい変更 / 設計変更時のみ)、定常レビューは ADR-031 週次レビューで実施」)

**Phase 2 の検討余地** (本 Phase pre のスコープ外、将来):
- **path-based selective review**: `docs/adr/`, `.claude/hooks-config.toml`, `src/cli-*` 等の高リスクパス変更時のみ手動確認フラグを required にする実装。GitHub CODEOWNERS + 部分的 Required reviewers で実現可能。「全部レビュー」ではなく「一部だけ」の思想
- **強制条件の追加**: テストカバレッジ下がったらブロック (codecov 等)、型エラー / lint エラーは既に CI でブロック。機械で止められるものは全部機械に寄せる思想の延伸

##### Phase α: ADR-031 Phase B (既存 todo.md エントリ、docs 整合性観点を追加)

- [ ] [docs/todo.md](todo.md) の「週次プロジェクト全体レビューパイプライン」エントリを実装
- [ ] architecture-reviewer の rubric に docs 整合性観点 (semantic drift) を追加
- [ ] dogfood 1 回成功

##### Phase broken-link: broken-link-check の quality_gate 統合 (PR-broken-link、独立 PR)

- [ ] markdown-link-check or lychee の選定 (実行時間 + 検査品質)
- [ ] `pnpm lint:links` script + push-runner-config.toml の lint group 統合
- [ ] `.markdown-link-check.json` 等の設定ファイル (除外 URL / リトライ / timeout)
- [ ] 既存違反の clean baseline 確立 (別 commit で先に対応)
- [ ] Required status checks に追加 (PR-pre 経由で予約済)

##### Phase β: ADR-032 起案 + 実装 (PR-β)

- [ ] [docs/adr/adr-032-docs-only-fast-path.md](adr/adr-032-docs-only-fast-path.md) 起案 (試験運用)
- [ ] [src/lib-jj-helpers/src/lib.rs](../src/lib-jj-helpers/src/lib.rs) に追加:
  - `classify_docs_only(files: &[String]) -> DocsOnlyClassification` 純関数 + unit test (boundary cases)
  - `should_force_full_review(files: &[String], threshold: usize) -> bool` (PR サイズ guard)
  - `log_docs_only_decision(...)` (メトリクスログ append)
- [ ] [src/cli-push-runner/src/stages/diff.rs](../src/cli-push-runner/src/stages/diff.rs) — `DiffResult::DocsOnly { files: Vec<String> }` variant 追加、`jj diff --name-only` 取得追加
- [ ] [src/cli-push-runner/src/main.rs](../src/cli-push-runner/src/main.rs) — `DocsOnly` で `skip_takt = true`、`--full-review` フラグ + `TAKT_NO_SKIP` env チェック + size guard
- [ ] [src/cli-pr-monitor/src/stages/monitor.rs](../src/cli-pr-monitor/src/stages/monitor.rs) — `start_monitoring` 冒頭で docs-only + size guard 判定 → takt skip
- [ ] [src/cli-merge-pipeline/src/feedback.rs](../src/cli-merge-pipeline/src/feedback.rs) — `run_ai_step` 入口に docs-only + size guard ガード追加
- [ ] config に `[docs_only] enabled = false, full_review_threshold_files = 20, metrics_log_path = ".claude/docs-only-metrics.jsonl"` 追加
- [ ] `.gitignore` に `.claude/docs-only-metrics.jsonl` 追加
- [ ] [CLAUDE.md](../CLAUDE.md) に ADR-032 リンク追加
- [ ] e2e 検証 (正例 / 反例 / override / size guard / boundary / CodeRabbit 非ブロッキング動作 / メトリクス検証)

##### Phase γ: enablement (PR-γ、PR-α + PR-broken-link 完了確認後)

- [ ] `[docs_only] enabled = true` への 1 行 flip
- [ ] 派生プロジェクト (techbook-ledger, auto-review-fix-vc) への展開も同期で検討

##### Phase δ: dogfood (PR-γ マージ後)

- [ ] 任意の docs PR で実体験
- [ ] 所要時間 ~15min → ~30sec を実証
- [ ] メトリクスログから skip 率 / saving / 誤検出を観測

##### Phase 観測: 4-6 週運用後の判断

- [ ] メトリクス集計 (skip 率、平均 saving、override 頻度、誤検出疑い)
- [ ] ADR-032 ステータスを「承認済み」に更新 or 改善 / 廃止判断
- [ ] 本 todo2.md エントリを削除 (運用ルール: 完了タスクは ADR/仕組みに反映後に削除)

##### Phase 2 (任意、観測結果次第): 段階的緩和

- [ ] `docs/**` 配下の assets (`.png`, `.svg` 等) も docs-only 扱いに含める拡張を検討
- [ ] size guard 閾値の調整
- [ ] `docs/adr/` 配下の特別扱い検討 (ADR 新規追加は full review か否か)

#### 作業可能になるための前提情報 (新セッションで必読)

##### 既存コンポーネントとの参照関係

| ファイル | 役割 | 編集内容 |
|---|---|---|
| [src/cli-push-runner/src/main.rs](../src/cli-push-runner/src/main.rs) | push pipeline メイン (line 62-75 に `DiffResult::Empty → skip_takt` 既存パターン) | `DocsOnly` 派生で踏襲 |
| [src/cli-push-runner/src/stages/diff.rs](../src/cli-push-runner/src/stages/diff.rs) | diff 取得経路 (`.takt/review-diff.txt` 出力済) | `--name-only` 追加 |
| [src/cli-pr-monitor/src/stages/monitor.rs](../src/cli-pr-monitor/src/stages/monitor.rs) | takt 呼び出し前の判定挿入点 | docs-only ガード追加 |
| [src/cli-merge-pipeline/src/feedback.rs](../src/cli-merge-pipeline/src/feedback.rs) | `run_ai_step` 入口 | docs-only ガード追加 |
| [src/lib-jj-helpers/src/lib.rs](../src/lib-jj-helpers/src/lib.rs) | classifier / size guard / metrics の追加先 | 純関数 3 つ追加 |

##### 重要な既存 ADR (実装時に必ず参照)

| ADR | 関係 |
|---|---|
| **ADR-022** | 責務分離。label/marker 案を却下する根拠 |
| **ADR-024** | lib-jj-helpers の API 拡張方針 (YAGNI、2 例目で crate 切り出し) |
| **ADR-027** | push-time = simplicity 限定。本タスクは「diff 局所判定」原則を更に絞る位置付け |
| **ADR-030** | 3 層分離パターン。本タスクと直交 (post-merge-feedback の skip は L2 takt 起動制御のみ) |
| **ADR-031** | 週次レビュー。本タスクの compensating check 実装先 (architecture facet 拡張) |

##### memory 参照

- `feedback_test_dry_antipattern.md`: テストの DRY 不適用。classifier の unit test は独立性優先
- `feedback_side_effect_integration.md`: 副作用は新 phase ではなく既存 phase 末尾に統合
- `feedback_no_empty_change_before_push.md`: `jj describe` 後そのまま `pnpm push` する
- `feedback_review_list_with_assessment.md`: 未対応レビュー列挙時に対応推奨度評価を添える

##### 設計上の重要な制約 (実装時に必ず守る)

| 制約 | 根拠 | 影響 |
|---|---|---|
| **拡張子で判定する (`docs/` prefix で判定しない)** | `README.md` 等が抜ける | classifier は extension based |
| **marker file / cache を使わない** | amend で stale 化、silent loss | 各 exe が独立に diff source から判定 |
| **CI rerun で override env 消滅対策** | キャッシュ持ちは害 | invocation 毎にチェック |
| **CodeRabbit を完全停止しない** | コメントは「気付き」経路として残す | takt の自動 fix loop だけ skip |
| **broken-link-check を CI から外さない** | 「壊れる変更を CI で止める」設計の核 | Required status checks に含める |
| **PR サイズ guard を削らない** | 大規模 docs reorganization の俯瞰レビュー保護 | threshold 20 で運用調整 |
| **メトリクスログを取らずに enable しない** | 効果検証 / 誤検出検知が困難 | `.claude/docs-only-metrics.jsonl` |
| **ADR-031 Phase B dogfood 成功前に `enabled = true` にしない** | compensating check 不在で bypass 開始は危険 | sequential 順守 |
| **branch protection 整備を後回しにしない** | 「壊れる変更」を止められない | PR-pre を最初に実施 |
| **override を pnpm script default に固定化しない** | 一時的な人間判断のみ | `TAKT_NO_SKIP=1` は env 1 回限り |

##### 残るトレードオフ

- **CI failure → takt 自動修正経路の損失**: pr-monitor を skip すると違反検出時の自動 fix は走らない。**ただし quality_gate に broken-link-check 追加で push 前ガードが効く**ため CI は基本通過想定
- **誤検出時の影響**: classifier に bug があると非 docs PR が skip → strict 定義 + size guard + メトリクス + unit test で対策
- **週次レビューの遅延リスク**: 即時 broken-link-check + CodeRabbit 非ブロッキングコメントで「気付き」の即時性は保つ

##### 新セッションで最初に確認すべきこと

1. `git log --oneline -10` で master 最新確認
2. [docs/todo.md](todo.md) と本 todo2.md を両方読む
3. `~/.claude/plans/1-docs-todo-md-askuserquestion-validated-orbit.md` を読む (本タスク策定時の plan)
4. [docs/adr/adr-027-push-review-simplicity-focus.md](adr/adr-027-push-review-simplicity-focus.md) を読む (前提となる diff 局所原則)
5. [docs/adr/adr-031-weekly-review-pipeline.md](adr/adr-031-weekly-review-pipeline.md) を読む (compensating check 実装先)
6. [docs/adr/adr-024-shared-jj-helpers-library.md](adr/adr-024-shared-jj-helpers-library.md) を読む (classifier 配置先判断根拠)
7. [docs/adr/adr-022-automation-responsibility-separation.md](adr/adr-022-automation-responsibility-separation.md) を読む (label/marker 却下根拠)
8. **どの Phase を実施するか確認**: 前提依存の状態 (markdownlint task / Phase α / Phase pre / Phase broken-link) を git log で確認し、未完了なら先に対応

#### 完了基準

- ADR-032 試験運用で `enabled = true` 状態が 4 週以上安定稼働
- 実 docs PR の所要時間 ~15min → ~30sec を実証 (10 倍以上)
- branch protection が main に設定され、Required status checks (lint/test/build/markdownlint/broken-link-check) が緑必須として機能
- 週次レビュー (ADR-031) で docs 整合性観点 (semantic drift) が機能し、bypass された検証が拾えていることを確認
- broken-link-check が docs-only PR で broken link を即時検出した実績 1 回以上 (即時ガードの動作確認)
- 誤検出 (非 docs PR の誤 skip) ゼロ — メトリクスログで検証
- override 機構の発動実績 1 回以上 (人間判断の経路維持確認)
- size guard 発動実績 1 回以上 (大規模 docs reorganization で full review に降格した実績)
- ADR-032 ステータスを「承認済み」に更新 (or 改善 / 廃止判断)
- 本 todo2.md エントリを削除

#### 詰まっている箇所

なし (全方向確定済、Phase pre から着手可能)。ただし以下の sequential 依存に注意:

```
Phase pre (branch protection)
   ↓
Phase α (ADR-031 Phase B、既存 todo.md タスク)
   ↓
Phase broken-link (quality_gate 拡張)
   ↓
Phase β (ADR-032 起案 + 実装、enabled = false default)
   ↓
Phase γ (enablement flip)
   ↓
Phase δ (dogfood)
   ↓
Phase 観測 (4-6 週)
   ↓
Phase 2 (任意、段階的緩和)
```
