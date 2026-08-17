# Aggregate Weekly Review

5 つの whole-tree レビュー facet (simplicity / security / architecture / todo / jj-robustness) と決定論 scan 2 つ (file-length / workspace-hygiene) を統合し、週次レビューレポートと構造化 findings JSON を生成する。

ADR-031 § Findings スキーマ + § 採否フロー の input source として findings JSON を produce する設計。skill 側 (Phase C 予定) が JSON を読んで AskUserQuestion で採否を確認するため、本 facet は構造化データの単一 source。

**重要な原則:**

- 読み取り専用 (`edit: false`)。コードの修正は一切行わない (採否は Phase C skill とユーザー判断で行う)
- findings がない場合は「特筆事項なし」で正常終了する。無理に findings を捻出しない
- 重複する findings はマージし、`location` と `rationale` を統合する
- severity の自動配点を最終手段とせず、3 reports の articulation を尊重する
- **各 finding に Severity / Category を必須付与する** (ADR-031 § Findings スキーマ)

---

## Input

### Report Directory (takt が提供)

本 step (`pass_previous_response: false`) は前 step の response を受け取らない。代わりに Report Directory に保存された 7 reports を Read で読み取る:

- `simplicity-whole-review.md` — review-simplicity-whole facet の出力
- `security-whole-review.md` — review-security-whole facet の出力
- `architecture-whole-review.md` — review-architecture-whole facet の出力
- `review-todo-whole.md` — review-todo-whole facet の出力 (観点⑤ Todo 妥当性、順位154)。docs/todo*.md corpus の dead pattern / cross-file 重複 / preamble drift。**findings として Phase 1 統合に含める**
- `review-jj-robustness-whole.md` — review-jj-robustness-whole facet の出力 (観点⑧ jj-workspace robustness、順位247)。mtime staleness / CARGO_MANIFEST_DIR 実行時読み / --repo 無し gh / colocated .git 前提。**findings として Phase 1 統合に含める**
- `file-length-watchlist.md` — file-length-watchlist facet の出力 (PR-W0 拡張、順位154。deterministic な `.rs` 800 行 + `docs/todo*.md` 50KB scan)。本 watchlist は LLM 判断による findings ではなく機械的観測のため、Phase 1 統合では findings には含めず、Phase 2 の "file size watchlist" 専用 section として weekly report に転載する
- `ledger-candidates.md` — ledger-candidates step の出力 (2026-08-17 追加。`docs/todo-summary*.md` の全順位から自律実行台帳の現行タスク表に載っている順位を引いた差集合)。**findings に含めない** — 未掲載であること自体は欠陥ではなく、台帳へ載せるか / lane を `✅` `—` のどちらにするかは人間の割り当て判断だから (ADR-072 決定 18)。file size watchlist と同様に**専用 section へ件数と参照だけを転載**する (全件表は 250 行規模になるため本文へは展開しない)
- `workspace-hygiene-scan.md` — workspace-hygiene-scan facet の出力 (2026-08-14 追加。root 直下 allowlist 突合 + scratch pattern whole-tree + ignored 資産サイズの deterministic scan)。扱いは 2 分される: **root 直下の想定外ファイルと scratch pattern 合致は findings として Phase 1 統合に含める** (severity は report 記載の目安に従う。削除の実行判断をユーザーの採否フローに乗せるため)。**ignored 資産サイズは機械的観測**であり findings に含めず、file size watchlist と同様に専用 section へ転載する

### Context

実行日は本 step の wall clock を `YYYY-MM-DD` 形式で取得 (UTC でも JST でも一貫していればよい。findings id の prefix に使う)。

## Phase 1: findings source reports の統合

findings source は 6 report (5 review facet + workspace-hygiene-scan の検出部分)。file-length-watchlist と workspace-hygiene-scan の ignored サイズは機械的観測であり findings に含めない (§ Input の扱いに従う)。各 report の findings を抽出し以下のルールで統合する:

