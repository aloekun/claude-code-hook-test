# TODO (Part 6)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo5.md / 本ファイルが 50KB に到達 (PR #143 T3-#1) のため **新規エントリは [docs/todo8.md](todo8.md) へ移行**。本ファイルは既存タスクの編集・完了削除専用。todo.md / todo2-7.md / todo8.md の既存エントリは引き続き有効、相互に独立。新セッションでは九つすべてを確認 (todo.md / todo2-8.md / todo-summary.md)。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

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

---

### `with_num_ctx(X)` override 値 serialization 検証テスト (PR #136 T2-#1 採用)

> **動機**: PR #136 (§8.D / num_ctx 8192 land) で `OllamaClient::with_num_ctx` builder method を追加した際、test として `num_ctx_is_serialized_into_request_body` を入れたが、これは default 値 (8192) のみを mockito で assert する。`with_num_ctx(X)` を経由した override (例: 16384) が実際に request body に反映されるかは未検証で、builder chaining が壊れた場合 (例: `with_num_ctx` body の typo `self.num_ctx = num_ctx` → `self.num_ctx = self.num_ctx`) に **silent degrade** = default 値が常に送信されて override が無視される、を test で捕捉できない。
>
> **本タスクの位置づけ**: PR #136 post-merge-feedback Tier 2 #1 採用 (Severity Medium / Frequency Low / Effort S / Adoption Risk None)。CodeRabbit nitpick 起点ではなく post-merge-feedback agent が独立に発見した test gap (CodeRabbit は同 method の `0` guard は指摘したが override-serialization wiring までは見抜かなかった)。
>
> **参照**: `.claude/feedback-reports/136.md` Tier 2 #1、`src/lib-ollama-client/src/lib.rs` の既存 test `num_ctx_is_serialized_into_request_body` (default 値検証) と `num_ctx_defaults_and_overrides_apply` (struct field 検証) の合間にある wire-level wiring gap
>
> **実行優先度**: 🔧 **Tier 2** — Effort S。Phase d (PR-based 実環境 dogfood) で num_ctx tweak (16384 / 32768 等) する局面に入る前の安全網。

#### 設計決定 (案)

- **配置先**: `src/lib-ollama-client/src/lib.rs` の `#[cfg(test)] mod tests`
- **test 名 (案)**: `with_num_ctx_override_is_serialized_into_request_body`
- **実装方針**: 既存 `num_ctx_is_serialized_into_request_body` の mockito pattern を踏襲し、`OllamaClient::new(...).with_num_ctx(16384)` で構築 → request body に `num_ctx:16384` が含まれることを `Matcher::PartialJsonString` で assert
- **代替案**: `with_num_ctx(8192)` (= default 同値) でも builder chain が走ることを assert する pure unit test (mockito 不要) を追加し、wire-level test と組み合わせる二層構造も可

#### 作業計画

- [ ] 既存 `num_ctx_is_serialized_into_request_body` test を template に override 値検証 test を追加
- [ ] `with_num_ctx(16384)` を builder chain 経由で適用 → mockito の `Matcher::PartialJsonString` で `{"options":{"num_ctx":16384}}` を assert
- [ ] cargo test -p lib-ollama-client で 12 tests pass を確認 (現状 11 + 新 1)
- [ ] 本 todo6.md エントリを削除

#### 完了基準

- `with_num_ctx(X)` の builder chain が壊れた場合 (e.g. body の self-assign 化、struct field rename) に test が即 fail する
- Phase d で num_ctx を tweak する局面で、override 値が実際に Ollama に伝わっているかを test 層で seal できる

#### 詰まっている箇所

なし。Effort S / 既存 test の duplicate 風で実装容易。

---

### ADR-038 に PR #138 learning 2 件を追記 (cost-aware 実装層選択 + attention dilution pitfall) (PR #138 T3-#1+#2 採用)

