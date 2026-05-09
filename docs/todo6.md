# TODO (Part 6)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo5.md がファイルサイズ 50KB を超過したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo7.md の既存エントリは引き続き有効、相互に独立。新セッションでは八つすべてを確認すること (todo.md / todo2-7.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### Experimental feature の標準パターン codify (PR #123 T3-1 採用) ★ Bundle h

> **動機**: PR #123 (ADR-038 Phase 5: P-0 classifier opt-in + §10 ブランチ分離運用) で採用された運用 pattern が、既存の試験運用 ADR (ADR-031 週次レビュー / ADR-036 Bundle Z / ADR-038 ローカル LLM 等) と systemic に反復するパターンであることを post-merge-feedback で観測。3 点セット (config opt-in + kill-switch + bounded lifetime) を標準化することで、今後の試験運用導入で再利用可能なテンプレートとなる。
>
> **本タスクの位置づけ**: PR #123 post-merge-feedback Tier 3 #1 採用 (Frequency Medium / Effort XS / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/123.md` Tier 3 #1、ADR-031 (週次レビュー、試験運用)、ADR-036 (Bundle Z、試験運用)、ADR-038 (ローカル LLM、試験運用、本 PR の対象)、PR #123 PR body (kill-switch 経路の模範記述)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。Experimental Feature の 3 点セットを 1 箇所に codify。

#### 標準パターン (3 点セット)

1. **Config opt-in**: `enabled = false` をデフォルトとし、明示有効化 (`enabled = true`) で機能発動。env var / config 値での切り替えを必ず提供
2. **Kill-switch**: revert PR で `enabled = false` に戻す経路を PR body / ADR で明文化。crate 削除等の物理削除は dogfood 失敗判定後にまとめて実施 (本 PR の §10.6 C 採用 / 簡易版 / 完全版の階層化が参考)
3. **Bounded lifetime**: 試験期限を ADR 冒頭または計画書冒頭に明記 (例: 「6 ヶ月経過しても採用判断未達なら却下とみなす」)。retirement workflow (`docs-governance.md`) との接続を明示

#### 設計決定の余地 (実装時に検討)

- **配置先**:
  - **case 1**: project root の `CLAUDE.md` または global `~/.claude/CLAUDE.md` に「Experimental Features」section を直接追加 (post-merge-feedback の原案)
  - **case 2**: 別 ADR (例: ADR-039 experimental-feature-standard-pattern) で codify + CLAUDE.md からリンク
- **memory `feedback_claude_md_link_only.md` ("CLAUDE.md はリンクのみ") との整合**: case 2 が memory rule に整合的。case 1 は本タスク承認で memory を override する形になるため、実装時に再確認推奨

#### 作業計画

- [ ] 配置先 (case 1 / case 2) を決定
- [ ] 該当ファイルに Experimental Features の 3 点セットを XS で追記
- [ ] (任意) 既存試験運用 ADR (ADR-031 / 036 / 038) から本 section へのリンク追加
- [ ] 順位 90 と同 PR で land 推奨 (Bundle h コア、両者 XS+S)

#### 完了基準

- 試験運用 ADR を新規策定する際の参考点が明文化される
- 既存試験運用 ADR (031/036/038) と新規 section の整合がとれる

---

### グローバルルール: ephemeral 大規模コンテンツの ADR 昇格 + config コメント lifecycle (PR #123 T3-2 採用) ★ Bundle h

