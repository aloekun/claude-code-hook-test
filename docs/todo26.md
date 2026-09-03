# TODO (Part 26)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: `docs/todo25.md` がファイルサイズ 50121 B (2026-08-23 時点、50KB = 51200 B の安定読み取り閾値まで残り 1079 B) に到達したため、新規エントリは本ファイルに記録する (2026-08-23 新設)。**新規エントリの追加先は本ファイル**。todo.md / todo3.md 〜 todo25.md の既存エントリは引き続き有効、相互に独立。
>
> **サイズ表記について**: 各記載は**その時点の計測値**であり、現在値と一致しないことがある。現在値が必要なら計測すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 自律実行ガードレールの写しずれ (2026-08-25 PR W 着手前調査で判明)

### 順位 492: agent プロンプトの禁止パス列挙から台帳が欠落している (順位 492)

> **動機**: [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の禁止パスは 3 箇所に写しがあるが、
> **agent プロンプトの列挙だけ 1 件少ない**。2026-08-25 に全 3 箇所を突き合わせて実測した。
>
> | 写しの所在 | 件数 | `docs/claude-code-web-tasks.md` |
> |---|---|---|
> | Guard step の正規表現 ([nightly-todo.yml](../.github/workflows/nightly-todo.yml) 551 行付近) | 9 | **あり** |
> | ADR-072 決定 6 の列挙 | 9 | **あり** |
> | agent プロンプトの制約列挙 (同 438-440 行付近) | **8** | **無し** |
>
> **強制層はずれていない** — 実際に push を止めるのは Guard の正規表現で、そこには台帳が
> 入っている。したがって fail-closed は保たれており、agent が台帳を書き換えた PR が
> 人間のレビュー面へ出ることはない。
>
> **実害は run を 1 回捨てること**。agent は「台帳を触るな」と指示されないまま台帳を編集し、
> Guard で `[NIGHTLY_DENY] 自律動作のガードレールを変更しているため push しません` に当たる。
> 順位 486 (deny 該当行を台帳側で弾く) が塞ぐのと**同じクラスの損失**を、別の入り口から作る。
>
> **順位 454 との関係**: 454 は「3 点同期を cargo test で機械検証する」検査の実装で、
> 本タスクは**その検査が最初に検出するはずの現存ずれ 1 件の修正**である。454 を先に実装すると
> レッドで止まるため、どちらを先にしても本修正は要る。**454 に統合してもよい**が、
> 検査の実装 (Effort S) と現存ずれの修正 (Effort XS) は独立して着地できる。
>
> **参照**: [`.github/workflows/nightly-todo.yml`](../.github/workflows/nightly-todo.yml)
> (Guard step の deny 正規表現 / agent プロンプトの制約列挙)、
> [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6、順位 454 / 順位 486
>
> **実行優先度**: 🔧 **Tier 2** — Severity Low (fail-closed は成立しており、汚染は起きない) /
> Frequency Low (agent が台帳を触ろうとしたときだけ) / Effort **XS** / Adoption Risk None。

#### 設計決定 (案)

- agent プロンプトの列挙に `docs/claude-code-web-tasks.md` を足して 9 件に揃える。
  **他の 2 箇所は触らない** (そちらが正)
- 順位 454 の検査を同時に実装するなら、本修正を入れないとその検査が赤で着地する。
  実装順は「本修正 → 454」または「454 と同一 PR」のいずれか

- [ ] agent プロンプトの制約列挙に台帳のパスを追加する
- [ ] 3 箇所のパス集合が完全一致することを目視ではなく列挙で確認する
- [ ] `pnpm lint:workflows` green

#### 完了基準

Guard 正規表現 / ADR-072 決定 6 / agent プロンプトの 3 箇所が**同一のパス集合**を持つこと。
順位 454 の検査を実装したときに、この 3 点同期がそのまま green で通ること。

**auto lane に載せない** — `.github/workflows/` は Guard 禁止パス
([ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 3)。

---

### 順位 500: `cfg(test)` 判定を ident 単位にし testability gate の fail-closed 経路を固定する

> **動機**: `has_cfg_test` が文字列マッチで `#[cfg(test)]` を探しており、`#[cfg(test_util)]` の
> ような別の属性にも当たる。PR [#456](https://github.com/aloekun/claude-code-hook-test/pull/456)
> の post-merge feedback が指摘した。判定層にはテストがあったが、**ident 境界という入力の軸が
> 覆われていなかった** (G2)。
>
> **由来**: `[defect:G2]`。証拠 = PR #456。

#### 作業内容

- `has_cfg_test` の文字列マッチを ident のトークン単位比較へ変更する (`syn` で読んでいる情報を使う)
- `is_scan_target` の除外規則 (root 直下 `tests.rs` / `tests/foo.rs`) の回帰テストを追加する
- `scan_incomplete` を含む fail-closed 経路の回帰・統合テストを追加する

#### 完了基準

- `#[cfg(test_util)]` が `#[cfg(test)]` と誤認されないことをテストが固定している
- 除外規則と fail-closed 経路が、実装を潰すと落ちるテストで覆われている

---

### 順位 501: 由来タグ判定の単語境界と rustdoc 相対リンクの段数を検査する

> **動機**: 2 件の判定精度の欠陥。いずれも PR [#472](https://github.com/aloekun/claude-code-hook-test/pull/472)
> / PR [#463](https://github.com/aloekun/claude-code-hook-test/pull/463) の post-merge feedback で
> 判明した。
>
> 1. `origin_markers` の `contains_run_id` / `contains_pr_reference` に単語境界の判定が無く、
>    `rerun 123456` のような文字列を run ID と読む。証拠の検査が**緩む向き**の誤りである
> 2. rustdoc の相対リンク (`../../../docs/adr/...`) の `../` 段数が module の実際の深さと
>    合っているかを誰も見ていない。段数がずれたリンクは docs-lint の cross-ref 検査も通る
>
> **由来**: `[defect:G2]`。証拠 = PR #472 / #463。どちらも判定層にテストはあったが、入力空間
> (単語境界 / パス深度) が覆われていなかった。

#### 作業内容

- `contains_run_id` / `contains_pr_reference` に単語境界チェックを入れ、偽陽性の回帰テストを足す
  (`rerun` vs `run`、hex カラーコード等)
- GitHub slug 生成 (空白 → ハイフン、句読点除去、**連続空白を畳まない**) をアンカー検証の
  実装として固定する。PR #472 で照合スクリプトが連続空白を畳んで誤検知を出した実例がある
- rustdoc 相対リンクの `../` 段数を module の実深さと照合する検査を docs-lint へ追加する

#### 完了基準

- `rerun 123456` が run ID として受理されないことをテストが固定している
- 段数のずれた rustdoc リンクが検査で落ちる

---

### 順位 502: silent drop / `gh --repo` 欠落 / `spawnSync` timeout 未指定を lint で塞ぐ

> **動機**: いずれも実際に踏んだ欠陥で、**検査の場が無かった**ために人手のレビュー頼みだった (G1)。
>
> | 欠陥 | 実観測 |
> |---|---|
> | `read_dir(..).flatten()` が I/O エラーを黙って捨てる (fail-open) | PR [#454](https://github.com/aloekun/claude-code-hook-test/pull/454) — 統合前の `priority_inversion` / `preamble` が該当 |
> | `gh` 呼び出しの `--repo` 欠落 (非 colocated jj でリポジトリ解決に失敗) | PR [#470](https://github.com/aloekun/claude-code-hook-test/pull/470) — 順位 467 F-2 に続く 2 度目 |
> | `spawnSync` の timeout 未指定 (ネットワーク停止で無期限待機) | PR #470 |
>
> **由来**: `[defect:G1]`。証拠 = PR #454 / #470。

#### 作業内容

- 上記 3 パターンの custom-lint rule を `.claude/custom-lint-rules.toml` へ追加する
  (`spawnSync` は gh / network 呼び出しに限定し、ローカル実行の spawn を巻き込まない)
- `extensions` に `mjs` を足す (現状 js/jsx/ts/tsx のみで `scripts/*.mjs` が対象外)
- `cli-stale-branch-scan` が持つ `--repo` 明示パターンを他の gh wrapper へ展開する

#### 完了基準

- 3 rule とも、違反コードを入れると lint が落ちることを fixture で固定している
- 既存の `scripts/*.mjs` が新 rule で false positive を出さない

---

### 順位 503: doc と実装の同期を検査する (exit code 一覧 / 依存者リスト)

> **動機**: module doc に書いた事実が実装から乖離しても誰も気づかない箇所が 2 つある。
> PR [#456](https://github.com/aloekun/claude-code-hook-test/pull/456) /
> PR [#464](https://github.com/aloekun/claude-code-hook-test/pull/464) の post-merge feedback。
>
> - `main.rs` の `EXIT_*` 定数定義と module doc の「終了コード」一覧
> - `Cargo.toml` の依存と、lib 側 module doc が書いている「依存者リスト」
>
> **由来**: `[improvement]`。**実際に壊れた観測はまだ無い** — 予防のための検査であり、
> ADR-079 の線引きに従って defect を名乗らない。

#### 作業内容

- `EXIT_*` 定数と module doc の終了コード一覧が一致することを検査する (cargo test か docs-lint)
- `Cargo.toml` の依存追加時に、対象 lib の module doc 依存者リストが追随しているかを検査する

#### 完了基準

- どちらの検査も、片側だけを書き換えると落ちる
- 現行リポジトリで false positive を出さない

---

### 順位 504: 台帳検査の入力空間を埋める

> **動機**: F3 (順位索引の自己汚染) の実装中に、パーサが入力空間の一部で壊れることを
> 実測で見つけた。PR [#457](https://github.com/aloekun/claude-code-hook-test/pull/457) /
> PR [#458](https://github.com/aloekun/claude-code-hook-test/pull/458) /
> PR [#460](https://github.com/aloekun/claude-code-hook-test/pull/460) の post-merge feedback。
>
> **由来**: `[defect:G2]`。証拠 = PR #457。テストの場はあったが、`#[cfg(test)]` の宣言形の
> 全パターン (親ファイル型 / インライン型 / multi-arg のコンマ終端) を覆っていなかった。

#### 作業内容

- multi-arg `#[cfg(test)]` 関数のコンマ終端パターンの回帰テスト
- 実リポジトリに現存する `cfg(test)` 宣言パターンを全件走査して固定するテスト
- `declared_text` と `repository_text` の非対称性 (宣言先はテストコードを含む / 索引は含まない) の固定
- `SummaryRow.title` のような台帳の自由記述が出力前に `screen_for_public_output` を通ることの固定
- 統合テストの変異テスト標準化 (`split_ledger.rs` で確立した手順を convention として書く)
- **[ADR-049](adr/adr-049-incident-eval-regression-suite.md) に fix-induced-regression の case を追加**
  (fix が隣接エッジに穴を作った incident。case を書かないと再現テストの出所が失われる)

#### 完了基準

- 上記パターンを潰すと落ちるテストが揃っている
- ADR-049 の case 表に今回の incident が載っている

---

### 順位 505: telemetry の id 契約と TOML 構造の回帰を足す

> **動機**: `record_firing` の `id` に何を渡してよいかの線引きが、コードにも doc にも無い。
> また `.claude/hooks-config.toml` / `push-runner-config.toml` の `[section]` が編集で分断
> されても検査が無い — PR [#463](https://github.com/aloekun/claude-code-hook-test/pull/463)
> で実際に `[testability_gate]` のコメント間へ別セクションを挿入して壊した。
>
> **由来**: `[defect:G2]`。証拠 = PR #463 / PR [#456](https://github.com/aloekun/claude-code-hook-test/pull/456)。
> config パーサのテストは書ける場にあったが、セクションの連続性という軸が覆われていなかった。

#### 作業内容

- `record_firing` の呼び出し箇所すべての `id` が既知の安全パターン (固定リテラル) であることの検査
- 長い自由記述見出しや特殊文字を含む入力でも `record_firing` の出力が固定されることのテスト
- TOML パース後、各 `[section]` が定義どおりの範囲で連続していることの検査
- **[ADR-055](adr/adr-055-firing-telemetry-collection.md) に識別子の判定基準を追記**
  (`id` は呼び出し側の固定リテラルのみ / bookmark 名等の可変値は除外)。ADR-055 は
  「metadata only」と定めているが**何が識別子として安全か**の線引きが無く、コードからは復元できない

#### 完了基準

- 可変値を `id` に渡すコードが検査で落ちる
- セクションを分断する編集が検査で落ちる
- ADR-055 に判定基準の節がある

---

### 順位 506: 夜間ループと Node script 層の境界をテストで固定する

> **動機**: 無人経路と手元スクリプトで、実測して直した挙動がテストで固定されていない。
> PR [#466](https://github.com/aloekun/claude-code-hook-test/pull/466) /
> PR [#469](https://github.com/aloekun/claude-code-hook-test/pull/469) /
> PR [#470](https://github.com/aloekun/claude-code-hook-test/pull/470) /
> PR [#471](https://github.com/aloekun/claude-code-hook-test/pull/471) の post-merge feedback。
>
> **由来**: `[defect:G2]`。証拠 = PR #466 (fail-fast の後退を pre-push review が検出) /
> PR #470 (一時ディレクトリのリークを実測) / PR #471 (`change_id` でなく説明文で比較していた)。
> いずれもテストを書ける場にありながら、境界が覆われていなかった。

#### 作業内容

夜間ループ側 (B3):

- `workflow_dispatch` の `dry_run=true` 経路の E2E
- `delete_all` の fail-fast (最初の失敗で打ち切る) という loop-level boundary の固定
- `redact()` のスコープ差 (旧 shell の正規表現マッチ vs 新 exe の完全一致) の固定
- git I/O / network 失敗 / 権限まわりの corner case (SHA 不変、token ordering 等)
- pre-flight gate が背圧で deny し、`Select task from the ledger` step が `if:` で skip される経路

Node script 側 (B4):

- `process.exit()` が `finally` を飛ばさないこと (一時ディレクトリ後始末) — **実測済み、固定するだけ**
- `spawnSync` の timeout 動作 (`ETIMEDOUT`) — **実測済み、固定するだけ**
- コミット同一性検証が `change_id` を使っていること / `compareCommitSets` の双方向 — **実測済み、固定するだけ**
- `jj rebase -r` で親コミットが落ちる合成ブランチを CI で自動生成し、`pnpm rebase-nightly` が
  検出することを回す (**未実施**。手元の合成ブランチ検証を CI へ移す)

#### 完了基準

- 上記の各挙動を潰すと落ちるテストが揃っている
- 合成ブランチによる `-r` 事故の検出が CI で自動的に回っている
