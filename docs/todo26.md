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

- **一時ディレクトリの後始末が保証されること** — `process.exit()` は `finally` を実行せずに
  プロセスを終えるため、`try`/`finally` の中で呼ぶとリークする (実測で 4 個の残存を確認)。
  終了コードは `process.exitCode = main()` の経路で返す。**この形を固定する** (実測済み)
- `spawnSync` の timeout 動作 (`ETIMEDOUT`) — **実測済み、固定するだけ**
- コミット同一性検証が `change_id` を使っていること / `compareCommitSets` の双方向 — **実測済み、固定するだけ**
- `jj rebase -r` で親コミットが落ちる合成ブランチを CI で自動生成し、`pnpm rebase-nightly` が
  検出することを回す (**未実施**。手元の合成ブランチ検証を CI へ移す)

#### 完了基準

- 上記の各挙動を潰すと落ちるテストが揃っている
- 合成ブランチによる `-r` 事故の検出が CI で自動的に回っている

---

### 順位 507: 夜間 agent の `Edit(work/**)` がドット始まりディレクトリを覆わない

> **動機**: 2026-09-02 の夜間 run 33665621808 が順位 455 を選び、agent が **34 ターン・5.8 分・
> $1.75 を消費して 0 変更**で終わった。結果 JSON に `permission_denials_count: 2` が記録されている。
> 順位 455 の成果物は `.claude/custom-lint-rules.toml` + `tests/fixtures/incidents/{bad,good}/` で、
> agent の tool scope は `--allowedTools "Read(work/**),Edit(work/**),Glob,Grep"` である。
> **glob の `*` がドット始まりディレクトリにマッチしないため `.claude/` 配下を編集できない**、
> というのが denial の説明として最も整合する。
>
> **実害の規模**: auto lane に載っているのに構造的に完了できない行がもう 1 件ある (順位 281、
> 同じ `.claude/custom-lint-rules.toml` 構成)。両方とも 2026-09-03 に human lane へ退避したが、
> **custom lint rule の追加タスクは今後すべて夜間ループに載せられない**状態が残る。
>
> **由来**: `[defect:G1]`。証拠 = run 33665621808 (`permission_denials_count: 2`)。
> tool scope の設定はテストを書く場が無く、実走でしか確認できない。

#### 着手時に確定させること

- **原因の確定**: glob の不一致は推定である。`workflow_dispatch` の `dry_run=true` で
  `.claude/` 配下を対象とする順位を選ばせ、denial が再現するかを実測してから直す
  ([ADR-067](adr/adr-067-phase-b-unattended-fix-push.md): LLM を含む経路は実走でしか検証できない)
