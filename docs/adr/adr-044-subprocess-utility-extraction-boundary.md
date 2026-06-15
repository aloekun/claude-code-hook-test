# ADR-044: subprocess utility extraction の境界判定 — 共通化と分離の線引き

## ステータス

試験運用 (2026-06-15)

> ADR-039 (Experimental feature 標準パターン) に準拠: config opt-in なし (本 ADR は decision criteria であり実装機構ではないため該当しない) / kill-switch = 本 ADR を supersede する後続 ADR で停止可能 / bounded lifetime = 採用判定 6 ヶ月 (2026-12-15) を目安に dogfood 結果から本採用 / 修正 / 却下を判定。

## コンテキスト

### 問題

順位 173 (subprocess utils 5 crate 重複を `lib-subprocess` に extract) を 173a-e の 5 sub-PR に分割して実施した結果、`lib-subprocess` には複数 variant の subprocess utility が export された (PR #205 / #206 / #207 / #208 で land)。

最終 sub-PR 173e で「variant 共存維持 vs merge」を判断する局面で、**「ここまでは共通化、ここから先は共通化しない」という境界の判断根拠を ADR に残しておくべき** という指摘を受けた。理由は以下:

1. 将来同様の utility 重複が観測されたとき、「lib-subprocess に取り込むべきか / 各 crate に残すべきか」を再判定する判断材料が必要
2. 同様に「複数 variant を維持すべきか / 1 つに merge すべきか」を再判定する判断材料が必要
3. ADR 化しないと git 履歴と doc comments の散在状態となり、再判定時に判断根拠の再構築が必要になる

### 順位 173 の構造的成果

PR #205 (173a) - #208 (173d) で以下を `lib-subprocess` に集約:

| Function | Variants | 用途別の使い分け |
|---|---|---|
| `combine_output` | 1 (単一) | stdout / stderr 結合 (`\n` suffix 吸収) |
| `wait_with_timeout` | `_safe` / `_basic` | Err 経路で child を kill するか / しないか |
| `drain_pipe` | `_unlimited` / `_capped` / `_capped_reporting` | 全量 / silent truncate / truncate + 末尾報告 |
| `run_cmd_shell` | `_capped` / `_capped_reporting` | drain variant 違い (上記に従う) |
| `kill_and_join_err` | 1 (内部 helper) | Err 経路で child kill + reader thread join (PR #208 CR Major 対応) |

5 callsite (cli-push-runner / cli-push-pipeline / cli-merge-pipeline / hooks-stop-quality / hooks-post-tool-linter) が `lib_subprocess::*` 経由で utility を共有。

### 173e で判定した境界

**共通化したもの** (= lib-subprocess に extract):
- 上記 5 関数群。各 variant は明示的な policy 違いで使い分けられており、各 callsite が「どの variant を呼ぶか」で意図を表明している

**共通化しなかったもの** (= 各 crate に残置):
- `cli-pr-monitor/src/runner.rs::run_cmd_direct`: direct args (shell なし) + `drain_pipe_unlimited` + 独自 polling。Windows shell escape を避ける設計意図が `lib-subprocess` の `cmd /c` 経由 shell pattern と signature レベルで非互換
- `cli-pr-monitor/src/runner.rs::run_cmd_inherit` / `cli-push-runner/src/runner.rs::run_cmd_inherit`: stdio inherit + timeout 有無の policy 違い (cli-pr-monitor は timeout あり、cli-push-runner は timeout なし)。同じ名前の 2 実装が意図的に異なる semantics を持つ

**variant merge しなかったもの** (= lib-subprocess 内に複数 variant 維持):
- `wait_with_timeout_safe` / `_basic`: spec が明示した merge trigger (`.takt/runs/` に `zombie` / `defunct` / `Failed to wait` 発生記録) が dogfood で未顕在化 (2026-06-15 時点で `grep -rn` 結果 0 件)
- `drain_pipe_capped` / `_capped_reporting`: bool flag で merge 可能だが、`run_cmd_shell_capped_reporting` 等の callsite 名で variant 意図が直接読めるため self-documenting 価値が bool flag の認知コストを上回る
- `run_cmd_shell_capped` / `_capped_reporting`: 同上

## 検討した選択肢

### 選択肢 A: 順位 173 完了として境界判断を記録しない

- todo11.md / git log / lib-subprocess の doc comments に部分的に残るが、再判定時に判断根拠の再構築コストが高い
- 将来「同じような重複あるけど共通化する? しない?」の判断が出たとき、過去判例として参照可能な single source がない
- **却下**

### 選択肢 B: 新規 ADR で「境界判定」を独立 codify (採用)

- 順位 173 で形成された具体的な境界 = 「共通化した境界」「共通化しなかった境界」「variant merge しなかった境界」の 3 軸を ADR として記録
- ADR-024 (`lib-jj-helpers`) と同じく「共通 utility library の boundary 判定」の事例として参照可能
- 将来の re-evaluation 時に「以下の条件が変わったら見直す」という trigger 条件も併記
- **採用**

### 選択肢 C: ADR-024 (shared jj-helpers library) に拡張

- ADR-024 は jj 専用ヘルパーの boundary 判定で、subprocess utility は scope が異なる
- jj helpers と subprocess utility は責務が直交しており、同 ADR に詰め込むと両方の主旨が霞む
- **却下**

## 決定 (試験運用)

### 採用する 3 層の境界記録

#### 層 1: extract 対象の境界 (`lib-subprocess` に入れるか / crate 個別に残すか)

| 条件 | 判定 |
|---|---|
| **3+ crate で重複** かつ **signature が同じか同型化可能** | ✅ extract |
| **signature が構造的に異なる** (例: shell vs direct args、stdio inherit vs piped) | ❌ 各 crate に残置、別 ADR で extract 検討 |
| **2 crate で重複** だが variant policy が違う | 🤔 variant として export を検討、要 dogfood |
| **1 crate でしか使われていない** | ❌ extract せず、将来 2 つ目の使用例待ち (ADR-024 § 早期の共通化はリスク に従う) |

#### 層 2: variant 維持の境界 (1 関数 vs 複数 variant)

| 条件 | 判定 |
|---|---|
| **callsite ごとに intent が異なる** (例: 全量読み / truncate / truncate + 報告) | ✅ variant 維持、callsite が variant 名で intent 表明 |
| **policy 差が runtime 値で表現可能** (例: timeout secs、max lines) | ✅ parameter として統一 |
| **policy 差が control flow に影響** (例: Err 経路で kill する / しない) | 🤔 variant 維持、dogfood で merge trigger を待つ |
| **bool flag で merge 可能** だが flag 意味が callsite で自明でない | ❌ variant 維持、self-documenting variant 名を優先 |

#### 層 3: variant merge の境界 (再評価 trigger)

以下のいずれかが発生したら variant merge を再評価:

1. **dogfood で `_basic` 側が想定外の挙動** (例: zombie process / defunct child / "Failed to wait" ログ) — `.takt/runs/` に grep して証拠を確認
2. **callsite で「どの variant を呼ぶか」の判断ミス** が CR Major / pre-push review で複数 PR 連続観測 (Frequency Medium 以上)
3. **新規 callsite が増えて variant 名の選択が機械的でなくなった**

### 順位 173 の判定実績

| Item | 判定 | 根拠 |
|---|---|---|
| `combine_output` (5 crate 重複) | ✅ 共通化 | 3+ crate 重複 + signature 同一 (層 1) |
| `wait_with_timeout_safe` vs `_basic` | ✅ 2 variant 維持 | Err 経路で kill する / しないが control flow に影響 (層 2)、dogfood 未顕在 (層 3) |
| `drain_pipe_unlimited` / `_capped` / `_capped_reporting` | ✅ 3 variant 維持 | 読み戦略が構造的に異なる (`read_to_string` vs line-by-line)、callsite intent 異なる (層 2) |
| `run_cmd_shell_capped` vs `_capped_reporting` | ✅ 2 variant 維持 | drain variant 違い (層 2)、callsite 名で intent 表明 (層 2) |
| `run_cmd_direct` (cli-pr-monitor) | ❌ 各 crate 残置 | shell vs direct args で signature 非互換 (層 1) |
| `run_cmd_inherit` (2 crate) | ❌ 各 crate 残置 | timeout 有無の policy 差が意図的、signature 異なる (層 1) |

## 影響

### 良い影響

- 将来の utility extract 候補が出た際、3 層の境界基準で機械的に判定可能
- variant merge を回避した判断の根拠が文書化され、後続 PR で「merge した方が綺麗では?」と提案された際の reject 根拠を即座に提示できる
- ADR-024 (`lib-jj-helpers`) との対比で「subprocess utility は variant を許容、jj helpers は単一 API で統一」という domain 差を示せる

### 注意点

- 境界基準は **dogfood 観測前提**。新規 callsite 追加で境界が変わる可能性があり、本 ADR は静的なルールではなく **再評価可能な judgment criteria** として運用する
- 「self-documenting variant 名 vs bool flag」の trade-off は callsite 数が増えると逆転する可能性 (例: variant が 5+ 個になったら bool/enum flag に移行する方が読みやすい)

## 再評価 trigger

以下のいずれかが発生したら本 ADR を見直す:

1. 新規 subprocess utility extract 候補が出て、3 層の境界基準で判定不能な edge case が発生
2. `_basic` variant の dogfood で zombie process / "Failed to wait" 等が `.takt/runs/` に観測される
3. variant 数が 5+ に増えて self-documenting naming が破綻 (lib-subprocess の export 関数一覧で迷う状態)
4. 採用判定期限 (2026-12-15) で本採用 / 修正 / 却下を判定

## 関連 ADR

- ADR-012: src/ ディレクトリの命名規約 — `lib-*` naming
- ADR-024: 共通 jj ヘルパーライブラリ — 同型の boundary 判定事例
- ADR-026: Cargo workspace — 本 ADR の前提構造
- ADR-039: Experimental feature 標準パターン — 本 ADR のステータス管理形式
- ADR-042: ルール vs 仕組み化の境界基準 — 本 ADR は subprocess utility 領域での具体適用

## 由来

- 順位 173 (PR #205-#208) の作業完了後、ユーザー指示 (2026-06-15) で「ここまでは共通化、ここから先は共通化しない判断を ADR に残す」要望から起案
- 173a (PR #205): `combine_output` extract
- 173b (PR #206): `wait_with_timeout_safe` / `_basic` extract
- 173c (PR #207): `drain_pipe_unlimited` / `_capped` / `_capped_reporting` extract
- 173d (PR #208): `run_cmd_shell_capped` / `_capped_reporting` extract + `kill_and_join_err` helper (CR Major 対応)
- 173e: 評価のみ実施、本 ADR で結果を codify
