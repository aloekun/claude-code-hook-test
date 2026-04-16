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
- [ADR-024: 共通 jj ヘルパーライブラリ](docs/adr/adr-024-shared-jj-helpers-library.md) *(試験運用)*
- [ADR-025: CwdRestore Drop guard パターン](docs/adr/adr-025-cwd-restore-drop-guard.md) *(試験運用)*
- [ADR-026: Cargo workspace による Rust パッケージ統合](docs/adr/adr-026-cargo-workspace.md)

## Automated actor boundary (ADR-022)

takt / claude -p / cli-* の副作用は**コード修正とレポート生成**に限定する。以下は人間の責務であり、automated actor は触らない:

- commit message / commit description
- bookmark / branch 名
- tag
- PR title / PR body (作成時の user-supplied text をそのまま使う)

## Build

```sh
pnpm build:all     # 全 hooks/CLI exe を一括ビルド
pnpm deploy:hooks  # 派生プロジェクトに exe を配布
```
