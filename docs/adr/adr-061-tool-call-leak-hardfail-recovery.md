# ADR-061: tool call leak の hard-fail 経路対応 — Stop 不発火の回収層 + scan_tail 合成エントリ耐性

## ステータス

試験運用 (2026-07-28)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。
> [ADR-053 (Stop hook による tool call leak 検知)](adr-053-stop-tool-call-leak-detection.md) の拡張であり、
> bounded lifetime は ADR-053 と連動する。

## コンテキスト

[ADR-053](adr-053-stop-tool-call-leak-detection.md) は、ツール呼び出しが正規の tool_use block
ではなくテキスト領域に `<invoke name="...">...</invoke>` の生 XML として出力され実行されないまま
turn が終了する不具合 (tool call leak) を、Stop hook で検知し `decision: block` で再実行を誘導する。

しかし improve workspace (`claude-code-hook-test-improve`、CLI 運用) のセッション `828764ce` で、
ADR-053 で対処済みのはずの leak が 2 回発生し (2026-07-27 / 07-28)、いずれも Stop hook が
block しなかった。

### incident の transcript 構造 (1 回目、2026-07-27)

| 行 | 内容 |
|---|---|
| 273 | assistant text — leak 本体 (`court` + 行頭 `<invoke name="Bash">`、model `claude-opus-4-8`、正規の `msg_*` id) |
| 274 | assistant text — **ハーネス合成エントリ** "The model's tool call could not be parsed (retry also failed)." |
| 275 | system `turn_duration` — **直前に `stop_hook_summary` が無い = Stop hooks 不発火** |

合成エントリ (274) の識別フィールド (v2.1.206 実測):

```json
{
  "type": "assistant",
  "isApiErrorMessage": true,
  "message": {
    "id": "74272aea-... (UUID 形式、msg_* でない)",
    "model": "<synthetic>",
    "stop_reason": "stop_sequence",
    "content": [{ "type": "text",
      "text": "The model's tool call could not be parsed (retry also failed)." }]
  }
}
```

2 回目 (2026-07-28、同セッション resume 後) は、ユーザー発話の直後にハーネスが `isMeta: true` の
user エントリ "The previous response failed to produce a valid tool call. Please retry the tool
call now." を自動注入 → assistant が謝罪文 + **再 leak** → 同じ合成エントリ → 同じく Stop 不発火、
という流れだった。その次の turn では同じ注入の後に正規の tool_use で成功して正常終了しており、
正常終了 turn には `stop_hook_summary` が毎回記録されている (= hook の登録・配備・config は正常)。

### corpus 横断調査の結論

3 プロジェクト (本 repo / improve / ccht-improve)・約 150 セッション・2026-06-28〜07-28・
v2.1.191〜2.1.218 の全 transcript を調査した:

- leak 自体と hook の block 成功実績は `claude-vscode` / `cli` / `sdk-cli` の **3 entrypoint
  すべて**にある (CLI 固有の問題ではない)
- 今回の hard-fail 合成エントリ ("could not be parsed" 系 + `isApiErrorMessage: true`) は
  **corpus 全体で 828764ce の 2 件のみ**。他の `isApiErrorMessage: true` は 529 Overloaded /
  session limit / stalled stream という一般 API エラーだけ
- 同一バイナリ (v2.1.206)・同一 entrypoint (cli)・同一 workspace で、07-12 には正常経路
  (Stop 発火 → hook block 成功)、07-27/28 には hard-fail 経路が起きている → 経路分岐は環境・
  バージョンではなく応答の壊れ方 (内部リトライも失敗したか) に依存する
- hard-fail 経路は同一コマンド文脈 (background Bash で cli-push-runner 起動) で 2/2 再現しており、
  一回性ではない

## 根本原因 (2 層)

### 主因: hard-fail 終了経路では Stop hook イベント自体が発火しない

