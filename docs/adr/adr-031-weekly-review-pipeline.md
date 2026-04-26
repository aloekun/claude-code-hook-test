# ADR-031: 週次プロジェクト全体レビューパイプライン — whole-tree review の自己改善ループ

## ステータス

試験運用 (2026-04-27)

## コンテキスト

### 問題: 既存 3 パイプラインの review scope の空白

本プロジェクトには 3 つのレビューパイプラインが稼働しているが、いずれも **変更差分** を起点としており、**プロジェクト全体を俯瞰する視点** が欠けている。

| 既存パイプライン | レビュー対象 | 主な観点 | 拾えないもの |
|---|---|---|---|
| pre-push-review ([ADR-015](adr-015-push-runner-takt-migration.md), [ADR-027](adr-027-push-review-simplicity-focus.md)) | push 前の diff | simplicity (diff 局所) | architectural drift / cross-PR の冗長 |
| post-pr-review ([ADR-018](adr-018-pr-monitor-takt-migration.md), [ADR-019](adr-019-coderabbit-review-hybrid-policy.md)) | PR 単位の diff | CodeRabbit 由来の品質 | PR 跨ぎの ADR 違反 / 累積複雑度 |
| post-merge-feedback ([ADR-030](adr-030-deterministic-post-merge-feedback.md)) | マージ済み PR + transcript | 再発防止 (差分起点) | 全体俯瞰 |

ADR-027 は「push-time = simplicity 限定 / architectural review = post-PR」と決めたが、post-PR の CodeRabbit も **PR diff のみを見る** ため、PR 跨ぎの観点は依然空白のままである。

### 拾えていない具体的な瑕疵

- **cross-PR ドリフト**: 個別 PR では妥当でも、累積で見ると同じ責務の関数が複数モジュールに散らばる
- **ADR 違反の蓄積**: ADR で禁止したパターンが新規 PR では検出されるが、既に commit 済みの違反は誰も指摘しない
- **命名規約のドリフト**: ADR-012 で定めた命名が古いコードでは破られている
- **無駄の累積**: dead code / 未使用の抽象化 / overspec'd module が PR 単位では「今回の変更ではない」として見送られる
- **循環依存・レイヤ侵犯**: モジュール間関係は diff 単独では判断不可

### 設計上の知見: review scope 軸での既存パイプラインの分布

レビューを「scope (diff 局所 / PR diff / whole tree)」と「観点 (simplicity / security / architecture)」の 2 軸でマッピングすると、whole-tree × architecture と whole-tree × simplicity が空白である。

```text
              | diff 局所       | PR diff         | whole tree
--------------|-----------------|-----------------|-----------------
simplicity    | pre-push (027)  | CodeRabbit      | ❌ 空白
security      | pre-push        | CodeRabbit      | ❌ 空白
architecture  | (ADR-027 で除外)| post-pr-review  | ❌ 空白
```

### 既存の決定論パターン (ADR-030) との比較

ADR-030 は「機械的 = Rust / AI parallel = takt / ask-based = ユーザー対話」の 3 層分離を確立した。本 ADR はこのパターンを **4 例目** として継承するが、**must-run 要件を持たない** 点で性質が異なる:

| 観点 | ADR-030 (post-merge-feedback) | 本 ADR (weekly-review) |
|---|---|---|
| 起動タイミング | merge 直後 (機械的に必須) | 週次 (人間判断、忘れても致命的でない) |
| 失敗時の影響 | silent loss = 学習機会喪失 → must-run | 単に「今週はスキップ」で済む → best-effort で十分 |
| トリガー | cli-merge-pipeline (決定論的) | 手動 `/weekly-review` + reminder hook |
| 決定論ゲート | 必要 (`.failed` marker + L2 recovery) | 不要 (reminder で十分) |

must-run でないことが「skill を主動線に置ける」設計上の余地を生む。ADR-030 が skill を否定したのは must-run 要件下での話であり、本 ADR はその結論を一般化した規範ではない。

## 検討した選択肢