> **動機**: PR #123 で `docs/local-llm-offload-analysis.md` (ephemeral 試験運用計画書) に §10 (約 200 行の governance / procedure content) を追加した行為は、systemic に発生しているパターン。本来は ADR 化を検討すべき「永続的に参照される運用ルール」が ephemeral 内に閉じ込められると、retirement 時に dead pointer / 知識ロスのリスクが顕在化する。同 PR で `pr-monitor-config.toml` のコメントが ephemeral 計画書 (`local-llm-offload-analysis.md §A-2 / §10`) を参照する cross-file reference lifecycle 違反も発生 (post-merge-feedback で T3 #3 として "🤔 様子見" verdict、本タスクとは別件)。両事例を予防するグローバルルールを 2 ファイルに追加する。
>
> **本タスクの位置づけ**: PR #123 post-merge-feedback Tier 3 #2 採用 (Frequency Medium / Effort S / Adoption Risk None)。PR #94 / #110 / #111 で続いている ephemeral ↔ permanent lifecycle 違反シリーズの予防層を強化。
>
> **参照**: `.claude/feedback-reports/123.md` Tier 3 #2、`~/.claude/rules/common/docs-governance.md` 既存 § Retirement Workflow、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle、PR #94 / #110 / #111 (関連事例)、PR #123 §10 大規模追加 (本ルールのトリガ事例)
>
> **実行優先度**: 💎 **Tier 3** — Effort S。global rule 2 ファイル更新。

#### 追加する 2 ルール

##### (a) `~/.claude/rules/common/docs-governance.md`: Ephemeral 大規模コンテンツの ADR 昇格基準

Ephemeral artifact (`docs/*-analysis.md` 等の試験運用計画書) 内に **50 行超の governance / procedure content** を追加する場合、廃棄時に ADR (`docs/adr/adr-NNN-*.md`) への昇格を検討する判断基準を明文化:

- **50 行超 + 「他箇所から参照される運用ルール」性格** → ADR 昇格を検討
- **50 行超でも「1 つの試験運用 case の固有手順」** → ephemeral 内のままでよい
- **廃棄時の判断**: retirement workflow Step 1 (permanent value 移管) で ADR 昇格判断を必ず実施

書き先候補: 既存 § Retirement Workflow の Step 1 詳細化、または新規 § "Ephemeral 大規模コンテンツの ADR 昇格基準"。

##### (b) `~/.claude/rules/common/coding-style.md`: Config コメントの reference lifecycle

設定ファイル (`*.toml` / `*.json` / `*.yaml`) のコメントから ephemeral 計画書 (`docs/*-analysis.md` / `docs/todo*.md` 等) へリンクするのは anti-pattern。理由:

- 計画書は ephemeral lifecycle で削除される
- 設定ファイルは permanent lifecycle で長期保持される
- 永続 → ephemeral リンクは時間経過で dead pointer になる

代替案:

- **ADR 参照** (`# 詳細: docs/adr/adr-NNN-feature.md`)
- **インライン説明** (1-2 行で意図を直接記述)

書き先候補: 既存 § Cross-File Reference Lifecycle の anti-pattern 例として「config コメント → ephemeral 計画書」を追加。PR #123 `pr-monitor-config.toml` の事例を inline cite。

#### 作業計画

- [ ] `~/.claude/rules/common/docs-governance.md` に (a) を追記 (Step 1 詳細化 or 新規 §)
- [ ] `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle に (b) を追記
- [ ] PR #123 `pr-monitor-config.toml` の `local-llm-offload-analysis.md` 参照を cite (anti-pattern 例)
- [ ] 順位 89 と同 PR で land 推奨

#### 完了基準

- ephemeral 計画書に大規模 content を追加する際の判断基準が明文化される
- config コメント → ephemeral 参照の anti-pattern が global rule に明記される
- 順位 89 と同 PR で land (Bundle h コア)

---

### `[lint_screen]` config parse テスト (PR #132 T2-#4 採用) ★ Bundle i

> **動機**: PR #132 (Phase c MVP) で `push-runner-config.toml` に新 section `[lint_screen]` を追加したが、`config.rs` の test module には parse テストが不在。CodeRabbit nitpick で指摘 (`config_parses_with_diff` 相当が `[lint_screen]` には未存在)。serde TOML は field name の完全一致を要求するため、parse テストがないと将来の field rename / 追加で silent `None` fallback が発生し、機能が無音で停止するリスクがある。
>
> **本タスクの位置づけ**: PR #132 post-merge-feedback Tier 2 #4 採用 (Frequency Medium / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/132.md` Tier 2 #4、`src/cli-push-runner/src/config.rs` (テスト module の `config_parses_with_diff` を template に踏襲)、PR #132 commit `73903d72` (lint_screen config 追加)
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。順位 92 と同 PR (Bundle i) 推奨。

