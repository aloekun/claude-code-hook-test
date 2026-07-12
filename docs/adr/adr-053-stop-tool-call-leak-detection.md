# ADR-053: Stop hook による tool call leak 検知

## ステータス

試験運用 (2026-07-12)

> 本 ADR は [ADR-039 (試験運用標準パターン)](adr-039-experimental-feature-standard-pattern.md) に従う。
> Config opt-in / kill-switch / bounded lifetime の 3 点を満たす。

## コンテキスト

Claude Code がツール呼び出しを正規の tool_use block ではなく**テキスト領域に生の
XML として出力**し、ツールが実行されないまま turn が終了する不具合が多発している。
transcript 上では assistant メッセージの `type: "text"` content block に以下の形で残る:

```text
(日本語の説明文)

court
<invoke name="Bash">
<parameter name="command">pnpm push 2>&1</parameter>
</invoke>
```

直近 4 セッションの transcript 調査 (2026-07-12) で **197 件**の実 leak を確認した:

| セッション (先頭8桁) | leak 件数 | 化けたプレフィックス行 |
|---|---|---|
| 87387df2 | 59 | `court` |
| 931b72e4 | 90 | `court` |
| 05c197f1 | 35 | `count` |
| 025e5aeb | 13 | `code` |

問題の構造:

1. **turn がそのまま終了する**。leak 発生時はツールが動かないため後続がなく、
   Stop が発火して作業が停止する。
2. **ハーネスの自動リカバリは不十分**。`isMeta: true` の
   `Your tool call was malformed and could not be parsed. Please retry.` 自動注入は
   4 セッション計 15 件程度しか発動せず、大半はユーザーが手動で指摘して再開させていた。
3. **リトライ後の再 leak が実在する**。指摘を受けて「正しい形式で再実行します」と
   応答しながら再度 leak するケースを複数観測した。

根本原因は上流 (Claude Code / モデルのシリアライズ) にあり本リポジトリでは修正
できないため、Stop hook による検知 + 再実行誘導で被害 (作業停止) を止める。
これは [ADR-042 (ルール vs 仕組み化)](adr-042-rule-vs-mechanism-boundary.md) の
「仕組み化」側の対応である (決定論的に判定可能、CLAUDE.md ルールでは防げない)。

## 決定

新規 Stop hook exe `hooks-stop-tool-call-leak` (`src/hooks-stop-tool-call-leak/`) を
作成し、`settings.local.json.template` の Stop 配列の**先頭** (品質ゲートの前) に
登録する。leak 検知時は `{"decision": "block", "reason": "..."}` を返し、正規の
ツール呼び出しでの再実行を促す。

hooks-stop-quality への統合ではなく独立 exe とする
([ADR-022 (責務分離)](adr-022-automation-responsibility-separation.md)、
hooks-stop-feedback-dispatch と同じ先例)。

### 検知条件 (実データ 198 件で検証済み)

stdin の `transcript_path` から transcript JSONL の末尾 200 行を読み、
`isSidechain` でない**最後の assistant エントリ**の `type: "text"` block を検査する:

