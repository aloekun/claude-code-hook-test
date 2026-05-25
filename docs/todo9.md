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
  - 推奨: 案 A (本リポジトリは takt ベース push-runner で gate 統一済、`.github/workflows/` は未存在で順位 96 で初導入予定)
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

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo8.md 末尾の post-merge-feedback workflow stale marker 問題を参照。)
