# ADR-080: Rust module 分割の不変条件 — behavior 不変 / `pub(crate)` / test helper は複製

## ステータス

採用 (2026-09-13)

> 800 行超の `.rs` を module 分割するときに守る 3 つの制約を決定として固定する。
> 分割の起動そのものは `.claude/hooks-config.toml` の `[file_length_gate]` が機械的に促し、
> 関数長 50 行 / ファイル長 800 行 / 非 doc コメント禁止も既に hook が強制している。
> **機械が見られないのは「その diff が behavior 不変か」だけ**であり、本 ADR はそこだけを扱う。

## コンテキスト

file-length gate が 800 行で分割を促すため、module 分割は繰り返し発生する操作である。
分割 PR は「ただ動かすだけ」に見えて、レビューでは次を区別できない:

- 移動のついでに signature を変えた / default 値を変えた
- crate 外に出す必要のない項を `pub` にした
- test helper を共有 module へ括り出し、テスト間に隠れた結合を作った

いずれも diff 上は「移動」と同じ形をしており、**意味を読まないと判定できない**
([ADR-042](adr-042-rule-vs-mechanism-boundary.md) Step 1 の機械化不能条件)。
regex でも AST でも「behavior 不変の機械的な移動か」は判定できないため、
検査を書かずに**レビューが見る点を固定する**。

## 決定

800 行超 `.rs` の module 分割に、次の 3 制約を適用する。

### 1. behavior 不変

関数 signature の変更 / field rename / default 値の変更をしない。機械的な移動と
visibility 調整のみを行う。**test count も分割前後で一致させる** (着手前に master HEAD で
baseline を測る)。数が減っていれば移動の取りこぼし、増えていれば分割に便乗した追加であり、
どちらも分割 PR の外へ出す。

> test count の一致を検査として機械化しない判断は [ADR-042](adr-042-rule-vs-mechanism-boundary.md)
> § 改訂 (2026-09-12) が持つ — 実害の観測なしに機械化を試みない。

### 2. cross-module visibility は `pub(crate)`

`pub` は crate 外への公開を意味する。分割で生じた module 間参照に `pub` を使うと、
本来内部実装である項が crate の公開 API になる。分割の副作用で API 面を広げない。

### 3. test helper は per-module に複製する

`unique_temp_root` のような test helper は共有 module を抽出せず、各 test module へ
独立に複製する。共有 test util module は anti-pattern である — helper の変更が
無関係なテストへ波及し、テストが**独立に失敗しなくなる**
([ADR-041](adr-041-test-isolation-patterns.md) の test isolation と同じ理由)。

## 帰結

### 利点

- 分割 PR のレビューが「移動以外が混ざっていないか」の 1 点に絞れる。
- 分割を何度繰り返しても crate の公開 API が広がらない。

### 欠点 / 留意点

- helper の複製はコード重複に見える。重複を嫌って共有化すると 3 が崩れるため、
  **複製が意図であること**を各 helper の doc コメントに書く。
- 並列 PR で `Cargo.lock` が競合したときの rebase 手順は
  [ADR-045](adr-045-jj-workspace-parallel-sessions.md) § 調整ポイント 2 が持つ。
- `PR_SIZE_CHECK_OVERRIDE` / `FILE_LENGTH_CHECK_OVERRIDE` の正当な use case は
  [ADR-069](adr-069-pr-chain-declaration.md) § 3 (切断点ヒューリスティクス) の項目 3 と
  `.claude/hooks-config.toml` の該当 section が持つ。

## References

- [ADR-042: ルール vs 仕組み化の境界基準](adr-042-rule-vs-mechanism-boundary.md) — 本 ADR を検査にしない根拠
- [ADR-041: Test Isolation Patterns](adr-041-test-isolation-patterns.md) — 制約 3 の背景
- [ADR-069: PR chain 宣言規約](adr-069-pr-chain-declaration.md) § 3 — size gate に当たったときの切断点と override の扱い
- [ADR-012: src/ ディレクトリの命名規約](adr-012-src-naming-convention.md) — 分割先の命名

> 本 ADR の内容は、2026-09-13 に廃止した開発 convention 集の Rust 分割節から移設した (順位 445)。
