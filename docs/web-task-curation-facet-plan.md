# 台帳キュレーション専用 facet 分離 作業計画 (2026-08-12)

> **位置づけ**: 週次レビュー (weekly-review, ADR-031) に「`docs/claude-code-web-tasks.md` (以下「台帳」) の棚卸し ＋ `docs/todo-summary.md` / `docs/todo-summary2.md` の新規タスクのうち Claude Code Web が扱えるものの昇格検討」を、専用 facet として分離・強化する作業計画。方針・PR 構成・スコープはユーザー承認済み (2026-08-12)。
>
> **最終目標**: 下記 PR 1 → PR 2 → PR 3 の完了後、**本ファイル自身を削除する** (PR 3 の最終コミットで削除する。完了した作業計画書は仕組みに反映後に削除する運用ルールと同じ扱い。経緯は git log に残る)。恒久ドキュメントではない。
>
> **PR 構成 (承認済み)**: **PR 3 本、うち新規 3 本。ADR-069 チェーン (PR 1 → PR 2 → PR 3)** の順で実施する。各 PR は `claude/` prefix ブランチで作成し、PR 作成前にタイトル・ボディをユーザーへ提示して明示承認を得ること (ADR-028/ADR-052、PR 作成はスコープ承認と別)。
>
> **実作業者への前提**: 本ファイルの内容だけで着手できるよう、調査済み事実・再利用資産の API・対象ファイル・DoD をすべて記載した。記載済みの事実は再調査不要。ただしコード移設時の細部 (import 解決等) は実際のファイルを Read して確認すること。

---

## 1. 背景 (調査済み事実 — 再調査不要)

### 1.1 要望と既存機構の重なり

ユーザー要望「台帳の棚卸し ＋ Web 対応可能タスクの昇格検討パイプライン」は、**既存の weekly-review facet `review-todo-whole` の Criterion 3「自律実行台帳の鮮度」に実質すでに存在する** (`.takt/facets/instructions/review-todo-whole.md` の Criterion 3)。ただし現状は:

- **read-only (advisory) のみ**。findings を上げるだけで台帳は編集しない (`edit: false`)。台帳編集は `/weekly-review` skill の採否ステップで人間が行う (ADR-022)。
- **`review-todo-whole` の Criterion 0〜3 の一部**として埋もれており、専用の可視性・機械層の裏打ちがない。

台帳自身の § ライフサイクル「定期更新 (週次)」も「接続は `review-todo-whole.md` (観点⑤) が担う」と明記しており、この接続は設計上意図されたもの。本計画はこれを**専用 facet へ分離し、機械層で裏打ちする**。

### 1.2 確定した設計 (ユーザー承認済み)

| 決定項目 | 結論 |
|---|---|
| 分離方針 | `review-todo-whole` の Criterion 3 を**専用 facet へ移設** (複製ではない) |
| 編集主体 | **read-only advisory を維持**。台帳の自動編集はしない (findings 報告のみ、採否・編集は人間) |
| 判定方式 | **ハイブリッド** — 機械判定できる集合差は決定論 exe、Web 適格性の判断は LLM facet |

### 1.3 機械層 / LLM 層の責務分割 (重要な訂正含む)

**訂正**: `lib-docs-policy::is_docs_only_summary` は「実際の jj diff summary」を分類する関数であり、**未実装タスクの Web 適格性判定には直接使えない** (diff がまだ存在しないため)。当初案で再利用先に挙げたが、これは誤り。機械層の責務は下記の集合差に閉じる。

| 層 | 責務 | 実装 |
|----|------|------|
| **機械 (決定論 exe)** | 台帳 active table の順位集合 `L` と todo-summary/summary2 の順位 table の集合 `S` を parse し、集合差を出す:<br>**`L∖S` = land 済み削除候補 (棚卸し)**<br>**`S∖L` = 昇格 gap (追加検討の入口)** | 新 exe `cli-web-task-curation` + 新 lib `lib-ledger` |
| **LLM (新 facet, read-only)** | `S∖L` の各順位を詳細エントリから読み、§採用タスク/(2) の基準で Web 適格性を判断。`L∖S` は land 実在 (成果物 grep) を確認して削除を推奨 | 新 facet instruction `review-ledger-curation-whole.md` |

