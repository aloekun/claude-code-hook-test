# TODO (Part 6)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo5.md がファイルサイズ 50KB を超過したため、Claude Code の読み取り安定性 (50KB 超で不安定化) を考慮して新規エントリは本ファイルに記録する。todo.md / todo2.md / todo3.md / todo4.md / todo5.md / todo7.md の既存エントリは引き続き有効、相互に独立。新セッションでは八つすべてを確認すること (todo.md / todo2-7.md / todo-summary.md)。
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

### `num_ctx` overflow detection — JSON parse error 検知時の context window 診断ログ (PR #137 T1-#1 採用)

> **動機**: PR #136 セッションで「mistral:7b の JSON schema breakdown」を観測した際、Claude は当初「prompt 設計の問題」と誤診し `§8.D v4 prompt 改訂ループ` という名前の誤った means に向かいかけた (実 root cause は `num_ctx` default 4096 超過)。advisor の指摘 + raw Ollama output dump で軌道修正されたが、**runtime layer に診断 hint が出ていれば pivot 時間を短縮できる** ことが判明。`lib-ollama-client` の response validation 層で JSON parse error 検知時に `num_ctx` / `prompt_eval_count` / response length を warn log で auto-emit することで、将来の同型事故 (LLM dogfood 全般で systemic に再発し得る) を decisive に診断できる。
>
> **本タスクの位置づけ**: PR #137 post-merge-feedback Tier 1 #1 採用 (Severity Medium / Frequency Low / Effort M / Adoption Risk: 派生プロジェクト deploy コストのみ、低)。ADR-038 試験運用配下の infrastructure 強化、Phase d で num_ctx tweak する局面に入る前の安全網としても機能。
>
> **参照**: `.claude/feedback-reports/137.md` Tier 1 #1、PR #136 で `__dump_raw_ollama.sh` で確認した `prompt_eval_count: 4096` 上限到達 (現在は scratch ファイル削除済、`docs/local-llm-offload-history.md` に経緯記録)、`src/lib-ollama-client/src/lib.rs` の `generate_raw_json` / `OllamaResponse` 構造体
>
> **実行優先度**: 🚀 **Tier 1** — Effort M。Phase d kickoff 前か実 dogfood 中に整備するのが理想 (dogfood で実際に context overflow を起こした PR があれば即診断できる layer になる)。

#### 設計決定 (案)

- **配置先**: `src/lib-ollama-client/src/lib.rs` の `generate_json::<T>` ヘルパー (型付き parse 失敗時) または raw response 検証層
- **emit 条件 (案)**:
  - **A (主軸)**: `serde_json::from_str` が `missing field` 系 error を返した場合 → `prompt_eval_count` が response の `eval_count` field と比較して context cap に近接していれば warn log
  - **B (補助)**: response length が threshold (例: 100 chars) 未満で truncate を疑える場合
  - **C (簡易)**: 常に `prompt_eval_count` / `eval_count` を debug log で emit (low-noise、auto OFF default)
- **emit 内容 (案)**:
  ```text
  [lib-ollama-client] WARN: Ollama JSON output may be truncated.
    parse_error: <serde error message>
    prompt_eval_count: <N> (vs num_ctx: <M>)
    eval_count: <K>, response_length: <L> chars
    hint: 大規模 prompt は num_ctx を増やすことで解決可能 (with_num_ctx で override)。
  ```
- **fallback 経路への副作用**: 既存 fallback (block しない、`human_review` + `fallback_reason` を埋める) は維持、log は副次的な diagnostic 出力のみ

#### 作業計画

- [ ] `OllamaResponse` 構造体に `eval_count` / `prompt_eval_count` フィールドを追加 (現状 `response` / `error` のみ deserialize)
- [ ] `generate_json::<T>` ヘルパー (型付き parse) で error 時に上記情報を warn log emit (`log::warn!` または `eprintln!`)
- [ ] threshold ベースの判定ロジック (主軸 A) を実装、補助 B はオプション
- [ ] tests:
  - `warn_log_emitted_on_truncated_response_when_prompt_eval_count_high` (mockito + log capture)
  - `no_warn_emitted_for_parse_errors_unrelated_to_truncation` (e.g., format-违反 JSON)
- [ ] PR #136 で観測した eval13/15 fixture の `prompt_eval_count: 4096` 状況を dogfood で再現し、log が emit されることを確認
- [ ] cli-finding-classifier 経由でも log が表面化することを smoke 確認 (push-runner step ログに乗るか)
- [ ] 派生プロジェクト (techbook-ledger / auto-review-fix-vc) 向け deploy 判断 (lib-ollama-client は本リポ専用なら deploy 不要、共有なら別途配布計画)
- [ ] 本 todo6.md エントリを削除

#### 完了基準

- JSON parse error + context cap 近接の併発時に warn log が emit される
- 既存 fallback 経路 (block しない、graceful degradation) を破壊しない
- LLM dogfood セッションで「context window 起因か prompt 起因か」を log だけで切り分けられる構造
- 単体テストで diagnostic log の emit 条件 + non-emit 条件の両方を seal

#### 詰まっている箇所

- **派生プロジェクト deploy 戦略**: `lib-ollama-client` が本リポ専用なら deploy なし、共有 crate 化するなら別 repo への copy / git submodule / cargo registry の判断が必要。Phase d 着手判定と合わせて検討
- **log destination**: `eprintln!` (cli 用途で十分) vs `tracing` / `log` crate 統合 (既存の cli-* との一貫性)。本 lib は現状 ureq + serde_json のみで logging crate なし、初期は `eprintln!` で warn 接頭辞付け、将来必要なら crate 統合という段階導入が自然

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