### 選択肢 A: 既存 post-pr-review に whole-tree モードを追加

`post-pr-review.yaml` に `--whole-tree` フラグを追加し、PR ごとに全体レビューも走らせる案。**却下**:

- PR ごとに whole-tree レビューを走らせると重複が大量に発生し、CodeRabbit 指摘との優先順位付けも難しい
- 「週次」のリズムで俯瞰したいという本要件のセマンティクスを満たさない
- post-pr-review は ADR-019 のハイブリッド構成で機能分担が確立しており、責務を増やすと崩れる

### 選択肢 B: skill 単独 (手動 + AskUserQuestion で対話的レビュー)

`/weekly-review` skill が単一の Claude セッション内で全観点を順次レビューする案。**却下**:

- 3 観点 (simplicity / security / architecture) を逐次実行すると context window が肥大化し、後半の facet ほど判断が劣化する
- 並列性がないため wall-clock が長くなる
- ADR-015 / 018 / 030 で確立した「AI 並列処理は takt」原則と乖離する

### 選択肢 C: takt 単独 (parallel facets, no skill)

`weekly-review.yaml` workflow を直接呼び、レポートだけ出力する案。**却下**:

- ユーザー採否対話 (採用 / 却下 / 保留) の UX が takt の loop / supervise 機構では表現しにくい
- todo.md への追記は「ユーザー意図表現を含む既存 artifact への書き込み」で、ADR-022 原則 1 の「未承認での確定」を避けるためにユーザー確認ゲートが必須 → ask-based な経路が必要
- skill (AskUserQuestion) を介さないと、採否単位の細かい意思決定ができない

### 選択肢 D: hybrid (takt workflow + skill, manual + reminder hook)

並列レビューは takt、ユーザー対話と todo.md 反映は skill、リマインドは Rust hook。各層が得意な役割に専念する。**採用**。

## 決定

**選択肢 D を採用する。**

### アーキテクチャ: 3 層構成

| 層 | 機構 | 責務 | 失敗時の挙動 |
|---|------|-----|------------|
| **L1 Reminder** | `hooks-session-start` (Rust) 拡張 | `.claude/weekly-review-last-run.json` の mtime を見て、7 日以上経過していれば `additionalContext` で `/weekly-review` を促す | reminder 不在 (致命的でない、ユーザーが気付けば実行) |
| **L2 Review** (AI parallel) | takt workflow `weekly-review` | 3 facets (simplicity / security / architecture) を **whole-tree** で並列レビュー、aggregate facet で findings JSON + markdown 統合 | `.claude/weekly-reviews/<date>.md.failed` marker 残存 → 次セッションの L1 hook が recovery context を出力 |
| **L3 Approval & Apply** | skill `/weekly-review` | takt 起動 → pending JSON 読み込み → AskUserQuestion で採否一括選択 → 採用分のみ docs/todo.md に追記 | best-effort (ユーザーが skill を再起動すれば pending JSON から再開可能) |

### 全体フロー

```text
SessionStart hook (hooks-session-start.exe)
  └─ .claude/weekly-review-last-run.json の mtime チェック
       ├─ 7日未経過: silent exit
       ├─ 7日経過: additionalContext で /weekly-review を促す (reminder)
       └─ *.md.failed marker 検出: additionalContext で /weekly-review --resume を促す (recovery)

  ▼ (ユーザーが /weekly-review を実行)

skill /weekly-review (Phase 1-4)
  ├─ Phase 1: 起動条件チェック (--dry-run / --resume の判定)
  ├─ Phase 2: takt run weekly-review.yaml を同期実行
  │     ├─ parallel:
  │     │   ├─ review-simplicity-whole  (whole-tree, ADR-027 制約解除)
  │     │   ├─ review-security-whole    (whole-tree, security knowledge)
  │     │   └─ review-architecture-whole (新 persona, ADR 整合性)
  │     └─ aggregate-weekly  (3 reports → findings JSON + markdown)
  │     成功: .claude/weekly-reviews/<YYYY-MM-DD>.md + .claude/weekly-review-pending.json
  │     失敗: .claude/weekly-reviews/<YYYY-MM-DD>.md.failed marker
  ├─ Phase 3: pending JSON を読み込み AskUserQuestion で採否一括選択
  │     (採用 / 却下 / 保留 を finding ごとに記録)
  └─ Phase 4: 採用 finding を docs/todo.md の「週次レビュー採用 (date)」セクションに追記
              + .claude/weekly-review-last-run.json を更新
              + .claude/weekly-review-pending.json をクリア
```

