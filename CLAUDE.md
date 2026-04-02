# CLAUDE.md

## Architecture Decisions

- [ADR-001: hooks の実装言語として Rust を採用](docs/adr/adr-001-hooks-implementation-language.md)
- [ADR-002: PostToolUse で Biome + oxlint の二段階構成](docs/adr/adr-002-post-tool-use-linter-composition.md)
- [ADR-003: hooks の配置規則とビルド戦略](docs/adr/adr-003-hooks-layout-and-build-strategy.md)
- [ADR-004: Stop フックによる品質ゲート](docs/adr/adr-004-stop-hook-quality-gate.md)
- [ADR-005: hooks の exe パスをテンプレートから自動生成](docs/adr/adr-005-hooks-path-resolution-with-template.md)
- [ADR-006: hooks の設定駆動型アーキテクチャ](docs/adr/adr-006-config-driven-hooks.md)
- [ADR-007: カスタムリンターの正規表現層/AST層の線引き](docs/adr/adr-007-custom-linter-layer-boundary.md)
- [ADR-008: Push Pipeline ハーネスの実装](docs/adr/adr-008-push-pipeline-harness.md)
- [ADR-009: Post-PR Monitor — push/PR作成後の CI・CodeRabbit 自動監視](docs/adr/adr-009-post-pr-monitor.md)

## Build

```sh
pnpm build:hooks   # 全 hooks exe を一括ビルド
pnpm deploy:hooks  # 派生プロジェクトに exe を配布
```
