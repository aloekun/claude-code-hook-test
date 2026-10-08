# ADR-084: テスト専用コードは共通化しない — test module ごとに複製する

## ステータス

採用 (2026-10-08)

> テストのためだけに書いたコード (guard / setup / lock / fixture 生成など) は、test module を
> またいで共有しない。使う test module ごとに複製する。共通化した helper を介して
> テスト同士が依存し、1 つのテストのための修正が無関係なテストを壊すのを防ぐ。

## コンテキスト

テスト専用コードの重複は、レビューで繰り返し DRY 違反として指摘されてきた:

- `CwdRestore` (cwd を退避・復元する Drop guard) は 10 定義 / 7 ファイルに複製されている
  (2026-10-08 実測)。[ADR-025](adr-025-cwd-restore-drop-guard.md) は当初「2 例目が出たら
  `src/lib-test-helpers/` へ集約する」と定めていた。PR #385 の pre-push review も重複を指摘し、
  順位 413 として集約か見送りかの判断が台帳に残っていた
- `unique_temp_root` は 7 定義 / 7 ファイルに複製されている
- `EXEC_STAGING_LOCK` / `exec_staging_guard()` は PR #423 で 2 crate に意図的に複製した。
  pre-push review と post-merge feedback の双方が DRY 違反として指摘していた。共有化の再評価として
  台帳に残っていた順位 473 は、本 ADR により却下した

それぞれの判断は個別の ADR や台帳エントリに散っていた。プロジェクト全体の方針として
書かれていたのは [ADR-080](adr-080-rust-module-split-invariants.md) § 3 だけで、その対象は
800 行超の `.rs` を module 分割する場面に限られる。分割以外の場面 (新しい crate のテスト、
既存の複製の集約提案) で同じ問いが出るたびに、判断をやり直していた。

### 共通化の害

共通 helper を入れると、それを使うテストはすべて helper の実装に依存する。
あるテストの都合で helper の挙動を変えると (待ち時間を延ばす、後片付けを足す、
戻り値を変える)、helper を使う無関係なテストの前提が変わる。壊れたテストの原因は
そのテストのコードではなく helper の差分にあるため、失敗を見ても原因に辿り着きにくい。

テストは**独立に失敗する**ことに価値がある ([ADR-041](adr-041-test-isolation-patterns.md) の
test isolation と同じ理由)。重複の除去で得られる保守性より、この独立性を優先する。

## 決定

### 1. 共有の単位は test module まで

テスト専用コードを共有してよいのは、1 つの test module の中だけである:

- Rust の unit test: 1 ファイルの `#[cfg(test)] mod tests` (または分割した `tests.rs`)
- Rust の integration test: `tests/` 配下の 1 ファイル
- Node のテスト: 1 つの `*.test.mjs`

その範囲を超えて使うときは、使う側の test module へ**複製する**。次のような形の共有は作らない:

- テスト専用の crate (`src/lib-test-helpers/` など)
- integration test 間の共有 module (`tests/common/mod.rs` など)
- production crate に置いたテスト専用の公開項 (`#[cfg(any(test, feature = "test-support"))]` で
  公開する helper など)

### 2. 対象はテスト専用コードに限る

本 ADR が禁じるのは「テストのためだけに存在するコード」の共有である。production の
ライブラリをテストから使うことは対象外で、たとえば `lib-subprocess` の bounded wait を
integration test が `[dev-dependencies]` 経由で使うのは従来どおりでよい。production コードは
production の都合で変わり、その契約はそれ自身のテストが固定しているため、テスト側の都合で
挙動が変わることがない。

### 3. 複製が意図であることを doc コメントに書く

複製した helper には、複製が意図であることを doc コメントで示す。重複はレビューで繰り返し
DRY 違反として指摘されるため、書いておかないと集約の提案が再発する
([ADR-080](adr-080-rust-module-split-invariants.md) § 欠点 / 留意点 と同じ扱い)。

### 4. 機械化しない

現時点でリポジトリにテスト専用の共有 module は無く (2026-10-08 確認: `tests/common/`、
テスト専用 crate、`test-support` feature のいずれも存在しない)、違反の実害は観測されていない。
[ADR-042](adr-042-rule-vs-mechanism-boundary.md) § 改訂 (2026-09-12) に従い、観測なしに
検査を書かない。共有 module が実際に作られ、レビューを通過した事例が観測されたら、
`tests/common/` や `lib-test-helpers` の新設を検出する lint を検討する。

## 却下した案

- **2 例目 (または 3 例目) が出たら集約する**: ADR-025 と PR #423 の当初方針。閾値を何にしても、
  集約した時点で上記の依存が生まれる。重複の数は害の大きさと関係しないため、閾値で判断しない。
- **集約したうえで helper を変更しない運用にする**: helper を凍結しても、新しいテストの都合で
  変更したくなる圧力は消えない。凍結を守れたかを機械で確かめる手段も無い。

## 結果

### 利点

- テストの失敗原因がそのテストのファイル内に閉じる。
- 「集約するか」の判断を場面ごとにやり直さずに済む。

### 欠点 / 留意点

- 同じ helper のバグを直すときは、複製をすべて探して直す必要がある。`rg` で定義名を
  検索すれば足りる規模であり (最大で `CwdRestore` の 10 定義)、許容する。
- 本 ADR は ADR-025 の集約計画を置き換える。

## References

- [ADR-025](adr-025-cwd-restore-drop-guard.md): `CwdRestore` の集約計画を本 ADR により却下
- [ADR-041](adr-041-test-isolation-patterns.md): test isolation
- [ADR-042](adr-042-rule-vs-mechanism-boundary.md): ルールと仕組み化の境界
- [ADR-044](adr-044-subprocess-utility-extraction-boundary.md): production の subprocess utility の
  共通化基準 (本 ADR の対象外)
- [ADR-080](adr-080-rust-module-split-invariants.md) § 3: module 分割時に test helper を複製する制約
