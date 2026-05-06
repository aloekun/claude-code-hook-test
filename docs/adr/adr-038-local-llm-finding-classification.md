# ADR-038: ローカル LLM による CodeRabbit findings classification

## ステータス

試験運用 (2026-05-06)

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

Ollama 不在 / timeout / parse 失敗 / invalid action は **fallback として `human_review` + `confidence=0.0` + `fallback_reason` を返す**。consumer (Claude / takt 等) は finding を失わず、後段で人間/Claude が判断できる。

これは [docs/local-llm-offload-analysis.md](../local-llm-offload-analysis.md) §6 の方針 (「失敗してもリトライ可能 / Claude が後段で検証」) と整合する。

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
- Windows shell 経由で日本語含む JSON を Ollama に渡すと CP932 漏れで壊れる事例あり ([docs/local-llm-offload-analysis.md](../local-llm-offload-analysis.md) ベンチで実証)。Rust 実装は reqwest/ureq の UTF-8 シリアライズで影響なし
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

## 関連

- [docs/local-llm-offload-analysis.md](../local-llm-offload-analysis.md) — 本 ADR の origin 調査レポート
- [ADR-018: cli-pr-monitor の takt ベース移行](adr-018-pr-monitor-takt-migration.md)
- [ADR-026: Cargo workspace](adr-026-cargo-workspace.md) — workspace member 追加方針
- [ADR-034: CodeRabbit 監視・自動化戦略](adr-034-coderabbit-auto-monitoring.md) — 監視層との将来統合先