- **`.github/` との関係**: ドット始まりを許すと `.github/` も allow 側に入る。**Guard 禁止パス
  ([ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6) との二重防御の関係を整理する** — deny 側で
  `.github/**` を明示するか、allow を `.claude/**` に限定して列挙するかの選択がある
- 決定 12 の「保護の主体は deny 側」という設計を崩さないこと

#### 完了基準

- `.claude/custom-lint-rules.toml` を成果物とするタスクが夜間ループで完走する (実走で確認)
- `master-ref/` への書き込みが従来どおり deny されることを実測で再確認している
- 順位 281 / 455 を auto lane へ戻せる状態になっている

---

### 順位 508: 台帳追加候補の除外クラスを決定論で機械適用する

> **動機**: 2026-09-03 の weekly-review で、台帳未掲載 238 件から追加候補を選ぶ作業を人手で行った。
> [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) の除外クラス 1〜5 のうち複数は決定論で
> 判定できる (同 決定 6 が「対象パスの実在検査は決定論」と分類済み) のに、`ledger-candidates`
> step は**差集合を出すだけ**で絞り込みをしていない。結果、weekly-review の報告時点では
> 「238 件」という数しか見えず、**候補の見落としが構造的に起きる**。
>
> **由来**: `[improvement]`。実際に見落とした観測はまだ無く、運用改善のための機械化である。

#### 設計方針 (2026-09-03 ユーザー決定)

**LLM に適格判定をさせない。** [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 18 が
「skill は昇格を提案しない」と定めた由来は、LLM に適格判定を強制した旧方式が 2 週連続で失敗した
ことである (164 件中約 50 件 / 251 件中 13 件しか判定せず、いずれも「候補 0 件」と報告)。
したがって**禁じられたのは LLM による判定**であって、決定論による絞り込みではない。

決定 18 は「**LLM が適格判定しない**」と読み替え、skill の制約 (「件数と report パスを提示する
だけ」) を改訂する。

#### 機械適用する除外クラス

| クラス | 判定方法 |
|---|---|
| 1 グローバル `~/.claude` の編集 | 本文の語彙 |
| 2 実行環境依存 (hook 発火 / 実走 / `pnpm push` / e2e) | 本文の語彙 |
| 3 Guard 禁止パスの**書き換え** | 対象ファイル欄 × deny リスト (順位 486 が実装する検査と同一) |
| 4 ADR の起票・改訂 | 対象ファイルが `docs/adr/` |
| 5 判断留保 (再選定 / 検討 / 未定 / 複数案 / 着手時判断 / 要設計) | 注意欄・本文のキーワード走査 (順位 447 が実装する検査と同一) |
| **新規: `.claude/` 配下の書き換え** | 対象ファイル欄。順位 507 が解決するまでの暫定 |

**決定 3 の 3 種 (文書タスク / 並行性・ロックのテスト / 完了基準が二択) は機械適用しない** —
ADR-074 決定 6 が非決定論と分類済み。残った候補に対して人間が判断する。

#### 着手時判断

- 順位 486 / 447 が実装する検査と**同じ判定ロジックを 2 度書かない**こと。どちらを先に実装するか、
  共通化するかは着手時に決める
- 出力は `ledger-candidates.md` に統合するか、別 report にするかを決める

#### 完了基準

- weekly-review の報告に、除外クラス適用後の候補一覧 (順位 / Tier / 内容 / 除外されなかった理由) が出る
- 除外されたものは件数とクラス別内訳が出る (「0 件」と「未実施」を読み手が区別できる)
- ADR-072 決定 18 と weekly-review skill の制約が改訂されている

---

### 順位 509: `cli-merge-pipeline` の gh 呼び出しが非 colocated workspace で解決に失敗する

> **動機**: 2026-09-03 の weekly-review finding WR-2026-09-03-J01 (severity high)。
> [`src/cli-merge-pipeline/src/github.rs`](../src/cli-merge-pipeline/src/github.rs) の
> `detect_owner_repo()` と `detect_pr_number()` が `gh` を `--repo` なしで呼んでおり、
> 非 colocated jj workspace ([ADR-045](adr/adr-045-jj-workspace-parallel-sessions.md) の並列
> セッション運用) では `.git` が無いため gh がリポジトリを解決できない。
>
> **順位 467 F-2 / PR [#470](https://github.com/aloekun/claude-code-hook-test/pull/470) と同型**。
> 同じ穴を 3 度踏んでいる。
>
> **由来**: `[defect:G1]`。証拠 = PR #470 (同型の実観測) / weekly-review finding J01。
> gh のリポジトリ解決は実行環境に依存し、テストを書く場が無かった。

#### 着手時に確定させること

**2 つの関数は性質が違う。同じ修正を当てられない。**

| 関数 | 状況 |
|---|---|
| `detect_pr_number()` (137 行) | `owner_repo` が既知なら `--repo` を渡せる。**素直に直せる** |
| `detect_owner_repo()` (81 行) | **リポジトリを特定する関数自身**なので `--repo` は循環する。`gh` に頼らず `jj git remote list` 等から導出するか、環境変数 `GH_REPO` を受けるかの選択がある |

順位 502 が追加する「`gh --repo` 欠落検出」の lint は、**この 2 箇所を最初に検出する現存違反**に
あたる。502 を先に入れると赤くなるため、lint 側に例外を置くか本タスクを先に片付けるかを決める。

#### 完了基準

- 非 colocated workspace で `cli-merge-pipeline` がリポジトリと PR 番号を解決できる (実測で確認)
- `detect_owner_repo()` の解決経路が gh のリポジトリ自動解決に依存しない
- 順位 502 の lint と矛盾しない (例外を置くならその根拠が書かれている)

---

### 順位 510: 夜間ループの稼働状況を週次レビューで見張る

> **動機**: 直近 8 晩の夜間 run のうち **5 晩が red**、うち**直近 4 晩は連続**している
> (2026-08-30 / 08-31 / 09-01 / 09-02)。ところが 2026-09-03 の weekly-review が出した
> findings 8 件のうち、**夜間ループに言及したものは 0 件**だった。
>
> **なぜ気づけないか**: `weekly-review.yaml` は全 provider に `network_access: false` を課しており、
> facet はソースツリーしか読めない。**run の結果はネットワークの向こう側**にあるため、
> 現在の構成では原理的に観測できない。
>
> **実害**: 夜間ループは**開発作業で生まれたタスクの消化を助ける補助**であって、止まっても主線の
> 開発は進む。だからこそ**無音のまま何晩も過ぎる**。1 晩の red は agent 1 回分 (実測で
> 5.8 分・$1.75、run 33665621808) を捨てており、その間タスクの消化も進まない。
>
> 実際、2026-09-03 のセッションで見つかった 2 件はどちらも**人間がログを手で読んで初めて**
> 判明した — 順位 455 の権限拒否 (run 33665621808、`permission_denials_count: 2`) と、
> 順位 324 の空振り (run 90894308468)。weekly-review の出力には一度も現れていない。
>
> **由来**: `[defect:G1]`。証拠 = run 33665621808 / run 90894308468。観測の場そのものが無かった。

#### 置き場所

**L3 (skill) の決定論 scan** に置く。`gh` が要るため L2 (takt workflow) には置けない
([ADR-031](adr/adr-031-weekly-review-pipeline.md) § L2 に置けない決定論 scan は L3 が直接呼ぶ)。
`pnpm stale-branch-scan` / `pnpm ledger-residue-scan` と同じ配置になる。

#### 出す材料 (案)

- 直近 7 日の run の `conclusion` 集計 (success / failure / 未実行)
- red の run について `[NIGHTLY] cleanup=... publish=... handoff=...` のサマリ行 (どの段で止まったか)
- handoff marker の現存一覧と、それが指す順位
- 連続 red の日数 (「今週たまたま 1 晩落ちた」と「4 晩連続で助けが止まっている」を区別する)

#### 着手時判断

- **どこまでログを読むか**。run の `conclusion` だけなら `gh run list` で軽いが、停止段まで出すには
  各 run のログ取得が要る (1 run 数 MB)。直近 7 日ぶんを毎週取るコストと得られる情報を比較して決める
- agent の消費 (`num_turns` / `total_cost_usd`) を出すかどうか。出せば「回して捨てた量」が見えるが、
  ログ本文の取得が前提になる

#### 完了基準

- red が続いている週に、weekly-review の報告へ必ずその事実が現れる
- 停止段が分かる粒度で出る (「red が 4 晩」だけでなく「guard で 3 晩、verify で 1 晩」)
- 取得に失敗した週は「未確認」と明示される (「0 件」と書かない)

---

### 順位 511: `todo-summary2.md` を 3 分割し、明示列挙している呼び出し元を追随させる

> **動機**: `docs/todo-summary2.md` が **79KB** に達した (50KB が Claude Code の読み取り安定閾値)。
> 2026-07-20 に `todo-summary.md` から分割した後半で、順位が増えるたびに伸び続ける。
>
> **機構側は「一部だけ」3 分割へ対応済み**。Phase F の F1 で name prefix を `SUMMARY_FILE_PREFIX`
> 1 箇所に集約し、`docs_files.rs` の列挙は `todo-summary*.md` を glob するため、**cli-docs-lint の
> 各 check は新しい part を追加するだけで拾う** (テストは `todo-summary3.md` を fixture に使う)。
>
> **一方、台帳削除の経路は 2 ファイル決め打ちのままである。**
> [`src/cli-ledger-cleanup/src/apply.rs`](../src/cli-ledger-cleanup/src/apply.rs) の `plan_summary_removal`
> は `["todo-summary.md", "todo-summary2.md"]` を配列でハードコードしており (88 行)、**3 分割すると
> 第 3 part に載った順位の後始末が「順位 table にありません」で失敗する** (CodeRabbit #473)。
> 夜間ループのマージ経路が壊れるため、分割と同じ PR で直す必要がある。
>
> **由来**: `[improvement]`。閾値超過は観測しているが、読み取りが実際に壊れた観測はまだない。

#### 追随が要る「明示列挙している呼び出し元」

glob ではなく 2 ファイルを並べている 3 箇所は手で足す必要がある。

- `package.json` の `ledger-candidates` スクリプト (`--summary-file` ×2)
- `.github/workflows/nightly-todo.yml` の `Select task from the ledger` step (`--summary-file` ×2)
- **`src/cli-ledger-cleanup/src/apply.rs` の `plan_summary_removal`** (配列のハードコード。ここが漏れると台帳の後始末が失敗する)

加えて `docs/todo.md` の preamble routing 表を更新する。

#### 着手時判断

- **どこで切るか**。順位の境界をどこに置くかは、ファイルサイズと「よく参照する範囲」の兼ね合いで決める
- workflow を触るため [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パスに該当し、**auto lane には載せられない**

#### 完了基準

- 3 つの part すべてが `pnpm lint:docs` / `cargo test` の検査対象に入っている (`todo-summary3.md` を足しても検査が素通りしない)
- 夜間ループの選択が 3 part すべてを見ている (`--summary-file` の追随漏れがない)
- **第 3 part に載った順位を `cli-ledger-cleanup --apply` が後始末できる** (`apply.rs` のハードコードが解消されている)
- 2 ファイル決め打ちが再発しないよう、列挙は `docs_files.rs` の共有層を使うか、使えない理由が書かれている

---

### 順位 512: 50KB 超の詳細エントリファイル (`todo14.md` / `todo22.md`) を分割する

> **動機**: `docs/todo14.md` (61KB) と `docs/todo22.md` (59KB) が閾値を超えている。どちらも
> 「編集・完了削除専用」で新規追加はされないが、既存エントリが残る限り縮まない。
>
> **由来**: `[improvement]`。

#### 作業の性質

**詳細エントリの移動は順位 table の「ファイル」列とセットである。** 移動した各エントリについて
`docs/todo-summary*.md` の該当行が指すファイル名を更新しないと、`entry_pairing` 検査 (順位 441 /
Phase D の D3) が 1:1 対応の破れとして落とす。件数に比例して差分が増える。

#### 着手時判断

- **分割するか、完了エントリの削除で足りるかを先に測る**。両ファイルの全エントリについて、
  対応する順位が順位 table に現存するかを確認し、孤児があればまず削除する
- 分割する場合の新ファイル名 (連番の次) と、`docs/todo.md` preamble への追記

#### 完了基準

- 両ファイルが 50KB 未満
- `pnpm lint:docs` の entry-pairing が緑 (移動したエントリの参照がすべて追随している)

---

### 順位 513: 50KB 超の恒久ドキュメント (ADR-072 / 台帳 / workflow 2 件) の扱いを決める

> **動機**: 週次の file-length watchlist は `docs/todo*.md` と `src/**/*.rs` しか見ていないため、
> **より大きい恒久ドキュメントを構造的に見逃している**。2026-09-03 の実測:
>
> | サイズ | ファイル | 性質 |
> |---|---|---|
> | 126KB | `docs/adr/adr-072-nightly-todo-loop.md` | 恒久 ADR。決定と検証記録が追記され続ける |
> | 60KB | `docs/claude-code-web-tasks.md` | 台帳。恒久 |
> | 67KB | `.github/workflows/nightly-todo.yml` | workflow。コメントが厚いこと自体が価値 |
> | 64KB | `.github/workflows/pr-monitor.yml` | 同上 |
>
> **由来**: `[improvement]`。
>
> **watchlist の走査範囲そのものが問題**である。閾値を超えたファイルに気づけない構造が
> 週次レビューに残っている (2026-09-03 のセッションで、報告されていた 3 件より大きい
> 4 件が見えていなかった)。

#### 着手時判断

**機械的な分割では済まない。** 以下をタスクごとに決める必要がある。

- **ADR-072**: 決定本文と検証記録を分けるか。ADR は 1 決定 1 ファイルが原則で、分割は参照の
  付け替えを伴う。「検証記録だけを appendix ファイルへ出す」案が最有力だが設計判断
- **台帳**: 恒久かつ夜間ループの選択元。分割は選択ロジックに影響する
- **workflow 2 件**: コメントを削ると設計意図が失われる。「コメントを ADR へ移して本体を薄くする」
  のは可能だが、**その場で読める価値**とのトレードオフ
- **watchlist の走査範囲拡張**: `docs/**/*.md` と `.github/workflows/*.yml` を対象に加えるか。
  加えると恒久ファイルが毎週報告され続けるため、「閾値超過が N 週続いたら報告」等の設計が要る

#### 完了基準

- 4 ファイルそれぞれについて「分割する / しない (理由つき)」が決まっている
- watchlist の走査範囲が、決めた方針と整合している