台帳パーサが `無人可` 列で active table を識別する既存ロジックが、そのまま「棚卸し履歴・無人可としなかった表を除外」という Criterion 3 のスコープ規則に一致する (§1.4 参照) ため再利用価値が高い。

### 1.4 再利用する既存資産 (file path + API)

- **`src/cli-nightly-task-select/src/ledger.rs`** — 台帳 markdown table を I/O なしで parse する純粋層。公開 API: `Task` 構造体、`select(markdown: &str, excluded_ranks: &BTreeSet<u32>) -> Result<Option<Task>, String>`。
  - `無人可` 列を持つ表だけを走査対象にする (`header_columns` が `"無人可"` 列の有無で判定)。**棚卸し履歴表・「無人可としなかった N 件の理由」表は `無人可` 列を持たないため自動的に除外される** — これは Criterion 3 のスコープ規則 (これらの表は順位列を持つが対象外) と一致する。
  - 列ずれ・順位重複・未知マーク・エスケープパイプ `\|`・prompt framing 脱出文字・不可視文字を**すべてエラー**で止める堅牢なパーサ。40 件超の unit test を同梱。
  - サブモジュール `src/cli-nightly-task-select/src/ledger/screening.rs` (`mod screening;`) を持つ。公開 API: `screen_for_public_output`, `screen_for_title`。`LEDGER_DATA_FRAME_MARKER` 定数は `.github/workflows/nightly-todo.yml` の `===BEGIN/END_LEDGER_DATA===` 区切りと**対**であり、片方だけ変えると framing が破れる (ADR-072 決定 13)。移設後も doc comment でこの対応を維持すること。
- **`src/cli-nightly-task-select/src/main.rs`** — 現在の consumer。`mod ledger;` (35 行目)、`use ledger::{screen_for_public_output, screen_for_title, Task};` (40 行目)、`ledger::select(...)` (119 行目)。lib 抽出後はこれらを `lib_ledger::` 参照へ書き換える。
- **todo-summary の順位 table 形式**: `| 順位 | Tier | title | todoX.md | ... |`。cell の形 `| <順位> |` で照合する (bare number 照合は禁止 — 行数・バイト数など無関係な数値に誤マッチする。Criterion 3 のスコープ規則)。

### 1.5 スコープ外 (今回入れない)

- **無人可マークの再検証 (Criterion 3 の 3 つ目のチェック)**: remote bookmark / in-flight PR 参照が必要だが、weekly-review workflow は `workflow_config.provider_options.*.network_access: false` (`.takt/workflows/weekly-review.yaml` 冒頭)。自動経路では既存 facet でも "unverified" 止まりで、これは変えられない。ご要望の 2 点 (棚卸し = `L∖S`、昇格 gap = `S∖L`) は**local read のみで完結**するため影響なし。この 3 つ目のチェックは `review-todo-whole` に残すか別経路を検討するかを **PR 3 内で明示判断**する (§4.3 参照)。
- **台帳の自動編集**: read-only 選択のため実装しない。編集は `/weekly-review` 採用ステップで人間。

---

## 2. PR 1 (新規・純リファクタ): `lib-ledger` 抽出

### 目的
`cli-nightly-task-select` 内部の台帳パーサを共有 lib へ抽出し、PR 2 の新 exe から再利用可能にする (ADR-024 共通 lib、ADR-026 workspace)。cli-* crate から別 cli-* crate を直接呼ばないため、共有ロジックは lib に置く。