### takt workflow 構成 (3 review facets + 1 aggregate)

[ADR-020](adr-020-takt-facets-sharing.md) の facets 共通化原則に倣う。本 workflow は 4 facet を 2 step で chain する:

| facet | 役割 | 派生元 |
|---|---|---|
| `review-simplicity-whole` | whole-tree の simplicity 観点 (重複 / 累積複雑度 / dead code / overspec'd 抽象化) | `review-simplicity.md` から派生 (※後述「アンチパターン」で共通化不可) |
| `review-security-whole` | whole-tree の security 観点 (機密漏出パターン / 入力検証の偏在 / 暗号アルゴリズム) | `review-security.md` から派生 |
| `review-architecture-whole` | ADR 整合性 / モジュール境界 / [ADR-012](adr-012-src-naming-convention.md) 命名規約 / 循環依存 / レイヤ侵犯 | 新規 |
| `aggregate-weekly` | 3 reports → findings JSON + markdown (採否単位の構造化) | `aggregate-feedback.md` を参考 |

**並列構成**: 3 review facets を `parallel:` block で並列実行し、`aggregate-weekly` で統合する。これは [post-merge-feedback.yaml](../../.takt/workflows/post-merge-feedback.yaml) の構造を流用する (analyze 3 並列 → aggregate)。fix loop は不要 (修正対象がコードではなく findings レポート生成)。

### 入力源

- **ソースツリー全体**: 主要 dir (`src/`, `scripts/`, `.claude/`, `.takt/`, `docs/`) を各 facet が Glob で順読
- **ADR コーパス**: `docs/adr/*.md` を architecture facet が参照 (ADR 違反検出のため)
- **CLAUDE.md**: プロジェクト規約の根本 (architecture facet が参照)

サブツリー分割は MVP では行わない。context 圧迫が観測されてから 2nd PR で facet 内分割を切り出す ([YAGNI](../../CLAUDE.md))。

### 出力

| ファイル | 用途 | gitignore |
|---|---|---|
| `.claude/weekly-reviews/<YYYY-MM-DD>.md` | レポート本文 (履歴) | ✅ |
| `.claude/weekly-reviews/<YYYY-MM-DD>.md.failed` | 失敗マーカー (内容は失敗理由 + 復旧手順) | ✅ |
| `.claude/weekly-review-pending.json` | finding 配列 + decision フィールド (skill が読み書き) | ✅ |
| `.claude/weekly-review-last-run.json` | SessionStart hook 用タイムスタンプ | ✅ |

### Findings スキーマ

```json
{
  "run_date": "2026-04-27",
  "report_path": ".claude/weekly-reviews/2026-04-27.md",
  "findings": [
    {
      "id": "WR-2026-04-27-A03",
      "facet": "simplicity | security | architecture",
      "severity": "critical | high | medium | low",
      "category": "nesting | naming | adr-violation | cyclic-dep | dead-code | ...",
      "location": { "path": "src/foo.rs", "line_range": "120-145" },
      "description": "...",
      "proposal": "...",
      "decision": "pending | adopted | rejected | deferred"
    }
  ]
}
```

`id` は `WR-<run_date>-<facet_initial><sequence>` 形式。aggregate-weekly facet が一意に採番する。

### 採否フロー (pending JSON 経由)

skill Phase 3 では AskUserQuestion で finding ごとに採否を聞く。`multiSelect: true` で「採用したい finding を選択 → 残りは却下扱い」のフローを基本とする。各 finding は `severity` でグループ化して提示し、critical/high を優先表示する。

ユーザー判断:

- **採用 (adopted)**: docs/todo.md の「週次レビュー採用 (date)」セクションに展開して追記
- **却下 (rejected)**: pending JSON 内に履歴として残るが、次回以降は出てこない (重複検出キーは `category + location.path` の組合せ)
- **保留 (deferred)**: 次週の `weekly-review` で再提示する (skill が pending JSON を読み込む際に保留分を注入)

### todo.md 反映ルール

採用 finding は docs/todo.md の `## 現在進行中` の **新セクション「週次レビュー採用 (YYYY-MM-DD)」** にまとめて追記する。各 finding を以下のテンプレートで展開:

```markdown
### [finding.description の要約タイトル]

> **動機**: [finding.description]
> **本タスクの位置づけ**: 週次レビュー [finding.id] で採用 (severity={severity}, facet={facet})

#### 背景: [finding.location でのコンテキスト]
#### 設計決定: [finding.proposal]
- [ ] サブタスク (ユーザーが後で詳細化)
#### 完了基準: [proposal の達成条件]
```

**重複検出は MVP では実装しない**。skill 側で「todo.md の既存セクション一覧を Read → タイトル一致っぽい場合は警告のみ」程度に留める。

却下 / 保留 finding は `.claude/weekly-reviews/<date>.md` 内にのみ履歴として残し、todo.md には書かない (運用ルール「完了タスクを残さない」と整合 — todo.md は作業予定のみ)。

### 失敗ポリシー: best-effort

takt 失敗時の挙動:

- skill Phase 2 で `.claude/weekly-reviews/<date>.md.failed` marker が残る
- 次セッションの SessionStart hook (L1) が `*.md.failed` を検出 → `additionalContext` で `/weekly-review --resume` を促す
- ユーザーが応答しなければ marker は残り続けるが、**ユーザー学習機会を逸するだけで実害なし** (must-run ではない)

ADR-030 の `.failed` marker パターンを流用するが、L2 recovery (UserPromptSubmit hook) は実装しない。理由:

- L1 (SessionStart) で十分 (週次レビューは「次のセッション開始時に思い出せば良い」レベルの粒度)
- UserPromptSubmit hook を増やすと session 起動時のオーバーヘッドが増える

### トリガー方式と reminder

- **手動トリガー**: `/weekly-review` skill を明示呼出
- **reminder**: SessionStart hook が `.claude/weekly-review-last-run.json` の mtime を見て、7 日以上経過していれば `additionalContext` で促す (強制起動はしない)
- **将来の自動化**: 機能安定後に schedule スキル (CronCreate-based) や `/loop 7d /weekly-review` を検討するが、MVP では実装しない (YAGNI、機能の安定性を観測してから判断)

### ADR-027 (push-time = simplicity 限定) との関係

ADR-027 は「architectural review は post-PR に委ねる」と決めたが、ここで言う「post-PR」は CodeRabbit による **PR diff レビュー** を指していた。**cross-PR な architectural review は明示的に空白** だったため、本 ADR がその空白を埋める。

ADR-027 の本質的判断 (push 時に重い arch review を走らせない) は維持し、本 ADR は **週次という別リズム** で whole-tree な architectural review を入れる。両者は競合しない。

### ADR-022 (責務分離原則) との整合性

L2 (takt) と L3 (skill) の副作用範囲は ADR-022 原則 1 の枠内に収まる:

- **takt facets**: 全て `edit: false`、Read/Glob/Grep のみ → 副作用なし
- **aggregate-weekly facet**: `.claude/weekly-reviews/<date>.md` と pending JSON への書き込み → **新規 artifact への自己記述**
- **skill Phase 4**: docs/todo.md への追記 → **既存 artifact だが意図表現ではない作業ファイルへの追記**、かつユーザー採否承認を経た後の確定

docs/todo.md は ADR-022 で言う「意図表現を含む既存 artifact」(commit description / PR title / bookmark 名) には該当せず、作業計画ファイルなのでユーザー承認後の追記は許可される。ただし skill 側で「採用 finding 一覧をユーザーに見せて確認 → 確定後に書き込む」フローを必須とすることで、未承認確定を避ける。

### ADR-028 (外部可視成果物ゲート) との関係

本 ADR は **内部 artifact のみ生成・更新**:

- `.claude/weekly-reviews/*` — local 専用、`.gitignore` で除外
- `.claude/weekly-review-pending.json` — local 専用、`.gitignore` で除外
- `.claude/weekly-review-last-run.json` — local 専用、`.gitignore` で除外
- `docs/todo.md` — repo に含まれるが PR でレビュー可能、外部公開 (GitHub PR / tag / commit description) ではない

GitHub 上に観測可能な成果物 (PR / tag) を直接生成・改変することはないため、ADR-028 の `permissions.ask` ゲートの **対象外**。

### ADR-030 パターン継承

ADR-030 で確立した「機械的 = Rust / AI 並列 = takt / ask-based = skill or hook」3 層分離パターンの **4 例目** として位置付ける:

| 例 | L1 (機械的) | L2 (AI 並列) | L3 (ask-based / 補助) |
|---|---|---|---|
| 1 (ADR-015 push) | quality gates (Rust) | pre-push-review (takt) | (なし) |
| 2 (ADR-018 PR monitor) | cli-pr-monitor poll (Rust) | post-pr-review (takt) | (なし) |
| 3 (ADR-030 post-merge) | cli-merge-pipeline (Rust) | post-merge-feedback (takt) | UserPromptSubmit hook (recovery, Rust) |
| **4 (本 ADR)** | **SessionStart hook (reminder, Rust)** | **weekly-review (takt)** | **`/weekly-review` skill (approval & apply)** |

差分は L3 が実装の中心であること。これは **must-run でない** ことに起因する自然な分布。

## 実装タスク

詳細な実装手順は [`docs/todo.md`](../todo.md) の「週次プロジェクト全体レビューパイプラインの導入」セクション Phase A-F を参照。本 ADR は仕様のみを規定する。

- **Phase A**: 本 ADR 起案 (PR 1) — 設計のみ
- **Phase B**: takt workflow + 4 facets + architecture-reviewer persona (PR 2)
- **Phase C**: skill + SessionStart hook 拡張 (PR 3)
- **Phase D**: e2e 検証 (PR 3 マージ後 / PR 4 起案前)
- **Phase E**: 試験運用 dogfood (PR 4 — 1〜2 週運用 + ADR-031 ステータス更新)
- **Phase F**: 自動化検討 (本採用後の任意 — schedule スキル経由の cron 化)

## アンチパターン

### `review-simplicity.md` を whole-tree 用と共有してはならない

ADR-027 が `review-simplicity.md` を **diff 局所** に責務を絞ったのは、コンテキストサイズと判断空間の両面で本質的最適化だった。whole-tree 用 facet (`review-simplicity-whole.md`) はこの制約を解除する別物として **派生コピー** で実装する。共通化すると:

- diff 用が累積複雑度の判断空間に引きずられて遅くなる (ADR-027 の改善が回帰)
- whole-tree 用が diff 局所制約に縛られて拾えるべき finding を見逃す

両方とも目的が異なるため separation が正しい。これは [ADR-020](adr-020-takt-facets-sharing.md) の「責務が同じものだけを共通化する」原則の帰結。

### whole-tree レビューを must-run にしてはならない

本 ADR は best-effort で十分という判断をした。これを「PR ごとに必ず走らせる」「マージブロック条件にする」等の must-run 化に拡張すると:

- レビュー結果の重複処理 (同じ finding が複数 PR で繰り返し提示される)
- 開発速度の低下 (週次のリズムを失う)
- ADR-030 が解決した silent loss 問題が再発する余地が生まれる

「週次という低頻度・俯瞰的な視点」自体に価値があり、頻度を上げると価値が逆に失われる設計上の知見。

### 採否対話を Phase 4 で省略してはならない

「全部 todo.md に書いてユーザーが後で取捨選択」案は実装が簡単だが、todo.md が **採用していない作業案で膨らむ** ため、運用ルール「完了タスクを残さない」「作業予定のみ記録」と背反する。skill Phase 3 の AskUserQuestion を経由する設計は、todo.md の純度を保つために必須。

### Reminder を強制起動 (auto-trigger) にしてはならない

SessionStart hook が `additionalContext` で促すのみで、skill を勝手に起動してはいけない。理由:

- ADR-029 / 030 で得た「skill 強制起動は構造的に成立しない」教訓
- 週次レビューはユーザーが自分のタイミングで実行すべき (must-run でない以上、強制は害)

## 影響

### Positive

- **レビュー scope の空白が埋まる**: cross-PR ドリフト / ADR 違反蓄積 / 累積複雑度を週次で拾える
- **ADR-030 パターンの一般化**: 「機械的 / takt / ask-based」3 層分離の 4 例目として確立し、今後のパイプライン設計の参照例になる
- **既存 ADR との非競合**: ADR-027 / 030 が空けた空白を埋めるだけで、既存パイプラインの責務には介入しない
- **dogfood しやすい**: 内部 artifact のみで完結し、失敗しても致命的でないため、試験運用がしやすい

### Negative

- **新規 takt workflow + 3 facets + 1 persona の保守コスト**: pre-push-review / post-pr-review / post-merge-feedback に続く 4 つ目の workflow となる
- **`review-simplicity.md` と `review-simplicity-whole.md` の派生関係を保守する負担**: ADR-027 改訂時に whole 版も追従する必要 (ただし共通化は不可、上述アンチパターン参照)
- **whole-tree レビューの context window 圧迫リスク**: 初回 dogfood で観測してから対処判断 (Phase E)
- **派生プロジェクトへのバックポート工数**: takt-test-vc 等への展開時は workflow + facets + persona + skill + hook 拡張のセット移植が必要

### 将来の展望

- **Phase E dogfood 安定後の本採用化**: ステータスを `承認済み` に更新
- **schedule スキル経由の自動化** (Phase F): 機能安定後に weekly cron 化
- **派生プロジェクトへのバックポート**: takt-test-vc / techbook-ledger 等
- **finding 重複検出の自動化**: MVP では未実装、運用で必要性が観測されてから検討
- **review scope 軸の他の空白埋め**: 例えば「whole-tree × performance」「whole-tree × accessibility」など、軸自体の拡張余地

## References

- [ADR-012: src/ ディレクトリの命名規約](adr-012-src-naming-convention.md) — architecture facet の検証ルールに組み込む
- [ADR-015: Push Pipeline takt 移行](adr-015-push-runner-takt-migration.md) — 「機械的 = Rust、AI = takt」原則の先行事例 (1 例目)
- [ADR-018: cli-pr-monitor takt 移行](adr-018-pr-monitor-takt-migration.md) — 同原則の 2 例目
- [ADR-019: CodeRabbit レビュー運用のハイブリッド構成](adr-019-coderabbit-review-hybrid-policy.md) — post-pr-review の現行責務範囲を確認する根拠
- [ADR-020: takt facets 共通化戦略](adr-020-takt-facets-sharing.md) — facets の共通化判断基準
- [ADR-022: 自動化コンポーネントの責務分離原則](adr-022-automation-responsibility-separation.md) — `edit: false` 方針 / 副作用範囲の根拠
- [ADR-027: Push-time review を simplicity に限定](adr-027-push-review-simplicity-focus.md) — 本 ADR が補完する空白の特定根拠
- [ADR-028: 外部可視成果物ゲート](adr-028-pnpm-create-pr-gate.md) — 外部可視成果物との軸別境界 (本 ADR は対象外)
- [ADR-030: 決定論的 Post-Merge Feedback](adr-030-deterministic-post-merge-feedback.md) — 3 例目、本 ADR は 4 例目として 3 層分離パターンを継承