ハーネスの API レベル自動リトライが失敗すると、合成エントリを記録して turn をエラー終了させ、
この経路では Stop hooks を呼ばない (正常終了 turn には毎回ある `stop_hook_summary` が leak turn に
だけ無いことで確認済み)。hook は呼ばれる機会自体が無く、Stop hook 側の修正では構造的に届かない。
上流 (Claude Code) の「エラー終了経路で Stop hooks が発火しない」挙動が本質的な原因である。

### 副因: scan_tail が合成エントリでチェーンを打ち切る

`src/hooks-stop-tool-call-leak/src/transcript.rs` の `scan_tail` は「非 leak の assistant エントリに
到達したら打ち切り」で、合成エントリは `type: "assistant"` のため最終 assistant として検査され、
leak なし → `consecutive_leaks = 0` になる。`is_main_assistant()` は `isApiErrorMessage` /
`model == "<synthetic>"` を見ていない。仮に Stop が発火していても取り逃がしていた。

## 決定

### 1. 単一 exe で両イベントを処理する

hook 入力 JSON の `hook_event_name` で分岐し、Stop (従来の block 判定) と UserPromptSubmit
(回収層) の両方に `hooks-stop-tool-call-leak` を登録する。責務は「leak 検知」で同一
([ADR-022 (責務分離)](adr-022-automation-responsibility-separation.md) 整合)、detect/transcript
モジュールを共有できる。`hook_event_name` 欠落時は Stop 扱い (後方互換)。

### 2. 副因修正: 合成エントリをチェーンを切らずスキップ

`isApiErrorMessage == true` または `message.model == "<synthetic>"` の assistant エントリを
isMeta user と同扱いにする (leak とも数えない)。一般 API エラー (529 Overloaded 等) も本条件に
該当するが、スキップされるだけで leak 判定には影響しない (直前が leak でなければチェーンは伸びない)。

### 3. 回収層の検知条件: 「最後の assistant 活動が hard-fail leak」

末尾から user エントリを含む非 assistant をすべてスキップし、最初に現れた main assistant が
合成エントリ (連続する場合は連続分を通過) で、その直前の実 assistant エントリの text block が
leak なら発火する。これにより:

- (a) UserPromptSubmit 時点で現 prompt が transcript に載っていても頑健
- (b) 正常 turn を挟んだ古い leak では発火しない
- (c) overloaded 等 (直前が leak でない) では発火しない

**leak が最後で合成エントリが無いケースでは発火しない** (それは Stop hook の責務。fail-open 後の
再誘導ループを避ける意図的スコープ限定)。

### 4. 回収層の出力は non-blocking

UserPromptSubmit で `decision: block` は絶対に出さない (ユーザーの prompt 自体を拒否してしまう)。
出力は 2 チャネル:

- `additionalContext` (モデル向け): 直前 turn のツール呼び出し (ツール名を明示) が実行されて
  いないこと、正規のツール呼び出し機構で直ちに再実行すること、**応答テキストに XML を書き直さない
  こと** (ADR-053 の block reason と同旨。ハーネス自身の注入 "Please retry..." は実データで再 leak を
  防げなかったため、leak 固有の禁止事項を補強する)
- `systemMessage` (config opt-in、[ADR-059](adr-059-hook-system-message-visibility.md) チャネル
  分離): ユーザー可視 1 行

additionalContext には ADR-059 defense-in-depth として「セッション最初の応答でユーザーに一言
伝えよ」を明示し、systemMessage 非表示環境でもモデル経由で届ける。

### 5. Telemetry

回収発火時に `lib_telemetry::record` で hook `hooks-stop-tool-call-leak` / kind `Hook` /
id `hooks-stop-tool-call-leak/prompt-recovery` / `Decision::Warn` を記録 (non-blocking なので
Block ではない。id suffix で Stop block と区別する)。

### 検知条件 (実データ由来、ADR-049)