### 作業内容
1. 新 crate `src/lib-ledger/` を作成 (`Cargo.toml` + `src/lib.rs`)。
2. `src/cli-nightly-task-select/src/ledger.rs` と `src/cli-nightly-task-select/src/ledger/screening.rs` を `lib-ledger` へ移設する。
   - `ledger.rs` の中身を `lib-ledger/src/lib.rs` に移し (または `lib.rs` から `mod` で束ねる構成でもよい)、`screening` サブモジュールもそのまま持ち込む。
   - 同梱の `#[cfg(test)] mod tests` (40 件超) も移設。移設先で全 pass すること (挙動不変の担保)。
3. `Task`, `select`, `screen_for_public_output`, `screen_for_title` を crate の公開 API として `pub` で出す。
4. `src/cli-nightly-task-select/Cargo.toml` に `lib-ledger = { path = "../lib-ledger" }` を追加。
5. `src/cli-nightly-task-select/src/main.rs` の `mod ledger;` を削除し、`use lib_ledger::{screen_for_public_output, screen_for_title, Task};` と `lib_ledger::select(...)` へ書き換える。移設で消えた `src/ledger.rs` / `src/ledger/` を削除。
6. ルート `Cargo.toml` の `[workspace] members` に `"src/lib-ledger"` を追加 (現在のリストはアルファベット順ではなく機能順だが、`lib-*` は末尾付近に並んでいるので `lib-jj-helpers` 近辺に挿入)。

### DoD
- `cargo test --workspace` green (移設した ledger テストが `lib-ledger` で全 pass)。
- `cli-nightly-task-select` の挙動不変 (ビルド通過 + 既存の main 経路が `lib_ledger::select` を呼ぶ)。
- `cargo clippy --workspace --all-targets -- -D warnings` green。
- **これは cargo test で検証完結する純リファクタ**。

---

## 3. PR 2 (新規): 決定論 curation exe `cli-web-task-curation`

> **exe 名は ADR-012 (src/ 命名規約: `cli-<動詞-名詞>`) に準拠して最終確認すること。** `cli-web-task-curation` を第一候補とする。

### 目的
台帳と todo-summary の順位集合差 (`L∖S` / `S∖L`) を決定論的に算出し、markdown watchlist を出力する。LLM 判断ゼロ・回帰テスト可能・network 不要。`file-length-watchlist` step と同じ「exe 出力を markdown へ転記」型。

### 作業内容
1. `lib-ledger` に **`list_active_ranks(markdown: &str) -> Result<Vec<LedgerRow>, String>`** を追加する (既存 `select` の派生 — 全 active 行を列挙。`select` が使う `Scan` / table 検出ロジックを共有)。`LedgerRow` は最低限 `rank: u32` を持ち、必要に応じ `summary` 等も。エラー条件は `select` と同じ (曖昧さは停止側へ)。
   - **設計上の要注意点 (PR 2 で確定する)**: docs-only の §採用タスク表が `無人可` 列を持たない場合、`無人可`-キーの表識別だと取りこぼす。現状 §採用タスク (docs-only) は 0 件だが台帳は再追加を許している。表識別ルールをここで確定すること。候補: 「`無人可` 列を持つ表」に加え「§採用タスク 見出し配下の順位表」も active とみなす、等。**取りこぼしは棚卸しの穴になるため、識別ルールと根拠を exe の doc comment に明記し、fixture で固定する。**
2. 新 exe crate `src/cli-web-task-curation/` を作成。
   - **todo-summary 順位 table parser** を新規実装 (`| 順位 | Tier | ... |` 形式、cell の形 `| <順位> |` で照合、bare number 照合は禁止)。`docs/todo-summary.md` と `docs/todo-summary2.md` の両方を読む (順位 220 以降は summary2)。
   - 台帳を `lib_ledger::list_active_ranks` で parse し `L` を取得、todo-summary から `S` を取得。
   - `L∖S` (land 済み削除候補) と `S∖L` (昇格 gap) を算出。
   - markdown watchlist を stdout へ出力 (フォーマットは `.takt/facets/instructions/file-length-watchlist.md` の Phase 2 を参考に、両集合を常に section 化。0 件でも「clean state」を明示)。