1. markdown code fence (```` ``` ```` / `~~~`) 内の行を除外する
2. 残る行に**行頭 (空白許容) `<invoke name="` の行**が存在し、**かつ**
   行頭 `</invoke>` または行頭 `<parameter name="` の行が存在すれば leak と判定

判定条件の根拠:

- 正常なツール呼び出しは `type: "tool_use"` block に記録されるため、`text` block
  のみの検査で正常呼び出しへの誤爆は構造的に起きない (実データで tool_use 203 件
  に対し誤検知 0)
- 実 leak 197 件は全て行頭 `<invoke name="` 行を持つ。一方、唯一の正当例
  (assistant が不具合を説明するためにインラインコードで `<invoke>` を引用) は
  行頭に現れず、**行頭アンカーで leak 197 / 正当引用 1 を完全分離**できた
- `</invoke>` 終端は保証されない (自己言及テキストが後続する 49 件) ため、
  末尾アンカーではなく行アンカーを使う
- 化けたプレフィックス語 (`court` / `count` / `code`) はセッション間で変動する
  ため判定に使わない
- fence 除外は、ドキュメント執筆時に assistant が例示として leak 構造を fence 内に
  書くケースを許容するため (誤検知より取り逃がしを優先する方向に倒す)

### ループ防止 — 連続カウント上限 (ADR-004 からの意図的逸脱)

[ADR-004 (Stop 品質ゲート)](adr-004-stop-hook-quality-gate.md) の
`stop_hook_active` skip (最大 1 retry) は採用しない。理由:

1. 品質ゲートが block した後の retry 中に leak が発生すると、`stop_hook_active`
   が true のため検知がスキップされ、取り逃がす
2. 再 leak の実績があり、1 retry では収束しないケースがある

代わりに transcript 末尾から**連続する leak assistant エントリ数**をカウントし、
`max_consecutive_blocks` (既定 3) 到達で fail-open (stderr 警告 + 停止許可) する。
走査規則:

- `isMeta: true` の user エントリ (Stop hook feedback / ハーネス自動注入) と
  tool_result はチェーンを切らずにスキップ (実データで Stop hook の block reason が
  `isMeta: true` user エントリとして記録されることを確認済み)
- 実ユーザーの発話に到達したらチェーンをリセット (新しい試行の起点)
- 非 leak の assistant エントリで打ち切り

検知は決定論的なので、Claude が正しく再実行すれば最終 assistant エントリが変わり
自然収束する。収束しない場合も上限で必ず停止できる。

### エラー時 fail-open (ADR-043 からの意図的逸脱)

transcript 読み取り失敗 / stdin parse 失敗 / `transcript_path` 欠落時は stderr
警告 + 停止許可とする。[ADR-043 (fail-closed 原則)](adr-043-security-gates-fail-closed.md)
と逆だが、本 hook で fail-closed (block) にすると読み取り失敗が持続する環境では
連続カウントも取得できず**無限ブロック**に陥る。本 hook はセキュリティゲートでは
なく UX 復旧装置であり、取り逃がしのコストは「従来どおりユーザーが手動指摘する」
に留まるため、fail-open が正しい。

## ADR-039 3 点セット

### Config opt-in (default OFF)

`hooks-config.toml` の `[stop_tool_call_leak]` section:

```toml
[stop_tool_call_leak]
enabled = true                # code default は false (unwrap_or(false))
max_consecutive_blocks = 3    # 連続 block 上限 (到達で fail-open)
```

section 不在 / `enabled` 未設定 / `false` では完全 skip。本リポジトリは dogfood の
ため `enabled = true`。派生プロジェクトへの deploy 時は code default OFF を継承する。

### Kill-switch

| 停止手段 | 影響範囲 |
|---|---|
| `enabled = false` (or section 削除) | 恒久停止。検査を完全 skip |
| env `STOP_TOOL_CALL_LEAK_OVERRIDE=1` (truthy 値) | 緊急バイパス。`FILE_LENGTH_CHECK_OVERRIDE` と同 pattern |

### Bounded lifetime

根本原因は上流の不具合であり、本 hook は上流が修正されるまでの時限的な防御層である。
撤去判定 trigger:

- **撤去**: 上流 (Claude Code / モデル) の修正が確認できた、または leak が
  **4 週間観測されなくなった**時点で、hook 登録解除 + crate 削除の revert PR を作成
- **継続**: leak が観測され続ける間は維持。block 発火が透明になるよう stderr /
  reason に検知回数を明示している

dogfood 計測項目: block 発火数、fail-open (上限到達) 数、誤検知報告 (正当なテキスト
出力が block された件数、期待値 0)。

## 帰結

### 利点

- leak 発生時にユーザーの手動指摘を待たず、Stop 時点で即座に再実行を誘導できる
- 判定が決定論的 (正規表現も LLM も不使用、行アンカーの文字列判定のみ) で高速 (数 ms)
- 実データ 198 件で leak 197 / 正当引用 1 を完全分離 (誤検知 0)
- 検知条件・fixture は実 incident 由来
  ([ADR-049 (incident→eval)](adr-049-incident-eval-regression-suite.md) 準拠の回帰テスト)

### 欠点 / 留意点

- 検知条件は現在観測されている leak の形 (`<invoke name="...">` 構造) に固有。
  上流の不具合の形が変われば取り逃がす (その場合は fixture を追加して条件を拡張)
- 連続 3 回で fail-open するため、収束しない leak は最終的にユーザー介入が必要
  (無限ループ防止との意図的なトレードオフ)
- fence 内の leak は検知しない (誤検知回避を優先。実データでは fence 内 leak は 0 件)
- 本 hook の block reason 自体が `<invoke` への言及を含むが、reason は
  `isMeta: true` の user エントリとして記録されるため検知対象にならない

## 関連 ADR

- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン
- [ADR-004](adr-004-stop-hook-quality-gate.md) — Stop 品質ゲート (ループ防止方式の逸脱元)
- [ADR-043](adr-043-security-gates-fail-closed.md) — fail-closed 原則 (エラー処理方式の逸脱元)
- [ADR-022](adr-022-automation-responsibility-separation.md) — 責務分離 (独立 exe の根拠)
- [ADR-042](adr-042-rule-vs-mechanism-boundary.md) — ルール vs 仕組み化 (本対応は仕組み化側)
- [ADR-049](adr-049-incident-eval-regression-suite.md) — incident→eval 回帰スイート (fixture 方針)