#### 作業計画

- [ ] `src/cli-push-runner/src/config.rs` の `#[cfg(test)] mod tests` に `config_parses_with_lint_screen_section` を追加
- [ ] 全 7 field (`enabled`, `exe_path`, `model`, `endpoint`, `timeout_secs`, `max_diff_lines`, `output_path`) の deserialize 検証
- [ ] `enabled = false` でも `Option<LintScreenConfig>` が `Some(...)` で構築されることを assert (= section があれば parse される、なくなれば None)
- [ ] 一部 field 省略時に default (`None`) になることを assert (`config.lint_screen.unwrap().exe_path.is_none()` 等)

#### 完了基準

- 上記テストが pass
- 将来 `LintScreenConfig` に field 追加 / rename した時に test 側で気付ける構造になる

#### 詰まっている箇所

なし

---

### scale-aware eval fixtures (200+ 行) — Phase d 投入前の必須 infrastructure (PR #132 T2-#5 採用) ★ Bundle i

> **動機**: PR #132 smoke dogfood で 868 行の現実 PR diff を mistral:7b に流したところ、JSON 出力が不完全 (`missing field 'screen_decision'`) になり fallback path が作動した。Phase b' eval fixtures (10-30 行/件) では出ない failure mode で、Phase d 本番 PR 投入時に頻発するリスクが顕在化していた。fixture 化することで再現可能化し、 §8.D prompt v3 / v4 改善ループの reference point として固定する。
>
> **本タスクの位置づけ**: PR #132 post-merge-feedback Tier 2 #5 採用 (Frequency Medium / Effort M / Adoption Risk None)。Phase d 着手前の必須 infrastructure 拡充。
>
> **参照**: `.claude/feedback-reports/132.md` Tier 2 #5、`src/cli-finding-classifier/evals/lint-screen-evals.json` (eval セット)、`src/cli-finding-classifier/tests/lint_screen_evals.rs` (compare ロジック)、PR #132 PR body §smoke dogfood 結果 (868 行 diff の fallback 観測)
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。Phase d 着手前に必須。順位 91 と同 PR (Bundle i) 推奨。

#### 追加する fixture 案 (3 件以上)

| # | 名前 | 規模 | 検証目的 |
|---|---|---|---|
| 13 | eval13-large-refactor-real | ~300 行 / 5 file | mistral:7b の context 限界、fallback 頻度 |
| 14 | eval14-mid-mixed | ~150 行 / 3 file | scale 中域での recall 安定性 |
| 15 | eval15-syntax-stress | ~200 行 / 1 file | 単 file の long diff、JSON 完全性 |

baseline は Phase a/b' と同じく Claude Code 一次起案 → ユーザー確認。期待結果 (`screen_decision`) は **agreement 75% 維持** が目標、未達なら §8.D v4 prompt 改訂ループ。

#### 作業計画

- [ ] 200-300 行 diff fixture を 3 件以上作成 (実 PR から抽出 or 合成)
- [ ] 各 fixture に SYNTHETIC FIXTURE comment header (ADR-038 規約) を付与
- [ ] `lint-screen-evals.json` に baseline + expectations 追加
- [ ] `eval_set_loads_and_has_phase_b_prime_twelve_entries` test を 15+ 件期待に更新
- [ ] cargo test --ignored 再走、agreement rate と fallback rate を記録
- [ ] agreement < 75% なら §8.D v4 prompt 改訂で対処

#### 完了基準

- 200+ 行 fixture 3 件以上が `evals/files/` に追加
- cargo test --ignored が pass
- 大規模 diff の fallback rate が記録される (Phase d 改善ループの baseline)
- agreement 75% 以上が維持されているか、未達理由が文書化される

#### 詰まっている箇所

なし。Phase d 本番 PR 投入前の必須 infra。

---

### `coding-style.md` Cross-File Reference Lifecycle に partial fix 例を追記 (PR #132 T3-#8 採用)