1. **重複検出**: 同じ `location.path` + 似た description / 同じ category の finding はマージする (facet 間で観点が重なるケースあり、例: simplicity が dead code、architecture が ADR-012 違反として同じ symbol を flag)
2. **rationale 統合**: マージした finding の rationale 部分に該当する複数 facet 名を併記する
3. **severity 確定**: 各 finding の severity は facet が articulate した severity を尊重する。複数 facet が異なる severity を articulate した場合は **高い方** を採用する (例: simplicity が medium、security が high なら high)
4. **品質フィルタ** (最初から表に乗せない):
   - 一般的なベストプラクティスの押し付け (具体的な file / line evidence なし)
   - すでに hooks-config.toml / custom-lint-rules.toml / cli-docs-lint で機械的に検出される pattern (Read で確認可能)
   - 対象ファイルが read-only zone (`.takt/runs/`, `.claude/feedback-reports/` 等の generated artifact) のみで意味のある編集箇所が示せないもの

## Phase 2: 各 finding に Severity / Category / Recommendation を確定する

各 finding について以下の rubric に基づいて判定列を埋める。**この評価は採用判定をユーザーへ委ねるための材料**であり、AI が判定を独占しない。明確に判定できない場合は中庸な値 (`medium` / `🤔 様子見`) を選び、rationale で不確実性を明示する。

> **AI agent への明示禁則**: 本 report の生成完了 = ユーザーへの提示完了に過ぎず、Claude / Codex / Opencode 等の agent は `✅ 採用候補` を読んだだけで採用処理 (`docs/todo*.md` への entry 追加 / 実装着手 / ADR 編集 等) に進んではならない。**必ずユーザーの明示承認 (AskUserQuestion 回答 or テキスト承認のいずれか) を待つこと**。本 report の Recommendation 列は analyzer 推奨であり、確定判断ではない。

### Severity rubric (ADR-031 § Findings スキーマ準拠)

| 値 | 該当する状況 |
|---|---|
| `critical` | data loss / security 脆弱性 / 致命的バグ / production-down リスク |
| `high` | 機能 bug / silent failure / data integrity 違反 / systemic harness drift |
| `medium` | UX 低下 / 累積複雑度 / dead code / 局所 ADR drift |
| `low` | style / micro-optimization / docs typo |

### Category rubric

simplicity / security / architecture facets が emit する category を以下に正規化する:

- `harness-duplication` — rule / pipeline / hook 重複
- `adr-alignment` — ADR と実装の drift
- `docs-internal` — docs 間 cross-ref drift (cli-docs-lint で取れない meta pattern)
- `docs-source-drift` — docs と source の矛盾
- `module-boundary` — モジュール境界違反
- `cyclic-dep` — 循環依存
- `layer-violation` — レイヤ侵犯
- `adr-naming` — ADR-012 命名違反
- `test-anti-pattern` — TDD anti-pattern / 境界欠落
- `cumulative-complexity` — 累積複雑度
- `dead-code` — 未参照コード
- `overspec` — overspec'd abstraction
- `todo-dead-entry` — todo corpus の完了済/陳腐化 entry (削除漏れ)
- `todo-duplicate` — todo*.md 跨ぎの重複 entry / 順位 table と detail の不整合
- `todo-preamble-drift` — todo.md preamble の routing 契約と実態の乖離
- `workspace-hygiene` — リポジトリに残るべきでないファイル (root 直下の想定外ファイル / scratch pattern 合致)
- `jj-mtime-staleness` — mtime を staleness 判断に使用 (jj workspace で reset され silent-fresh)
- `jj-manifest-dir` — CARGO_MANIFEST_DIR 等 compile-time 絶対パスでの実行時ファイル読み
- `jj-gh-no-repo` — `--repo`/`GH_REPO` 無しの gh (非 colocated jj で失敗)
- `jj-state-lifecycle` — gitignored local state の消失/存在前提の hazard
- `secret-exposure` — 機密漏出パターン
- `injection` / `auth-flaw` / `crypto-weak` / `unsafe-no-safety` / `path-traversal` / `prompt-injection` — security category

