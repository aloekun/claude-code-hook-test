# ADR-038: ローカル LLM による CodeRabbit findings classification

## ステータス

採用 (2026-05-15、Phase E 採否判定で昇格)

> 本 ADR の運用パターンは [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) で標準化された 3 点セット (config opt-in / kill-switch / bounded lifetime) の対象。Phase A〜D dogfood (6 PR / 9 data points) で採用条件を充足したため、2026-05-15 に試験運用 → 採用へ昇格 (詳細は本文「採用判定 (Phase E、2026-05-15)」section)。

## コンテキスト

Claude Code セッションにおける反復作業の token 消費を抑える目的で、3 層構造 (思考層 / 実行層 / 制御層) のオフロード戦略を 2026-05-06 に提案した。GTX 3070 + Ollama (mistral:7b) でローカル推論可能な範囲を切り出す (origin の探索資料は `docs/local-llm-offload-{analysis,history,phase-d-outcomes,phase-d-guide}.md` だったが、Phase E 採用昇格と同時に retire、permanent value は本 ADR + [ADR-040](adr-040-local-llm-context-size.md) に migrate 済)。

調査の結果、CodeRabbit findings のうち以下は既に決定論的に処理されている:

- **severity 分類**: [check-ci-coderabbit/src/main.rs:721-736](../../src/check-ci-coderabbit/src/main.rs) で first-line のマーカー (Critical / 🔴 等) からマッチ
- **resolved 判定**: `resolved:` reply prefix の検出で thread を outdated 扱い

一方、Claude が反復していたのは:

1. **finding ごとの「次の手」の判定** — 自動修正で済むのか / 人間判断が要るのか / false-positive 疑いか / 単なる informational か
2. **issue 要約の正規化** — `extract_issue` の bold-line fallback が `(詳細はコメント参照)` になるケース、コメント本文を読んで一行要約を生成し直す作業
3. (将来) **effort 見積もり** / **cross-finding clustering**

これらは「変換処理」であり推論深度を要しないため、ローカル LLM のオフロード対象として適している。

## 決定

新 crate を 2 本追加し、Ollama 経由で structured JSON を取得する分類層を構築する。**初版の対象は 1 と 2 のみ** (3 は予約フィールド・将来拡張)。

### crate 構成

| crate | 種類 | 責務 |
|---|---|---|
| `lib-ollama-client` | lib | Ollama HTTP API (`/api/generate`) を `format: "json"` で叩く blocking クライアント。プロンプトは保持しない |
| `cli-finding-classifier` | bin + lib | stdin から `Vec<Finding>` を受け、Ollama で classify した `Vec<ClassifiedFinding>` を stdout に出力 |

### 出力スキーマ

`lib-report-formatter::Finding` の全 field を `#[serde(flatten)]` で保持しつつ、以下を追加:

```rust
pub struct ClassifiedFinding {
    #[serde(flatten)]
    pub finding: Finding,
    pub action: String,            // "auto_fix" | "human_review" | "false_positive_likely" | "informational"
    pub action_confidence: f32,    // 0.0 - 1.0 (clamped)
    pub normalized_issue: Option<String>,  // 元 issue が不明瞭なときのみ
    pub fallback_reason: Option<String>,   // Ollama 失敗時に充填
}
```

### action カテゴリの境界

- **auto_fix** — mechanical, single location, 業務ロジック影響なし
- **human_review** — 副作用 / 多ファイル / 設計判断 / Critical state changes 等
- **false_positive_likely** — bot が文脈を読めていない明確な signal がある場合のみ
- **informational** — note / praise / explicitly optional

迷ったら `human_review` に倒す保守ルールをプロンプトに明示する。

### 失敗時の振る舞い (ブロックしない設計)

Ollama 不在 / timeout / parse 失敗 / invalid action は **fallback として `human_review` + `action_confidence=0.0` + `fallback_reason` を返す**。consumer (Claude / takt 等) は finding を失わず、後段で人間/Claude が判断できる。

