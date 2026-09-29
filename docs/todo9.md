# TODO (Part 9)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo8.md がファイルサイズ 60KB に到達したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する (PR #172 仕組み化方針切替セッション = 2026-05-25)。ほかの todo ファイル (現存する一覧は [docs/todo.md](todo.md) の preamble が正) の既存エントリは引き続き有効、相互に独立。**2026-06-06 分割**: 本ファイルが 75KB / 890 行に到達したため、PR-specific follow-up entries (順位 157, 160-173) を [docs/todo11.md](todo11.md) に分離。残置: 既存ルール仕組み化バンドル (順位 146-151) + 週次レビュー拡張 (順位 152-154)。新セッションでは25つすべてを確認すること (todo.md / todo3-28.md / todo-summary.md / todo-summary2.md / todo-summary3.md。todo2.md は 2026-08-12 退役)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### 既存ルール仕組み化バンドル — 6 件 (PR #172 仕組み化方針切替由来、2026-05-25 ユーザー判断採用)

本 section は PR #172 (順位 144 = `jj-message-required` preset) の hook 化 dogfood 成功事例を踏まえ、`~/.claude/rules/common/*.md` 内の既存ルールから機械強制可能な 6 件を仕組み化に切り替えるバンドルです。memory rule `feedback_pipeline_over_rules.md` の体系的適用で、session 毎の rule load コスト削減 + 別セッションでの結果一定化を実現します。

仕組み化後は対応する rule docs section を縮小または削除 (block message に集約) し、`~/.claude/rules/common/*.md` の総量を削減します。

---

### 順位 148: Test coverage 80% CI gate 追加 — `testing.md` § Minimum Test Coverage 80% 移管 (PR #172 仕組み化方針切替由来)

> **動機**: `~/.claude/rules/common/testing.md` § Minimum Test Coverage 80% は rule docs 記載のみで実行時 gate なし。`cargo llvm-cov --fail-under-lines 80` を pre-push step または CI step に追加することで、80% 未満 push を構造的に防止する。memory rule に頼らず実行時に gate を働かせることで session 跨ぎ品質一定化。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドル 3 件目。Effort S-M (CI 追加 + 既存カバレッジ実測 + 80% 未満なら段階導入計画)。
>
> **参照**: `~/.claude/rules/common/testing.md` § Minimum Test Coverage、`push-runner-config.toml` (新 step 追加候補)、`.github/workflows/` (未存在の場合 CI workflow 新設)、`cargo-llvm-cov` crate
>
> **実行優先度**: **Tier 2** — Effort S-M。実測カバレッジ次第で段階導入計画が必要 (現状未測定)。

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

### 順位 149: Long-running subprocess pipe truncate hook 拡張 — `development-workflow.md` § subprocess pipe truncate 禁止 移管 (PR #172 仕組み化方針切替由来)

