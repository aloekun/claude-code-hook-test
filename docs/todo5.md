# TODO (Part 5)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo4.md がファイルサイズ約 50KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する。todo.md / todo2.md / todo3.md / todo4.md の既存エントリは引き続き有効、相互に独立。新セッションでは五つすべてを確認すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo.md](todo.md#recommended-order-summary) を参照。

---

## 現在進行中

### `>` vs `>=` boundary inconsistency lint rule (PR #101 T1-2)

> **動機**: PR #101 で `parse_listed_findings` の `created_at > push_time` が CodeRabbit から境界 inclusive (`>=`) への揃え修正を指摘された。auto-fix が同一ファイル内 `parse_new_comments` / `parse_findings` にも `>=` を適用 (= 3 関数 latent drift)。`parse_rate_limit` だけが既に `>=` で、後続関数を書くたびに著者が意識せず `>` を選ぶ構造的問題。custom-lint-rule で書いた瞬間に block すれば bug class が排除される。
>
> **本タスクの位置づけ**: PR #101 post-merge-feedback Tier 1 #2 採用 (高頻度 finding)。Bundle Z #B-α (Rust comment lint hook) と同じ「決定論的防止層」哲学。AST 解析ではなく正規表現層 (ADR-007) で対応可能。
>
> **参照**: `.claude/feedback-reports/101.md` Tier 1 #2、ADR-007 (custom lint rule の正規表現 / AST 層線引き)、CodeRabbit PR #101 round 1 Minor finding
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。`.claude/custom-lint-rules.toml` への regex rule 追加。

#### 設計決定 (案)

- **配置先**: `.claude/custom-lint-rules.toml` に新規 rule entry
- **検出パターン (正規表現案)**:
  - 狭め: `\.(created_at|submitted_at|updated_at)\b.*\.map\(\|\w+\|\s*\w+\s*[><](?!=)\s*(push_time|since)`
  - 広め: `\b(created_at|submitted_at|updated_at|comment_event_time|event_time)\b.*[><](?!=)` で時刻フィールドの strict inequality 全般を flag
- **適用対象**: `.rs` ファイル
- **rule 名 (案)**: `time-boundary-strict-inequality`
- **suppress マーカー**: `// SAFETY: <理由>` 行末付与で suppression (例: 意図的に exclusive 比較する場合)

#### 作業計画

- [ ] 既存 `.claude/custom-lint-rules.toml` の rule 構造を確認
- [ ] regex + path filter を新 rule として記述
- [ ] PostToolUse hook の lint runner で synthetic test (修正前 `parse_findings` 系の `>` パターンを再現してマッチ確認)
- [ ] 既存 codebase で false positive 影響範囲をグレップして確認
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy 確認
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- `.claude/custom-lint-rules.toml` に新 rule が追加され `.rs` ファイル内の時刻フィールド strict inequality を検出
- 1〜2 PR で dogfood し false positive がないこと

#### 詰まっている箇所

- false positive の評価 (時刻フィールド以外で legitimate な `>` が誤 block されないか)。着手時に実 codebase でグレップして影響範囲を確認。

---

### 関数長スケーリング検出 oxlint rule (PR #101 T1-4)

> **動機**: PR #101 で `parse_listed_findings` が 60 行となり、CLAUDE.md `coding-style.md` の 50 行ガイドラインを超過 (takt review が W-001 として warning)。ガイドラインは ask-based のため drift 蓄積中。96.md でも類似言及あり (関数長関連 finding)、**複数 PR で繰り返される drift**。oxlint rule で warning 40-50 行 / error 50+ 行を機械検出すれば、書いた瞬間に block されて drift しない。
>
> **本タスクの位置づけ**: PR #101 post-merge-feedback Tier 1 #4 採用 (高頻度 finding)。Bundle Z #B-α と同じ決定論的防止層。`.oxlintrc.json` + `src/oxlint-rules/` への追加で完結。
>
> **参照**: `.claude/feedback-reports/101.md` Tier 1 #4、`.claude/feedback-reports/96.md`、`~/.claude/rules/common/coding-style.md` (50 行ガイドライン)
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。oxlint plugin への rule 追加。

#### 設計決定 (案)

- **配置先**: `.oxlintrc.json` に rule 設定 + `src/oxlint-rules/` (自作 rule の配置先) に rule 実装
- **閾値 (案)**:
  - warning: 40 行超
  - error: 50 行超 (block)
- **対象**: `.rs` / `.ts` / `.js` (言語間で共通化、ただし AST 抽象差異あり)
- **suppress**: `// oxlint-disable function-length` 行末
- **既存 rule との関係**: 既存 `src/oxlint-rules/` の rule 構造を参照 (custom rules がすでに存在する想定)

