# Aggregate Feedback

3 つの分析レポート (PR 知見・セッション知見・pre-push レポート知見) を Plankton 優先度で統合し、最終的な再発防止策レポートを生成する。

旧 `/post-merge-feedback` skill (`E:\work\claude-code-skills\post-merge-feedback\SKILL.md`) の Phase 4 ロジックから port。

**重要な原則:**
- 読み取り専用。コードの修正は一切行わない (実装は L2 recovery / ユーザー判断で行う)
- 知見がない場合は「提案なし」で正常終了する。無理に提案を捻出しない
- 重複する提案はマージし、根拠 (rationale) を統合する
- Tier 1 を最優先で提案する。Tier 3 のみの提案は価値が低い
- **各提案には Severity / Frequency / Adoption Risk / Recommendation を必須で評価する** (順位 58 / PR #106 評価セッションで合意)

---

## Input

### Report Directory (takt が提供)

本 step (`pass_previous_response: false`) は前 step の response を受け取らない代わりに、**Report Directory** に保存された 3 つの先行レポートを Read で読み取る:

- `pr-analysis.md` — analyze-pr facet の出力
- `session-analysis.md` — analyze-session facet の出力
- `prepush-analysis.md` — analyze-prepush-reports facet の出力

### Context file

`.takt/post-merge-feedback-context.json` も Read で読み、PR 番号・タイトル等のメタデータを取得する:

```json
{
  "pr_number": 123,
  "owner_repo": "aloekun/claude-code-hook-test",
  "merged_at": "2026-04-25T10:00:00Z"
}
```

PR タイトルが context に含まれていない場合は、レポート内の `### PR: ...` ヘッダから抽出する。

## Phase 1: 3 レポートの統合

各レポートの提案リスト (Tier 1 / 2 / 3 の表) を抽出し、以下のルールで統合する:

1. **重複検出**: 同じ `Target` + 似た `Description` の提案はマージする
2. **根拠統合**: マージした提案の `Rationale` カラムには複数ソース (PR diff / session / prepush) を併記する
3. **Tier 並び**: 最終リストは Tier 1 → Tier 2 → Tier 3 の順
4. **品質フィルタ**: 以下の提案は除外する (Recommendation 列での `❌ 却下` と区別: ここで除外するのは「最初から表に乗せない」レベル)
   - 一般的なベストプラクティスの押し付け (具体的根拠がない)
   - すでに hooks-config.toml / custom-lint-rules.toml に存在するルール (Read で確認可能)
   - 対象ファイルが read-only zone (`.takt/`, `docs/adr/`, `templates/`) のみで具体的な編集箇所が示せないもの
     - **判定方法**: `Target` 列に含まれるパスを基準に、**編集可能 (write zone) なパスが一つでも含まれる場合は除外しない**
     - 逆に、`Target` が上記 read-only zone に**完全に限定**され、かつ編集可能な行/差分/コードブロックが示されない提案のみ除外する

## Phase 2: 各提案に Severity / Frequency / Adoption Risk / Recommendation を付与

各提案について、以下の rubric に基づいて 4 つの判定列を埋める。**この評価は採用判定をユーザーへ委ねるための材料**であり、AI が判定を独占するわけではない。明確に判定できない場合は中庸な値 (`Medium` / `🤔 様子見`) を選び、`Rationale` で不確実性を明示する。

### Severity rubric

| 値 | 該当する状況 |
|---|---|
| `Critical` | data loss / security 脆弱性 / 致命的バグ / production-down リスクの再発防止 |
| `High` | 機能 bug / silent failure / data integrity 違反 |
| `Medium` | silent degrade / UX 低下 / 開発体験劣化 / token 浪費 |
| `Low` | style / micro-optimization / 局所改善 / 命名 convention |

### Frequency rubric

| 値 | 該当する状況 |
|---|---|
| `High` | 複数 PR で観測済み / systemic pattern (3 PR 以上で言及あり) |
| `Medium` | 1 PR + 類似コードベースで再発見込みあり / 過去 1-2 PR で関連事象 |
| `Low` | 本 PR のみで観測 / 局所現象 |
| `Very Low` | extreme edge case / 単発の特殊事情 |

### Effort rubric

実装に要する工数。Recommendation 判定の入力として使うため、許容値を以下に固定する:

| 値 | 該当する状況 |
|---|---|
| `XS` | 文言/設定の微修正、単一箇所の変更、テストなしで完結 (1-数行) |
| `S` | 単一ファイルの局所変更、テスト追加含めて半日以内で完結 (数行〜数十行) |
| `M` | 複数ファイルにまたがる変更、テスト + 動作確認で 1-2 日 (数十〜数百行) |
| `L` | アーキテクチャレベルの変更、複数 PR / 新規モジュール追加、design doc 推奨 |
| `XL` | 大規模リファクタ / 機構新設、専用 design doc + 段階的 rollout 必須 |

### Adoption Risk rubric

採用時に発生しうるリスクや負債を **1-2 語の短いタグ** で記述する。よく出る選択肢:

- `None` — リスクなし、採用に伴う overhead が極小
- `既存ルール重複` — 既存 ADR / hooks / facets と内容が overlap
- `過剰一般化` — 局所事象を universal rule に昇格させるリスク
- `NLP 必要` — コメントと実装の照合等、自然言語処理が必要で実装非現実的
- `OS 依存` — Windows / Unix 等の差異で挙動が変わる
- `false positive リスク` — regex / pattern matching で誤検出が高頻度に発生
- `reviewer instruction 肥大化` — facet prompt に追加することで attention drift 再発リスク
- `派生プロジェクト deploy コスト` — techbook-ledger / auto-review-fix-vc 等への展開負荷
- `takt test infra 未調査` — takt 側のテスト機構の有無で Effort が大きく変動
- `runner 複雑化` — Rust 実装の cli-* に parse logic 等を追加する複雑度

該当しないものは独自に短いタグを作ってよい (1-2 語、英日混在可)。

### Recommendation rubric (必須)

3 種類のいずれかを必ず emit する。条件式は **括弧で評価順を明示**しているので、人間 / AI どちらの解釈でもズレない:

| 値 | 該当する状況 |
|---|---|
| `✅ 採用` | `(Effort ∈ {XS, S, M})` AND `(Severity ∈ {Medium, High, Critical} OR Frequency ∈ {Medium, High})` AND `(Adoption Risk が weak)` |
| `🤔 様子見` | 採用根拠は弱いが将来発生時に再評価したい (一般原則 / 不確実性高 / dogfood トリガ待ち / Severity 高だが Frequency Very Low 等)。✅ にも ❌ にも振り切れない場合の中庸 |
| `❌ 却下` | `(Frequency ∈ {Low, Very Low} AND Effort ∈ {L, XL})` OR `(Adoption Risk が strong)` OR `(実害観測前の preventive over-engineering)` |

**Adoption Risk の「weak / strong」定義** (上記条件式で参照):

- `weak` (✅ 側): `None` または採用に伴う overhead が極小なタグ。例: `派生プロジェクト deploy コスト` 単独 (= 単純な配布作業)
- `strong` (❌ 側): 採用 = 別の問題を生むタグ。例: `既存ルール重複` / `過剰一般化` / `NLP 必要` / `false positive リスク` / `reviewer instruction 肥大化` / `runner 複雑化` / `takt test infra 未調査`
- 中間 (= 🤔 様子見側に倒す): 上記いずれにも明確に分類できないタグ、または複数タグの組み合わせで weak/strong の境界が不明瞭な場合

### Rationale (拡張)

従来の `Source` 表記 + **採用判断の根拠** を 1-2 文で記述する。Format:

```text
<Source>; <採用根拠>
```

- Source 凡例: `PR diff` / `Review comment` / `Session` / `Prepush:simplicity` / `Prepush:security` (複数は `;` 区切りで 1 つの Source に集約してから採用根拠を続ける)
- 採用根拠: なぜ Severity × Frequency × Effort × Adoption Risk から Recommendation に至ったか

例:

- 採用 例: `PR diff; Session; collect_all_violations の MAX_VIOLATIONS contract を test 化、将来の lint 追加時の regression 防止網。Effort S かつ Frequency Medium`
- 様子見 例: `Session; Honesty constraint で抑制中、実観測 0 件、dogfood で虚偽申告観測後に着手`
- 却下 例: `Session; 1 観測の局所 artifact、汎用 regex は英語固有名詞・略語で誤検出確実、ROI 不見合い`

---

## Required output

```markdown
## Post-Merge Feedback Report

### PR: <owner/repo>#<number> <title>
- マージ日時: <merged_at>
- 分析ソース: PR data, Session transcript, Pre-push reports

### 統合された再発防止策

#### Tier 1: Hooks/Linter 改善 (決定論的防止)

| # | Type | Description | Target | Severity | Frequency | Effort | Adoption Risk | Recommendation | Rationale |
|---|------|-------------|--------|----------|-----------|--------|---------------|----------------|-----------|
| 1 | custom_lint_rule | ... | .claude/custom-lint-rules.toml | Medium | High | S | None | ✅ 採用 | PR diff; Session; ... |

#### Tier 2: テスト/自動化

| # | Type | Description | Target | Severity | Frequency | Effort | Adoption Risk | Recommendation | Rationale |
|---|------|-------------|--------|----------|-----------|--------|---------------|----------------|-----------|

#### Tier 3: ドキュメント/ルール

| # | Type | Description | Target | Severity | Frequency | Effort | Adoption Risk | Recommendation | Rationale |
|---|------|-------------|--------|----------|-----------|--------|---------------|----------------|-----------|

### 次のアクション

- ユーザーがレポートを確認後、Recommendation 列を参考に採用判断を下す (✅ 採用は基本採用、🤔 様子見は dogfood トリガ次第、❌ 却下は不要)
- 採用された提案は `docs/todo.md` 系列に登録するか直接実装へ進む
- このレポートは `.claude/feedback-reports/<pr_number>.md` に保存される (`.gitignore` 除外、内部 artifact)
```

提案がない Tier はセクションごと省略する。

提案がゼロの場合は以下:

```markdown
## Post-Merge Feedback Report

### PR: <owner/repo>#<number> <title>

この PR から特筆すべき再発防止策は見つかりませんでした。
3 レポート (PR / Session / Pre-push) のいずれにも、決定論的な防止策に値する事象が記録されていませんでした。
```

最後に `aggregation complete` で終了する。