この設計は「失敗してもリトライ可能 / Claude が後段で検証」という方針 (Phase A〜D dogfood で `human_review` fallback path が pipeline ブロック無しで運用 viable と実証) と整合する。

### Diagnostic logging の scope と移行条件 (PR #142 / Phase A)

JSON parse error 時の context overflow 診断 log は **`eprintln!` で stderr に出力する** 設計 (`src/lib-ollama-client/src/lib.rs` の `emit_overflow_diagnostic`)。これは現状の consumer (`cli-finding-classifier` / `cli-push-runner` lint_screen stage) がすべて CLI 起動で stderr を report に取り込む前提に基づく。

**structured logging (log / tracing crate) への移行条件**:

- `lib-ollama-client` が CLI 以外 (例: 長期常駐 daemon / web service) から呼ばれるようになった場合
- 複数 consumer が log level / target filter で diagnostic 出力を制御したいニーズが生じた場合
- log aggregation (Loki / Datadog 等) への連携を求める要件が出た場合

これらが揃うまでは `eprintln!` で十分。早期の structured logging 導入は依存追加 (log + env_logger / tracing + subscriber) のコストに対して得られる柔軟性が見合わない。

### 90% 閾値の rationale + tuning 方針 (PR #142 / Phase A)

`overflow_hint()` は `prompt_eval_count >= num_ctx * 0.90` で hint を emit する保守設計。**90% 採用根拠**:

- mistral:7b の prompt_eval_count は num_ctx の cap で clamp される (Ollama の internal 仕様、Phase B で実測確認)。100% 到達 = 確定 overflow だが、90% 到達 = overflow 寸前で同一症状を予兆できる
- false positive の負担は warn log 1 件のみ (block しない)、false negative (= overflow を見逃す) の方が debug cost が高い

**tuning 方針**:

- **Phase C/D dogfood で得た data が蓄積するまで本閾値を変更しない**。根拠なき早期変更で false positive 増加 / false negative 増加のいずれかに振れるリスクが高い
- Phase D 完了時 (3-5 PR 観測) に hint emit 件数と実 overflow 件数を突合し、precision / recall を確認して再評価
- 派生プロジェクト (techbook-ledger / auto-review-fix-vc) で diff 規模 / prompt 構造が異なる場合は、本リポジトリ baseline と別系統で再 calibration を検討

### プロンプト

`src/cli-finding-classifier/prompts/classify.txt` に同梱 (`include_str!`)。`--prompt-file` で差し替え可能。テンプレート placeholders は `{severity}` `{file}` `{line}` `{issue}` `{suggestion}`。

### 依存技術

- HTTP クライアント: **ureq 2.x** (blocking, tokio 不要、依存ツリー軽量)
- Ollama API: `POST /api/generate` with `format: "json"` + `temperature: 0.1` (再現性確保)
- モデル: `mistral:7b` (Q4_K_M, ~4.4GB) を default、`llama2:13b` は `--model` で切替可能

## 帰結

### Pros

- Claude の入力 token 削減 — finding triage の往復が減る
- レイテンシは GTX 3070 + mistral:7b で 1.4-3.6 秒/件 (実測)、5 件で 18 秒程度
- 既存決定論的抽出 (severity / file / line / resolved) を破壊せず augmentation に徹する
- Ollama 不在環境でも block しない (fallback で全件 human_review)
- `lib-ollama-client` は他用途 (将来の lint screen, PR description draft 等) でも再利用可能

### Cons / リスク

- mistral:7b の structured output は `format: "json"` で安定するが、prompt 違反 (action enum 外の文字列) を返す可能性ゼロではない → enum 外は fallback で吸収
- VRAM 8GB では `mistral:7b` 常駐 + `llama2:13b` swap 構成。同時起動は不可
- `normalized_issue` が日本語指示でも英語混じりで返る場合がある (実 dogfood で観測)。実害は小さいが prompt v2 の改善余地
- Windows shell 経由で日本語含む JSON を Ollama に渡すと CP932 漏れで壊れる事例あり (Phase 0 ベンチで実証)。Rust 実装は ureq の UTF-8 シリアライズで影響なし
- **プロンプトインジェクション**: `{issue}` / `{suggestion}` placeholder には CodeRabbit コメントの生テキストが展開されるため、adversarial な finding 内容により LLM 出力が操作される可能性がある (例: `false_positive_likely` への誤分類強制)。fallback で異常出力は `human_review` に倒れるため実害は限定的だが、Phase 5 (cli-pr-monitor 統合時) でサニタイズ or プレースホルダのブラケット化を検討する

