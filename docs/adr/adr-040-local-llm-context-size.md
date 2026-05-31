# ADR-040: Local LLM Context Size と Resource Trade-off

## ステータス

試験運用 (2026-05-12)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) 配下の knowledge record。本 ADR 自体は実装変更を持たず、ADR-038 (Local LLM finding classification) で進行中の Phase D/E で得た empirical data を permanent record として固定する性格を持つ。

## コンテキスト

ADR-038 配下の lint_screen / finding-classifier では `lib-ollama-client` の `DEFAULT_NUM_CTX` を Phase A → Phase C で `2048 → 8192 → 16384 → 32768` と段階的に拡大した。各段階で観測した latency / VRAM 使用量 / `step_timeout` 整合性は、将来 num_ctx を再選定する局面 (派生プロジェクトへの porting / 別 model 採用 / diff 規模変化) で再利用価値の高い empirical data だが、現状は以下に分散していて参照が難しい:

- `src/lib-ollama-client/src/lib.rs` L128-139 の dogfood evolution コメント
- `push-runner-config.toml` L8-10 の step_timeout 履歴コメント
- 旧 `docs/local-llm-offload-analysis.md` (ephemeral 計画書、Phase E 採用昇格 = 2026-05-15 に retire 済)

ephemeral artifact (旧 analysis.md) には permanent data を残さない原則 (`~/.claude/rules/common/docs-governance.md` § Ephemeral 大規模コンテンツの ADR 昇格基準) に従い、Phase D/E 進行中で再利用機会が高い本 data を ADR として固定する。

## 決定

`mistral:7b` を Ollama 経由で本リポジトリの lint_screen / finding-classifier 用途で利用する際の **context size 選定の trade-off** を以下に codify し、ephemeral 経路 (lib.rs コメント / analysis.md) を本 ADR への参照に置き換える。

### 実測値 (mistral:7b on RTX 3070 8GB)