category が複数該当する場合は最も特徴的な 1 つを採用、補助 category は description で言及する。

### Recommendation rubric

3 種類のいずれかを必ず emit する:

| 値 | 該当する状況 |
|---|---|
| `✅ 採用候補` | `severity ∈ {medium, high, critical}` AND `category が systemic (= harness-duplication / adr-alignment / test-anti-pattern / secret-exposure 等)` AND `Adoption Risk が弱い`。**ユーザー承認後に採用確定**。 |
| `🤔 様子見` | 採用根拠は弱いが将来発生時に再評価したい (Severity 高だが location が局所的、Adoption Risk が中間、Phase C/D で再評価したい等)。✅ にも ❌ にも振り切れない場合の中庸 |
| `❌ 却下推奨` | `severity ∈ {low}` AND `category が局所 (= docs typo / style)` OR `(Adoption Risk が strong: 過剰一般化 / NLP 必要 / false positive リスク / takt test infra 未調査)` OR `(実害観測前の preventive over-engineering)`。**ユーザー承認後に却下確定** (Claude 単独で却下処理しない)。 |

## Phase 3: findings id 採番と JSON 生成

各 finding に id を採番:

- format: `WR-<YYYY-MM-DD>-<facet_initial><sequence>`
- facet_initial: `S` (simplicity) / `C` (security) / `A` (architecture) / `T` (todo) / `J` (jj-robustness) / `W` (workspace-hygiene) / `M` (multi-facet merged)。`T` / `J` は 2026-07〜08 の実 run が既に採番していた表記の明文化 (WR-2026-08-13-T02 / WR-2026-07-19-J01 等が docs/todo.md に採用済みで、変更すると既存参照が壊れる)
- sequence: 同 facet 内で 01 から連番 (`01` / `02` / ...)

例: `WR-2026-05-29-A03` = 2026-05-29 実行、architecture facet 由来、3 番目。

JSON は ADR-031 § Findings スキーマ準拠で `findings.json` というファイル名で write する (workflow の output contract では `name: findings.json` + `format: findings-json` として宣言されている — `findings.json` がファイル名、`findings-json` が契約 (format) 名)。