### 試験運用 → 本採用への昇格条件 (Phase E、2026-05-15 充足)

以下を満たした時点で「試験運用」flag を外した:

1. ✅ 5 PR 以上の実 review サイクルで dogfood し、classification 妥当性を目視確認 (6 PR / 9 data points 累積、詳細は「採用判定 (Phase E、2026-05-15)」section)
2. ✅ `cli-pr-monitor` または takt facet への統合 (代替: `cli-push-runner` lint_screen stage として PR #132 で統合済、cost-aware 実装層選択により Rust stage に pivot — 詳細は「実装層選択: takt facet vs Rust stage」section)
3. ✅ Claude session の入力 token 削減効果は質的傾向観察で確認 (reviewer cross-check で advisory として消費される pattern が D-3〜D-7 で実証)

却下していた場合の手順 (参考、未実行):

- 本 ADR を **却下** ステータスに更新
- 2 crates (`lib-ollama-client`, `cli-finding-classifier`) + `cli-push-runner` lint_screen stage を削除

## 採用判定 (Phase E、2026-05-15)

### 累積 dogfood metrics

Phase A〜D で取得した 6 PR / 9 data points の集計:

| 指標 | 観測値 | 採用基準 | 判定 |
|---|---|---|---|
| dogfood PR 数 | 6 PR / 9 data points (D-3〜D-9) | 5 PR 以上 | ✅ |
| Fallback rate (累積) | 1/9 ≈ 11% (D-7 で 1 件 = Ollama HTTP timeout) | < 50% kill-switch | ✅ |
| Verdict variance | `auto_fix` / `informational` / `human_review` の 3 経路すべて観測 | 判定空間 cover | ✅ |
| FP rate | 5 件 (D-3〜D-6) すべて `minor` severity、reviewer cross-check で blocking なし | blocking 無し | ✅ |
| Structural fix (FP 対策) | Bundle k-1 (PR #155) = `*.md` ハンク除外フィルター land、D-8/D-9 で再現せず | 構造的解消 | ✅ |
| Pipeline integration | Phase D で 6 PR end-to-end 完走 (`cli-push-runner` lint_screen stage 経由) | 機能性確認 | ✅ |
| Reviewer cross-check | `review-simplicity.md` の advisory 読込で findings の真偽が reviewer 出力に反映 | qualitative 確認 | ✅ |

### 実装の最終構成

| 構成要素 | crate / file | 役割 |
|---|---|---|
| HTTP クライアント | `src/lib-ollama-client` | Ollama `/api/generate` blocking client (ureq 2.x、`format: "json"`、`DEFAULT_NUM_CTX = 32768`、`OllamaMetadata` 診断) |
| classification mode | `src/cli-finding-classifier` (`--mode classify`) | CodeRabbit findings の triage (auto_fix / human_review / false_positive_likely / informational) |
| lint-screen mode | `src/cli-finding-classifier` (`--mode lint-screen`) | diff stdin → `LintScreenResult` JSON (本採用の primary 経路) |
| pipeline 統合 | `src/cli-push-runner/src/stages/lint_screen.rs` | `cli-finding-classifier --mode lint-screen` を subprocess 起動、`.takt/lint-screen-report.md` 出力 |
| Markdown 除外 filter | `cli-push-runner` lint_screen stage 前段 (Bundle k-1 / PR #155) | `*.md` ハンク除外で diff-外 context hallucinate FP を構造的解消 |
| reviewer 連携 | `.takt/facets/instructions/review-simplicity.md` | reviewer が advisory として lint-screen-report.md を読込 |

### 実装層選択 (takt facet vs Rust stage、cost-aware)

lint_screen は当初 takt facet (Sonnet 動作) として設計されていたが、実装段階で「Sonnet 動作はコスト削減という主目的と矛盾」と判明し、`cli-push-runner` の Rust stage (mistral 直呼び) に pivot。将来の LLM 系 feature 設計時 (例: PR body draft) の prior assumption として codify:

- **takt facet (Sonnet) を選ぶ条件**: 意味的判断が必要、コスト感度低、Claude session の context 内で完結させたい
- **Rust stage (local mistral) を選ぶ条件**: コスト削減が主目的、決定論的判定が可能、latency 許容範囲 (~5-90s per invoke、ADR-040 reference table)
- **lint_screen の実例**: 当初 takt facet 想定 → cost 矛盾検出 → Rust stage に pivot (PR #132)

### Known failure modes (永続記録)

#### Failure mode 1: docs/Markdown context hallucinate (5 PR 連続観測 → Bundle k-1 で構造的解消)

- **観測**: D-3 (PR #148) / D-4 CR fix (PR #150) / D-5 ×2 (PR #151) / D-6 (PR #152) / PR #153 の 5 push events で「mistral:7b が docs-only diff や `.md` ファイルに対して Rust `unused-import` を hallucinate」する FP pattern が一貫して観測
- **Root cause**: LLM context window に hook source コード (例: `byte_offset_to_line` 周辺の `use std::io::Write;`) が混入 → 過去 commit / test fn 内の `use` 文を current diff として hallucinate → `unused-import` FP を生成
- **Structural fix**: Bundle k-1 (PR #155) で `cli-push-runner` lint_screen stage の前段に「`*.md` ハンク除外フィルター」を実装、diff 段階で `.md` 系ハンクを除外して LLM に渡さない。post-fix dogfood (D-8/D-9 = PR #155 self-dogfood) で再現せず
- **将来別モデル評価時の prior**: LLaMa / phi 等で同 failure mode を検証する出発点として `Markdown 除外フィルター` が必要かを最初に評価する

#### Failure mode 2: Ollama HTTP timeout (D-7、運用上の新軸)

- **観測**: D-7 (PR #154) で `os error 10060` = HTTP 接続 layer timeout により mistral:7b 到達前に fallback
- **設計通り**: `screen_decision = "human_review"` で soft-fail、pipeline 完走 (757s で simplicity-review 独立 APPROVE)
- **運用 implication**: Ollama サーバ可用性が新軸として顕在化。累積 1/9 ≈ 11% で kill-switch 50% との距離は十分確保、ただし定常運用では Ollama 起動状態を session 開始時に確認することを推奨

#### Failure mode 3: Phase b' v2 attention dilution (historical、prompt tuning prior)

- **観測**: Phase b' v2 で prompt example に full diff header (`--- a/<path>` `+++ b/<path>`) を追加した結果、agreement rate が 75% → 50% に 33pt 低下
- **Root cause**: anti-hallucination preamble の効果が context scarcity で打ち消される。large few-shot context は新規問題への generalization を阻害する pattern
- **将来 LLM prompt tuning 時の prior**: effective signal-to-noise ratio を保つ context budgeting を最優先、few-shot example の追加は necessarily 改善ではなく劣化の可能性がある

### num_ctx 拡大の根因解析 (Phase A〜C)

Phase B で `prompt_eval_count == num_ctx` (100% 到達) を観測し、JSON 出力欠落の真因が `num_ctx truncation` と確定。Phase C で `DEFAULT_NUM_CTX = 8192 → 32768` に拡大し、3 PR replay で fallback rate 100% → 33% を実証。詳細 trade-off (VRAM / latency / step_timeout 比例係数) は [ADR-040](adr-040-local-llm-context-size.md) に migrate 済 — 派生プロジェクトへの porting 時は ADR-040 reference を参照。

## 本 ADR の scope 外 (将来 ADR 候補)

- `cli-pr-monitor` poll stage への classifier 統合 (本採用後、cli-push-runner lint_screen と並列に検討)
- takt facet 経由でのプロンプト DI 設計
- `effort` / `cross-finding clustering` field の実装
- 他用途 (PR description draft = 提案 3 / §8.F) への `lib-ollama-client` 流用

## eval の起動 — env opt-in の手動実行 (T1 / PR #279、2026-07-16)

lint-screen eval (`run_lint_screen_against_all_fixtures`) は **`LINT_SCREEN_EVALS=1` を設定したときだけ走る**。未設定なら skip メッセージを出して即 return する。

```sh
LINT_SCREEN_EVALS=1 cargo test -p cli-finding-classifier --test lint_screen_evals \
  -- --ignored --nocapture run_lint_screen_against_all_fixtures
```

前提: Ollama 起動 + `mistral:7b` pull 済。判定は出力 (agreement rate / confusion matrix / verdict) を人間が読んで行う。

### なぜ `#[ignore]` だけでは足りなかったか

`#[ignore]` は「`--ignored` を付けたら走る」でしかなく、このリポジトリでは `--ignored` を**無条件で付ける自動経路が 2 つ**ある:

1. `push-runner-config.toml` の quality_gate — `cargo test -- --ignored --test-threads=1`
2. takt fix step の検証義務 (`.takt/facets/instructions/fix.md`) — 同じコマンドを実行

結果、計測専用テストが毎 push (fix があれば 2 回) 走っていた。塞ぐ場所をコマンド側でなく**テスト側**にしたのは、呼出箇所が複数あり漏れるため。

### なぜ自動 gate から外すのが正しいか

このテストは **assert を持たない**。`report_summary` は println のみで、agreement rate が閾値を割っても pass する。つまり自動 gate では 15 fixture 分の mistral:7b 実呼出の時間だけを払い、何も検証しない。gate に置く意味がない一方、モデル/プロンプト変更時の人手評価には価値があるため、削除ではなく opt-in 化した。

### 実測 (2026-07-16、RTX PRO 5000 48GB)

| 対象 | before | after |
|---|---|---|
| eval テスト単体 | 41.3s | 0s (skip) |
| `cargo test -- --ignored` スイート全体 | 63s | 21s |

`step_timeout` を 600s に上げた根拠だった「`--ignored` 全体 269s」は本実測で再現しなかった (63s)。[ADR-040](adr-040-local-llm-context-size.md) 記録時の GPU (RTX 3070 8GB) から更新済みで、mistral:7b の推論が当時より大幅に速いため。**ADR-040 の resource 数値は stale** であり、それを前提にした見積りは疑うこと。除外を受けて `step_timeout` は 300s に right-size した (根拠は `push-runner-config.toml` のコメント履歴)。

なお本実測時の agreement rate は **86.7% (13/15、GO)** で、Phase b' 記録の 75% (conditional GO) から改善している。num_ctx 32768 化 (Phase C) の効果と見られるが、`[lint_screen] enabled = false` のままなので採否判断は変更しない。

## eval fixture 設計の 3 軸

`src/cli-finding-classifier/evals/files/eval*.diff` は LLM の挙動を測定するための **合成 fixture** であり、現実のコードではない。fixture 追加・編集時は以下の 3 軸を file 先頭コメントで明示すること (PR #130 → Phase b' 拡張で codify):

| 軸 | 内容 | 例 |
|---|---|---|
| **issue_pattern** | この fixture が含む lint 観点 | `unused-import` / `deep-nesting` / `magic-number` / `clean (FP 検知)` / `multi-issue mixed` / `existing-lint-overlap` |
| **expected_screen_decision** | baseline で期待される screen_decision | `auto_fix` / `human_review` / `informational` |
| **verification_purpose** | 何を測りたいか (recall / precision / boundary / context-handling 等) | 「4 levels 境界でも flag しないか」「N=4 unused-import の取りこぼし測定」 |

### 標準コメントヘッダ

各 `eval*.diff` の先頭に以下フォーマットでコメントブロックを置く (diff の `diff --git` 行より前):

```text
# SYNTHETIC FIXTURE: eval3-magic-number
# issue_pattern: magic-number 検出
# expected_screen_decision: auto_fix
# verification_purpose: 複数 magic-number (5 と 30000) の取りこぼし検証
# Note: dead-code (delay_ms > 30000 unreachable guard) は意図的、検出対象外
```

LLM 入力時には runner が `#` で始まる leading 行を skip し `diff --git` 以降のみを LLM に渡す (= コメントは LLM の挙動に影響しない、reviewer 用ドキュメント)。

**適用範囲**: Phase b' 以降に追加する新規 fixture には必須。Phase a 既存 6 件 (eval1-6) は backfill 任意 (LLM 挙動への影響はないが、reviewer 視認性向上には寄与)。

### 由来

- PR #130 review で eval3 の `delay_ms > 30000` unreachable guard が「dead-code 観点で fixture 品質低い」と CodeRabbit に指摘された。意図 (`magic-number` 検出専用 fixture) を comment header で明示すれば reviewer の往復が減る
- post-merge-feedback T3-2 (Frequency Medium / Effort S / Adoption Risk None) として採用
- Phase b/c/d で fixture 追加が継続するため、設計意図のドリフトを構造的に防ぐ

## classify モデル格上げの評価と見送り (2026-07-05 追記、WP-04)

### 動機と方法

classify モードの精度向上 (特に `false_positive_likely` 判定改善による下流の無駄 fix 削減) を狙い、`mistral:7b` からの格上げ候補を実測評価した。方法は本 ADR Phase a の lint-screen eval と同じく **Claude (Opus) 出力を gold baseline** とする:

- eval セット: real CodeRabbit findings 30 件 (過去 PR harvest) + キュレート FP 例 5 件 = **35 件**。各 finding に Opus が gold action (`auto_fix` / `human_review` / `false_positive_likely` / `informational`) を付与。
- 候補: `mistral:7b` (現行 baseline) / `gemma4:12b` (中 dense) / `gemma4:26b` / `gemma4:31b` / `qwen3-coder:30b`。
- 指標: gold 一致率 (accuracy) / FP 処理 / human_review 安全軸 / latency / VRAM。

### 実測結果 (RTX PRO 5000 48GB、num_ctx 8192)

| モデル | accuracy | FP→auto_fix (有害) | human_review 誤送 auto_fix (危険) | invalid | latency 中央 | VRAM |
|---|---|---|---|---|---|---|
| **mistral:7b** | 0.63 | 4/6 | **0/14 (完璧)** | 0 | 0.35s | 5.6GB |
| gemma4:12b | 0.57 | 4/6 | 0/14 | 3 | 0.75s | 8.4GB |
| gemma4:26b | 0.20 | — | — | **27 (破綻)** | 1.46s | 17.6GB |
| gemma4:31b | 0.57 | 3/6 | 0/14 | 0 | 1.49s | 20.9GB |
| qwen3-coder:30b | 0.69 | 3/6 | **1/14 (安全後退)** | 0 | 0.34s | 19.4GB |

### 決定: 格上げを見送り、`mistral:7b` を維持

- **FP 検出は全モデルで未達**: gold FP 6 件のうち正しく `false_positive_likely` にできたのは最良 (qwen3-coder) でも 1 件、全モデルが 3〜4 件を有害な `auto_fix` に誤分類。計画の主目的 (FP 判定改善) を満たす候補が無い。→ これは mistral 固有ではなく、この規模の局所 LLM の **能力上の限界**。
- **mistral:7b は安全軸で完璧**: 人間判断を要する finding (design / state / concurrency / security) を一度も `auto_fix` に倒さない。プロンプトの保守バイアス (「迷ったら human_review」) が正しく機能。accuracy が僅かに上 (+0.06) の qwen3-coder は逆に human_review を 1 件 auto_fix に誤送する **安全後退**を起こす。分類器の目的 (安全な triage) では、accuracy より「人間判断案件を auto_fix に倒さない」保守性が優先される。
- **中型 dense は劣化・破綻**: gemma4:12b / 31b は mistral より accuracy が低く、gemma4:26b は 35 件中 27 件で invalid action を返し破綻 ([ADR-046](adr-046-local-llm-review-spike.md) WP-01 で観測した gemma4:26b の大入力破綻と同系)。
- accuracy 差 (0.63 vs 0.69) は temperature 0.1 のばらつき (±0.03) と同程度で有意でない。mistral:7b は最軽量 (5.6GB) ・最速 (0.34s) ・安全軸完璧のため、格上げを正当化する候補が無い。

### 再利用可能な知見

- **classify モードの eval 手法**: Opus gold baseline との action 一致率 + FP 処理 + 安全軸 (human_review→auto_fix 誤送) の 3 軸評価は、将来のモデル/プロンプト再評価に再利用できる (`cli-finding-classifier` の lint-screen eval と同型)。
- **保守バイアスは分類器の安全機能**: 「迷ったら human_review」は accuracy を下げるが、誤自動修正リスクを構造的に抑える。格上げ候補は accuracy だけでなく安全軸で評価すべき。
- **accuracy と安全軸は独立指標として評価する ([ADR-043](adr-043-security-gates-fail-closed.md) と同根、順位260)**: model evaluation では gold 一致率 (accuracy) と**安全軸 (human_review → auto_fix への誤送ゼロ)** を独立に測り、両者が trade-off するときは**安全軸を優先**する。WP-04 で accuracy 最上位 (0.69) の qwen3-coder を見送り accuracy 下位 (0.63) の mistral:7b を維持したのは本原則の適用例。これは [ADR-043](adr-043-security-gates-fail-closed.md) の fail-closed 原則 (判定不能時は安全側 = block にデフォルト) を gate 層でなく**助言/分類層に一般化**したもの ―「不確実性は楽観 (auto_fix) でなく保守 (human_review) に倒す」という同じ設計思想であり、accuracy 改善が安全後退を隠しうる tension を明示する。
- **FP 検出はプロンプト再調整の余地**: `classify.txt` は mistral 向けに tune 済み。FP 検出強化プロンプトで能力限界かプロンプト不適合かを切り分ける follow-up は順位 256。候補の VRAM/latency 実測は [ADR-040](adr-040-local-llm-context-size.md) の新 GPU 再 calibration (順位 255) にも供する。

### 妥当性の脅威

- gold は Opus 判断で `auto_fix` にやや寛容な一方、`classify.txt` は保守設計 (迷ったら human_review) のため、auto_fix/human_review 軸の一致率は過小評価気味。ただし結論を支える 2 軸 (FP 全滅・human_review 安全軸) はこの較正差に頑健。
- N=35 (FP 6 件) と小さい。ただし 5 モデルで一貫した傾向。

## 関連

- [ADR-018: cli-pr-monitor の takt ベース移行](adr-018-pr-monitor-takt-migration.md)
- [ADR-026: Cargo workspace](adr-026-cargo-workspace.md) — workspace member 追加方針
- [ADR-034: CodeRabbit 監視・自動化戦略](adr-034-coderabbit-auto-monitoring.md) — 監視層との将来統合先
- [ADR-039: 試験運用標準パターン](adr-039-experimental-feature-standard-pattern.md) — 3 点セット (opt-in / kill-switch / bounded lifetime) の標準化
- [ADR-040: Local LLM Context Size と Resource Trade-off](adr-040-local-llm-context-size.md) — Phase A〜C num_ctx empirical data の永続記録
- [ADR-043: Security/Quality Gate での Fail-Closed 原則](adr-043-security-gates-fail-closed.md) — WP-04 の「accuracy 向上 ≠ 安全性維持」tension が同 ADR の fail-closed 思想 (不確実性は安全側に倒す) の助言/分類層への一般化 (順位260)
