# TODO (Part 9)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo8.md がファイルサイズ 60KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #172 仕組み化方針切替セッション = 2026-05-25)。todo.md / todo2.md 〜 todo8.md の既存エントリは引き続き有効、相互に独立。新セッションでは十つすべてを確認すること (todo.md / todo2-9.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### 既存ルール仕組み化バンドル — 6 件 (PR #172 仕組み化方針切替由来、2026-05-25 ユーザー判断採用)

本 section は PR #172 (順位 144 = `jj-message-required` preset) の hook 化 dogfood 成功事例を踏まえ、`~/.claude/rules/common/*.md` 内の既存ルールから機械強制可能な 6 件を仕組み化に切り替えるバンドルです。memory rule `feedback_pipeline_over_rules.md` の体系的適用で、session 毎の rule load コスト削減 + 別セッションでの結果一定化を実現します。

仕組み化後は対応する rule docs section を縮小または削除 (block message に集約) し、`~/.claude/rules/common/*.md` の総量を削減します。

---

### Secret detection PreToolUse hook 追加 — AWS/OpenAI/GitHub token 等の hardcoded secret 検出 (PR #172 仕組み化方針切替由来、`security.md` § Secret Management 移管)

> **動機**: `~/.claude/rules/common/security.md` § Secret Management の「NEVER hardcode secrets in source code」は現在 rule docs 記載のみで機械強制なし。session 毎に security.md を読み込まないと AI が rule を解釈しない構造的脆弱性が残る。PreToolUse hook で Edit/Write 時に AWS key / OpenAI key / GitHub token 等の regex 検出を行い、即 block + feedback を返すことで漏洩を構造的に防止する (ユーザー判断 2026-05-25 = PreToolUse hook 方式採用)。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドルの第 1 件。順位 144 (`jj-message-required`) と同型実装パターン。`feedback_pipeline_over_rules.md` 適用 = パイプライン側機械的修正で Claude 判断介入を排除。
>
> **参照**: `~/.claude/rules/common/security.md` § Secret Management、`src/hooks-pre-tool-validate/src/main.rs` (`preset_jj_message_required` を template に追加)、`.claude/hooks-config.toml`、PR #172 (順位 144 hook 化 dogfood)
>
> **実行優先度**: 🚀 **Tier 1** — Effort M。security-critical かつ漏洩観測前の preventive 層。

#### 設計決定 (案)

- **配置**: `src/hooks-pre-tool-validate/src/main.rs` に新 preset `secret-detection` 追加
- **検出対象 regex** (高頻度 secret pattern):
  - AWS Access Key: `AKIA[0-9A-Z]{16}`
  - AWS Secret Key: `aws_secret_access_key\s*=\s*[A-Za-z0-9/+=]{40}`
  - OpenAI API Key: `sk-[A-Za-z0-9]{20,}` (現 sk-proj 系を含む形式)
  - GitHub Personal Access Token: `ghp_[A-Za-z0-9]{36}` / `github_pat_[A-Za-z0-9_]{20,}`
  - GitHub OAuth Token: `gho_[A-Za-z0-9]{36}` / `ghs_[A-Za-z0-9]{36}`
  - Anthropic API Key: `sk-ant-[A-Za-z0-9_-]{20,}`
  - 汎用高エントロピー string (要 false positive 評価): `[A-Za-z0-9+/]{40,}={0,2}` (base64-like) は対象外とする (汎用過ぎる)
- **exception field 不使用**: secret pattern に正当な使用例はない (test fixture は dummy で十分)
- **block message**: 「機密情報が検出されました。環境変数 / secret manager に移管してください」+ 検出 pattern type
- **hooks-config.toml**: `blocked_patterns` に `"secret-detection"` 追加 (opt-in 設計だが Tier 1 のため default 推奨)

#### 作業計画 (順位 144 と同 phase 構造)

- [ ] Phase 1: `preset_secret_detection()` 関数を実装 (6-8 種の BlockedPattern を vec で返す)
- [ ] Phase 2: `build_blocked_patterns` の `resolve_preset_or_custom` dispatch に登録 + `.claude/hooks-config.toml` の `blocked_patterns` に追加 + コメント section 説明追加
- [ ] Phase 3: test 拡充 — block ケース (6+ 種類の secret pattern) × allow ケース (regular code) × non-regression
- [ ] Phase 4: `pnpm build:hooks-pre-tool-validate` で exe deploy + dogfood (dummy AWS key 等で block 動作確認)
- [ ] Phase 5: `pnpm push` + `pnpm create-pr`
- [ ] post-merge: 派生プロジェクト deploy + `~/.claude/rules/common/security.md` § Secret Management の hook 化記述追加 (rule docs 縮小は別 follow-up)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 6+ 種類の高頻度 secret pattern が Edit/Write 時に block される
- regular code (variable name "key" / "secret" の使用、test fixture の dummy "AKIATEST...") は通過
- 既存 preset と non-regression
- `cargo test -p hooks-pre-tool-validate` pass
- security.md § Secret Management から具体 pattern 列挙を hook block message に移管 (docs 縮小)

#### 詰まっている箇所

- false positive リスク: API key 形式の文字列が test fixture / 説明文に登場する可能性。test fixture は paths filter 除外で対応 (順位 150 magic number lint と同 pattern)
- pattern 漏れ: 検出対象 6-8 種類は主要のみ。Anthropic API key 形式変更 / 新 service token 追加時は手動更新が必要 (feedback loop)

---

### File length lint (800 行 max) 追加 — `coding-style.md` § File Organization 移管 (PR #172 仕組み化方針切替由来)

> **動機**: `~/.claude/rules/common/coding-style.md` § File Organization の「200-400 lines typical, 800 max per file」ガイドラインは現在 rule docs 記載のみで、機械強制されていない。順位 48 (関数長 50 行) は `hooks-post-tool-comment-lint-rust` で touch-trigger ratchet 方式により既に機械強制済の前例があり、ファイルサイズも同 pattern で実装可能。session 毎の rule load コスト削減 + 800 行突破時の編集時即 block を実現する。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドル 2 件目。順位 48 (関数長) と同 pattern で工数把握済。touch-trigger ratchet 適用で既存超過ファイルを編集時のみ flag (grandfather)、新規 800 行超え発生を block。
>
> **参照**: `~/.claude/rules/common/coding-style.md` § File Organization、`src/hooks-post-tool-comment-lint-rust/src/main.rs` (`find_function_length_violations` を template に file length 版を追加)、順位 48 PR #101 T1-4 実装
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。順位 48 同 pattern で ~50 行 + test。

#### 設計決定 (案)

- **配置**: `src/hooks-post-tool-comment-lint-rust/src/main.rs` に `find_file_length_violations` を追加
- **閾値**: `MAX_FILE_LINES = 800` (constant 定義、coding-style.md と同期)
- **touch-trigger ratchet**: 既存 800 行超ファイルは触られた時のみ flag (関数長 ratchet と同 pattern)
- **対象拡張子**: Rust (`.rs`) のみ最初は対象、将来 TS/Py 拡張は別 task
- **MAX_VIOLATIONS との関係**: 既存 `collect_all_violations` の truncate に乗せる (順位 57 contract test 適用済)
- **block message**: 「ファイル長 N 行 > 上限 800 行 (coding-style.md File Organization)」+ 分割提案

#### 作業計画