> **`report_path` の所有権**: 下記 JSON 例の `report_path` field は **Phase C skill `/weekly-review` が copy 後の canonical location** を指す (`.claude/weekly-reviews/<date>.md`)。本 facet は `edit: false` のため自身で copy できないので、`report_path` field は将来 location を予告する形で記述する (= 「skill copy 後に存在する場所」を意味する forward-pointing 記述)。Phase C skill (`~/.claude/skills/weekly-review/SKILL.md`) Phase 2 が Report Directory から `.claude/weekly-reviews/<date>.md` に copy する責務を持つ。Phase C 未実装時 (= Phase B のみ稼働) は `report_path` は dead pointer になるが、Phase C skill が land した後は資源が realize される (PR #182 pre-push reviewer P-1 finding の Phase C 対応):

```json
{
  "run_date": "2026-05-29",
  "report_path": ".claude/weekly-reviews/2026-05-29.md",
  "findings": [
    {
      "id": "WR-2026-05-29-A03",
      "facet": "architecture",
      "severity": "high",
      "category": "harness-duplication",
      "location": { "path": "src/foo.rs", "line_range": "120-145" },
      "description": "...",
      "proposal": "...",
      "decision": "pending",
      "recommendation": "✅ 採用候補",
      "rationale": "..."
    }
  ]
}
```

`decision` field は常に `pending` で出力 (Phase C skill が AskUserQuestion 経由で `adopted` / `rejected` / `deferred` に書き換える)。`recommendation` / `rationale` は analyzer 推奨を保持する補助 field。

## Phase 4: Markdown report 生成

Markdown は人間 / Claude が読む summary 層。findings table を severity 順 (critical → low) で並べ、facet ごとの観察メモを追記する。

### Required output (Markdown)

```markdown
## Weekly Review Report (<YYYY-MM-DD>)

### スコープ
- 対象ツリー: `src/` / `scripts/` / `.claude/` / `.takt/` / `docs/`
- レビューファセット: simplicity-whole / security-whole / architecture-whole / todo-whole / jj-robustness-whole
- 決定論的観測: file-length-watchlist (`.rs` 800 行 + `todo*.md` 50KB) / workspace-hygiene-scan (root allowlist + scratch pattern + ignored サイズ) / ledger-candidates (台帳未掲載の順位)
- 採否方針: Phase C skill `/weekly-review` で AskUserQuestion 経由

### File Length Watchlist (機械的観測)

`file-length-watchlist.md` の内容を本 section に転載する (header 行は省略、件数表示 + table 部分を再現)。0 件 (clean state) の場合は「現時点で 800 行超 file は存在しない (clean state)」を表示。

詳細は Report Directory の `file-length-watchlist.md` を参照。

### 台帳未掲載の順位 (機械的観測)

`ledger-candidates.md` の**件数行だけ**を本 section に転載し、全件表は Report Directory への参照に留める (250 行規模のため)。0 件の場合は「台帳未掲載の順位なし (clean state)」を表示。exe が失敗して `(未実施: ...)` と報告されている場合は**その旨をそのまま**書く — 0 件と書いてはならない。

**これは findings ではない。** 未掲載であること自体は欠陥ではなく、台帳へ載せるか / lane を `✅` `—` のどちらにするかは人間の割り当て判断である (ADR-072 決定 18)。採否フローには乗せない。

詳細は Report Directory の `ledger-candidates.md` を参照。

### Workspace Hygiene (機械的観測)

`workspace-hygiene-scan.md` の「ignored 資産の堆積」section を本 section に転載する (サイズ table)。root 直下の想定外ファイルと scratch pattern 合致は本 section ではなく **統合 findings に含める** (category=`workspace-hygiene`、採否フローに乗せるため)。3 検査すべて 0 件の場合は「workspace hygiene: clean state」を表示。

### 統合 findings

#### Severity: critical / high

| ID | Facet | Category | Location | Description | Proposal | Recommendation | Rationale |
|---|---|---|---|---|---|---|---|

#### Severity: medium

| ID | Facet | Category | Location | Description | Proposal | Recommendation | Rationale |
|---|---|---|---|---|---|---|---|

#### Severity: low

| ID | Facet | Category | Location | Description | Proposal | Recommendation | Rationale |
|---|---|---|---|---|---|---|---|

### Facet 観察メモ

- **simplicity-whole**: <observable patterns / クライテリア 0-3 で目立った傾向>
- **security-whole**: <observable patterns / hotspots>
- **architecture-whole**: <observable patterns / 観点 ① ハーネス遵守 + ② ③ sub criterion>
- **review-todo-whole**: <観点⑤ todo corpus の dead pattern / 重複 / preamble drift の傾向>
- **review-jj-robustness-whole**: <観点⑧ jj workspace fragility (mtime / CARGO_MANIFEST_DIR / gh --repo / .git 前提) の傾向>

### 次のアクション

**重要**: 本 report の Recommendation 列はすべて analyzer の推奨であり、ユーザー明示承認なしに採用・却下を確定してはならない。Claude / 他 AI agent は report を読んだだけで `docs/todo*.md` への entry 追加、実装着手、ADR 編集等を実行してはならず、**必ずユーザー承認 (AskUserQuestion 回答 or テキスト承認のいずれか) を待つこと**。

- `✅ 採用候補`: Phase C skill `/weekly-review` での AskUserQuestion 採用、`docs/todo*.md` 系列への登録または直接実装
- `🤔 様子見`: Phase C/D の dogfood トリガで再評価、現時点で action なし
- `❌ 却下推奨`: ユーザー承認後に却下確定、`docs/todo*.md` への登録不要 (Claude 単独で却下処理しない)
- このレポートは `.claude/weekly-reviews/<run_date>.md` に保存される (`.gitignore` 除外、内部 artifact)
- 構造化 findings は `findings.json` として並置保存される (Phase C skill 入力)
```

findings がゼロ severity (= 該当 finding なし) の section は省略する。

findings 全体がゼロの場合は以下を出力:

```markdown
## Weekly Review Report (<YYYY-MM-DD>)

### スコープ
- 対象ツリー: `src/` / `scripts/` / `.claude/` / `.takt/` / `docs/`
- レビューファセット: simplicity-whole / security-whole / architecture-whole / todo-whole / jj-robustness-whole
- 決定論的観測: file-length-watchlist (`.rs` 800 行 + `todo*.md` 50KB) / workspace-hygiene-scan (root allowlist + scratch pattern + ignored サイズ) / ledger-candidates (台帳未掲載の順位)

### File Length Watchlist (機械的観測)

`file-length-watchlist.md` の内容を本 section に転載する (件数表示 + table 部分)。0 件 (clean state) の場合は「現時点で 800 行超 file は存在しない (clean state)」を表示。

### 台帳未掲載の順位 (機械的観測)

`ledger-candidates.md` の件数行を転載する。0 件の場合は「台帳未掲載の順位なし (clean state)」を表示。exe が失敗して `(未実施: ...)` と報告されている場合は**その旨をそのまま**書く (0 件と書かない)。

### Workspace Hygiene (機械的観測)

`workspace-hygiene-scan.md` の「ignored 資産の堆積」section を転載する。検出 0 件の場合は「workspace hygiene: clean state」を表示。

特筆すべき findings なし。各 facet いずれも whole-tree レビューで blocking concern を発見しませんでした。

決定論層 + diff-local レビュー + post-pr-review が現状の coherence を保っている状態と解釈できます。
```

最後に `aggregation complete` で終了する。

## 出力言語

> **本 facet が週次レビューの言語契約点である。** 入力レポートは 8 件 (5 review facet + 決定論 scan 3 つ)
> で、その言語は**日本語以外が混ざりうるし、それは許容される** — 2026-08-17 の 2 回の実走で、同じ
> instruction・同じ persona・同じ model でも facet の出力言語が run ごとに揺れることを実測した
> (入力 8 件のうち日本語だったのは 1 回目 7 件 / 2 回目 6 件。残りは英語で、1 回はハングルの混入もあった)。
> **内容はどの言語でも正確**であることをコードと突き合わせて確認済みで、言語は表層の差でしかない。
>
> **したがって「全 facet が日本語で出ること」は保証しない。日本語で書くのは本 facet の出力だけである。**
> 混在した入力から**日本語の最終レポートを生成する**こと — 人間が読むのはこの 1 枚であり、ここが
> 日本語であれば中間レポートの言語は問わない。**日本語以外の finding** (英語・ハングルなど言語を問わない)
> を統合するときは、**自由記述の全体を内容を落とさず日本語へ訳して**表へ載せる (原文をそのまま貼らない)。
> `findings.json` の `description` / `proposal` / `rationale` も同じ扱い。
>
> **この契約は現時点で instruction による best-effort であり、決定論的な検査はまだ無い** (`weekly-review.yaml`
> は `aggregation complete` の有無しか見ない)。検査の追加は順位 465 (docs 整合性と output-contract の
> drift を機械検証) の範囲に含めてある。**「保証」と書いて検査を持たないのは、本節が問題視している
> 「指示文で守らせようとする」構図そのもの**なので、検査が入るまでは best-effort と明示しておく。

- **レポート本文は日本語で書く。** コード識別子・ファイルパス・ADR 番号・コマンドはもちろん、**本 facet が出力する固定トークンも訳さない** — 完了条件の `aggregation complete` (`weekly-review.yaml` の `rules.condition` が英語リテラルで照合)、および Markdown report の section 見出しと表の列名
- **`findings.json` の自由記述 field も日本語で書く** (`description` / `proposal` / `rationale`)。`/weekly-review` skill はこれらを `docs/todo*.md` のエントリへ展開するため、英語のままだと展開時に翻訳工程が挟まり、原文と登録文が食い違う余地が生まれる。**`id` / `facet` / `severity` / `category` / `decision` / `location` は enum・識別子なので原文のまま**