#### 作業計画

- [ ] 既存 `src/oxlint-rules/` のディレクトリ構造を確認 (Rust / TS どちらの impl か)
- [ ] 関数長計測 rule を実装 (AST node line range ベース)
- [ ] `.oxlintrc.json` に rule 有効化設定を追加
- [ ] 既存 codebase で 50 行超関数の数を事前調査 (段階的 rollout が必要か判断)
- [ ] dogfood: `parse_listed_findings` (修正前なら error として detected されるはず) を synthetic test で確認
- [ ] 派生プロジェクトへ deploy
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- oxlint rule が `.rs` / `.ts` で関数長 50 行超を error として block
- 既存 codebase で false positive 多発しないこと (1〜2 PR で dogfood)

#### 詰まっている箇所

- 既存 codebase に 50 行超関数が多数残っている場合、段階的に warning → error のロールアウトが必要。事前調査を着手時に実施。
- multi-language 対応の実装複雑度: Rust と TS で AST 抽象が異なるため、別実装か共通 abstraction 層を選定する必要あり。

---

### `parse_findings` 系の error-path test infrastructure (PR #101 T2-1) ★ Bundle a Sub-PR 2

> **動機**: PR #101 で `run_list_findings` が `unwrap_or_else(|_| "[]")` で gh api 失敗を `[]` に潰していて CR Major finding を受けた。99.md でも `silent fail` (Windows path mismatch で early return) として類似言及あり。**`unwrap_or_else(|_| empty)` の anti-pattern が複数 PR で再発**。test 層で機械検証することで未然に塞ぐ。本タスクは Bundle a Sub-PR 2 (cli-pr-monitor の rate-limit auto-retry) で同 API を消費するので、同一 PR land で test 二重投資なし。
>
> **本タスクの位置づけ**: PR #101 post-merge-feedback Tier 2 #1 採用 (高頻度 anti-pattern finding)。Bundle a Sub-PR 2 (順位 42 / 43 / 46) と同 PR で land 推奨。CLAUDE.md `coding-style.md` "Never silently swallow errors" 原則の test 層実装。
>
> **参照**: `.claude/feedback-reports/101.md` Tier 2 #1、`.claude/feedback-reports/99.md`、`~/.claude/rules/common/coding-style.md` "Never silently swallow errors"
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。新 test ファイル + gh API モック。Sub-PR 2 と一体実装。

#### 設計決定 (案)