- [ ] `find_file_length_violations` 関数を実装 (`source.lines().count()` + line_filter 整合チェック)
- [ ] `collect_all_violations` から呼び出し追加 (順位 57 truncate contract 維持)
- [ ] test 拡充: 800 行未満 (no violation) / 800 行ちょうど (no violation) / 801 行 (violation) / 既存 1000 行ファイル + line_filter touch (violation) / 既存超過 + no touch (grandfather)
- [ ] `pnpm build:hooks-post-tool-comment-lint-rust` で exe deploy + dogfood
- [ ] `~/.claude/rules/common/coding-style.md` § File Organization の縮小 (= block message に集約、rule docs から具体閾値を削除)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 800 行超ファイル編集時に block + 分割提案 feedback
- 既存超過ファイルの未編集箇所は touch-trigger で grandfather (false positive なし)
- 順位 57 truncate contract test pass
- coding-style.md § File Organization 縮小

#### 詰まっている箇所

- TS / Py 拡張: 本 task は Rust 限定。多言語対応は別 hook (`hooks-post-tool-linter` 系) で実装する場合は別 task に分離
- 800 行は coding-style.md 記載値。CLAUDE.md (project) の「200-400 lines typical」とは整合 (typical/max の 2 段階)

---

### Test coverage 80% CI gate 追加 — `testing.md` § Minimum Test Coverage 80% 移管 (PR #172 仕組み化方針切替由来)

> **動機**: `~/.claude/rules/common/testing.md` § Minimum Test Coverage 80% は rule docs 記載のみで実行時 gate なし。`cargo llvm-cov --fail-under-lines 80` を pre-push step または CI step に追加することで、80% 未満 push を構造的に防止する。memory rule に頼らず実行時に gate を働かせることで session 跨ぎ品質一定化。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドル 3 件目。Effort S-M (CI 追加 + 既存カバレッジ実測 + 80% 未満なら段階導入計画)。
>
> **参照**: `~/.claude/rules/common/testing.md` § Minimum Test Coverage、`push-runner-config.toml` (新 step 追加候補)、`.github/workflows/` (未存在の場合 CI workflow 新設)、`cargo-llvm-cov` crate
>
> **実行優先度**: 🔧 **Tier 2** — Effort S-M。実測カバレッジ次第で段階導入計画が必要 (現状未測定)。

#### 設計決定 (案)

- **配置方式の選択** (実装時判断):
  - 案 A: `push-runner-config.toml` の `[quality_gate]` に coverage step 追加 (pre-push 時に gate)
  - 案 B: `.github/workflows/coverage.yml` 新設 (CI 時に gate)
  - 推奨: 案 A (本リポジトリは takt ベース push-runner で gate 統一済、`.github/workflows/` は未存在、docs 整合性も cli-docs-lint で push-runner 配下に統合済)
- **ツール**: `cargo llvm-cov --fail-under-lines 80` (workspace 全体)
- **段階導入**: 現状実測カバレッジが 80% 未満の crate がある場合、crate 別閾値設定 or temporary exception
- **rule docs 縮小**: testing.md § 「Minimum Test Coverage: 80%」は実行時 gate 化により「ガイドライン」記述を削除可能

#### 作業計画

- [ ] 全 crate の現状カバレッジを実測 (`cargo llvm-cov` で workspace 全体)
- [ ] 80% 未満の crate があれば段階導入計画 (現状値を temporary baseline、増分対象を明示)
- [ ] 案 A/B 選択 (推奨: 案 A、push-runner-config.toml [quality_gate] に integration)
- [ ] `push-runner-config.toml` または `.github/workflows/coverage.yml` に gate step 追加
- [ ] dogfood: 1-2 PR で gate 動作確認 (80% 切る変更で block される)
- [ ] `~/.claude/rules/common/testing.md` § 80% coverage 記述を実行時 gate に置換 (rule docs 縮小)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- workspace 全体カバレッジが gate で実行時検証される
- 80% 未満 push が block される (or warning で reviewer 判断、段階導入次第)
- testing.md § 80% coverage は実行時 gate への参照のみ残す形に縮小

#### 詰まっている箇所

- 現状カバレッジ未測定。実装着手前に実測 + baseline 設定が必要
- 段階導入の影響範囲: 既存 PR workflow が一時的に gate failure になるリスク。段階閾値 (50% → 60% → 70% → 80%) 設計が必要かもしれない

---

### Long-running subprocess pipe truncate hook 拡張 — `development-workflow.md` § subprocess pipe truncate 禁止 移管 (PR #172 仕組み化方針切替由来)

