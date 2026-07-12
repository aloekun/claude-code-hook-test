# Stop hook: tool call leak 検知 — 作業計画 (一時ファイル)

> **このファイルは一時的な作業計画ファイルです。作業完了時 (WP-7) に削除します。**
> セッション破損・別モデルへの引き継ぎを想定し、実装に必要な調査結果と設計判断を
> 自己完結的に記録しています。

## 背景 / 問題

Claude Code がツール呼び出しを正規の tool_use block ではなく**テキスト領域に
生テキストとして出力**し、ツールが実行されないまま turn が終了する不具合が多発している。
出力は化けたプレフィックス行 (例: `court`) + `<invoke name="...">...</invoke>` の形。
turn がそのまま終了するため作業が停止し、ユーザーが都度指摘して再開させる必要がある。

対策として、Stop hook で直前の assistant 出力を検査し、leak を検知したら
`decision: block` で即座にエラーフィードバックを返して正規のツール呼び出しでの
再実行を促すカスタムリンター exe を新規作成する。

## 調査結果 (2026-07-12 実施、実データ根拠)

セッションログ (`%USERPROFILE%\.claude\projects\c--Users-owner-work-claude-code-hook-test\` 配下の
`*.jsonl`) を調査した。主要ログ: `87387df2-e72e-488a-8548-9a1dd68b7948.jsonl`。

### 発生規模

| セッション (先頭8桁) | leak 件数 | プレフィックス行 |
|---|---|---|
| 87387df2 | 59 | `court` |
| 931b72e4 | 90 | `court` |
| 05c197f1 | 35 | `count` |
| 025e5aeb | 13 | `code` |

計 197 件の真の leak + 1 件の正当引用 (後述) = `<invoke` を含む text block 198 件。

### transcript 上での leak の構造

assistant エントリの `message.content[]` 内、`type: "text"` block に以下の形で残る:

```text
(日本語の説明文)

court
<invoke name="Bash">
<parameter name="command">pnpm push 2>&1</parameter>
</invoke>
```

931b72e4 では 49 件が `</invoke>` の後に英語の自己言及テキスト
(例: `I keep failing. Let me use the correct format only.`) を伴う。
**末尾アンカー (`</invoke>` で終端) だけの検知は不可**。

### 検知設計の根拠となる観測事実

1. **正常な tool calling は `type: "tool_use"` block** に記録される (87387df2 で 203 件
   確認)。`text` block のみ検査すれば正常呼び出しへの誤爆は構造的に起きない。
2. **プレフィックス語はセッション間で変動** (`court` / `count` / `code`)。
   プレフィックス語に依存した検知は不可。`<invoke name="` 構造そのものを見る。
3. **leak は必ず turn 終端に位置する** (leak 後にツールが動かないため)。
   最後の assistant エントリのみの検査で足りる。
4. **ハーネスの自動リカバリは機能不全**。`isMeta: true` の
   `Your tool call was malformed and could not be parsed. Please retry.` 自動注入は
   4 セッション計 15 件程度しか発動せず、大半は turn 終了 (= Stop 発火) している。
5. **リトライ後の再 leak が実在** (87387df2 の L918→L925、931b72e4 の連鎖)。
   1 回限りのリトライ設計では不十分。同時に無限ループ対策が必須。
6. **正当引用の実例が 1 件存在** (05c197f1): assistant が不具合をユーザーに説明する際、
   `` `<invoke>` `` をインラインコード (行中) で引用。
   **「行頭が `<invoke name="` で始まる行の存在」を条件にすると、実データ 198 件を
   leak 197 / 正当引用 1 に完全分離できた** (line-start アンカーの実測検証済み)。

## 設計

### 配置 / 責務

- 新規 crate: `src/hooks-stop-tool-call-leak/` (ADR-012 命名、ADR-026 workspace 追加)
- hooks-stop-quality への統合ではなく独立 exe (ADR-022 責務分離。
  feedback-dispatch と同じ先例)
- `templates/settings.local.json.template` の Stop 配列の**先頭**に登録
  (検知は数 ms で完了し、「意図した作業が未実行」の指摘は品質ゲートより優先)。
  timeout は 5 秒
- 決定論的検知であり ADR-042 の「仕組み化」側

### 検知ロジック

1. stdin の Stop hook 入力 JSON から `transcript_path` を取得
2. transcript JSONL を読み、`isSidechain == true` を除外した**最後の
   `type: "assistant"` エントリ**を特定
3. その `message.content[]` の `type: "text"` block それぞれに対し:
   - markdown code fence (```` ``` ```` / `~~~`) 内の行を除外
   - 残る行に **行頭 (空白許容) `<invoke name="`** の行が存在し、**かつ**
     行頭 `</invoke>` または行頭 `<parameter name="` の行が存在すれば leak と判定
4. leak 判定時、`<invoke name="([^"]+)"` からツール名を抽出して reason に含める

### 検知時の出力

```json
{"decision": "block", "reason": "..."}
```

reason の骨子: 「直前の出力に含まれるツール呼び出し (`<invoke name="Bash">` 等) は
テキストとして表示されただけで**実行されていない**。呼び出し記法が壊れている。
同じ内容を正規のツール呼び出しとして直ちに再実行せよ。テキスト領域に
`<invoke ...>` の XML を書いてはならない」

### ループ防止 — `stop_hook_active` skip は採用しない (ADR-004 からの意図的逸脱)

理由: (a) 品質ゲートブロック後の retry 中に発生した leak を取り逃がす、
(b) 再 leak の実績があり 1 回で打ち切ると不十分。

代替: transcript 末尾から後方に走査し、**連続する leak 判定 assistant エントリ数**を
カウント。`max_consecutive_blocks` (デフォルト 3) 到達で fail-open
(stderr 警告 + 停止許可)。非 assistant エントリ (queue-operation / last-prompt /
ai-title / isMeta user 等) は走査中スキップし、非 leak の assistant エントリで
カウント打ち切り。後方走査はエントリ 50 件で上限。
正しく再実行されれば最終 assistant エントリが変わるため自然収束する。

### エラー処理 — fail-open (ADR-043 からの意図的逸脱)

transcript 読み取り失敗 / JSON パース失敗 / `transcript_path` 欠落時は
stderr 警告 + 停止許可 (exit 0)。fail-closed (block) にすると、読み取り失敗が
持続する環境では連続カウントも取得できず無限ブロックに陥るため。
本 hook はセキュリティゲートではなく UX 復旧装置であり、逸脱は ADR に明記する。

### 設定 (ADR-039 experimental feature 標準パターン)

`hooks-config.toml` に追加:

```toml
[stop_tool_call_leak]
enabled = true                # code default は false (opt-in)。本 repo は dogfood のため true
max_consecutive_blocks = 3    # 連続ブロック上限 (到達で fail-open)
```

- Kill-switch: `enabled = false` で恒久停止。緊急バイパスは env
  `STOP_TOOL_CALL_LEAK_OVERRIDE=1` (FILE_LENGTH_CHECK_OVERRIDE と同パターン)
- Bounded lifetime: 根本原因は上流 (Claude Code / モデルのシリアライズ) の不具合。
  **上流修正の確認、または leak が 4 週間観測されなくなった時点で撤去を判定**
- 派生プロジェクト配布 (`pnpm deploy:hooks`) は code default OFF なので安全

## 実装 WP

- [x] **WP-1**: crate 骨格。`src/hooks-stop-tool-call-leak/` (Cargo.toml + main.rs)、
  workspace members へ追加、`pnpm build:all` 対象化 (ADR-010 のビルド戦略に従う)
- [x] **WP-2**: 検知ロジック実装 + 単体テスト (`src/detect.rs` / `src/transcript.rs`)。
  fixture は実ログから sanitize して抽出 (ADR-049 incident→eval 準拠):
  court/count/code 3 変種、trailing 自己言及テキスト付き、正当引用 (非検知)、
  tool_use のみ (非検知)、fence 内引用 (非検知)
- [x] **WP-3**: 設定読み込み + kill-switch env + 連続カウント fail-open + fail-open
  エラー処理。それぞれ単体テスト (計 36 unit tests)
- [x] **WP-4**: `.claude/settings.local.json.template` の Stop 配列先頭に登録
  (timeout 5s)、`.claude/hooks-config.toml` に `[stop_tool_call_leak]` 追記。
  ※ template の実体パスは `.claude/settings.local.json.template`
  (`pnpm build:hooks-settings` が参照)
- [x] **WP-5**: E2E テスト (`tests/e2e.rs`、7 tests)。実 exe を `CARGO_BIN_EXE` で
  spawn し、一時 jsonl + stdin JSON で block/fail-open/kill-switch を検証
- [x] **WP-6**: ADR-053 起草
  (`docs/adr/adr-053-stop-tool-call-leak-detection.md`、試験運用マーク)、
  CLAUDE.md 索引に追加。
  ※ 当初 ADR-052 で起草したが、master 側 PR #260 が ADR-052
  (自律実行境界の 2 クラス分類) を先に使用したため rebase 時に 053 へ採番変更
- [ ] **WP-7**: 実データ検証 → 品質ゲート通過 → push / PR → **本ファイル削除**
  - [x] 実データ検証 (2026-07-12 実施): メインリポジトリ全セッションログに対し
    実装済み `text_block_has_leak` で **検知 198 / 非検知 2**
    (198 = 調査時の 197 + 検証当日に 87387df2 で新規発生 1 件。
    非検知 2 は両方とも正当な言及で、誤検知 0・取り逃がし 0)
  - [x] 品質ゲート: cargo test --workspace 全パス (新規 43 tests 含む)、
    cargo clippy --workspace -D warnings クリーン、pnpm lint / lint:md / test / build
    パス、pnpm build:all で exe 配置 + settings.local.json 再生成、
    配置済み release exe のスモークテスト (block JSON / 無出力) 確認済み
  - [ ] push / PR (ADR-028 ゲートによりユーザー承認待ち)
  - [ ] 本ファイル削除

## 検証方法

1. `cargo test -p hooks-stop-tool-call-leak` (単体 + E2E)
2. 実ログ 4 セッションに対する分離検証 (WP-7。leak 197 検知 / 正当引用 1 非検知)
3. `pnpm build:all` → `.claude/` に exe 配置 → 実セッションで dogfood
