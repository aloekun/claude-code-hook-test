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

## 開発 convention / チェックリスト

- [開発 convention / チェックリスト](docs/dev-conventions.md) — spike 見送り (negative result) 永続化 convention (順位261)、外部 SaaS 無料枠 / 制限の調査チェックリスト (順位262)

## Build

```sh
pnpm build:all     # 全 hooks/CLI exe を一括ビルド
pnpm deploy:hooks  # 派生プロジェクトに exe を配布
```