> **動機**: PR #138 (Phase d kickoff prep) 関連セッションで観測された 2 件の重要 learning が ADR-038 未記録。両者とも次回 LLM/Ollama 系 feature 開発時に再発可能性が高く、ADR に codify することで以下を構造的に防ぐ:
>
> 1. **cost-aware 実装層選択**: lint_screen を当初 `takt facet` (Sonnet 動作) として ADR-038 に記述していたが、実装段階で「Sonnet 動作はコスト削減という主目的と矛盾」と判明し `cli-push-runner` の Rust stage (mistral 直呼び) へ pivot。判断根拠が ADR-038 に未記録のため、後続の §8.F (PR body draft) 等で同型の選択を再検討する際に学習が再現されない
> 2. **attention dilution pitfall**: Phase b' v2 で eval prompt example に diff header (`--- a/<path>` `+++ b/<path>`) を full に追加した結果、agreement rate が **75% → 50% に 33pt 低下** した実証データ。anti-hallucination preamble の効果が context scarcity で打ち消される pattern で、再発すると prompt tuning コストが大きい
>
> **本タスクの位置づけ**: PR #138 post-merge-feedback Tier 3 #1 + #2 採用 (Tier 3 #1: Severity Low / Frequency Medium / Effort S / Adoption Risk None / Tier 3 #2: Severity Medium / Frequency Low / Effort S / Adoption Risk None)。両者とも ADR-038 への追記で 1 ファイル編集、bundle land 推奨。
>
> **参照**: `.claude/feedback-reports/138.md` Tier 3 #1 + #2、`docs/adr/adr-038-local-llm-finding-classification.md`、`docs/local-llm-offload-history.md` (Phase b' v2 の attention dilution 観測)
>
> **実行優先度**: 💎 **Tier 3** — Effort S。次の LLM 系 feature (§8.F PR body draft 等) 着手前 or Phase d 完了集約 PR と同 timing で land 推奨。

#### 設計決定 (案)

- **配置先**: `docs/adr/adr-038-local-llm-finding-classification.md` 内に 2 つの新 section を追加
- **#1 cost-aware 実装層選択**: `## Architecture decision: takt facet vs Rust stage trade-off` (or 既存 §architecture を拡張)
  - takt facet (Sonnet) を選ぶ条件: 意味的判断が必要、コスト感度低
  - Rust stage (local mistral) を選ぶ条件: コスト削減が主目的、決定論的判定が可能、latency 許容範囲
  - lint_screen の実例: 当初 takt facet → コスト矛盾検出 → Rust stage に pivot
- **#2 attention dilution pitfall**: `## Prompt engineering: attention dilution case study` (or §prompt engineering 拡張)
  - 観測: Phase b' v2 で diff header full 追加 → agreement 75% → 50% (33pt 低下)
  - 根因: anti-hallucination preamble の効果が context scarcity で打ち消される
  - 教訓: prompt examples は最小 viable diff snippet で記述、metadata は省略

#### 作業計画

- [ ] `docs/adr/adr-038-local-llm-finding-classification.md` の構造確認 (既存 section header の慣習に合わせる)
- [ ] #1 architecture decision section を追加 (lint_screen pivot 根拠 + 一般化)
- [ ] #2 prompt engineering pitfall section を追加 (Phase b' v2 観測値 + 教訓)
- [ ] 既存 section との整合性確認 (重複説明の有無)
- [ ] markdownlint clean 確認
- [ ] 本 todo6.md エントリを削除

#### 完了基準

- ADR-038 に 2 つの learning が permanent record として codify される
- 後続 LLM 系 feature 開発時に「過去の選択根拠 / pitfall」を git log でなく ADR で参照可能になる
- markdownlint clean

#### 詰まっている箇所

なし。Effort S、ADR への追記のみで副作用最小。

#### 参考: 不採用理由 (Tier 3 #4)

