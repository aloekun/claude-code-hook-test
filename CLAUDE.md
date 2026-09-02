# CLAUDE.md

## Architecture Decisions

- [ADR-001: hooks の実装言語として Rust を採用](docs/adr/adr-001-hooks-implementation-language.md)
- [ADR-002: PostToolUse で Biome + oxlint の二段階構成](docs/adr/adr-002-post-tool-use-linter-composition.md)
- [ADR-003: hooks の配置規則とビルド戦略](docs/adr/adr-003-hooks-layout-and-build-strategy.md) *(Superseded by ADR-010)*
- [ADR-010: hooks の配置規則とビルド戦略 v2](docs/adr/adr-010-hooks-layout-and-build-strategy-v2.md)
- [ADR-004: Stop フックによる品質ゲート](docs/adr/adr-004-stop-hook-quality-gate.md)
- [ADR-005: hooks の exe パスをテンプレートから自動生成](docs/adr/adr-005-hooks-path-resolution-with-template.md)
- [ADR-006: hooks の設定駆動型アーキテクチャ](docs/adr/adr-006-config-driven-hooks.md)
- [ADR-007: カスタムリンターの正規表現層/AST層の線引き](docs/adr/adr-007-custom-linter-layer-boundary.md)
- [ADR-008: Push Pipeline ハーネスの実装](docs/adr/adr-008-push-pipeline-harness.md)
- [ADR-009: Post-PR Monitor — push/PR作成後の CI・CodeRabbit 自動監視](docs/adr/adr-009-post-pr-monitor.md)
- [ADR-011: jj の新規ブックマーク push 戦略](docs/adr/adr-011-jj-push-new-bookmark-strategy.md)
- [ADR-012: src/ ディレクトリの命名規約](docs/adr/adr-012-src-naming-convention.md)
- [ADR-013: Merge Pipeline — PR マージ + ローカル同期](docs/adr/adr-013-merge-pipeline.md)
- [ADR-014: Post-Merge Feedback — マージ後のフィードバックループによる再発防止](docs/adr/adr-014-post-merge-feedback.md) *(試験運用)*
- [ADR-015: Push Pipeline を takt ベースの push-runner に移行](docs/adr/adr-015-push-runner-takt-migration.md) *(Supersedes ADR-008 の push 前パイプライン部分)*
- [ADR-016: Claude Code Bash ツールでの長時間コマンド実行戦略](docs/adr/adr-016-long-running-command-strategy.md)
- [ADR-017: takt バージョン固定と検証環境の維持](docs/adr/adr-017-takt-version-pinning.md)
- [ADR-018: cli-pr-monitor の takt ベース移行と CronCreate 廃止](docs/adr/adr-018-pr-monitor-takt-migration.md) *(Supersedes ADR-009 の daemon + CronCreate 部分)*
- [ADR-019: CodeRabbit レビュー運用のハイブリッド構成](docs/adr/adr-019-coderabbit-review-hybrid-policy.md)
- [ADR-020: takt facets (fix/supervise) の pre-push/post-pr 共通化戦略](docs/adr/adr-020-takt-facets-sharing.md)
- [ADR-021: jj 変更検出ロジックの設計原則](docs/adr/adr-021-jj-change-detection-principles.md)
- [ADR-022: 自動化コンポーネントの責務分離原則](docs/adr/adr-022-automation-responsibility-separation.md)
- [ADR-023: CodeRabbit false positive 対応スキル](docs/adr/adr-023-coderabbit-reject-thread-skill.md) *(試験運用)*
- [ADR-024: 共通 jj ヘルパーライブラリ](docs/adr/adr-024-shared-jj-helpers-library.md)
- [ADR-025: CwdRestore Drop guard パターン](docs/adr/adr-025-cwd-restore-drop-guard.md) *(試験運用)*
- [ADR-026: Cargo workspace による Rust パッケージ統合](docs/adr/adr-026-cargo-workspace.md)
- [ADR-027: Push-time review を simplicity に限定し architectural review は post-PR に委ねる](docs/adr/adr-027-push-review-simplicity-focus.md)
- [ADR-028: 外部可視成果物の生成コマンド (PR 作成/マージ) の実行ゲート](docs/adr/adr-028-pnpm-create-pr-gate.md)
- [ADR-029: Post-Merge Feedback の自動起動 — pending file + 現セッション起動](docs/adr/adr-029-post-merge-feedback-auto-trigger.md) *(試験運用)*
- [ADR-030: 決定論的 Post-Merge Feedback — takt 経由の同期実行 + 失敗マーカーによる recovery](docs/adr/adr-030-deterministic-post-merge-feedback.md) *(試験運用 / Supersedes ADR-014 full, ADR-029 partial)*
- [ADR-031: 週次プロジェクト全体レビューパイプライン — whole-tree review の自己改善ループ](docs/adr/adr-031-weekly-review-pipeline.md) *(試験運用)*
- [ADR-033: todo.md 採番管理の簡素化 — 絶対番号は table のみに保持](docs/adr/adr-033-todo-numbering-simplification.md) *(試験運用)*
- [ADR-034: CodeRabbit 監視・対話の自動化戦略 — Bundle a 設計根拠](docs/adr/adr-034-coderabbit-auto-monitoring.md) *(試験運用)*
- [ADR-035: docs-only PR 評価ポリシー](docs/adr/adr-035-doc-evaluation-policy.md)
- [ADR-036: Bundle Z — 決定論層 + 制約付き修正 + 異常検知レビュアーの 3 層アーキテクチャ](docs/adr/adr-036-bundle-z-three-layer-review.md) *(試験運用)*
- [ADR-037: takt fix-trust shortcut — convergence_verdict による Iter 3 短絡](docs/adr/adr-037-takt-fix-trust-shortcut.md) *(試験運用)*
- [ADR-038: ローカル LLM による CodeRabbit findings classification](docs/adr/adr-038-local-llm-finding-classification.md) *(試験運用)*
- [ADR-039: Experimental feature 標準パターン (config opt-in + kill-switch + bounded lifetime)](docs/adr/adr-039-experimental-feature-standard-pattern.md) *(試験運用)*
- [ADR-040: Local LLM Context Size と Resource Trade-off](docs/adr/adr-040-local-llm-context-size.md) *(試験運用)*
- [ADR-041: Test Isolation Patterns for Multi-Condition Guards](docs/adr/adr-041-test-isolation-patterns.md) *(試験運用)*
- [ADR-042: ルール vs 仕組み化の境界基準](docs/adr/adr-042-rule-vs-mechanism-boundary.md) *(試験運用)*
- [ADR-043: Security/Quality Gate での Fail-Closed 原則](docs/adr/adr-043-security-gates-fail-closed.md) *(試験運用)*
- [ADR-044: subprocess utility extraction の境界判定 — 共通化と分離の線引き](docs/adr/adr-044-subprocess-utility-extraction-boundary.md) *(試験運用)*
- [ADR-045: jj workspace による並列セッション運用 — メイン作業と細粒度改善の分離](docs/adr/adr-045-jj-workspace-parallel-sessions.md) *(試験運用)*
- [ADR-046: ローカル LLM pre-push レビュアー — 選定スパイクと不採用判断](docs/adr/adr-046-local-llm-review-spike.md) *(却下)*
- [ADR-047: pre-push review の反証（refute）facet](docs/adr/adr-047-prepush-refute-facet.md) *(試験運用)*
- [ADR-048: reviewers→fix findings handoff の output-contract 標準化（markdown 統一・JSON 却下）](docs/adr/adr-048-facet-findings-handoff-markdown-contract.md) *(試験運用)*
- [ADR-049: incident→eval 回帰スイート（カスタムルールの由来 incident 再現テスト）](docs/adr/adr-049-incident-eval-regression-suite.md) *(試験運用)*
- [ADR-050: multi-iteration workflow の decision criteria scope 明示](docs/adr/adr-050-iteration-aware-decision-criteria.md) *(試験運用)*
- [ADR-051: クロスシステム設定 coupling パターン — 内部設定と外部 SaaS 設定の論理結合の設計規律](docs/adr/adr-051-cross-system-config-coupling.md) *(試験運用)*
- [ADR-052: 自律実行境界の 2 クラス分類（ADR-028 の 2 段化）](docs/adr/adr-052-autonomy-execution-boundary-classes.md) *(試験運用)*
- [ADR-053: Stop hook による tool call leak 検知](docs/adr/adr-053-stop-tool-call-leak-detection.md) *(試験運用)*
- [ADR-054: prompt injection 信頼境界の 3 層防御](docs/adr/adr-054-prompt-injection-trust-boundary-defense.md) *(試験運用)*
- [ADR-055: 発火テレメトリ収集層 — ハーネス ROI 棚卸しの決定論的観測基盤](docs/adr/adr-055-firing-telemetry-collection.md) *(試験運用)*
- [ADR-056: takt builtin review policy の shadow — policy 層を anomaly 設計に整合させる](docs/adr/adr-056-review-policy-anomaly-shadow.md) *(採用)*
- [ADR-057: docs-only / 空 diff の決定論 routing — instruction 規約から決定論機構への昇格](docs/adr/adr-057-docs-only-deterministic-routing.md) *(採用)*
- [ADR-058: fix 後の決定論再ゲート (post-takt re-gate) — pre-push 経路への機械的 backstop 拡張](docs/adr/adr-058-post-takt-regate.md) *(採用)*
- [ADR-059: hook 通知の可視化チャネル分離 (systemMessage = ユーザー向け / additionalContext = モデル向け)](docs/adr/adr-059-hook-system-message-visibility.md) *(採用)*
- [ADR-060: Cloud ハーネス有効化 — tracked dispatcher 登録 + SessionStart 実体確保の 2 層分離](docs/adr/adr-060-cloud-harness-sessionstart-dispatcher.md) *(試験運用)*
- [ADR-061: tool call leak の hard-fail 経路対応 — Stop 不発火の回収層 + scan_tail 合成エントリ耐性](docs/adr/adr-061-tool-call-leak-hardfail-recovery.md) *(試験運用)*
- [ADR-062: 月次ハーネス ROI レビュー — telemetry 発火実績によるハーネス複雑度の棚卸し (WP-12 step 2/3)](docs/adr/adr-062-monthly-harness-roi-review.md) *(試験運用)*
- [ADR-063: Linux 可搬性レイヤ + nightly release + cloud-setup — クラウド向けプリビルドバイナリ配布](docs/adr/adr-063-linux-portability-release-binaries.md)
- [ADR-064: PR 監視 success 判定の陽性証拠要求 — レート制限 silent success の排除](docs/adr/adr-064-monitor-success-positive-evidence.md)
- [ADR-065: CI matrix による移植退行防止 — 両 OS で同一スイートを回す](docs/adr/adr-065-ci-matrix-cross-os-regression.md) *(試験運用)*
- [ADR-066: 自律実行の全体 kill-switch — 正極性単一フラグと「欠損 → 安全状態」原則](docs/adr/adr-066-autonomy-global-kill-switch.md) *(試験運用)*
- [ADR-067: Phase B 無人 fix push — agent を push の主体にしない 4 軸ゲート](docs/adr/adr-067-phase-b-unattended-fix-push.md) *(試験運用)*
- [ADR-068: pre-push fix step の権限境界 — 後退検知 backstop と設計級 remedy の human routing](docs/adr/adr-068-fix-step-authority-boundary.md) *(試験運用)*
- [ADR-069: PR chain 宣言規約 — 分割チェーンと missing-consumer 検査の両立](docs/adr/adr-069-pr-chain-declaration.md) *(試験運用)*
- [ADR-070: weekly-review の分析フェーズを cloud routine へ移行 — 常時性の獲得と成果物デリバリの未解決](docs/adr/adr-070-weekly-review-cloud-routine.md) *(試験運用)*
- [ADR-071: 未マージの自律 PR 数による背圧 — autonomous-pr クラスの自主減速](docs/adr/adr-071-draft-pr-backpressure.md) *(試験運用)*
- [ADR-072: 夜間 todo 消化ループ — 無人実装から PR 作成までの決定論経路](docs/adr/adr-072-nightly-todo-loop.md) *(試験運用)*
- [ADR-073: 作業パッケージの完了条件 — 生んだ運用問題を外へ押し出さない](docs/adr/adr-073-work-package-completion-boundary.md) *(試験運用)*
- [ADR-074: auto lane 選別基準 — 台帳のどの行を夜間ループに割り当てるか](docs/adr/adr-074-auto-lane-screening-criteria.md) *(試験運用)*
- [ADR-075: 着手前の前提検証 — 台帳・フィードバックの記述を実測で確かめる](docs/adr/adr-075-verify-premises-before-acting.md)
- [ADR-076: testability gate — I/O 出力のインライン解釈を push で止める](docs/adr/adr-076-testability-gate.md) *(試験運用)*
- [ADR-077: open-questions gate — 未解決の設計の問いが push を止める](docs/adr/adr-077-open-questions-gate.md) *(試験運用)*
- [ADR-078: takt verdict gate — REJECT のまま push されるのを止める](docs/adr/adr-078-takt-verdict-gate.md) *(試験運用)*
- [ADR-079: 起票由来タグ — 機構の効果を印象でなく数で測る](docs/adr/adr-079-defect-origin-tagging.md) *(試験運用)*

## 開発 convention / チェックリスト

- [開発 convention / チェックリスト](docs/dev-conventions.md) — spike 見送り (negative result) 永続化 convention (順位261)、外部 SaaS 無料枠 / 制限の調査チェックリスト (順位262)、外部 fixture 参照テストは値まで assert (順位274)、PR chain の分割と宣言 (ADR-069)、LLM を含む自動化経路は実走でしか検証できない (ADR-067)、takt facet の出力言語は各 instruction に直書きする、GitHub Actions の `run:` は常に `-e` 付きで起動する (順位 319)、台帳の `照合除外:` マーカー (理由必須・fail-closed)、夜間 PR のリベースは `pnpm rebase-nightly` で行う (ADR-072 決定 21)

## Build

```sh
pnpm build:all     # 全 hooks/CLI exe を一括ビルド
pnpm deploy:hooks  # 派生プロジェクトに exe を配布
```
