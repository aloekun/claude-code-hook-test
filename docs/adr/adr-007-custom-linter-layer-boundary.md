# ADR-007: カスタムリンターの正規表現層/AST層の線引き

## ステータス

承認済み (2026-03-29)

## コンテキスト

Claude Code の実装中に発見した禁止事項を都度カスタムリンタールールとしてビルドアップし、PostToolUse フックでフィードバックループを構築する方針を採用した（参考: harness-engineering-best-practices-2026）。

カスタムルールの検出エンジンとして「正規表現（Rust exe 内蔵）」と「AST 解析（ast-grep 外部委譲）」の 2 層構成を取るが、ルール追加時にどちらの層に配置するかの判断基準が必要。

### 考慮事項

- 正規表現は高速（~1ms）だがコメント・文字列リテラル内の誤検出リスクがある
- AST 解析は正確だがプロセス起動コスト（~50-100ms）が毎回発生する
- PostToolUse hook は Write/Edit のたびに発火するため、速度が体験に直結する
- ルールは段階的に増やすため、判断基準が曖昧だと層の責務が崩れる

## 決定

### 判断フロー（3 問で決定）

```text
Q1. 違反は 1 行だけ見て判定できるか？
    └─ No → AST 層

Q2. コメント・文字列リテラル内の誤検出が問題になるか？
    └─ Yes → AST 層

Q3. パターンはリテラル文字列のマッチのみで表現できるか？
    （後読み・先読み・複雑なキャプチャグループは「No」）
    └─ No → AST 層
    └─ Yes → 正規表現層
```

3 問すべてが正規表現層を指す場合のみ正規表現層に配置する。**迷ったら AST 層に寄せる**。

### 正規表現層（Rust exe 内・custom-lint-rules.toml）

- **用途**: リテラル文字列のマッチのみ
- **速度**: ~1ms（プロセス起動なし）
- **設定**: `.claude/custom-lint-rules.toml`
- **適用例**:
  - `console.log(` — トークンが固定、コメント内に書く動機がない
  - `from '../../../` — 深い相対パスの文字列マッチ
  - `from '../infra/` — 禁止 import パスの文字列マッチ
  - `TODO` / `FIXME` 残留検出

### AST 層（ast-grep 外部委譲・YAML ルールファイル）

- **用途**: 構文上の文脈が必要なもの + 複雑なパターン
- **速度**: ~50-100ms（プロセス起動コスト）
- **設定**: `hooks-config.toml` のパイプラインステップ + ast-grep YAML ルール
- **適用例**:
  - TypeScript `any` 型使用禁止（`company`, `many` 等の誤検出回避）
  - 命名規則違反（変数宣言の文脈が必要）
  - 特定メソッド呼び出し禁止（スコープ判定が必要）
  - コメント・文字列内の誤検出が許容できないルール全般

### 配置の具体例

| ルール | Q1 (1行) | Q2 (誤検出) | Q3 (リテラル) | 配置 |
|--------|----------|------------|-------------|------|
| console.log 禁止 | Yes | No | Yes | 正規表現層 |
| 深い相対パス制限 | Yes | No | Yes | 正規表現層 |
| 禁止 import パス | Yes | No | Yes | 正規表現層 |
| any 型使用禁止 | Yes | **Yes** | - | AST 層 |
| 命名規則違反 | **No** | - | - | AST 層 |
| 未使用 export | **No** | - | - | AST 層 |

## 影響

- ルール追加時の判断が 3 問のフローチャートで機械的に決定可能
- 正規表現層は「シンプルで速い」、AST 層は「正確だが遅い」という責務分離が維持される
- 将来 ast-grep を導入する際、既存の正規表現ルールを移行する必要がない（責務が明確に分かれているため）
- `regex` crate 追加による exe サイズ増加は約 200KB 程度（許容範囲）

## Amendment (2026-05-17, PR #140 由来)

PR #140 のルール⑧ (`no-docs-relative-back-to-docs`) で「`paths` filter 未適用のまま pattern semantics で自己限定する」設計を採用した経験から、以下 2 項目を追記する。

### Semantic self-limitation の安全条件 vs explicit `paths` filter 必須条件

正規表現層のルールでスコープを絞る手段は 2 通りある:

1. **Semantic self-limitation**: pattern 自身が path-context を含意し、対象外ファイルでは false positive を発生させない
2. **Explicit `paths` filter**: `paths = ["docs/**/*.md"]` 等で対象ファイルを明示的に限定

判断基準:

| 条件 | 推奨手法 | 例 |
|------|---------|-----|
| pattern が path-context を含意する（対象外ファイルでは自然な記述形式と区別される） | Semantic self-limitation で OK | ルール⑧ の `DOTDOT/docs/` 形式 = parent-dir 経由で `docs/` を再参照するため、`docs/` 配下以外では自然に出現しない |
| pattern が path-agnostic（対象外ファイルでも頻出し、特定スコープに限定したい） | Explicit `paths` filter 必須 | `eprintln!` は src/ 全体で頻出、特定 crate のみに限定したい場合 |
| 対象外ファイルで意図的に fire させたい（true positive として扱う設計） | `paths` filter 適用は避ける | ルール⑧ の root-level MD (CLAUDE.md / README.md) からの参照は true positive 扱い |

**判断 flow**: pattern を grep で実測し、対象外ファイルでの false positive 比率が低水準（目安: 数 % 以下）に収まるなら semantic self-limitation で OK、超えるなら `paths` filter 必須。**迷ったら `paths` filter に寄せる**（AST 層への寄せと同方針）。

### Lint rule 最小テストチェックリスト

新規 lint rule 追加時の必須テスト構成:

1. **Pattern detection test**: 想定するアンチパターン入力で fire することを確認（rule が機能する基本契約）
2. **Case-insensitive test** (該当する場合): pattern に `(?i)` を含めるなら、大文字バリアント / 小文字バリアントの双方で fire することを確認（PR #91 の PowerShell `(?i)` 教訓 = case-insensitive 宣言と実テストの乖離を防止）
3. **False positive skip test**: 似て非なる正当パターンで fire しないことを確認（semantic self-limitation を採用したルールではこのテストが scope 契約を担保する）

3 項目を最低水準とし、ルールごとに追加の boundary test（UTF-8 マルチバイト境界、複数行 / 単一行等）を加える。
