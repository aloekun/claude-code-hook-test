# ADR-038: ローカル LLM による CodeRabbit findings classification

## ステータス

試験運用 (2026-05-06)

> 本 ADR の運用パターンは [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) で標準化された 3 点セット (config opt-in / kill-switch / bounded lifetime) の対象。本採用判定または却下時に ADR-039 の retirement workflow に従う。

## コンテキスト

Claude Code セッションにおける反復作業の token 消費を抑える目的で、[docs/local-llm-offload-analysis.md](../local-llm-offload-analysis.md) で 3 層構造 (思考層 / 実行層 / 制御層) のオフロード戦略を提案した。GTX 3070 + Ollama (mistral:7b) でローカル推論可能な範囲を切り出す。

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

これは [docs/local-llm-offload-analysis.md](../local-llm-offload-analysis.md) §6 の方針 (「失敗してもリトライ可能 / Claude が後段で検証」) と整合する。

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
- Windows shell 経由で日本語含む JSON を Ollama に渡すと CP932 漏れで壊れる事例あり ([docs/local-llm-offload-analysis.md](../local-llm-offload-analysis.md) ベンチで実証)。Rust 実装は ureq の UTF-8 シリアライズで影響なし
- **プロンプトインジェクション**: `{issue}` / `{suggestion}` placeholder には CodeRabbit コメントの生テキストが展開されるため、adversarial な finding 内容により LLM 出力が操作される可能性がある (例: `false_positive_likely` への誤分類強制)。fallback で異常出力は `human_review` に倒れるため実害は限定的だが、Phase 5 (cli-pr-monitor 統合時) でサニタイズ or プレースホルダのブラケット化を検討する

### 試験運用 → 本採用への昇格条件

以下を満たした時点で「試験運用」flag を外す (本 ADR を更新):

1. 5 PR 以上の実 review サイクルで dogfood し、classification 妥当性を目視確認
2. `cli-pr-monitor` または takt facet への統合 (本 ADR の **scope 外**、別 PR で扱う)
3. Claude session の入力 token 削減効果が体感で確認できる

却下する場合:

- 本 ADR を **却下** ステータスに更新
- 2 crates (`lib-ollama-client`, `cli-finding-classifier`) を削除
- [docs/local-llm-offload-analysis.md](../local-llm-offload-analysis.md) の引退条件に従い同ファイルも削除

## 本 ADR の scope 外 (将来 ADR 候補)

- `cli-pr-monitor` poll stage への統合 → 別 PR (Phase 5 相当)
- takt facet 経由でのプロンプト DI 設計
- `effort` / `cross-finding clustering` field の実装
- 他用途 (PR description draft, lint screen) への `lib-ollama-client` 流用

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

## 関連

- [docs/local-llm-offload-analysis.md](../local-llm-offload-analysis.md) — 本 ADR の origin 調査レポート
- [ADR-018: cli-pr-monitor の takt ベース移行](adr-018-pr-monitor-takt-migration.md)
- [ADR-026: Cargo workspace](adr-026-cargo-workspace.md) — workspace member 追加方針
- [ADR-034: CodeRabbit 監視・自動化戦略](adr-034-coderabbit-auto-monitoring.md) — 監視層との将来統合先