| num_ctx | VRAM 使用量 | per-invoke latency (lint-screen prompt + 200-500 行 diff) | overflow 発生 |
|---|---|---|---|
| 2048 (Ollama default) | ~400MB | 評価不可 (prompt 単体で overflow) | 確実 |
| 4096 | ~450MB | (実測スキップ、PR #135 で 8192 へ直接) | 確実 |
| **8192 (Phase b'/c MVP)** | ~512MB | **5-20s** (median ~7s、Bundle i evals 15 件 p50=4.6s / p95=8.4s) | 大規模 diff (200+ 行) で発生 (Bundle i eval13 / eval15) |
| 16384 (Phase A 試行) | ~1GB | ~15-40s | PR #141 (487 行 diff) で 100% overflow 再観測 |
| **32768 (Phase C 確定値、mistral:7b theoretical max)** | ~2GB | **30-90s** (mean ~50s、3 PR replay 平均) | 確認なし (487 行 diff まで) |

### Trade-off 軸 (context 選定時の判断基準)

| 軸 | 8K | 32K |
|---|---|---|
| **Latency** | 5-20s/invoke = UX 許容範囲 | 30-90s/invoke = pipeline 滞留が顕在化 |
| **Memory** | 512MB = 同時に他 model 起動可能 | 2GB = `mistral:7b` 単独占有、`llama2:13b` swap 不可 |
| **Accuracy** | 大規模 diff で truncation → fallback rate 高 (Bundle i 73.3% agreement) | overflow 解消 → fallback rate < 50% に低下 (Phase C smoke で 33% 達成) |
| **Timeout 整合性** | `step_timeout = 180s` で 12 件 mistral invoke ([cargo test -- --ignored]) を完走 | `step_timeout = 600s` (= 3.33x) が必要、`push-runner-config.toml` 側で整合化 |

### `step_timeout` 比例係数の根拠

`push-runner-config.toml` の `step_timeout` は num_ctx に対して **sublinear** に拡大する (context 4x = 8K → 32K に対して timeout は 3.33x = 180s → 600s)。per-token budget で見ると `180s / 8192 = 22 ms/token` ↔ `600s / 32768 = 18.3 ms/token` で、大規模 context の方が per-token 推論コストがわずかに低い (KV cache の locality 効果と推定):

- Phase b' (8K): 180s で 12 件 mistral invoke (`cargo test --ignored`) を完走
- Phase C (32K): 269s 観測 (= 180s 超過、cargo test 全体) → 600s に拡大
- per-invoke latency は num_ctx に対して**ほぼ線形**だが、KV cache locality 効果でわずかに sublinear (`22 ms/token` → `18.3 ms/token`、17% 改善)

**実測値 vs 線形 derivation の使い分け** (派生プロジェクトでの porting 時の判断指針):

- **実測値 (600s) を正規採択**: Phase C cargo test で 269s 観測 → 2x safety margin で 600s。本 ADR が定義する canonical 値。
- **線形 derivation (= 720s) は保守上限見積もり**: per-token 不変を仮定した線形スケール (`180s × (32768 / 8192) = 180s × 4 = 720s`) は KV cache locality を無視するため過大評価。新規 model / 未測定環境での fallback ceiling として使う (per-token 表示 `22 ms/token × 32768 ≈ 721s` は丸め由来の誤差、canonical な係数は線形式の 720s)。
- **canonical 600s が線形 ceiling 720s を下回る差 (= 120s, 17%) が sublinear 性の定量表現**: 大規模 context ほど KV cache の locality により per-token 推論コストが下がるため (`≈22 ms/token → ≈18.3 ms/token`、同じく 17% 改善で reference table の 600s と整合)。この gap は model-specific で、別 model (llama2:13b 等) では sublinear 係数が変わるため再 calibration 必須。

**reference 値** (派生プロジェクトでの derivation 用):

| num_ctx | 採用 step_timeout | 根拠 |
|---|---|---|
| 8192 | 180s | Phase b' 実測 (12 件 mistral invoke、cargo test --ignored 完走) |
| 32768 | 600s | Phase C 実測 (269s 超過観測 → 2x margin で確定、線形 ceiling 720s を sublinear 効果で 17% 下回る) |

reference table の **600s (canonical / 実測由来)** と上記係数 section の **720s (線形 ceiling / `180s × 4`)** の差 120s は KV cache locality による sublinear 効果分。未測定環境では安全側の 720s を初期 ceiling に置き、実測 cargo test が取れ次第 600s 系の sublinear 補正に寄せる。派生プロジェクトでは上記 reference 値を最初の見積もりに使い、実測 cargo test 経過時間の **2x margin** で補正する (例: 観測 250s → 500s に設定)。

### Context 選定の判断 flow

新規 LLM 系 feature 導入時 / num_ctx 再選定時の判断順序:

1. **prompt + 想定入力の token 量を実測** (`prompt_eval_count` を Phase A diagnostic log で取得可)
2. token 量の **1.5x を初期 num_ctx 候補** とする (margin で truncation 回避)
3. 候補値が `mistral:7b theoretical max (32768)` を超える場合は、prompt 圧縮 / diff truncation / 別 model (llama2:13b 等 8K context) への切替を検討
4. 選定した num_ctx に対して上記 reference table から initial `step_timeout` を取り、実測 cargo test 経過時間の 2x margin で補正
5. dogfood で fallback rate / latency p95 を実観測し、`overflow_hint` (ADR-038、90% 閾値) の emit 件数を監視

## 帰結

### Pros

- num_ctx 再選定時の判断基準が ADR として permanent 化、ephemeral 計画書 retire 時の data loss を防ぐ
- `step_timeout` 比例係数の根拠が明示化、将来の derivation 時に再発見コストが消える
- `src/lib-ollama-client/src/lib.rs` の evolution コメントが本 ADR への 1-line reference で済み、code comment 肥大化が解消

### Cons / リスク

- mistral:7b 固有の実測値のため、別 model (llama2:13b / qwen2.5:7b 等) では再 calibration が必要
- RTX 3070 8GB の VRAM 制約に依存する値、より大容量 GPU では memory 軸の trade-off が変わる
- 実測 latency は warm context 前提、cold start (model load 直後) では 1.5-2x の variance がある

### 試験運用 → 本採用への昇格条件

ADR-038 が「採用」に昇格 (Phase E 完了 = 2026-05-15) したため、本 ADR の data は採用 ADR の dependent knowledge として本採用相当扱い (本 ADR のステータス更新は次回派生 project porting 等で再 calibration が発生した時に「採用」へ昇格)。

## 関連

- [ADR-038](adr-038-local-llm-finding-classification.md) — Local LLM finding classification、本 ADR の data 元
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン、本 ADR の運用基盤
- `src/lib-ollama-client/src/lib.rs` — `DEFAULT_NUM_CTX = 32768`、Phase C 確定値
- `push-runner-config.toml` L8-10 — `step_timeout = 600` の根拠