`~/.claude/rules/common/coding-style.md` §Markdown に「重複表現 grep チェック手順」を追加する提案 (#3-4) は **ユーザー判断で見送り**。理由: 重複ワードのバリエーションが多すぎて grep pattern 列挙では網羅できないため、`feedback_no_unenforced_rules.md` 方針 (機械検知不可なルールは追加しない) と整合的に却下相当。週次レビュー (ADR-031) や reviewer の主観判断で対処する位置づけを維持。

---

### `development-workflow.md` に 「同一ファイル複数編集の 1 task 統合」 + 「partial completion + 後続 PR 追補明記」 を追補 (PR #139 T3-#1 採用)

> **動機**: PR #139 (Bundle h+g-2 land) の post-merge-feedback で 2 つの暗黙知が systemic に観測された:
>
> 1. **同一ファイル複数編集の 1 task 統合**: PR #119/#120/#121 の sub-PR 分割では同一ファイル (`~/.claude/rules/common/*`) の複数編集を 1 task に統合した方が review 重複を回避できた。明文化されていないため次回類似 sub-PR で再発する余地
> 2. **partial completion + 後続 PR 追補明記**: PR #139 で Bundle g-2 (順位 87+88) を land したが Bundle g-1 (順位 85+86) は未着手という partial completion を PR body / analysis.md で明記する pattern。Bundle h でも同様 (8 試験運用 ADR への back-link は本 PR 範囲外と明示)。明文化されていないと「全部やった」誤認や曖昧 review が生じる
>
> **本タスクの位置づけ**: PR #139 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。`feedback_no_unenforced_rules.md` 方針との整合: 本提案は「既存実践の明文化」であり機械検知不可なルール追加ではない (review/PR body 記述で人間の意識付けに用いる目安) ため例外的に採用相当。
>
> **参照**: `.claude/feedback-reports/139.md` Tier 3 #1、`~/.claude/rules/common/development-workflow.md`、PR #119/#120/#121 (sub-PR 分割実例)、PR #139 (partial completion 実例)

#### 作業計画

- [ ] `~/.claude/rules/common/development-workflow.md` の Feature Implementation Workflow 直後 (現 § Edge case 観測頻度の前後 etc.) に新 section を追加
  - **(a) 同一ファイル複数編集の 1 task 統合**: 「sub-PR 分割時、同一ファイルへの複数 task 編集は 1 commit / 1 task に統合する。理由: review 重複回避 + diff の局所化」
  - **(b) partial completion + 後続 PR 追補明記**: 「bundle / scope を全消化できない場合、PR body の "Out of scope" や planning doc に未消化分を明示。理由: 「全部やった」誤認の防止 + 後続 PR の起点として trackable」
- [ ] 既存 § Edge case 観測頻度との接続 (相互参照 or 配置順序検討)
- [ ] markdownlint clean 確認
- [ ] 本 todo6.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 上記 2 pattern が rule として codify される
- 次回 sub-PR 分割時 / partial completion 時に reviewer/Claude が rule から逆引き可能になる
- markdownlint clean

#### 詰まっている箇所

なし。Effort XS、global rule への追記のみで副作用最小。配置先 (Feature Implementation Workflow 直後 vs 別 § で独立) は実装時の判断。

---

### `paths` filter を lint runner に実装 (PR #140 T1-#2 採用)

> **動機**: rule⑧ (PR #140) の TOML コメントで「`paths` filter は lint runner 未実装、`extensions` のみ」と明記し pattern semantics で self-limit したが、これは設計-実装 gap の workaround。今後 path-sensitive な lint rule (例: `tests/` 内のみ / `src/cli-*/` のみ等) を追加するたびに同じ workaround を強いる systemic pattern が予測される。`src/hooks-post-tool-linter/src/filter.rs` (or 等価な path filter モジュール) で `paths = [...]` glob filter をサポートする。
>
> **本タスクの位置づけ**: PR #140 post-merge-feedback Tier 1 #2 採用 (Severity Medium / Frequency Medium = systemic / Effort M / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/140.md` Tier 1 #2、`src/hooks-post-tool-linter/src/main.rs` の `CustomRule` struct (line ~76-87)、PR #140 rule⑧ TOML コメント、ADR-007 (custom lint rule の正規表現/AST 層線引き)

#### 設計決定の余地

- **glob 構文**: `**/*.md` 形式で十分か / `regex` ベースの方が柔軟か → 既存 `extensions` が単純 string match なので glob で平仄を取る方が自然
- **AND vs OR**: `extensions` と `paths` 両方指定時の filter 結合 (両方 match を要求 = AND が直感的)
- **test 規模**: 単体 test を同 commit に含める (PR #140 教訓: rule 追加と同 PR で test 不在を防ぐ)

#### 作業計画

- [ ] 設計決定 (glob 構文 / AND-OR / test 戦略) を確定
- [ ] `CustomRule` struct に `paths: Option<Vec<String>>` を追加 (Optional で既存 rule に影響なし)
- [ ] glob match 実装 (例: `globset` crate or 既存 `glob` 機能で代替)
- [ ] `extensions` × `paths` の組合せ test (path match / 不一致 / 未指定で AND fallback の各ケース)
- [ ] rule⑧ を `paths = ["docs/**/*.md"]` で書き換え (semantic self-limit から explicit filter へ)
- [ ] 順位 101 の test 整理と整合性確認 (本実装後に depth-1 root MD は paths filter で skip される設計に変わる可能性)
- [ ] ADR-007 amendment (順位 104) と同 PR で land 推奨 (rationale 同期)
- [ ] 本 todo6.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- `paths` filter が動作 (test 検証)
- rule⑧ の TOML コメントから「paths filter 未実装」記述が消え、explicit filter に置き換わる
- 後続 path-sensitive rule 追加時の動線が確立

---

### ADR-007 amendment: semantic self-limitation 安全条件 + lint rule 最小テストチェックリスト (PR #140 T3-#1 採用)

> **動機**: rule⑧ (PR #140) で `paths` filter 不在を pattern semantics で代替した判断は妥当だったが、**どんな条件下で semantic self-limitation が安全か** / **explicit filter が必須な条件は何か** が ADR-007 に明文化されていない。3 ソース (PR diff / prepush / session) でこの documentation 不足を独立指摘。同時に lint rule 最小テストチェックリスト (pattern detection / case-insensitive / false positive skip の 3 項目) も ADR レベルで確立すると future rule author の prior が安定化する。
>
> **本タスクの位置づけ**: PR #140 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Low = 3 ソース観測 / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/140.md` Tier 3 #1、`docs/adr/adr-007-custom-linter-layer-boundary.md`、PR #140 rule⑧ 設計判断

#### 追加する 2 section (ADR-007 への amendment)

##### (a) Semantic self-limitation の安全条件 vs explicit filter 必須条件

- **OK**: pattern が path-context を含意する場合 (例: `](../docs/` は parent-dir 経由で docs/ を再参照 → docs/ 配下以外では自然な記述形式と区別される)
- **NG**: pattern が path-agnostic で paths filter 必須 (例: `eprintln!` は src/ 全体で頻出、特定 crate のみに限定したい場合は path filter が必要)
- **判断 flow chart**: 「pattern semantics で false positive が <X% (X 値は実測 grep 結果) 以下なら OK / 超えるなら paths filter 必須」

##### (b) Lint rule 最小テストチェックリスト

新規 lint rule 追加時の必須 test 構成:

1. **Pattern detection test**: 想定するアンチパターン入力で fire することを確認
2. **Case-insensitive test** (該当する場合): 大文字バリアント / 小文字バリアントで fire することを確認 (PR #91 PowerShell `(?i)` 教訓)
3. **False positive skip test**: 似て非なる正当パターンで fire しないことを確認

#### 作業計画

- [ ] ADR-007 既存 section 構造を確認
- [ ] (a) Semantic self-limitation 判断基準を新 section として追加
- [ ] (b) 最小テストチェックリストを新 section として追加 (もしくは既存 § 拡張)
- [ ] 順位 102 (`paths` filter 実装) と同 PR で land 推奨 (実装と documentation の同期)
- [ ] 順位 105 (グローバル CLAUDE.md table) との内容重複を確認 (ADR は判断基準、CLAUDE.md は field reference で補完関係)
- [ ] 本 todo6.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- ADR-007 に「semantic self-limit OK 条件」と「最小 test checklist」が明記される
- 次回 lint rule 追加時に rule author が ADR-007 から判断基準を逆引きできる

---

### グローバル CLAUDE.md に lint runner サポートフィールド一覧表 (PR #140 T3-#2 採用)

> **動機**: 派生プロジェクト (techbook-ledger / auto-review-fix-vc 等) で hooks を porting する際、lint runner がサポートするフィールド (`pattern` / `extensions` / `severity` / `message` / `why`、planned: `paths`) を一目で把握できる reference が グローバル CLAUDE.md に存在しない。順位 103 (code comment) と相補的で、cross-project 可視性を即時向上。
>
> **本タスクの位置づけ**: PR #140 post-merge-feedback Tier 3 #2 採用 (Severity Low / Frequency Medium / Effort XS / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/140.md` Tier 3 #2、`~/.claude/CLAUDE.md` (global、リンクのみ方針 = `feedback_claude_md_link_only.md`)、`src/hooks-post-tool-linter/src/main.rs` `CustomRule` struct

#### 設計決定の余地

- **`feedback_claude_md_link_only.md` との整合**: グローバル CLAUDE.md は「リンクのみ」方針。table 形式で field 一覧を inline すると memory rule に違反する可能性
- **代替案 1**: グローバル CLAUDE.md には「lint runner field reference は `~/.claude/rules/...` 配下に独立 doc」とリンクのみ書き、本体 doc は `~/.claude/rules/<topic>/lint-runner-fields.md` 等に配置
- **代替案 2**: project 内 ADR-007 amendment (順位 104) で field 一覧を含めて、グローバル CLAUDE.md は ADR-007 へのリンクのみ
- **判断**: 順位 104 land 後に決定。重複が無いように lifecycle 整合性を取る

#### 作業計画

- [ ] 順位 104 (ADR-007 amendment) の land 後、配置案 1 / 2 / 別案を決定
- [ ] `feedback_claude_md_link_only.md` 方針を再確認
- [ ] 配置先に table 追加 (現サポート field + planned + 派生プロジェクト porting 時の参照点)
- [ ] グローバル CLAUDE.md にリンク追加 (memory rule 遵守)
- [ ] 派生プロジェクト 2 つ (techbook-ledger / auto-review-fix-vc) に本変更を伝播する deploy step を確認
- [ ] 本 todo6.md エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 派生プロジェクトの rule porting 時に field reference を 1 hop で参照可能
- `feedback_claude_md_link_only.md` 違反なし

#### 詰まっている箇所

- 配置先決定が順位 104 (ADR-007 amendment) の land と依存。順位 104 で field 一覧を inline するなら本タスクはリンク追加のみで済むが、ADR は判断基準中心であれば独立 reference doc が必要

---

### `development-workflow.md` に PR #125→#141 anti-pattern 事例補強 (PR #141 T3-#2 採用)

> **動機**: memory `feedback_verify_task_not_already_done.md` (PR #141 セッションで追加) は session-scoped で「PR #125 → #141 で 4 日間 stale todo 残存 → P-3 起動時に手動発見」事例を含むが、`~/.claude/rules/common/development-workflow.md` の central rule 側には反映されていない。`feedback_todo_no_history.md` と合わせて central 化することで、memory file 閉鎖の structural risk を軽減する。
>
> **本タスクの位置づけ**: PR #141 post-merge-feedback Tier 3 #2 採用 (Severity Low / Frequency Medium = 2 観測 / Effort XS / Adoption Risk None)。memory rule の central reference への昇格パターン。
>
> **参照**: `.claude/feedback-reports/141.md` Tier 3 #2、`~/.claude/rules/common/development-workflow.md`、memory `feedback_verify_task_not_already_done.md` / `feedback_todo_no_history.md`、PR #125 / PR #141

#### 作業計画

- [ ] `~/.claude/rules/common/development-workflow.md` の「タスク完了削除手順」に 2-3 行追記:
  - 「マージ後 N 日間 stale entry 残存 → 後続 phase で手動発見」anti-pattern 事例 (PR #125 → #141)
  - 「マージ → 即削除」サイクル強調 (memory `feedback_todo_no_history` central 化)
  - 「task 着手時に jj log + 既存 test で land 済確認」recovery layer (memory `feedback_verify_task_not_already_done` central 化)
- [ ] central rule から両 memory file への双方向参照を追加
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- central rule に PR #125→#141 anti-pattern が anchor として記録される
- 新セッションでも両 memory rule の趣旨を central から逆引き可能になる

---

### CLAUDE.md に「Tier 2 偽装検知 + 却下パターン」table (PR #141 T3-#3 採用)

> **動機**: PR #140 / PR #141 で post-merge-feedback agent が Tier 2 (テスト/自動化) と称した提案を出したが、中身は ルール追加 / checklist 必須化 等の **unenforced rule** で、ユーザー判断で却下相当 (memory `feedback_no_unenforced_rules.md` で codify 済)。memory ファイルは session-scoped で新セッション AI からは見えにくく「Tier 2 = 採用必須」と誤解する構造的リスクがある。グローバル CLAUDE.md に signal + 却下パターン table を可視化し、policy をユーザー可視 + 新セッション AI からも逆引き可能にする。
>
> **本タスクの位置づけ**: PR #141 post-merge-feedback Tier 3 #3 採用 (Severity Low / Frequency Medium = 複数 session 観測 / Effort S / Adoption Risk None)。memory policy の central reference への昇格パターン。
>
> **参照**: `.claude/feedback-reports/141.md` Tier 3 #3、`~/.claude/CLAUDE.md`、memory `feedback_no_unenforced_rules.md` (PR #140 / #141 で追記済)

#### 設計決定

- **`feedback_claude_md_link_only.md` との整合**: CLAUDE.md は「リンクのみ」方針。table を inline すると memory rule に違反するため、別 stable doc (`~/.claude/rules/common/post-merge-feedback-policy.md` 等) に table を移し、CLAUDE.md からリンクする運用を推奨

#### 検知 signal table 案

| Signal | 例 | 判定 |
|---|---|---|
| target field に `*.md` / `test convention` 等 **文書 path** | "lint rule テスト checklist に <条件> を必須化" | ⚠️ Tier 2 偽装疑い |
| description に「**必須化**」「**標準化**」「**チェックリスト追加**」 | "lint rule テストで大文字バリアント必須化" | ⚠️ unenforced rule 強い signal |
| 機械強制 (CI / lint / test 存在検証) なし | "verbal checklist", "guideline 追記" | ❌ 却下相当 |

#### 作業計画

- [ ] 配置先 (新 doc / 既存 doc) を決定
- [ ] 上記 signal table を新 doc or 既存 doc に追加
- [ ] CLAUDE.md に link 追加 (memory rule 遵守)
- [ ] 派生プロジェクトへの伝播も検討
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 新セッション AI が CLAUDE.md → link → table の動線で Tier 2 偽装判定を逆引き可能になる
- `feedback_claude_md_link_only` 違反なし

---


### pure function test pattern template を `testing.md` に追記 (PR #142 T2-#3 採用)

> **動機**: Phase A (PR #142) の `overflow_hint()` は副作用なしの純粋関数で、境界値 (90%) / None (metadata 欠落) / 閾値未満 (90% 未満) の 3 パターンで test 化できる構造になっていた。このパターンを `~/.knee/rules/common/testing.md` にテンプレ化することで、Rust lib 全般で副作用分離と test 容易性が促進される。
>
> **本タスクの位置づけ**: PR #142 post-merge-feedback Tier 2 #3 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/142.md` Tier 2 #3、`~/.claude/rules/common/testing.md`、`src/lib-ollama-client/src/lib.rs` の `overflow_hint()` (PR #142)

#### 作業計画

- [ ] `~/.claude/rules/common/testing.md` に「Pure function test pattern」section を追加 (境界値 / None / 閾値未満 の 3 パターン例)
- [ ] `overflow_hint()` (PR #142) をモデル例として cite
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- testing.md に template が記載され、次回 Rust lib で副作用分離する局面で参照可能になる

---

### `docs-governance.md` に todo5/todo6 routing rule 明文化 (PR #142 T3-#1 採用)

> **動機**: PR #142 で CR Minor #2 として「todo-summary.md 順位 106-108 が todo5.md を指すが intro policy は todo6.md」の bifurcation 指摘あり、本 PR 内で修正済。しかし routing rule が文書化されておらず次回も同型 bifurcation の再発リスクがある。docs-governance.md に「新規詳細は todo6.md」routing rule + 50KB 超過時の対応方針を明文化することで構造的予防。
>
> **本タスクの位置づけ**: PR #142 post-merge-feedback Tier 3 #1 採用 (Severity Low / Frequency Medium / Effort S / Adoption Risk None)。
>
> **参照**: `.claude/feedback-reports/142.md` Tier 3 #1、`~/.claude/rules/common/docs-governance.md`、PR #142 CR Minor #2

#### 作業計画

- [ ] `~/.claude/rules/common/docs-governance.md` に「todo*.md 新規詳細 routing rule」section を追加: 新規詳細は最新の todoN.md (現在 = todo6.md)、50KB 超過時は todo(N+1).md を新設
- [ ] todo*.md 既存 file の preamble との整合確認 (todo6.md / todo7.md の冒頭文と矛盾しないか)
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- 次回 todo*.md 50KB 超過時に routing 判断が明確になり、CR Minor #2 と同型の bifurcation が再発しない