> **動機**: `~/.claude/rules/common/development-workflow.md` § 長時間 subprocess pipe truncate 禁止 (PR #109 SIGPIPE 事故由来) は既存 `exe-help-block` preset で部分的に機械強制済。具体的には `cli-*.exe --help | head` 等を block する preset だが、`cli-merge-pipeline ... | head` のような副作用ある実 subprocess の出力 truncate は未カバー。本 task では `cli-*.exe ... | (head|tail|awk)` 等のパターン検出を拡張し、SIGPIPE リスクを完全構造化する。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドル 4 件目。既存 `exe-help-block` preset 拡張または新 `subprocess-pipe-truncate-block` preset 追加。
>
> **参照**: `~/.claude/rules/common/development-workflow.md` § 長時間 subprocess pipe truncate 禁止、`src/hooks-pre-tool-validate/src/main.rs` (`preset_exe_help_block` を template に拡張)、PR #109 SIGPIPE 事故 (ADR-030 root cause)
>
> **実行優先度**: **Tier 2** — Effort S。既存 preset 拡張 ~30 行 + test。

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

### 順位 150: Magic number lint 追加 — `coding-style.md` § Magic Numbers 移管 (PR #172 仕組み化方針切替由来、ユーザー判断 2026-05-25 = source folder 限定)

> **動機**: `~/.claude/rules/common/coding-style.md` § Magic Numbers の「Use named constants for meaningful thresholds, delays, and limits」は rule docs 記載のみで機械強制なし。ユーザー判断 (2026-05-25) で「**source folder のみ対象、test/config 除外**」方針確定。数値リテラル定数化を `src/**/*.rs` 等に paths filter 適用で検出する。
>
> **本タスクの位置づけ**: 既存ルール仕組み化バンドル 5 件目。順位 102 (Phase D D-3) で実装した `paths` filter (順位 118 で適用範囲検討中) を活用する custom lint rule。
>
> **参照**: `~/.claude/rules/common/coding-style.md` § Magic Numbers、`.claude/custom-lint-rules.toml` (新 rule 追加候補)、順位 102 paths filter 実装、順位 118 rule⑧ paths filter 適用範囲検討
>
> **実行優先度**: **Tier 2** — Effort M。custom lint rule 1 件追加 + paths filter design + test coverage 必要。

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

### 順位 152: todo entry 削除時の事前 land 確認手順 — 順位 136 hook 拡張 or 独立 follow-up (PR #173 T2-1 採用、2026-05-26)

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
> **実行優先度**: **Tier 2** — Effort XS-S。順位 136 に統合する場合は追加 ~15 行 (edit 種別判定 + delete branch)、独立 hook の場合は ~40 行 (構造的に分離)。

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

### 順位 153: `review-harness-whole` facet 追加 — 観点 ① 独立 facet 化 (ADR-031 weekly-review 拡張、Phase B+1、2026-05-26 ユーザー合意)

> **動機**: ADR-031 weekly-review (本採用 2026-06-01) の MVP は 3 facets (simplicity / security / architecture) 構成で start し、観点 ① ハーネス遵守 (rule < pipeline < hook 重複検出) は architecture-whole facet の prompt 重点 criteria として組込。dogfood で「① 観点が architecture-whole の他 criteria (ADR 整合性 / モジュール境界 / 命名規約 / 循環依存) と context 圧迫」が観測されたら、独立 facet `review-harness-whole` に extract する。
>
> **本タスクの位置づけ**: ADR-031 weekly-review 拡張、Phase B+1。本採用後の dogfood 結果 (2026-05-30 + 2026-06-01 観測時点で context 圧迫は未観測) を見てから着手判断 (extract 不要なら本 entry close)。順位 146-151 (Bundle 既存ルール仕組み化) の **継続的発見源** として機能し、新 rule → hook 昇格候補を週次で systemic に拾う構造を強化する。
>
> **参照**: ADR-031 (週次レビュー設計、本採用 2026-06-01)、順位 146-151 Bundle 既存ルール仕組み化、`feedback_no_unenforced_rules.md`、`feedback_pipeline_over_rules.md`
>
> **実行優先度**: **Tier 2** — Effort S。ADR-031 本採用後 (2026-06-01) のさらに 2-3 週 dogfood 後に着手判断。

#### 設計決定 (案)

- 配置: `.takt/facets/instructions/review-harness-whole.md` 新規 facet (allowed_tools: Read/Glob/Grep のみ)
- 観点: `~/.claude/rules/common/*.md` の各 rule を全文走査 + `.claude/custom-lint-rules.toml` / `.claude/hooks-config.toml` / `push-runner-config.toml` と突き合わせ → rule docs に記載があるが hook / pipeline 未実装の項目を finding として抽出
- aggregate-weekly 側で finding category `harness-rule-coverage-gap` として独立 group 化
- Phase B+1 着手判断条件: Phase B dogfood で architecture-whole の output から ① 観点 finding 数が多く他 criteria の finding 質が劣化、または ① 観点が見落とされていると観測された場合

#### 作業計画

- [ ] ADR-031 本採用 (2026-06-01) 後の 2-3 週 dogfood 運用 → ① 観点 finding の context 圧迫 / 見落としを観測
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

> **2026-06-06 分割**: 順位 157, 160, 161, 162, 163, 165, 170, 171, 172, 173 は [docs/todo11.md](todo11.md) を参照。

## 既知課題 (記録のみ、本セッションで未対応)

(現時点で本ファイルへの既知課題は無し。docs/todo8.md 末尾の post-merge-feedback workflow stale marker 問題を参照。)

---

## 旧 todo12.md から移した分 (2026-09-29 統合)

### 順位 178: `state.rs` の behavioral invariant test を ADR-041 pattern で追加 (週次レビュー 2026-05-30 S02 採用)

> **動機**: 週次レビュー WR-2026-05-30-S02 で検出。`src/cli-pr-monitor/src/state.rs:226-510` の test は JSON round-trip (serde 直列化 / 逆直列化) のみを検証し、**behavioral invariant** (例: `rate_limit` が `Some` の場合に `update_state_from_check_result()` が `ci` field を populate しない) を test していない。状態遷移 regression が test suite を通り抜ける構造的リスク。ADR-041 (Test Isolation Patterns for Multi-Condition Guards) で確立された「sentinel 事前投入 + mutation 不在を assert」pattern が本リポジトリの canonical 対策。
>
> **本タスクの位置づけ**: 週次レビュー (ADR-031) 2026-05-30 dogfood で採用 (severity=medium, facet=simplicity, category=test-anti-pattern、2026-05-30 ユーザー承認)。analyzer rationale: "Concrete, low-effort (add 3-5 tests), high value (catches state regression bugs). ADR-041 is already the project's documented pattern for this exact problem type."
>
> **参照**: `.claude/weekly-reviews/2026-05-30.md` § Findings、`src/cli-pr-monitor/src/state.rs:226-510` (既存 test、JSON round-trip のみ)、`docs/adr/adr-041-test-isolation-patterns.md` (適用 pattern source)
>
> **実行優先度**: **Tier 2** — Effort S。3-5 test 追加で済む、既存 ADR-041 pattern 流用。

#### 設計決定 (案)

- **対象 invariant 候補** (analyzer 提案 + 派生):
  1. `rate_limit` が `Some` 時、`update_state_from_check_result()` は `ci` field を更新しない
  2. `rate_limit.until_unix_secs` が過去時刻になった場合、次回 update で `rate_limit` が `None` に reset される (timer expiry)
  3. `notified` flag が `true` の場合、再度 `update_state_*` を呼んでも `notified` は維持される (idempotency)
  4. (実装側で発見し次第追加)
- **ADR-041 pattern 適用**:
  - 各 test variant は独立 setup (`memory feedback_test_dry_antipattern`)
  - sentinel value を事前投入 (`ci.overall = "MUTATION_CHECK_SENTINEL"` 等)、mutation が起こったか否かを明示的に assert
  - guard condition を partial に偽にする setup で「他 guard が真でも mutation が起こらないこと」を保証

#### 作業計画

- [ ] `src/cli-pr-monitor/src/state.rs:226-510` を Read で全件確認、現状の test スコープと不足 invariant を整理
- [ ] ADR-041 § Test Isolation Patterns を Read で再確認
- [ ] 3-5 behavioral invariant test を `#[cfg(test)]` mod に追加 (memory `feedback_test_dry_antipattern` 適用、各 variant 独立 setup)
- [ ] cargo test -p cli-pr-monitor で全 pass 確認
- [ ] mutation regression check: 意図的に invariant を破る変更 (例: rate_limit check を削除) を local で適用して新 test が落ちるか手動検証
- [ ] cargo clippy clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 3-5 behavioral invariant test が追加され全 pass
- silent regression を test で 1 件以上検出できる構造 (意図破壊で新 test 落ちる確認済)
- ADR-041 pattern の rationale を test コメントで cite (sentinel 事前投入 + mutation 不在 assert)

#### 詰まっている箇所

なし。Effort S、既存 pattern + 既存 test 構造への追加で完結。

---

### 順位 179: rate-limit retry decision boundary test を rstest parameterized で追加 (週次レビュー 2026-05-30 S03 採用)

> **動機**: 週次レビュー WR-2026-05-30-S03 で検出。`src/cli-pr-monitor/src/config.rs:94-122` の rate-limit retry logic + `stages/poll.rs` 周辺で、`max_retries=3` (固定値) のみが test されており **decision boundary** (`max_retries=0` で retry されない / `max_retries=1` で 1 回だけ retry / `max_retries=3` で boundary 通過後 `action_required` 遷移) が未検証。off-by-one error (`<` vs `<=`) が silent regression として通る構造的リスク。rstest crate は既に本リポジトリで使用済のため新 dep 不要。
>
> **本タスクの位置づけ**: 週次レビュー (ADR-031) 2026-05-30 dogfood で採用 (severity=medium, facet=simplicity, category=test-anti-pattern、2026-05-30 ユーザー承認)。Bundle CR-RL (順位 167-169) と隣接領域 (= rate-limit detection の周辺 logic) のため follow-up 価値高。analyzer rationale: "Low effort (rstest parameterized test, ~15 lines). rstest is already in use in the codebase. High value: catches the exact off-by-one class that single-value tests miss."
>
> **参照**: `.claude/weekly-reviews/2026-05-30.md` § Findings、`src/cli-pr-monitor/src/config.rs:94-122` (RateLimitConfig + max_retries field)、`src/cli-pr-monitor/src/stages/poll.rs` (retry 適用 site)、Bundle CR-RL (順位 167-169) の隣接 context、`feedback_test_dry_antipattern` 適用

#### 設計決定 (案)

- **rstest parameterized test 構造**:

  ```rust
  #[rstest]
  #[case(0, vec![])]                                      // max_retries=0: retry なし
  #[case(1, vec![true, false])]                           // max_retries=1: 1 retry 後 stop
  #[case(3, vec![true, true, true, false])]               // max_retries=3: full boundary coverage
  fn rate_limit_retry_boundary(#[case] max_retries: u32, #[case] expected_continues: Vec<bool>) {
      // setup + execution + assert
  }
  ```

- **boundary 観点**:
  - 0 retry: 最初の attempt の後 `action_required` 遷移を確認
  - max_retries 到達: 連続 retry 後の最終 attempt で `action_required` に遷移
  - off-by-one: `< max_retries` か `<= max_retries` かを test 経由で pin (実装の `<` を `<=` に変えると新 test が落ちる)

#### 作業計画

- [ ] `src/cli-pr-monitor/src/config.rs` の `RateLimitConfig::max_retries` 周辺 + `stages/poll.rs` の retry decision logic を Read で確認
- [ ] 既存 rstest 使用箇所を grep で確認、import / pattern を踏襲
- [ ] `#[cfg(test)]` mod に `rate_limit_retry_boundary` parameterized test を追加 (3-4 case)
- [ ] cargo test -p cli-pr-monitor で全 pass 確認
- [ ] off-by-one regression check: `<` を `<=` に意図的変更で test が落ちることを手動検証
- [ ] cargo clippy clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 3-4 parameterized case が全 pass
- off-by-one error が test で検出可能 (`<` ↔ `<=` mutation で test 落ちる)
- 既存単一値 test は維持 (backward compat)

#### 詰まっている箇所

なし。Effort S、rstest pattern 既存使用 + 約 15 行で完結。

---

### 順位 180: `lib-report-formatter` に markdown pipe / newline escape を追加 (週次レビュー 2026-05-30 C01 採用)

> **動機**: 週次レビュー WR-2026-05-30-C01 で検出。`src/lib-report-formatter/src/lib.rs:51-79` の `format_table()` が PR title / commit message を markdown table 行に直接埋め込むが、`|` / `\n` を escape していない。「`Fix | Critical | src/main.rs`」のような PR title が markdown table 構造を破壊し、**downstream AI facet が malformed row を Read 時に misinterpret する prompt injection リスク** が存在。PR title は外部 actor (= PR 作成者) が制御可能な input source のため defense-in-depth 重要。
>
> **本タスクの位置づけ**: 週次レビュー (ADR-031) 2026-05-30 dogfood で採用 (severity=medium, facet=security, category=prompt-injection、2026-05-30 ユーザー承認)。本セッションの 5 PR chain で AI facet 連鎖が systemic 化したため、prompt injection 防御層は今後の facet 拡張 (順位 153 / 154 等) でも継続価値あり。
>
> **参照**: `.claude/weekly-reviews/2026-05-30.md` § Findings、`src/lib-report-formatter/src/lib.rs:51-79` (format_table 実装)、`src/cli-merge-pipeline/src/feedback.rs:114-123` (PR title データ source)、ADR-022 (責務分離) との整合 (= utility は lib-* に集約)

#### 設計決定 (案)

- **`escape_markdown_pipe(s: &str) -> String`** ユーティリティを `lib-report-formatter` に追加:

  ```rust
  pub fn escape_markdown_pipe(input: &str) -> String {
      input.replace('|', "\\|").replace('\n', " ")
  }
  ```

- **call site 修正**: `format_table()` で user-controlled field (PR title / commit message / author) を embed する箇所に `escape_markdown_pipe()` を適用
- **test 追加** (memory `feedback_test_dry_antipattern` 適用、各 variant 独立):
  - 通常 ASCII (pipe / newline なし) → 変更なし
  - pipe 単独 (`a | b`) → `a \| b`
  - newline 単独 (`a\nb`) → `a b`
  - pipe + newline 混合
  - empty string

#### 作業計画

- [ ] `src/lib-report-formatter/src/lib.rs:51-79` を Read で `format_table()` 全体確認、user-controlled embed 箇所を特定
- [ ] `escape_markdown_pipe()` を lib に追加、pub export
- [ ] `format_table()` の embed call site を escape 経由に書き換え
- [ ] `#[cfg(test)]` に 5 variant test を独立 setup で追加
- [ ] cargo test + cargo clippy clean
- [ ] 受け先 (cli-merge-pipeline 等) で test がまだ pass することを cargo test --workspace で確認 (call signature 変更なしのため backward compat 維持)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `escape_markdown_pipe()` が `lib-report-formatter` に pub function として追加される
- 5 variant test が独立 pass
- `format_table()` の user-controlled embed が escape 経由になる
- markdown table 構造破壊 PR title (`Fix | Critical`) を fixture で渡しても table 整合維持を assert

#### 詰まっている箇所

なし。Effort S、5 行 utility + call site 修正 + 5 test で完結。

---

### 順位 182: `/weekly-review` skill に重複検出 (簡易 grep) を Phase 4 で追加 (Phase D dogfood D-B 採用)

> **動機**: Phase D dogfood (週次レビュー 2026-05-30 実行) で **WR-2026-05-30-S05 (`combine_output` dead-code) が既存 順位 173 (PR #182 dry-run S01 採用) と完全重複**であることを実観測。ADR-031 § Phase 4 で「**重複検出は MVP では実装しない**」と明示済だが、本 dogfood で「2 PR で同じ finding が出る」を実証したため、最低限の grep ベース簡易検出を後追い追加する妥当性が確立。MVP は description 先頭 40 chars の grep ヒットを警告表示するのみで、自動 merge は行わない (user 判断に委ねる ADR-031 原則維持)。
>
> **本タスクの位置づけ**: Phase D dogfood (本セッション 2026-05-30 実施) で observability gain (= 重複が見える) を実証、user 承認 (2026-05-30) で skill 拡張採用。Phase E 試験運用 dogfood の前に整備しておくと user 判断負荷を圧縮。
>
> **参照**: `~/.claude/skills/weekly-review/SKILL.md` Phase 4 (修正対象)、ADR-031 § todo.md 反映ルール (「重複検出は MVP では実装しない」記述、本タスクで「MVP+1」相当に拡張)、Phase D dogfood の実観測 (WR-2026-05-30-S05 ↔ 順位 173 重複)

#### 設計決定 (案)

- **簡易 grep 重複検出 in Phase 4**:

  ```bash
  # finding を docs/todo.md 系列に書き込む前に実行
  TITLE_PREFIX=$(echo "$finding_description" | head -c 40)
  HITS=$(grep -li "$TITLE_PREFIX" docs/todo.md docs/todo*.md 2>/dev/null)
  if [ -n "$HITS" ]; then
      # AskUserQuestion で「augment / 新規 / skip」を聞く
  fi
  ```

- **3 択 AskUserQuestion**:
  1. **augment**: 既存 entry に補足追記 (= 「重複 observation を別 dogfood で再確認、優先度上昇」記録)
  2. **新規**: 重複と認識した上で別 entry 化 (= scope or角度 が異なる場合)
  3. **skip**: 重複と認識して書き込まない (= 既存 entry で十分)
- **自動 merge は行わない**: ADR-031 原則の「重複検出は MVP では実装しない」(自動 merge は MVP 超過、observability のみ提供) は維持
- **grep target**: `docs/todo.md docs/todo2-11.md` 全件 (= 現在の todo file 集合、新 todoN+1.md 追加時は SKILL.md update が必要)

#### 作業計画

- [ ] `~/.claude/skills/weekly-review/SKILL.md` Phase 4 § 重複検出 (簡易) を expansion: 現状の grep + 警告のみ → 警告 + 3 択 AskUserQuestion に変更
- [ ] grep target file 列を docs/todo*.md glob 化、追加ファイル時の自動追従 (固定 list 回避)
- [ ] dogfood 観測の cite を skill 内 inline で明示 (= Phase D 2026-05-30 で WR-2026-05-30-S05 ↔ 順位 173 重複検出済、本 logic が機能した実例)
- [ ] **`feedback_global_config_backup` 適用**: ~/.claude/skills/ 編集前 snapshot 取得
- [ ] markdownlint clean
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc 等) への skill 配布判断は別タスク (本 skill は global 配置だが ADR-031 自体は本リポジトリ ADR、派生展開は要検討)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- skill Phase 4 で grep 重複検出 → 3 択 AskUserQuestion → user 判断 経路が実装される
- 次回 `/weekly-review` 実行で重複候補が user 提示される dogfood 観測
- ADR-031 「重複検出 MVP 未実装」を「MVP+1 (簡易 grep)」相当に格上げ、但し自動 merge なし原則は維持
- skill 編集前後の ~/.claude snapshot が backup される

#### 詰まっている箇所

なし。Effort XS-S、SKILL.md の Phase 4 section 拡張 + Bash snippet 追加で完結。

---

### 順位 194: development-workflow.md 「1. Plan First」に「task 着手前に grep で既存 section 確認」step 追記 (PR #196 T3-5 採用)

> **動機**: PR #196 pre-push reviewer OBS-1 で「tasks 191/192 が既実装 sections を再度計画対象としていた」と指摘 (実態は cleanup diff の誤読だが、similar pattern は PR #123 でも観測済で Frequency Medium)。task 計画段階で「対象 section が既に global rules / ADR に存在するか `grep` で確認する」step を `~/.claude/rules/common/development-workflow.md` "1. Plan First" に追記し、後続 task 計画時の redundant 提案を構造的に予防する。
>
> **本タスクの位置づけ**: PR #196 post-merge-feedback Tier 3 #5 採用 (Severity Medium / Frequency Medium / Effort XS / Adoption Risk None、2026-06-05 ユーザー承認)。development-workflow.md への 1-2 行追記のみ、派生プロジェクト (techbook-ledger / auto-review-fix-vc) に global rule として自動波及。
>
> **参照**: `.claude/feedback-reports/196.md` Tier 3 #5、PR #196 pre-push OBS-1 (`.takt/runs/20260605-054100-pre-push-review/`)、PR #123 同型事象 (analyzer report 内 cite)、memory `feedback_global_config_backup` (snapshot 必須)
>
> **実行優先度**: **Tier 3** — Effort XS。global rule への 1-2 行追記。

#### 設計決定 (案)

`~/.claude/rules/common/development-workflow.md` の **Feature Implementation Workflow** "1. Plan First" sub-step に以下を追記:

```markdown
- **Codification 重複の事前確認**: 計画段階で「対象 section が既に global rules (`~/.claude/rules/common/*.md`) / ADR (`docs/adr/*.md`) / 既存 docs に存在するか」を `grep -n` で必ず確認する。重複追加は reviewer 混乱 + global rule の冗長化を招く。確認手順:
  - `grep -rn "<section title>" ~/.claude/rules/common/ docs/adr/ docs/`
  - hit があれば既存 codification を読み、追記 vs 新規 vs skip を判断
  - 由来: PR #123 / PR #196 で同型「既実装 section の重複計画」事象を観測
```

- **適用範囲**: 「ADR/global rule への新規 section codify」を含む全 task 計画
- **派生プロジェクト波及**: `~/.claude/rules/common/` 配下のため techbook-ledger / auto-review-fix-vc に自動

#### 作業計画

- [ ] `~/.claude` snapshot 取得 (memory `feedback_global_config_backup` per)
- [ ] `~/.claude/rules/common/development-workflow.md` Feature Implementation Workflow "1. Plan First" sub-step に上記項目を追加
- [ ] PR #123 + #196 を実例として inline cite
- [ ] markdownlint clean
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- development-workflow.md "1. Plan First" に Codification 重複確認 step が追記
- 派生プロジェクトに global rule として波及
- 由来 cite (PR #123, #196) で reviewer / Claude が rule 背景を理解可能

#### 詰まっている箇所

なし。Effort XS、docs 編集のみ。

---
