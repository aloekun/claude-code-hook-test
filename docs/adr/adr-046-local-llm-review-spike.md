# ADR-046: ローカル LLM pre-push レビュアー — 選定スパイクと不採用判断

> 採番 046 は暫定 (land 時に空き番号で確定 — [ADR-039](adr-039-experimental-feature-standard-pattern.md) 配下 / 順位 135 placeholder policy)。

## ステータス

却下 (2026-07-04、選定スパイクの negative result により local_review stage の実装を見送り)

> 本 ADR は実装機構を持たず、[ADR-040](adr-040-local-llm-context-size.md) と同様に「実施したスパイクの empirical data + 判断根拠」を permanent record として固定する性格を持つ。CodeRabbit 往復削減を目的にローカル LLM の push 前レビューを挟む提案 (以下「local_review 案」) を、実測に基づき却下する。

## コンテキスト

### スパイクの目的

CodeRabbit 無料枠のレートリミット (3 件/時) が運用上の最大ボトルネックであり、push 前にローカル LLM で「CodeRabbit が出す指摘を先取りするレビュー」を挟めば往復を削減できる、という仮説を検証する。判断基準は **CodeRabbit findings に対する再現率**: ローカルモデルが同じ問題を push 前に surface できれば、著者が事前修正でき CodeRabbit 到達時に指摘が減る。

受け入れ基準 (事前定義): **再現率 50% 未満なら local_review 案 (実装フェーズ) を中止**する。

### 実測環境

- GPU: **NVIDIA RTX PRO 5000 Blackwell 48GB VRAM** ([ADR-038](adr-038-local-llm-finding-classification.md) / [ADR-040](adr-040-local-llm-context-size.md) が前提とする RTX 3070 8GB から更新済み。全候補モデルが 100% GPU で稼働し VRAM は制約にならない)。
- Ollama 0.30.10。
- 候補モデル (`ollama show` 実測、全 Q4_K_M):

| モデル | arch | params | context | 種別 |
|---|---|---|---|---|
| qwen3-coder:30b | qwen3moe | 30.5B | 262144 | MoE・コーディング特化 |
| gemma4:31b | gemma4 | 31.3B | 262144 | dense |
| gemma4:26b | gemma4 | 25.8B | 262144 | MoE (active 小) |
| mistral:7b | llama | 7.2B | 32768 | baseline (ADR-038 現行 classifier) |

### 評価データ (ground truth)

`check-ci-coderabbit --list-findings` を PR #90〜#242 に走査し、CodeRabbit の**未解決インライン findings** を **67 件 / 33 PR** 取得 (code=41, docs=24, config=2 / Major 35, Minor 30, Critical 2)。各 finding は file / line / severity / 要約を持つ。ツールは未解決スレッドルートのみ返す (`resolved:` 返信済みは除外) ため、これは歴史的 findings の下限。