- **配置先**: `src/check-ci-coderabbit/tests/parse_error_handling_test.rs` (integration test、既存 unit test と分離)
- **テスト対象シナリオ**:
  - **gh API HTTP error 返却時**: `run_list_findings` がエラーを propagate するか verify (現状 PR #101 fix で `.map_err(...)?` 化済 → regression 防止)
  - **JSON 不正形式入力**: `serde_json::from_str` 失敗時の挙動 (現状 `unwrap_or_else(|e| { eprintln!(...); vec![] })` で warn は出すが空配列返却 = silent fall) — 望ましい設計を test で固定
  - **空 JSON `[]`**: 正常 path (空 findings 返却) の境界条件
- **モック戦略**:
  - gh API 直接モックは不要 (parse 関数は JSON string を受け取る純関数)
  - `run_gh` を trait 化して mock injection or `mockito` HTTP mock — Sub-PR 2 の cli-pr-monitor 実装方針と整合
- **既存 unit test との関係**: 既存 16 件は normal path 中心。本 task は error path 専用

#### 作業計画

- [ ] `src/check-ci-coderabbit/tests/` ディレクトリ作成 (現在 unit test only)
- [ ] gh API モック戦略の選定 (trait injection or shell wrapper stub) — Sub-PR 2 の cli-pr-monitor 実装方針と整合
- [ ] error-path シナリオ 3 件 (HTTP error / 不正 JSON / 空 JSON) を実装
- [ ] `cargo test --workspace` で pass 確認
- [ ] dogfood: 実 PR で `unwrap_or_else(|_| empty)` を一時的に書き戻して test が fail するか sensitivity 検証
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- `parse_listed_findings` / `parse_findings` の error-path 3 シナリオ test が pass
- `unwrap_or_else(|_| empty)` の silent fallback パターンが test で fail 検出される
- Sub-PR 2 の cli-pr-monitor 実装で同 mock infrastructure を流用できる

#### 詰まっている箇所

- gh API モック戦略の選定: HTTP mock library `mockito` vs `run_gh` の trait injection — 単純さ優先なら後者、real API 結合に近づけたいなら前者。
- `eprintln!` (stderr) を assert する仕組みが Rust 標準にないため、`gag::BufferRedirect` や custom logger 注入が必要 — 着手時に評価。

---

### comment-lint hook の scope を変更行に限定 (PR #102 T1-1)

> **動機**: PR #99 で導入した `hooks-post-tool-comment-lint-rust` はファイル全体を scan する設計のため、変更と無関係な pre-existing violations も flag する。PR #102 実装セッション中、`poll.rs` / `monitor.rs` / `feedback.rs` / `main.rs` への 1 行追加レベルの edit でも pre-existing 20 violations が毎回 flag され、scope creep への暗黙圧力 + token 消費の浪費が発生した (hook 出力が 12.6KB を毎回繰り返し)。
>
> **本タスクの位置づけ**: PR #102 post-merge-feedback Tier 1 #1 採用。**Rust 編集を伴う作業全般の効率改善**。`hooks-post-tool-comment-lint-rust` は `.rs` ファイルのみが対象のため Bundle Z Phase 2 / 3 (markdown / PowerShell / yaml 編集) は直接 block されないが、本タスク自体の実装 (Rust hook 改修) や他の Rust 編集 PR では引き続き 12.6KB の hook 出力が edit のたびに発生する。依存関係なし、いつでも単独着手可。
>
> **参照**: `.claude/feedback-reports/102.md` Tier 1 #1、PR #99 (Bundle Z Phase 1 — `src/hooks-post-tool-comment-lint-rust/`)、[docs/pipeline-token-efficiency.md](pipeline-token-efficiency.md) PR 2 (#B-β 制約付き fix instruction)
>
> **実行優先度**: 🚀 **Tier 1** — Effort M。Rust hook の violation collection ロジックに行 range filter を追加。

#### 設計決定 (案)

- **scope 変更方針 (2 案)**:
  - **案 A: 変更行のみ flag** — Edit / MultiEdit の `new_string` から行 range を計算し、その範囲内の comment のみ flag
  - **案 B: 変更ファイル全体を flag (現状維持) + pre-existing violations を baseline として ignore** — 初回 hook 起動時にファイル内の violation を baseline 化、以降は baseline 超過分のみ flag
- **推奨**: 案 A — シンプル。「変更ファイルだけど変更していない行」も無視され意図に近い。案 B は baseline ファイルの管理 (storage / invalidation) コストが高く、複数開発者環境で baseline drift する。
- **対象 tool**: PostToolUse の Edit / Write / MultiEdit。`Write` は新規ファイルなので全行が新規行扱い (= 全 violation flag)。
- **既存 marker は据え置き**: `ALLOWED_LINE_PREFIXES` / `ALLOWED_BLOCK_PREFIXES` (`// SAFETY:` / `// TODO:` 等) の判定はそのまま適用、scope 限定はその上位レイヤとして実装。

#### 作業計画

- [ ] Claude Code の PostToolUse hook 入力 schema を確認: Edit / Write / MultiEdit の `tool_input` に何が含まれるか (old_string / new_string / file_path 等)
- [ ] 変更行 range の計算ロジック実装 (new_string の行数 + Edit 位置から開始行特定、MultiEdit は edit list を順次適用して累積 range 計算)
- [ ] `src/hooks-post-tool-comment-lint-rust/src/main.rs` の violation collection に行 range filter を適用
- [ ] 単体 test 追加: 変更行外に違反コメントがあっても flag されないことを確認
- [ ] 単体 test 追加: 変更行内の新規違反は引き続き flag されることを確認
- [ ] dogfood: PR 1 セッションで観測した「無関係 file の 20 violations 問題」が再現しないことを確認
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) へ deploy (`pnpm build:hooks-post-tool-comment-lint-rust` + 派生側に exe コピー)
- [ ] 本 todo5.md エントリを削除

#### 完了基準

- 変更行外の pre-existing violations が flag されない
- 変更行内の新規 violations は引き続き flag される
- Bundle Z Phase 2 (#B-β) 着手時に hook scope 起因の不要 block が発生しない

#### 詰まっている箇所

- Claude Code hook の入力 schema に「変更前 file content」が含まれるか未確認 (Edit の `old_string` から探索すれば変更前ファイルでの該当行を特定できる想定。要 hook spec 調査)。
- MultiEdit の場合、複数 edit の `new_string` 行 range を順次累積する必要があり、実装複雑度がやや上がる。Phase 1 では Edit / Write のみ対応で MultiEdit は別タスクに切り出しても可。
