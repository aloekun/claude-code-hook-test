# CLAUDE.md

## Architecture Decisions

- [ADR-001: hooks の実装言語として Rust を採用](docs/adr/adr-001-hooks-implementation-language.md)
- [ADR-002: PostToolUse で Biome + oxlint の二段階構成](docs/adr/adr-002-post-tool-use-linter-composition.md)
- [ADR-003: hooks の配置規則とビルド戦略](docs/adr/adr-003-hooks-layout-and-build-strategy.md)
- [ADR-004: Stop フックによる品質ゲート](docs/adr/adr-004-stop-hook-quality-gate.md)

## Build

```sh
pnpm build:hooks  # 全 hooks exe を一括ビルド
```