> **動機**: `~/.claude/rules/common/development-workflow.md` § 長時間 subprocess pipe truncate 禁止 (PR #109 SIGPIPE 事故由来) は既存 `exe-help-block` preset で部分的に機械強制済。具体的には `cli-*.exe --help | head` 等を block する preset だが、`cli-merge-pipeline ... | head` のような副作用ある実 subprocess の出力 truncate は未カバー。本 task では `cli-*.exe ... | (head|tail|awk)` 等のパターン検出を拡張し、SIGPIPE リスクを完全構造化する。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドル 4 件目。既存 `exe-help-block` preset 拡張または新 `subprocess-pipe-truncate-block` preset 追加。
>
> **参照**: `~/.claude/rules/common/development-workflow.md` § 長時間 subprocess pipe truncate 禁止、`src/hooks-pre-tool-validate/src/main.rs` (`preset_exe_help_block` を template に拡張)、PR #109 SIGPIPE 事故 (ADR-030 root cause)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。既存 preset 拡張 ~30 行 + test。

#### 設計決定 (案)

- **拡張 vs 新 preset**:
  - 案 A: 既存 `exe-help-block` preset に pipe truncate 検出を追加 (1 preset で 2 機能、命名 misleading)
  - 案 B: 新 `subprocess-pipe-truncate-block` preset 追加 (preset 命名整合)
  - 推奨: 案 B (preset の単一責任原則、関係 rule docs と命名整合)
- **block pattern**:
  - `cli-*.exe ... | (head|tail|awk)` 系: `(cli-[\w-]+|hooks-[\w-]+|check-ci-[\w-]+)\.exe\s+[^|]*\|\s*(head|tail|awk\b)`
  - `gh api ... | head` 系 (rate-limit 中 risk): 順位 44 (gh-token-efficiency) と重複するため scope 重複回避を判断
  - `pnpm push | head` / `pnpm merge-pr | tail` 系: pnpm scripts も同型リスク
- **exception field**: `--jq` / `--json` 経由の structured 抽出は allow (順位 44 と整合)
- **block message**: 「長時間 subprocess の pipe truncate は SIGPIPE で中断される (ADR-030 PR #109 事故の根本原因)。`run_in_background: true` + `--jq` 抽出 / `> /dev/null` 破棄 を推奨」

#### 作業計画

- [ ] 既存 `preset_exe_help_block` のロジック分析 + 拡張 vs 新 preset 決定
- [ ] block pattern 実装 (cli-/hooks-/check-ci- 系 exe + pipe truncate)
- [ ] pnpm scripts カバー範囲決定 (pnpm push/merge-pr/create-pr 等の truncate も block するか)
- [ ] exception field で正当な短命確認系 (`ls -la | head -10` 等) を allow
- [ ] test 拡充: block ケース 5+ / allow ケース 5+ / 既存 exe-help-block との non-regression
- [ ] `pnpm build:hooks-pre-tool-validate` で exe deploy + dogfood
- [ ] `~/.claude/rules/common/development-workflow.md` § 該当 section 縮小 (具体的禁止パターンを hook block message に集約)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 副作用ある cli-*.exe 出力 truncate が block される (SIGPIPE 事故再発防止)
- 順位 44 (gh-token-efficiency) との scope 重複が整理される
- 既存 `exe-help-block` preset と non-regression
- development-workflow.md § 長時間 subprocess pipe truncate 禁止 を hook 化記述に縮小

#### 詰まっている箇所

- pnpm scripts のカバー範囲判断: `pnpm push | head` 等の truncate も block するか (= scope D の wrapper 制限と整合する判断必要)
- 順位 44 との scope 重複整理: `gh api ... | head` は順位 44 で扱い、本 task は cli-*.exe / pnpm scripts に限定する境界明示

---

### Magic number lint 追加 — `coding-style.md` § Magic Numbers 移管 (PR #172 仕組み化方針切替由来、ユーザー判断 2026-05-25 = source folder 限定)

> **動機**: `~/.claude/rules/common/coding-style.md` § Magic Numbers の「Use named constants for meaningful thresholds, delays, and limits」は rule docs 記載のみで機械強制なし。ユーザー判断 (2026-05-25) で「**source folder のみ対象、test/config 除外**」方針確定。数値リテラル定数化を `src/**/*.rs` 等に paths filter 適用で検出する。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドル 5 件目。順位 102 (Phase D D-3) で実装した `paths` filter (順位 118 で適用範囲検討中) を活用する custom lint rule。
>
> **参照**: `~/.claude/rules/common/coding-style.md` § Magic Numbers、`.claude/custom-lint-rules.toml` (新 rule 追加候補)、順位 102 paths filter 実装、順位 118 rule⑧ paths filter 適用範囲検討
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。custom lint rule 1 件追加 + paths filter design + test coverage 必要。

#### 設計決定 (案)

- **配置**: `.claude/custom-lint-rules.toml` に新 rule `no-magic-number` 追加
- **検出 pattern (案、要 dogfood 調整)**:
  - 関数 body 内の bare integer literal (regex で 限定的に検出、要試行錯誤):
    - 時間定数 candidate: `\b(1000|60|3600|86400)\b` (millisecond / minute / hour / day)
    - リトライ回数 candidate: `\b(3|5|10)\s*[;,)]` の文脈付き検出
    - 閾値 candidate: 関数 argument / 比較演算子付きの hardcoded number
  - 要試行錯誤: 全 integer literal を flag すると false positive 過多、特定 idiom (時間定数 / リトライ回数 / threshold) に絞る
- **paths filter** (ユーザー判断: source folder のみ):
  - `paths = ["src/**/*.rs", "src/**/*.ts", "src/**/*.py"]` 等
  - **除外**: `src/**/tests/**`、`src/**/*.test.*`、`src/**/test_*.rs`、`*.config.*`、`.claude/**`、`docs/**`
- **severity**: warning (false positive リスクのため block しない、reviewer 判断補助)
- **exception**: 関数内で `const` / `let` で名前付き定義済の値は対象外 (regex で前方検索)

#### 作業計画

- [ ] 検出 pattern 設計 (時間定数 / リトライ回数 / threshold の 3 category で MVP)
- [ ] paths filter 設計 (source folder 限定、test/config 除外)
- [ ] `.claude/custom-lint-rules.toml` に rule 追加 + `[rules.test_coverage]` meta field 設定 (testing.md § Custom Lint Rule Test Coverage 適用)
- [ ] test 拡充: positive (時間定数 hardcoded) / negative (定数化済 / test fixture / config) / paths filter 動作確認
- [ ] dogfood: 1-2 PR で false positive 観測 → pattern 調整
- [ ] `~/.claude/rules/common/coding-style.md` § Magic Numbers 削除可否判断 (lint で十分カバーされたら docs 縮小)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- source folder の hardcoded 数値リテラル (時間定数等) が warning として検出される
- test fixture / config / docs は false positive なし
- `[rules.test_coverage]` meta field で positive/negative test の存在が cargo test で検証される
- coding-style.md § Magic Numbers 削除 (lint rule の存在で代替) or 縮小

#### 詰まっている箇所

- pattern 設計の試行錯誤: bare integer literal の全検出は false positive 過多、idiom 限定が現実的だが取りこぼしリスク
- 既存 source code で hardcoded 数値が残存している場合、initial run で大量 warning 発生する可能性 → touch-trigger ratchet 必要か再評価

---

### PR diff lines check 追加 — `git-workflow.md` § Multi-PR chaining 移管 (PR #172 仕組み化方針切替由来、ユーザー判断 2026-05-25 = 条件付き block 3 段階)

> **動機**: `~/.claude/rules/common/git-workflow.md` § Multi-PR chaining の「1 PR あたり 250-800 lines」ガイドラインは rule docs 記載のみ。ユーザー判断 (2026-05-25) で「**条件付き block 3 段階: > 1500 block / 800-1500 warning / < 800 通過、threshold は config 化**」方針確定。pre-push step で line count を check し、巨大 PR を構造的に抑制する。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドル 6 件目。`push-runner-config.toml` に新 `[pr_size_check]` section を追加し、threshold を config 化することで大型 refactoring 時の override も config 経由で柔軟に対応。
>
> **参照**: `~/.claude/rules/common/git-workflow.md` § Multi-PR chaining、`src/cli-push-runner/src/` (新 stage 追加候補)、`push-runner-config.toml`、PR #119/#120/#121 (250-800 lines/PR ベストプラクティス実証)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。push-runner に新 stage 追加 ~50 行 + config schema + test。

#### 設計決定 (案)

- **配置**: `src/cli-push-runner/src/stages/` に新 stage `pr_size_check.rs` 追加
- **計測対象**: `jj diff -r 'master..@' --stat` の line count 合計 (additions + deletions)
- **3 段階閾値**:
  - `block_threshold` (default 1500): 超過時 push を block + 分割推奨 feedback
  - `warning_threshold` (default 800): 超過時 warning 出力 + push 続行
  - 800 未満: 通過、ログにのみ出力
- **config schema** (`push-runner-config.toml` の新 section):
  ```toml
  [pr_size_check]
  enabled = true
  block_threshold = 1500
  warning_threshold = 800
  # 大型 refactoring 時の override: false にして特定 PR で skip 可能
  ```
- **opt-in 設計**: 既存 push-runner-config.toml に section がない場合は default 値で動作 (= enabled、threshold default)
- **派生プロジェクト transferability**: config schema で threshold 調整可能、プロジェクト規模に応じて変更可

#### 作業計画

- [ ] `src/cli-push-runner/src/config.rs` に `PrSizeCheckConfig` struct 追加 (`enabled` / `block_threshold` / `warning_threshold`)
- [ ] `src/cli-push-runner/src/stages/pr_size_check.rs` 新 stage 実装 (jj diff stat 計測 + 3 段階判定)
- [ ] `src/cli-push-runner/src/stages/mod.rs` で export + `runner.rs` の stage ordering に挿入 (quality_gate 後 / push 前)
- [ ] `push-runner-config.toml` に `[pr_size_check]` section デフォルト設定追加
- [ ] test 拡充: line count 計測精度 / 3 段階判定 / config parse / opt-in 動作
- [ ] dogfood: 本 task PR (推定 ~400 行) で通過、過去 PR (PR #119 sub-PR 200 行 / PR #146 ~600 行) で warning 閾値検証
- [ ] `~/.claude/rules/common/git-workflow.md` § Multi-PR chaining を実行時 gate 参照に縮小
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- pre-push 時に PR line count が計測され 3 段階判定される
- block_threshold 超過時に push block (config で threshold 変更可能)
- warning_threshold 超過時に warning 出力 + 続行
- config schema が `push-runner-config.toml` の `toml::from_str` test でカバーされる (順位 91 を template、ただし opt-in classification は 順位 145 と整合)
- git-workflow.md § Multi-PR chaining 縮小

#### 詰まっている箇所

- jj diff stat の解析: `jj diff --stat` の出力 format が version 依存しないか確認必要 (ADR-017 jj version pin 適用範囲)
- 大型 refactoring 時の override 方法: `enabled = false` (config 編集) vs CLI flag (`--skip-size-check`)。前者推奨だが PR 単位 override は config 編集だけだとセッション横断で漏れるリスク

---

### todo entry 削除時の事前 land 確認手順 — 順位 136 hook 拡張 or 独立 follow-up (PR #173 T2-1 採用、2026-05-26)

> **動機**: PR #173 で land 済 entry (順位 125 / 139 / 141) を todo8.md から削除した際、削除前の land 状態確認は実装 grep ベースの「事後 verify」で実施し全て land 確認できたが、「事前確認」の機械強制はなかった。post-merge-feedback analyzer (T2-1) で「rank 125 / 141 の actual land status を `jj log` で確認、未実装なら todo に復帰」採用判定が成立 (Severity Medium / Frequency Low / Effort XS / Adoption Risk None)。今回 false alarm (実装は全 land 済) だったが、将来「削除前に land 確認」を機械強制すれば誤削除を構造的に防止できる。
>
> **本タスクの位置づけ**: 順位 136 (working copy staleness hook + stale todo entry 既実装 grep 提示) と **同型の機械強制タスク**、lifecycle 補完関係:
>
> - 順位 136: **add / edit 時**に既実装の commit を grep 提示 (= 「既に実装済では?」warning)
> - **本タスク (順位 152)**: **delete 時** に対応 land commit を grep 検証 (= 「本当に land 済?」warning)
>
> 順位 136 hook 実装時に統合検討 (= 同一 PreToolUse hook で add/edit/delete の edit 種別を判定して分岐)、または独立 hook (= shared utility 経由) で別 task 化のいずれか。ADR-042 § Decision matrix 適用 = **mechanizable + FP 低 + Adoption Risk None** で仕組み化 zone。
>
> **参照**: `.claude/feedback-reports/173.md` Tier 2 #1、順位 136 entry (本ファイル内)、PR #173 セッションで実施した実装 grep 検証 (rank 125 = `run_custom_rules_line_number_correct_with_multibyte_content` test 存在 / rank 139 = `docs/adr/adr-041-test-isolation-patterns.md` 存在 / rank 141 = `fix_push_time` + `RATE_LIMIT_BUT_MERGEABLE` シグナル存在)、ADR-042 (rule vs mechanism boundary)、memory `feedback_pipeline_over_rules.md`
>
> **実行優先度**: 🔧 **Tier 2** — Effort XS-S。順位 136 に統合する場合は追加 ~15 行 (edit 種別判定 + delete branch)、独立 hook の場合は ~40 行 (構造的に分離)。

#### 設計決定 (案)

- **検出条件**: `docs/todo*.md` への Edit/Write で `### 順位 N ` セクション (or `### <title>` headed entry) が削除されたパターン
  - Edit tool の `old_string` に `### ` で始まる entry header が含まれ、`new_string` に含まれない場合 = 削除と判定
  - Write tool で全文書き換えの場合は old/new file の `### ` header 数を比較
- **動作**: 削除対象 entry の keyword (見出し title から抽出、順位 prefix / 句読点 除去) を `jj log --limit 30` で grep
- **判定**:
  - 関連 commit (= 「順位 N land」「PR #XXX」「<keyword> land 済」等の description) を検出 → 削除を **allow** + 検出 commit を additional context に出力 (削除証跡として残る)
  - 関連 commit なし → **warning** (block ではなく feedback) + 「削除前に land 確認推奨。defer / withdraw の場合は commit message に明記推奨」を出力
- **scope**: 順位 136 hook (PreToolUse on docs/todo*.md edit) に統合する case が推奨。共通の `jj log` grep utility を共有
- **block vs warning 設計判断**: AI が大量 land 済 entry を一括削除するケース (本 PR #173 でも 3 件削除) を考慮し、warning にとどめる。block にすると mass cleanup PR で UX 阻害

#### 作業計画

- [ ] 順位 136 hook 実装時に edit 種別判定ロジック (add / edit / delete) を含める設計検討
- [ ] delete 検出: `old_string` に `### ` entry header あり / `new_string` になし pattern
- [ ] keyword 抽出 (順位 prefix / 句読点 除去) + `jj log --limit 30` grep
- [ ] 結果出力フォーマット (land 確認時 = additional context に commit 列挙 / 未確認時 = warning)
- [ ] test fixture (4 ケース): delete + land あり / delete + land なし / add + 既実装あり / add + 既実装なし
- [ ] 派生プロジェクト deploy 検討 (順位 136 と同タイミング)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `docs/todo*.md` への delete 操作時、対応 land commit が grep で検出されれば allow + 証跡 output
- land commit なし時は warning (block しない) で AI に再確認を促す
- 順位 136 (add/edit 時 既実装 grep) と統合 or 独立で lifecycle カバレッジ完成
- 派生プロジェクト transferability

#### 詰まっている箇所

- **edit 種別判定の複雑性**: Edit tool の old/new 比較で削除を判定可能だが、部分削除 + 他箇所改修の混在 edit で false negative リスク。最小単位は「順位 N entry 全体の削除」のみ対象とする MVP が現実的
- **keyword 抽出の精度**: 順位 prefix 除去後の title 残りで grep するが、title に表記揺れ (例: "ADR-041 Test Isolation Patterns" vs "Test Isolation Patterns ADR") があると false negative。順位 N をそのまま grep する case も併用検討
- **mass cleanup PR との両立**: 本 PR #173 のように 3 件以上の land 済 entry を一括削除する PR では各削除で warning が累積し UX 阻害。1 PR 内で同 file の delete N 件目以降は output 抑制 等の noise 軽減策必要

---

### `review-harness-whole` facet 追加 — 観点 ① 独立 facet 化 (順位 8 follow-up、Phase B+1、2026-05-26 ユーザー合意)

> **動機**: 順位 8 (週次レビュー Phase B) の MVP は 3 facets (simplicity / security / architecture) 構成で start し、観点 ① ハーネス遵守 (rule < pipeline < hook 重複検出) は architecture-whole facet の prompt 重点 criteria として組込。Phase B dogfood で「① 観点が architecture-whole の他 criteria (ADR 整合性 / モジュール境界 / 命名規約 / 循環依存) と context 圧迫」が観測されたら、独立 facet `review-harness-whole` に extract する。
>
> **本タスクの位置づけ**: 順位 8 の follow-up、Phase B+1。Phase B dogfood 結果を見てから着手判断 (extract 不要なら本 entry close)。順位 146-151 (Bundle 既存ルール仕組み化) の **継続的発見源** として機能し、新 rule → hook 昇格候補を週次で systemic に拾う構造を強化する。
>
> **参照**: 順位 8 entry (todo.md 「7 観点責務 mapping」表)、順位 146-151 Bundle 既存ルール仕組み化、`feedback_no_unenforced_rules.md`、`feedback_pipeline_over_rules.md`、ADR-031 (週次レビュー設計)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。順位 8 Phase B land + 2-3 週 dogfood 後に着手判断。

#### 設計決定 (案)

- 配置: `.takt/facets/instructions/review-harness-whole.md` 新規 facet (allowed_tools: Read/Glob/Grep のみ)
- 観点: `~/.claude/rules/common/*.md` の各 rule を全文走査 + `.claude/custom-lint-rules.toml` / `.claude/hooks-config.toml` / `push-runner-config.toml` と突き合わせ → rule docs に記載があるが hook / pipeline 未実装の項目を finding として抽出
- aggregate-weekly 側で finding category `harness-rule-coverage-gap` として独立 group 化
- Phase B+1 着手判断条件: Phase B dogfood で architecture-whole の output から ① 観点 finding 数が多く他 criteria の finding 質が劣化、または ① 観点が見落とされていると観測された場合

#### 作業計画

- [ ] Phase B (順位 8) land + 2-3 週 dogfood 運用 → ① 観点 finding の context 圧迫 / 見落としを観測
- [ ] facet extract 判断 (extract 不要なら本 entry close)
- [ ] `review-harness-whole.md` instruction 設計 (順位 146-151 land 済 / 未済の状況を踏まえた rule-vs-hook gap 検出ロジック)
- [ ] takt workflow weekly-review.yaml に facet 追加 + `parallel:` block 拡張
- [ ] aggregate-weekly facet 拡張 (新 category) + pending JSON schema 拡張
- [ ] dogfood + 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ① ハーネス遵守 観点が独立 facet で週次検出される
- architecture-whole は ADR 整合性 / モジュール境界 / 命名規約 / 循環依存 に集中
- 新規 rule 追加時の hook 昇格候補が systemic に提案される

#### 詰まっている箇所

- Phase B dogfood 結果次第 (extract 不要なら本 entry close)
- ① 観点と ② docs 内整合性の境界判断 (rule docs 整合 vs その他 docs 整合の cross-cut)

---

### `review-todo-whole` facet + aggregate 前 file size pre-step — 観点 ⑤ ⑦ 拡張 (順位 8 follow-up、Phase B+1、2026-05-26 ユーザー合意)

> **動機**: 順位 8 (週次レビュー Phase B) の MVP では観点 ⑤ Todo 妥当性 は順位 136 (todo hook 2 段構え) に委譲し、観点 ⑦ ファイルサイズ も対象外とした。順位 136 hook land 後、hook が拾えない broad な観点 (全 todo entry 横断の dead pattern 検出 / cross-todo file の重複 entry / docs/todo*.md preamble drift) を週次の `review-todo-whole` facet で補完する。並行して観点 ⑦ ファイルサイズ (50KB / 800 行) は aggregate-weekly facet 直前の Rust 機械 pre-step で計測し、LLM context を浪費せず ADR-031 の 3 層分離 (Rust 機械 / takt AI / skill ask) に整合させる。
>
> **本タスクの位置づけ**: 順位 8 の follow-up、Phase B+1。順位 136 hook land 後に着手判断 (= hook の immediate guard が機能している前提で、週次は batch 棚卸しに focus)。`feedback_pipeline_over_rules.md` 適用で、機械検査可能な観点 (file size) を LLM facet に乗せず分離する設計。
>
> **参照**: 順位 8 entry (todo.md 「7 観点責務 mapping」表)、順位 136 entry (todo8.md、todo hook 2 段構え)、cli-docs-lint (preamble file count + cross-ref、push-runner lint group 統合済)、順位 147 (file length lint 800 行)、ADR-031 (3 層分離 = Rust 機械 / takt AI / skill ask)、`feedback_pipeline_over_rules.md`
>
> **実行優先度**: 🔧 **Tier 2** — Effort M (facet 新規 + Rust pre-step ~80 行)。順位 136 land + Phase B 2-3 週 dogfood 完了後に着手。

#### 設計決定 (案)

**`review-todo-whole` facet (観点 ⑤ 補完):**

- 配置: `.takt/facets/instructions/review-todo-whole.md` 新規 facet (allowed_tools: Read/Glob/Grep のみ)
- 観点: 全 todo*.md entry を横断走査 → dead pattern (= 半年以上 stale + 関連 commit なし + 依存 task land 済) / cross-file 重複 entry / preamble routing drift を finding として抽出
- 順位 136 hook が拾えない範囲: 編集していない entry の経年劣化 / file 跨ぎの重複 / preamble file count drift

**aggregate 前 Rust 機械 pre-step (観点 ⑦):**

- 配置: takt workflow weekly-review.yaml の aggregate-weekly facet 直前に新 step 追加 (or aggregate facet 自身が呼び出す Rust binary)
- 計測対象:
  - `docs/todo*.md` の file size (50KB 閾値、PR #88 / #96 / #101 / #123 / #172 で実証された分割 trigger)
  - `src/**/*.rs` の line count (800 行閾値、順位 147 file length lint と整合)
- 出力: 閾値超過 / 接近 (90% 等) のファイル一覧を aggregate facet の入力として渡す
- 機械検査のため LLM context を浪費しない (ADR-031 3 層分離原則)

#### 作業計画

- [ ] 順位 136 hook land 待ち
- [ ] Phase B 2-3 週 dogfood 完了 + 観点 ⑤ ⑦ の必要性再評価 (cli-docs-lint / 順位 147 land 状況も確認)
- [ ] `review-todo-whole.md` instruction 設計 (順位 136 hook が拾える範囲との境界明示)
- [ ] aggregate 前 Rust pre-step 実装 (新 binary `cli-weekly-review-prep` or aggregate facet 内 step)
- [ ] takt workflow weekly-review.yaml に facet + pre-step 追加
- [ ] aggregate-weekly facet 拡張 (新 category) + pending JSON schema 拡張
- [ ] dogfood + 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 全 todo*.md entry の dead pattern / cross-file 重複 / preamble drift が週次検出される
- file size 閾値超過 / 接近が aggregate facet input として通知される
- 順位 136 hook と責務分離 (hook = 編集時 immediate / 週次 = batch 棚卸し) が機能

#### 詰まっている箇所

- 順位 136 hook 実装次第 (hook が拾える範囲が確定後に週次の補完範囲を確定)
- Phase B dogfood 結果次第 (有用な finding が出るかは運用観察)
- cli-docs-lint (preamble count、push-runner lint group 統合済) との scope 重複整理: push-runner = 機械検査即時 / 週次 pre-step = aggregate 入力、両立可能だが integration 検討

---

### cli-pr-monitor fix chain 末尾に空 commit 検査 + `jj abandon` step を追加 (PR #174 T1-#1 採用)

> **動機**: PR #174 で post-pr-monitor の `CleanupEmptyFixCommit` action 後に、別の空 commit (`kqvluqyv`) が祖父コミット位置に残存し、後続の Bundle 1 Minor fix push 時に PR diff を汚染する事象を観測。cleanup ロジックが「fix chain で直近 create された空 commit」のみ対象にしており、過去の空 commit を見逃す構造的欠陥が明らかになった。手動 `jj abandon` で 1 件解消したが、機械強制すべき。
>
> **本タスクの位置づけ**: PR #174 post-merge-feedback Tier 1 #1 採用 (Severity Medium / Frequency Low / Effort S / Adoption Risk None)。cli-pr-monitor の cleanup phase に「`jj log --no-graph` で空 description の commit を検出 → 全て abandon」step を追加し、空 commit による PR diff 汚染を構造的に予防する。
>
> **参照**: `.claude/feedback-reports/174.md` Tier 1 #1、PR #174 で観測した `kqvluqyv` 事例 (Bundle 1 fix loop 中に手動 abandon)、`src/cli-pr-monitor/src/`
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。cli-pr-monitor fix chain への追加 step 1 件、機械強制で重複事故を防止。

#### 設計決定 (案)

- 配置: `src/cli-pr-monitor/src/` の fix chain cleanup phase 末尾 (既存 `CleanupEmptyFixCommit` の後)
- 動作:
  1. `jj log -r 'master..@' --no-graph -T 'change_id ++ "\u{1f}" ++ if(empty, "EMPTY", "CONTENT") ++ "\n"'` で PR 範囲 commit を列挙 (`empty` は jj template の commit 自体が空か判定する keyword)
  2. 各行を `\u{1f}` (Unit Separator) で分割し、2 列目が `EMPTY` の commit を filter
  3. 該当 commit を `jj abandon <change_id>` で順次 abandon
  - 注意: `description.first_line()` は description の 1 行目を返すため「全 description 空」と「複数行 description で 1 行目だけ空」を区別できない。実装では jj template の `empty` keyword (= commit が file change を含まないか) を直接使うか、`if(description, "DESCRIBED", "UNDESCRIBED")` で description 有無を判定する設計に固定する
- scope 限定: `master..@` 範囲のみ (= PR に含まれる範囲)。master 以下は対象外
- 既存 `CleanupEmptyFixCommit` との関係: 既存は直近 fix commit のみ対象、本 step は全範囲 sweep の補完層
- fail-open: jj log / abandon の失敗時は warning ログのみで cleanup を継続 (push を block しない)

#### 作業計画

- [ ] cli-pr-monitor の cleanup phase 実装箇所を特定 (`CleanupEmptyFixCommit` action の呼び出し元)
- [ ] 空 commit 列挙ロジック (jj log + description filter) を追加
- [ ] abandon ループ + error handling 実装
- [ ] test 拡充: 空 commit 0 件 / 1 件 / 複数件 / 非空 commit のみ / mixed
- [ ] `pnpm build:cli-pr-monitor` で release 生成 + dogfood (次の PR で同様の状況を作って動作確認)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- post-pr-monitor の cleanup phase 完了時に PR 範囲内の空 commit が全て abandon される
- 既存 `CleanupEmptyFixCommit` action と non-regression
- dogfood で空 commit 自動 cleanup が動作確認される

#### 詰まっている箇所

なし。Effort S、既存 cleanup phase への追加 step で副作用最小。

---

### Bundle 1 dogfood checklist 実行 — `__test.ps1` block + override env 確認 (PR #174 T2-#2 採用、ADR-039 bounded lifetime data point #1)

> **動機**: PR #174 で実装した `scratch_file_warning` stage は ADR-039 § 3 Bounded lifetime 準拠で「3-5 PR の dogfood 後に default-ON 昇格 or 却下を判定」する設計。PR #174 の PR body に未消化の dogfood checklist が残っており (`__test.ps1` を意図的に作って push し block 動作確認 / override env でバイパス確認)、これが ADR-039 bounded lifetime の初回データポイント。次の PR (Bundle 2 等) merge 前の前提条件として消化が必要。
>
> **本タスクの位置づけ**: PR #174 post-merge-feedback Tier 2 #2 採用 (Severity Low / Frequency Low / Effort XS / Adoption Risk None)。manual operation で完結、Bundle 1 自身の運用検証 + ADR-039 bounded lifetime 体系の初回稼働確認。
>
> **参照**: `.claude/feedback-reports/174.md` Tier 2 #2、PR #174 PR body の Test Plan unchecked items、`docs/adr/adr-039-experimental-feature-standard-pattern.md` § 3 Bounded lifetime、`src/cli-push-runner/src/stages/scratch_file_warning.rs` (`SCRATCH_FILE_WARNING_OVERRIDE` env)
>
> **実行優先度**: 🔧 **Tier 2** — Effort XS。手動 dogfood 1 セット、~10 分。

#### 設計決定 (案)

- 手順:
  1. ローカル working dir に `__test_dummy.ps1` (or `.txt`) を作成 (中身は無害な dummy)
  2. `jj describe -m "test: scratch hook dogfood"` 等で commit
  3. `pnpm push` を実行 → scratch_file_warning stage が block する (EXIT_SCRATCH_FILE_WARNING = 6) を確認
  4. `$env:SCRATCH_FILE_WARNING_OVERRIDE = "1"; pnpm push` で override → 通過確認
  5. dogfood 完了後、`__test_dummy.ps1` ファイル削除 + commit abandon で working dir clean
- 記録: dogfood 結果 (block message / override 動作 / false positive 有無) を Bundle 2 PR body に「ADR-039 bounded lifetime data point #1」として記載
- 注意: 本 dogfood は本リポジトリで実施。派生プロジェクトへの deploy 後の dogfood は別タスク (派生プロジェクト側の bounded lifetime data point として記録)

#### 作業計画

- [ ] `__test_dummy.ps1` を working dir に作成
- [ ] `jj describe + pnpm push` で block 動作確認
- [ ] `$env:SCRATCH_FILE_WARNING_OVERRIDE = "1"; pnpm push` で override 動作確認
- [ ] cleanup: `__test_dummy.ps1` 削除 + commit abandon
- [ ] 結果を Bundle 2 PR body に記録
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- block 動作: scratch_file_warning stage が `__test_dummy.ps1` を検出し EXIT 6 で push を block する
- override 動作: env var 設定後に同 stage を通過、push が成功する
- ADR-039 bounded lifetime data point #1 が記録される

#### 詰まっている箇所

なし。Effort XS、manual operation で完結。

---

### docs-governance.md に「ADR multi-variant pattern section 追加時の checklist」を codify (PR #176 T3-#1 採用)

> **動機**: PR #175 (Minor: variant 網羅性不足) + PR #176 (Nitpick: 擬似コード vs 実コード齟齬) の 2 連続観測で、ADR の multi-variant pattern section を追加する際の「参照実装リスト完全性」「実装コード例の表記精度」取りこぼしが pattern 化された。本 PR #176 で追加した ADR-041 § State Preservation Invariant section が CR Nitpick を受けた事例も同パターン。Frequency Medium (2 観測) + Effort XS で採用条件成立。
>
> **本タスクの位置づけ**: PR #176 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。`~/.claude/rules/common/docs-governance.md` に 5-8 行 checklist を追記、ADR 拡張 PR の reviewer / Claude が逆引きで参照できる reusable rule に昇格。`feedback_no_unenforced_rules.md` 例外 = 2 PR で実証 + ADR 形式 (= 設計判断 doc) への追加で機械強制不要、reviewer の judgment 補助。
>
> **参照**: `.claude/feedback-reports/176.md` Tier 3 #1、PR #175 CR Minor finding 1 件、PR #176 CR Nitpick 1 件、`~/.claude/rules/common/docs-governance.md` (global rule、本リポジトリ外)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。global rule への 5-8 行追記、本リポジトリ外 (`~/.claude/`) ファイル編集。

#### 設計決定 (案)

- **配置**: `~/.claude/rules/common/docs-governance.md` の document lifecycle classification 周辺、もしくは新 section "ADR Multi-Variant Pattern Authoring Checklist"
- **追記内容案** (5-8 行 checklist):
  - ADR に multi-variant pattern (variant 1/2/3 等の列挙) section を追加する場合:
    1. **参照実装リストの完全性**: 各 variant に対応する参照実装 (test 関数 or 実装関数) を 1 件以上 cite。variant が言及されているのに参照実装が無い (例: variant 2 だけ書いて test が無い) ことを避ける
    2. **実装コード例の表記精度**: コード例が擬似コード (簡略化) か実コード (literal copy) かを明示。擬似コードなら「(概念)」「(簡略化)」等のマーカーを付け、実コードならパスと行番号を cite (`poll.rs:839-842` 等)
    3. **既存資料との関係**: 該当 ADR の「既存資料との関係」section に cross-link を追加
  - 由来: PR #175 (variant 網羅性不足、Minor) + PR #176 (擬似コード vs 実コード齟齬、Nitpick) の 2 連続観測
- **派生プロジェクト transferability**: global rule のため本リポジトリで合意した内容は派生プロジェクトにも自動波及 (本 PR で `~/.claude/` 配下を直接編集する必要がある制約)

#### 作業計画

- [ ] memory `feedback_global_config_backup` 適用でバックアップ取得 (`~/.claude/rules/common/docs-governance.md` を `.backup-YYYYMMDD` 等で snapshot)
- [ ] `~/.claude/rules/common/docs-governance.md` に checklist 5-8 行を新 section "ADR Multi-Variant Pattern Authoring Checklist" として追記
- [ ] PR #175 / PR #176 を実例 cite として 1-line 引用
- [ ] markdownlint clean 確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `docs-governance.md` に ADR multi-variant pattern checklist が明文化される
- 将来の ADR 拡張 PR で variant 網羅性 + 表記精度の取りこぼしが reviewer 視点で防止される
- PR #175 / PR #176 が実例として reverse-lookup 可能

#### 詰まっている箇所

- 本タスクは `~/.claude/` 配下 (本リポジトリ外) のため、repo PR には含められない。実装は別途グローバル設定編集として実施
- バックアップ要 (memory `feedback_global_config_backup` 適用)

---

### Subprocess timeout+kill lifecycle 検証テスト追加 (PR #177 T2-#1 採用)

> **動機**: PR #177 で CR Major #2 「`run_jj_with_timeout` が timeout 後に jj 子プロセスを kill しない」を fix push したが、修正の正当性 (child process が timeout 到達時に確実に terminate される) を OS レベルで assert する回帰テストが現在ゼロ。fix は `spawn()` + `try_wait()` polling + timeout 時 `kill()` + `wait()` に書き換えたが、テストなしでは将来の変更で同型 leak 再導入が silent regression する。
>
> **本タスクの位置づけ**: PR #177 post-merge-feedback Tier 2 #1 採用 (Severity High / Frequency Medium / Effort M / Adoption Risk None)。Major fix の回帰テスト + 今後の hook 実装で subprocess timeout pattern を使う際の reference test。Severity High = subprocess リーク (resource leak) は debug 困難な silent failure mode。Frequency Medium = 2 hook ファイル (hooks-session-start / hooks-pre-tool-validate) で同一 pattern 確認済、今後の hook 実装でも反復見込み。
>
> **参照**: `.claude/feedback-reports/177.md` Tier 2 #1、PR #177 CR Major finding (id 3309140888 hooks-session-start / 関連 fix in hooks-pre-tool-validate)、`src/hooks-session-start/src/main.rs` `run_jj_with_timeout` / `src/hooks-pre-tool-validate/src/main.rs` `run_jj_with_timeout` (両方が同一 pattern)
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。両 hook test module で integration test 風の subprocess lifecycle 検証 (~80-120 行 + helper)。

#### 設計決定 (案)

- **対象 helper**: `run_jj_with_timeout` (両 hook で実装、ADR-024 で shared lib 統合候補)
- **検証内容**:
  1. **正常完了 case**: jj コマンドが timeout 内に完了 → output が返る、child は `try_wait` で reaped 済
  2. **timeout case**: 意図的に slow command (例: `jj log` で巨大 revset / 存在しない remote への `git fetch`) → timeout 到達 → kill 発火 → child が is_finished 状態に遷移していることを assert
  3. **kill 後の resource cleanup**: kill 後 `wait()` で zombie 化していないことを assert (Unix では `waitpid` で確認、Windows では `Child::id()` の OS handle が closed か)
- **テスト fixture**:
  - `Child::is_finished()` (Rust 1.18+) で kill 後の状態確認
  - `Command::new("sleep")` or `Command::new("cmd")` `/c "ping -n 100 127.0.0.1 > NUL"` (Windows) で意図的 slow command
  - timeout は短く (~500ms) して test 全体を 1-2 秒で完結
- **OS 依存性**: Windows / Linux 両対応のため `#[cfg(target_os = ...)]` で fixture を分ける、または `jj log` で確実に時間がかかる revset を使う方式に統一
- **配置**: 両 hook の `#[cfg(test)] mod tests` 内 + 共通 helper を `tests/common/mod.rs` 等に切り出す検討
- **memory `feedback_test_dry_antipattern.md`**: 各 test は独立 fixture で記述 (DRY 適用しない)

#### 作業計画

- [ ] `Child::is_finished` (or `wait_timeout`) で lifecycle 検証手段を確定
- [ ] hooks-session-start / hooks-pre-tool-validate の `run_jj_with_timeout` test module に 3 case 追加
- [ ] OS 依存 fixture (slow command) を Windows / Linux で動作確認
- [ ] dogfood: 意図的に timeout を踏ませる test を CI で安定して走らせられるか確認 (flaky test 回避)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 両 hook の `run_jj_with_timeout` で timeout 後の child kill + cleanup が OS レベルで検証される
- 同型 leak の silent regression が future PR で検出可能
- ADR-024 (shared jj helpers library) 統合時に test も統合対象として再評価可能な構造

#### 詰まっている箇所

- OS 依存性: Windows の subprocess lifecycle API (`is_finished`) と Linux の `waitpid` で挙動差異あり。`Child::is_finished` (stable 1.78+) が両 OS 対応で推奨
- flaky test 回避: timeout を踏ませる test は CI 環境の jitter で flaky 化リスク、500ms ~ 1s の余裕を持つ調整必要

---

### fail-closed error path (Option::None) 個別テスト追加 (PR #177 T2-#2 採用)

> **動機**: PR #177 の CR Major #1 「`check_todo_staleness` / `build_todo_staleness_message` が `behind.unwrap_or(0) > 0` で None を non-stale 扱いし fail-closed をバイパス」については現状コード (`src/hooks-pre-tool-validate/src/main.rs:796, 846-849`) で `check_todo_staleness` 側が依然 `behind.unwrap_or(0) > 0` のまま gate バイパスの可能性が残り、`build_todo_staleness_message` 側は `if behind.is_none() { return None; }` で early return しているが回帰テスト不在。本タスクは **実装側 fix (unwrap_or → map_or(true, ...) への修正)** + **回帰テスト追加** の両方を scope に含める。security gate 関数 (Option 返値 + jj 呼び出し) の error path 検証は今後の hook でも反復必要。
>
> **本タスクの位置づけ**: PR #177 post-merge-feedback Tier 2 #2 採用 (Severity High / Frequency Medium / Effort S / Adoption Risk None)。Major fix の回帰テスト + security gate pattern の standard reference。Severity High = fail-closed バイパスは silent security 退化。Frequency Medium = security gate + Option return pattern は今後の hooks でも反復適用見込み。
>
> **参照**: `.claude/feedback-reports/177.md` Tier 2 #2、PR #177 CR Major finding (id 3309140878)、`src/hooks-pre-tool-validate/src/main.rs` の `check_todo_staleness` / `build_todo_staleness_message` / `count_commits_branch_ahead`
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。test module への追加 ~30-50 行、unit test で独立検証可能。

#### 設計決定 (案)

- **対象 function**: `check_todo_staleness` (fail-closed 判定)、`build_todo_staleness_message` (None ケース message 出力)
- **実装側 fix (本 PR で同時に land)**:
  - `check_todo_staleness` line 796: `behind.unwrap_or(0) > 0` → `behind.map_or(true, |n| n > 0)` (None を stale=true として fail-closed 化)
  - `build_todo_staleness_message` line 846-849: 現状 `if behind.is_none() { return None; }` で early return しているが、明示的な fail-closed message を返す形に変更検討 (caller が None を「メッセージ無し」と非 stale 解釈しないよう調整)
- **検証 case** (memory `feedback_test_dry_antipattern.md` 適用、各 variant 独立 fixture):
  1. **`check_todo_staleness_returns_stale_when_lineage_none`**: `count_commits_branch_ahead` mock で None を返すよう注入 → result.stale = true、message に「lineage 判定不能」を含む
  2. **`build_todo_staleness_message_none_behind_marks_stale`**: `behind = None` で msg を生成 → "fail-closed で block" 文言を含む
  3. **`check_todo_staleness_normal_paths_unchanged`**: behind = Some(0) / Some(3) で従来通り動作 (regression 防止)
- **mock 戦略**: `count_commits_branch_ahead` は jj 実行依存のため、function を引数で受け取る形に refactor or test 専用 stub を導入。簡易には `count_commits_branch_ahead` を `pub(crate)` で公開し、test で別ロジック (constant None / Some(n) を返す closure) を builder で渡す pattern
- **回帰検出**: 将来 `map_or(true, ...)` を `unwrap_or(0)` 等に戻す変更で test が failing する構造を確保
- **memory `feedback_test_dry_antipattern.md`**: 各 case は独立 setup (mock 値別)、共通 helper 化しない

#### 作業計画

- [ ] `check_todo_staleness` を mock 注入可能な形に minor refactor (or test 専用 stub 追加)
- [ ] 3 case の unit test 追加
- [ ] cargo test で pass 確認 + 意図的に fail-closed 削除して test が落ちることを手動検証
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `check_todo_staleness` / `build_todo_staleness_message` の None ケース挙動 (fail-closed) が unit test で independent 検証
- 将来 `map_or(true, ...)` を逆向きに変更した時に 1 test が落ちる構造
- security gate + Option return pattern の test reference として hook 実装者が参照可能

#### 詰まっている箇所

- mock 注入 vs 簡易 stub の trade-off: dependency injection で全 hook で reusable にするか、test 専用 closure で local 化するか。後者 (local stub) のが Effort S で確実
- function signature 変更の影響範囲: `check_todo_staleness` を refactor すると call site (main.rs handle_write_edit_tool) も追従必要。最小 diff 優先で stub closure 内 mock 推奨

---

### Cross-ref edge case test coverage 追加 (PR #179 T2-#1 採用)

> **動機**: PR #179 で cli-docs-lint の cross_ref validator を新規実装し push-runner quality_gate に統合したが、percent-encode (`%20` / `%23`)、GFM heading slug、relative path normalize (`../`) の各 variant が fixture テストで明示的に保護されていない。validator のロジック劣化を silent regression として放置するリスクがある。
>
> **本タスクの位置づけ**: PR #179 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Low / Effort S / Adoption Risk None、2026-05-28 ユーザー承認)。cross_ref validator の edge case coverage 拡充による silent regression 防止。
>
> **参照**: `.claude/feedback-reports/179.md` Tier 2 #1、`src/cli-docs-lint/src/cross_ref.rs` (既存 9 tests に追加)、PR #179 (cli-docs-lint 本体 land)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。既存 tests と同 pattern で fixture 追加。

#### 設計決定 (案)

- **対象 edge case**:
  1. **percent-encode**: 日本語 file name の percent-encode (例: `%20` 空白、`%E3...` UTF-8) を含む link を resolve できるか
  2. **GFM heading slug**: heading anchor (`#section-with-spaces` 等) の小文字化 / 空白→`-` 変換が GFM 仕様に従うか
  3. **relative path normalize**: 多段 `../` を含む link (例: docs/ から 2 階層上 root → 別 path) を正しく resolve できるか (現状の base_dir.join + canonicalize 経路)
- **fixture pattern**: 既存 cross_ref.rs の `#[cfg(test)]` mod 内の tempdir + 動的 fixture 生成 pattern を踏襲
- **memory `feedback_test_dry_antipattern`**: 各 variant 独立 setup、共通 helper 化しない

> NOTE: 本 entry の編集時に edge case の link 例を Markdown link 形式 (角括弧 + 丸括弧) で書くと、cli-docs-lint の cross_ref validator が backtick 内 link も誤検出する (= 本 entry land 時に発覚した false positive)。validator 自体の backtick-aware 化も本 entry 着手時に検討余地あり (現状は description + 拡張子のみで回避)。

#### 作業計画

- [ ] `src/cli-docs-lint/src/cross_ref.rs` の `#[cfg(test)]` mod に 3 case の fixture test を追加
- [ ] cargo test で pass 確認 + 意図的に validator から正規化ロジックを抜いて test が落ちるか手動検証
- [ ] (任意) validator の backtick-aware 化 (inline code 内の link を無視) を本 entry に同梱検討
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 3 edge case (percent-encode / GFM heading slug / relative path normalize) が unit test で independent 検証
- silent regression を test で 1 件以上検出できる構造
- 既存 9 tests と整合性を保つ

#### 詰まっている箇所

なし。Effort S、cli-docs-lint 内のみで完結。

---

### ADR-039 kill-switch standard pattern に「診断メッセージは実装の受理値を網羅」原則追記 (PR #179 T3-#1 採用)

> **動機**: PR #179 で `cli-docs-lint` の kill-switch を実装した際、`is_kill_switch_value` は `"1"` / `"true"` / `"TRUE"` / `"True"` の 4 受理値を持つが、SKIP 時の診断メッセージは `"{}=1 detected"` 固定で実受理値を反映しなかった (spec-impl drift)。pre-push simplicity reviewer から non-blocking finding として指摘。ADR-039 は全 experimental feature の kill-switch 実装テンプレートとして参照されるため、原則を明文化しないと次の experimental feature 実装で同パターンが再発する systemic reach がある。
>
> **本タスクの位置づけ**: PR #179 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None、2026-05-28 ユーザー承認)。ADR-039 を全 experimental feature の参照 source にする方針のため、Frequency Medium 判定で採用条件成立。
>
> **参照**: `.claude/feedback-reports/179.md` Tier 3 #1、`docs/adr/adr-039-experimental-feature-standard-pattern.md` (§ 決定 2. Kill-switch)、PR #179 の `src/cli-docs-lint/src/main.rs` の `is_kill_switch_value` + SKIP message 実装例、PR #179 simplicity reviewer の non-blocking finding

#### 設計決定 (案)

ADR-039 § 決定 2 (Kill-switch) に以下の原則を追記:

- **診断メッセージは受理値を網羅**: kill-switch 発動時の出力メッセージは、`is_*_value` 等の判定関数が受理する全 value variant を反映する。固定文字列 (例: `"=1 detected"`) ではなく、(a) 全受理値を列挙 (例: `"=1 (or =true) detected"`) または (b) 実際の env var 値を動的取得して表示 (例: `format!("{}={} detected", env_name, raw_value)`) のいずれかを採用する
- **理由**: spec-impl drift (判定ロジックは複数値受理、メッセージは 1 値のみ表記) は user が誤解する診断 UX 低下、かつ ADR-039 はテンプレートとして参照されるため全 experimental feature に波及する

#### 作業計画

- [ ] `docs/adr/adr-039-experimental-feature-standard-pattern.md` の § 決定 2 (Kill-switch) に上記原則を 2-3 行追記
- [ ] PR #179 を実例として inline cite (「`CLI_DOCS_LINT_DISABLE` で発生した spec-impl drift」)
- [ ] markdownlint clean 確認
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ADR-039 § Kill-switch に診断メッセージ網羅原則が codify される
- 次の experimental feature 実装時に reviewer / Claude が原則から逆引き可能になる
- markdownlint clean

#### 詰まっている箇所

なし。Effort XS、ADR の section 追記のみで副作用最小。

---

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo8.md 末尾の post-merge-feedback workflow stale marker 問題を参照。)