fixture は 828764ce の実構造 (leak → 合成、および leak → 合成 → 実 user → isMeta 注入 → leak →
合成) を再現する ([ADR-049 (incident→eval)](adr-049-incident-eval-regression-suite.md) 準拠)。

## ADR-039 3 点セット

### Config opt-in (default OFF)

`hooks-config.toml` の `[stop_tool_call_leak]` section に追加:

```toml
[stop_tool_call_leak]
prompt_recovery_enabled = true            # code default は false (unwrap_or(false))
recovery_system_message_enabled = true    # source default OFF (ADR-059)
```

未設定では回収層は完全 skip。本リポジトリは dogfood のため `true`。派生プロジェクトへの deploy 時は
code default OFF を継承する。

### Kill-switch

| 停止手段 | 影響範囲 |
|---|---|
| `prompt_recovery_enabled = false` | 回収層のみ恒久停止 (Stop block は継続) |
| `recovery_system_message_enabled = false` | systemMessage のみ停止 (additionalContext の回収は継続) |
| env `STOP_TOOL_CALL_LEAK_OVERRIDE=1` (truthy 値) | 緊急バイパス。Stop / UserPromptSubmit 両経路共通 |

### Bounded lifetime

根本原因は上流の不具合であり、本 hook は上流が修正されるまでの時限的な防御層である。
[ADR-053](adr-053-stop-tool-call-leak-detection.md) と連動して撤去を判定する:

- **撤去**: 上流 (Claude Code / モデル) の修正が確認できた、または leak が **4 週間観測されなく
  なった**時点で、ADR-053 とまとめて hook 登録解除 + crate 削除の revert PR を作成
- **継続**: leak が観測され続ける間は維持

dogfood 計測項目: 回収発火数 (telemetry の `hooks-stop-tool-call-leak/prompt-recovery`)、
UserPromptSubmit の発火順 (ハーネス自身の isMeta 注入との前後関係の実観測)、誤発火報告 (期待値 0)。

## 帰結

### 利点

- hard-fail 経路 (Stop 不発火) で取り逃がしていた leak を、次の UserPromptSubmit で回収できる
- 副因修正により、Stop が発火する経路でも合成エントリ跨ぎの leak を取りこぼさない
- 判定は決定論的 (行アンカーの文字列判定 + フラグ判定のみ) で高速
- fixture は実 incident 由来 (ADR-049 準拠の回帰テスト)

### 欠点 / 留意点

- **回収の限界**: ユーザー発話なしでは回収できない (hard-fail 時にはいかなる hook イベントも
  発火しないため構造的に不可避)。これは上流報告の主眼であり、本対応のスコープ外
- **UserPromptSubmit の発火順**: ハーネス自身の isMeta 注入との前後関係は制御できない。
  additionalContext は独立に届くため機能上は問題ないが、初回 dogfood で実挙動を観測する
- **検知条件の固有性**: 合成エントリの文言・フラグは v2.1.206 の実測に基づく。上流変更で形が
  変われば fixture を追加して追随する (ADR-053 と同じ前提)
- 上流 anthropics/claude-code への issue 報告 (「エラー終了経路で Stop hooks が発火しない」) は
  別途ユーザー承認制 ([ADR-052](adr-052-autonomy-execution-boundary-classes.md))

## 関連 ADR

- [ADR-053](adr-053-stop-tool-call-leak-detection.md) — 既存の Stop hook 検知 (本 ADR の拡張元)
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン
- [ADR-049](adr-049-incident-eval-regression-suite.md) — incident→eval 回帰スイート (fixture 方針)
- [ADR-059](adr-059-hook-system-message-visibility.md) — systemMessage / additionalContext チャネル分離
- [ADR-060](adr-060-cloud-harness-sessionstart-dispatcher.md) — cloud dispatcher 登録 parity
- [ADR-022](adr-022-automation-responsibility-separation.md) — 責務分離 (単一 exe 判断の根拠)
- [ADR-052](adr-052-autonomy-execution-boundary-classes.md) — 外部可視アクションのゲート