3. ルート `Cargo.toml` の `members` に `"src/cli-web-task-curation"` を追加。
4. (任意) `package.json` に `pnpm` エイリアスを追加する場合は既存 `telemetry-report` 等の記法に合わせる。必須ではない (facet から直接 exe を叩ける)。

### DoD
- fixture テストで境界を固定:
  - 列ずれ / エスケープパイプ `\|` / 棚卸し履歴表・「無人可としなかった」表の除外。
  - `L∖S` と `S∖L` が正しく算出される (台帳に有り summary に無い / summary に有り台帳に無い)。
  - docs-only §採用タスク表の識別ルール (§3 の要注意点)。
- `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` green。
- **cargo test で検証完結**。

### 注意: dead-exe 窓
本 exe の consumer は PR 3 で初めて追加される。PR 2 単独では未使用 exe が残る (simplicity facet の dead-code 検出対象になりうる)。**PR 2 のボディで ADR-069 のチェーン宣言を行い、consumer が PR 3 であることを明示する** (missing-consumer 検査との両立)。

---

## 4. PR 3 (新規・docs/config 主体): facet 配線 ＋ Criterion 3 移設 ＋ 本計画書削除

### 4.1 新 facet instruction の作成
`.takt/facets/instructions/review-ledger-curation-whole.md` を新規作成 (LLM 判断側)。内容は `review-todo-whole.md` の Criterion 3 を土台に:
- 入力: PR 2 exe が出力した watchlist (`L∖S` / `S∖L`)。
- `S∖L` の各順位について、詳細エントリ (`docs/todoN.md`) を Read し、§採用タスク (docs-only 3 基準) **または** §採用タスク (2) (cargo-test 検証 3 基準) のいずれかを満たすかを判断し、昇格候補として finding 化。両経路を見ること (docs-only 枠は 0 件だが再追加を許容)。
- `L∖S` の各順位について、land 実在 (成果物 grep / `jj log`) を確認してから削除を推奨。
- **read-only (`edit: false`)**。台帳・todo は編集しない。無人可マークの付与提案もしない (ADR-022、人間が付ける)。
- output contract 名 (例 `review-ledger-curation-whole`) を宣言。

### 4.2 workflow への配線 (`.takt/workflows/weekly-review.yaml`)
`reviewers` の `parallel` 配下に 2 つ追加:
1. **決定論 step** (PR 2 exe を実行): `file-length-watchlist` step (現行 yaml 113-129 行) と同じ構造 (`allowed_tools: [Bash, Read]`、persona は構造要件、Bash 出力を転記)。output_contract に watchlist md を宣言。
2. **LLM facet** `review-ledger-curation-whole`: `review-todo-whole` step (現行 137-155 行) と同じ構造 (`persona: architecture-reviewer`, `model: haiku`, `allowed_tools: [Read, Glob, Grep, Bash]`)。`instruction: review-ledger-curation-whole`。output_contract に report md を宣言。

`aggregate-weekly` step の統合対象に新 report を追加する必要があるため §4.4 も行う。

### 4.3 `review-todo-whole` からの Criterion 3 移設 (複製回避)
`.takt/facets/instructions/review-todo-whole.md` から **Criterion 3「自律実行台帳の鮮度」を除去**する。
- **理由**: 移設先と重複させると、観点③ `review-architecture-whole` の Criterion 0「harness/rule 重複」が毎週この重複自体を finding に上げる。**移設であって複製にしない**。
- Criterion 3 の 3 つ目のチェック (無人可マーク再検証、network 依存) の扱いを**ここで明示判断**する: (a) `review-todo-whole` に残す、(b) 新 facet に移すが "network 不可時は unverified" と明記する、のいずれか。§1.5 の通り自動経路では unverified 止まりなので、advisory として残すなら (b) で新 facet に集約し「network 不可なら unverified 報告」と instruction に書くのが一貫する。
- `review-todo-whole.md` の Judgment procedure / Output contract の Criterion 3 参照箇所も整理する。