> **動機**: PR #94 / #111 / #132 で「変更差分外ファイル (`evals/`, `tests/`, ADR 等) に同じ参照が残存して partial fix 再発」というパターンが反復観測された。既存 `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle はあるが「同一概念が変更差分外でも複数箇所に存在し、変更時には family_tag scope で全箇所を揃える必要がある」具体例が不在。Frequency High の anti-pattern として codify することで、機械強制 (lint rule⑥) と教育的ガイダンスの両層で予防する。
>
> **本タスクの位置づけ**: PR #132 post-merge-feedback Tier 3 #8 採用 (Frequency High / Effort XS / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/132.md` Tier 3 #8、`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle (既存ルール)、PR #94 (lint rule extensions 不揃い) / PR #111 (Bundle e cross-file drift) / PR #132 (lint_screen step が config / test / instruction で family_tag を持つ)
>
> **実行優先度**: 💎 **Tier 3** — Effort XS。独立並列実施可。

#### 追加する例

`~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle (= 既存 § "Multi-point synchronization") に「変更差分外への partial fix 再発」anti-pattern 例を追記:

```text
### Anti-pattern: 変更差分外への partial fix 再発

同一概念が複数ファイル (実装 / config / test / fixture / ADR / instruction) に分散している場合、
変更差分内のみを揃えて差分外の対応箇所を放置すると後続 PR で「あの参照は古い」指摘が無限再発する。

由来: PR #94 (lint rule extensions が rule code で更新済だが ADR で未更新) / PR #111 (Bundle e
の family_tag scope で同一概念が docs/ に複数残存) / PR #132 (Phase c の lint_screen step が
config.rs + push-runner-config.toml + review-simplicity.md + ADR で family_tag が分散) で実証。