パイロットは 5 PR / 23 findings (#233, #131, #113, #91, #97) を対象とし、diff は jj の squash commit から再構成 (全て num_ctx 32768 に収まるサイズを選定)。評価ハーネス (Node.js) は Ollama `/api/generate` をストリーミング呼び出し (`format:json`, temperature 0.1, num_ctx 32768, num_predict 4096)、「徹底的な senior reviewer」プロンプトで各モデルに diff をレビューさせ、findings を ground truth と照合した。

## 決定

**local_review 案を却下する** (push 前ローカル LLM レビュー stage を実装しない)。パイロットの再現率が受け入れ基準 50% を大きく下回り、かつ過剰検出が深刻なため。

### 実測結果

| モデル | 自動recall (行±25、上限甘) | 意味的recall (実力) | 過剰検出率 | latency 中央 | latency 最大 | VRAM |
|---|---|---|---|---|---|---|
| qwen3-coder:30b | 0.35 | 約 13% | 0.87 | 8.3s | 26.5s | 21.8GB |
| gemma4:31b | 0.26 | 約 9% | 0.77 | 23.4s | 26.5s | 20.9GB |
| gemma4:26b | 0.09 | 約 4% | 0.33 | 2.8s | 10.9s | 17.6GB |
| mistral:7b | 0.13 | 約 9% | 0.69 | 32.9s | 41.1s | 8.9GB |

- **どのモデルも意味的再現率 50% に遠く及ばない** (23 findings に対し union で約 6 件 ≈ 26%、厳密一致では約 3-4 件 ≈ 15%、単一最良モデルで約 13%)。
- 自動 recall 最良の qwen3-coder (0.35) は、大量に findings を出して GT 行の近くに偶然一致する **spray による水増し** (過剰検出率 0.87)。実力は約 13%。
- CodeRabbit findings の**約 4 割は docs / 合成 fixture** で、コードレビュープロンプトでは原理的に検出不能。
- コードファイル内でも、ローカルモデルは CodeRabbit とは**別の (多くは妥当だが異なる) 問題**を指摘する。意味的重複が低い。
- **過剰検出が深刻** (0.69〜0.87): push 前に挟むと CodeRabbit 指摘を先取りするどころか大量のノイズを足す。目的 (往復削減) に逆行する。

### 較正の過程で得た知見 (それ自体が再利用価値のある成果)

1. **再現率はプロンプトのフレーミングに強く依存する**。既存 lint-screen プロンプト由来の「flag しすぎるな・空が正解」抑制フレーミングでは qwen3-coder / gemma は findings 0 件になる。徹底フレーミングに変えて初めて出す。→ ローカル LLM レビューは prompt calibration に極めて敏感で、単一プロンプトの数字を過度に一般化できない。
2. **モデル別の固有失敗モード** (本リポの diff 規模で観測):
   - qwen3-coder:30b — 大 diff で退行的反復ループ (同一指摘を数十行にコピー) → num_predict 上限で JSON truncation。
   - gemma4:26b — 30KB 超の diff で沈黙 (diff は読むが `{"findings":[]}` を返す)。5 PR 中 4 PR で 0 件。
   - gemma4:31b — 中央 23s と遅い割に recall 低。
   - mistral:7b — 広範レビューで暴走生成 (非ストリーミングだと 300s タイムアウト)、最遅。
3. **行番号ベースの自動マッチは spray で水増しされる**ため、意味的判定が必須。モデルとレビュアーは同じ問題を別の行にアンカーする。

### 妥当性の脅威 (限界)

- N が小さい (5 PR / 23 findings)。ただし 4 モデル × 5 PR で一貫して 50% に届かず、シグナルは強い。
- プロンプトは未最適化。ただし失敗モードは「保守的すぎ」ではなく「CodeRabbit と別の問題を指摘」= 能力/整合の gap で、prompt tuning で 50% まで届く見込みは薄い。
- ground truth が未解決スレッドのみで母集団が偏る可能性。

## 帰結

### 却下による影響

- push 前 local_review stage は実装しない (`push-runner-config.toml` への `[local_review]` 追加、cli-push-runner への stage 追加は行わない)。
- CodeRabbit 往復削減は別アプローチ (レビュー対象の絞り込み = `.coderabbit.yaml` 設定、fix の push 束ね等) で追求する。

### 保持する価値

- **ground truth harvest の手法は再利用可能**: `check-ci-coderabbit --list-findings` を PR レンジに走査 → file/line/severity/要約付き findings を集約する評価データ生成法は、[ADR-038](adr-038-local-llm-finding-classification.md) 系の classifier eval や将来の LLM 機能評価に転用できる。
- **GPU 更新の事実**: [ADR-040](adr-040-local-llm-context-size.md) の実測値 (RTX 3070 8GB 前提) は陳腐化した。VRAM ではなく latency が実効制約になった。ADR-040 の再 calibration を follow-up とする (順位 255)。
- **独立レビュアーとしての別価値**: ローカル LLM は CodeRabbit と重複しない別の妥当な問題も出す。ただし過剰検出 (0.69〜0.87) に埋もれるため、「CodeRabbit 先取り」とは異なる premise であり、本 ADR の scope 外。必要なら別途評価する。

### classifier (ADR-038) との違い

本スパイクが却下したのは **review (open-ended な問題発見)** タスクであり、ADR-038 が採用している **classification (既知 findings の triage)** や lint-screen (狭い正規ルールの検出) は別タスク。分類は入力が構造化され判定空間が閉じているためローカル LLM が機能する。本結論は分類層の採用判断に影響しない。

## 関連 ADR

- ADR-038: ローカル LLM による CodeRabbit findings classification — 分類層 (本 ADR が却下した review 層とは別タスク、採用継続)
- ADR-039: Experimental feature 標準パターン — 本 ADR の判断形式 (bounded lifetime = スパイクで採否決定) の基盤
- ADR-040: Local LLM Context Size と Resource Trade-off — 本スパイクで GPU 更新により実測値が陳腐化、再 calibration の follow-up (順位 255)

## 由来

- 2026-07-04 のハーネス改善計画で定義されたローカル LLM レビュアー選定スパイクを実施。CodeRabbit 往復削減の仮説を、過去 PR の CodeRabbit findings を ground truth とした実測で検証し、再現率が受け入れ基準を満たさないため local_review 案を却下した。