### 4.4 aggregate-weekly への統合 (`.takt/facets/instructions/aggregate-weekly.md`)
- Input の「Report Directory」節に新 facet の report md と watchlist md を追加 (現行は 6 report を列挙)。watchlist は `file-length-watchlist` 同様「機械的観測で findings には含めず専用 section へ転載」か、`L∖S`/`S∖L` を findings として扱うかを決める (推奨: LLM facet の report を findings に、raw watchlist は参考 section に)。
- Category rubric に台帳キュレーション用カテゴリを追加 (既存 `ledger-staleness` / `todo-*` を流用可)。
- Phase 4 の Markdown report テンプレの「レビューファセット」列挙と「Facet 観察メモ」に新 facet を追加。
- ヘッダ冒頭が「3 つの whole-tree レビュー」のままで本文は 6 facet を列挙している既存 drift も、触るついでに実数へ更新してよい。

### 4.5 ドキュメント更新
- `docs/adr/adr-031-weekly-review-pipeline.md` に本分離 (Criterion 3 → 専用 facet + 機械層裏打ち) を追記。
- `docs/claude-code-web-tasks.md` の § ライフサイクル「定期更新 (週次)」の「接続は `review-todo-whole.md` (観点⑤) が担う」記述を、新 facet 参照へ更新。
- `CLAUDE.md` の ADR 一覧や dev-conventions に波及があれば整合させる (基本は ADR-031 追記で足りる)。

### 4.6 本計画書の削除 (最終目標)
- **PR 3 の最終コミットで `docs/web-task-curation-facet-plan.md` (本ファイル) を削除する。**
- 本ファイルは `todo*.md` ではないため 順位 152 の delete-time land 検証 hook は発火しない。削除理由は PR 3 のボディに記載する (経緯は git log に残る)。

### DoD
- `/weekly-review` をローカル起動し、新 facet が report を出力し `findings.json` に統合されることを**実測**する (workflow の実走は takt 経由でしか検証できない — 推論で済ませない)。
- `review-todo-whole` の Criterion 3 が重複していないこと (移設完了)。
- 本計画書が削除されていること。

---

## 5. 各 PR 共通の検証・規約

- **PR サイズ**: `pr_size_check` は warning 800 / block 1500 行 (insertions+deletions)。PR 1 の lib 移設は移動が主体なので行数が嵩む場合がある。block に当たったら `PR_SIZE_CHECK_OVERRIDE=1` を PR ボディに明記して bypass (移動主体で実質差分が小さいことを説明)。
- **cargo 検証**: PR 1・PR 2 は `cargo test --workspace` + `cargo clippy --workspace --all-targets -- -D warnings` を DoD として PR ボディに green を記載。
- **PR 作成ゲート**: 各 PR は `claude/` ブランチで作成し、**作成前にタイトル・ボディをユーザーへ提示して明示承認**を得る (ADR-028/ADR-052)。スコープ承認 ≠ 作成許可。
- **チェーン宣言**: PR 1 → PR 2 → PR 3 の依存を各 PR ボディで宣言 (ADR-069)。PR 2 は dead-exe の consumer が PR 3 である旨を明示。
- **実走検証**: workflow / facet の挙動は takt 実走でしか確認できない。PR 3 は `/weekly-review` 実走で検証する。
- **コメント規約**: Rust の非 doc コメントは write-time hook で block される。説明は doc comment (`///` / `//!`) で書く。

## 6. 削除条件

本ファイルは PR 1〜3 がすべて land (実装完了) した時点で不要になる。**PR 3 の最終コミットで本ファイルを削除する** (§4.6)。恒久価値を持つ設計判断は ADR-031 追記 (§4.5) へ移すため、本ファイルに残す永続情報はない。