対処:
- family_tag (例: `lint_screen`, `extensions`) を `grep -rn` で全 path 検索し、変更差分外も含めて揃える
- 変更差分外の対応漏れは PR description の "Out of scope" に明記して別 PR に切り出す (= partial fix を意図的にする)
- 何も書かないと reviewer / 自分自身の再 visit 時に消化不良として再発する
```

#### 作業計画

- [ ] `~/.claude/rules/common/coding-style.md` § Cross-File Reference Lifecycle (or 関連 §) に上記 anti-pattern を追記
- [ ] PR #94 / #111 / #132 を inline cite で trigger 事例として記録

#### 完了基準

- coding-style.md に「変更差分外への partial fix 再発」例が codify される
- 既存 lint rule⑥ (`no-ephemeral-todo-reference` 系) と組み合わせで教育効果が強化される

---

### `docs/` 内 Markdown の `../docs/` 相対パストラップ検出 lint rule (PR #133 T1-1 採用) ★ Bundle j

> **動機**: PR #133 (todo5.md 分割) で `docs/todo7.md` L103 に `[ADR-036](../docs/adr/adr-036-...)` 形式の壊れ link が混入。`docs/` 配下のファイルから `../docs/` を辿ると `docs/docs/` を指すため directory nesting mismatch で必ず broken link になる。todo5.md 時代から存在した pre-existing bug が分割で表面化した経緯で、CodeRabbit Minor finding として検出。custom lint rule で書いた瞬間に block すれば bug class が排除される。
>
> **本タスクの位置づけ**: PR #133 post-merge-feedback Tier 1 #1 採用 (Severity Medium / Frequency Low / Effort S / Adoption Risk None)。Bundle Z #B-α と同じ「決定論的防止層」哲学。AST 解析ではなく正規表現層 (ADR-007) で対応可能。
>
> **参照**: `.claude/feedback-reports/133.md` Tier 1 #1、ADR-007 (custom lint rule の正規表現 / AST 層線引き)、CodeRabbit PR #133 review #3
>
> **実行優先度**: 🚀 **Tier 1** — Effort S。`.claude/custom-lint-rules.toml` への regex rule 追加。

#### 設計決定 (案)

- **配置先**: `.claude/custom-lint-rules.toml` に新規 rule entry
- **検出パターン (正規表現案)**: `(?i)\]\(\.\./docs/`
  - case-insensitive flag は `.claude/rules/common/code-review.md` の Custom lint rule patterns 規約に従う (PowerShell 等向けだが安全側に倒す)
- **適用対象**: `docs/**/*.md` (rule の `paths` filter で限定。他 path 配下からは正当な `../docs/` 参照があり得るため)
- **rule 名 (案)**: `no-docs-relative-back-to-docs`
- **suppress マーカー**: 該当行末に `<!-- lint-ignore: no-docs-relative-back-to-docs -->` 等

#### 作業計画

- [ ] 既存 `.claude/custom-lint-rules.toml` の rule 構造を確認
- [ ] regex + path filter を新 rule として記述
- [ ] PostToolUse hook の lint runner で synthetic test (PR #133 で混入した `../docs/adr/...` パターンを再現してマッチ確認)
- [ ] 既存 `docs/` 配下を grep して false positive 影響範囲を確認 (`grep -rn '\]\(\.\./docs/' docs/`)
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) への deploy 確認
- [ ] 本 todo6.md エントリを削除

#### 完了基準

- `.claude/custom-lint-rules.toml` に新 rule が追加され `docs/**/*.md` 内の `\]\(\.\./docs/` を検出
- 1〜2 PR で dogfood し false positive がないこと
- PR #133 と同型の broken link 混入が新 PR で構造的に防止される

#### 詰まっている箇所

- 派生プロジェクト (techbook-ledger 等) で同 rule が適用された際、各 repo の `docs/` 構造が異なる可能性 — 着手時に各派生 repo の `docs/` レイアウトを確認

---

### `docs/todo*.md` preamble file count 自動照合スクリプト (PR #133 T2-#4 採用) ★ Bundle j

> **動機**: PR #133 で `docs/todo6.md` L5 (「六つすべてを確認すること」) と `docs/todo7.md` L5 (「七つすべて」) が実 8 ファイル (todo.md / todo2-7.md / todo-summary.md) と乖離。CodeRabbit Minor finding として 2 件検出され、fix commit (`4889413`) で修正したが、`todo*.md` 分割が今後も繰り返される pattern (todo3 → 4 → 5 → 6 → 7) のため CI 層で自動検証する価値がある。Tier 1 #1 (custom lint) と相補で防御層を構築。
>
> **本タスクの位置づけ**: PR #133 post-merge-feedback Tier 2 #4 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None)。shell script のみで実装可能、機械検知が容易な低リスク CI step。
>
> **参照**: `.claude/feedback-reports/133.md` Tier 2 #4、PR #133 fix commit `4889413`、CodeRabbit PR #133 review #1/#2
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。`.github/workflows/lint.yml` (現状未存在のため新規作成も視野) または PostToolUse hook + Stop hook での実装も検討可能。

#### 設計決定 (案)

- **配置先**: `.github/workflows/lint.yml` の docs check job に追加 (本リポジトリは現状 GitHub Actions 未設定なので、最初の workflow 作成を含む)。代替案として PostToolUse / Stop hook で local 段階で検出も可
- **検出ロジック (shell)**:
  ```bash
  EXPECTED=$(find docs -maxdepth 1 -name "todo*.md" | wc -l)
  for f in docs/todo*.md; do
    # preamble 内の "X つ" 数詞を抽出、期待値と照合
    PREAMBLE=$(sed -n '5p' "$f")
    # 「八つ」(8) / 「七つ」(7) 等の漢数字 → 数値変換で照合
    ...
  done
  ```
- **数詞 → 数値マッピング**: 一/二/三/四/五/六/七/八/九/十 を hash で持つ
- **対象範囲**: `docs/todo*.md` のみ (todo-summary.md の preamble 別仕様は scope 外)

#### 作業計画

- [ ] 現状 `.github/workflows/` が無いことを確認 (PR #133 で確認済) し、新規 lint.yml の足場を作るか PostToolUse hook 拡張で代替するか判定
- [ ] shell script (or Rust hook) で count 検証ロジックを実装
- [ ] 漢数字 → 数値マッピングと preamble grep の正規表現を定義
- [ ] PR #133 の修正前状態 (todo6.md「六つ」/ todo7.md「七つ」) を re-introduce した synthetic test で fail することを確認
- [ ] 採用後の dogfood で false positive がないことを 2-3 PR で確認
- [ ] 本 todo6.md エントリを削除

#### 完了基準

- preamble count と実 file 数の乖離が CI / hook で検出される
- PR #133 fix commit で修正した同型問題が機械的に再発防止される

#### 詰まっている箇所

- **GitHub Actions 未設定 repo であること**: workflows 新設は本タスク scope を超える可能性。代替として PostToolUse hook (Rust) での検証が低コスト。Tier 2 #3 (Markdown cross-reference validator) と同 PR で `.github/workflows/lint.yml` 新設を検討する形がまとまりよい
- **数詞表記の揺れ**: 「八つ」「8 つ」「8つ」等の異表記許容範囲を着手時に確定する必要

---

### Markdown cross-reference validator CI step (PR #133 T2-#3 採用) ★ Bundle j

> **動機**: PR #133 で `docs/todo7.md` L103 の壊れ ADR link (`../docs/adr/...`) が pre-push lint で早期検知できなかった (CodeRabbit Minor finding で post-PR 検出)。既存 `markdown-link-check` 系 tool は `docs/` 内 relative path を起点 file の directory レベルで正規化しないため broken link を見逃す。custom validator で directory-aware に解決する CI step が必要。Tier 1 #1 (custom lint で `../docs/` パターンを規約レベルで block) と Tier 2 #4 (preamble count 照合) と組み合わせて、docs/ 全体の構造的一貫性を多層検証する。
>
> **本タスクの位置づけ**: PR #133 post-merge-feedback Tier 2 #3 採用 (Severity Medium / Frequency Medium / Effort M / Adoption Risk: 実装工数中)。
>
> **参照**: `.claude/feedback-reports/133.md` Tier 2 #3、PR #133 fix commit `4889413` (todo7.md L103 修正)、関連 task: 順位 10 (ADR-032 PR-broken-link)
>
> **実行優先度**: 🔧 **Tier 2** — Effort M。validator 実装 + CI 統合。

#### 設計決定 (案)

- **配置先**: `.github/workflows/lint.yml` に validator step 追加 (順位 95 と同 PR で workflows 新設するのが効率的)
- **実装方針候補**:
  - **A**: 既存 `markdown-link-check` を fork or wrapper で directory-aware 化
  - **B**: custom Rust binary (cli-markdown-link-validator 等、既存 cli-* と同 workspace) で書き起こし
  - **C**: 軽量 shell + ripgrep ベースの解析 (`rg '\]\([^http][^\)]*\)' docs/` → 各 link を file path 起点で resolve)
- **検証範囲**: `docs/**/*.md` 内の relative link (`./`, `../`, または anchor 付きの内部 link) すべて
- **既存タスクとの関係**: 順位 10 (ADR-032 PR-broken-link) と方向性が近接。同タスクとして fold-in 検討の余地あり

#### 作業計画

- [ ] 既存 `markdown-link-check` 系 tool の機能調査 (directory-aware resolution の有無)
- [ ] 順位 10 (ADR-032 PR-broken-link) との重複排除判定: 同タスクで包含するか、独立 task として残すか
- [ ] 実装方針 A/B/C の比較評価 (Effort vs maintainability)
- [ ] PR #133 で混入した `../docs/adr/...` パターンを synthetic test で検出
- [ ] PR #133 で正常な相対 link (例: `[docs/todo-summary.md](todo-summary.md)`) を false positive 検出しないことを確認
- [ ] 採用後の dogfood で 3-5 PR の false positive 率測定
- [ ] 本 todo6.md エントリを削除

#### 完了基準

- `docs/` 内 broken relative link が CI で検出される
- PR #133 と同型の `../docs/` トラップを Tier 1 #1 と Tier 2 #3 の二重防御で抑止
- 既存正常 link で false positive 率 < 5%

#### 詰まっている箇所

- **順位 10 (ADR-032 PR-broken-link) との関係整理**: 設計上 fold-in が妥当か、独立 task が妥当か着手時に判断必要
- **GitHub Actions 未設定**: 順位 95 (Tier 2 #4) と同様、workflows 新設の判断を含む。この場合 95 + 96 + (将来の lint workflow 整備) を 1 PR の Bundle として land する案も検討余地
