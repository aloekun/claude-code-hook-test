# ADR-034: CodeRabbit 監視・対話の自動化戦略

## ステータス

試験運用 (2026-05-02)

## コンテキスト

PR #99 セッションで以下の運用痛が観測された:

1. **CR rate-limit の手動回復**: rate-limit 発生時、cli-pr-monitor の検出ロジック gap (review state = `not_found` 時の見逃し) により自動 retrigger が機能せず、ユーザーが手動で walkthrough comment を確認 → sleep + `@coderabbitai review` 投稿が必要
2. **CR review listing の token bloat**: `gh api .../pulls/N/reviews` + `pulls/N/comments` の重複取得で 44KB 級の生 metadata が context に乗る (cache_creation 9x で 約 400K tokens 蓄積)
3. **POST 応答の無駄**: `gh api -X POST .../replies` が 24KB の reply object を返すが、Claude は success/fail のみで十分なため body 破棄が必要

これらは Bundle Y2 (haiku 化、PR #98) でパイプラインが加速 (1〜2m/iter) した結果として CR への push 頻度が増えた **逆説的副作用**。

## 検討した選択肢

`docs/pipeline-token-efficiency.md` #D セクションで 4 案を検討:

- **#D-1**: gh CLI 使用ルール (rule 追記)
- **#D-2**: `pnpm cr:findings <PR>` wrapper script
- **#D-3**: `check-ci-coderabbit --list-findings` Rust モード
- **#D-4**: Claude 応答スタイル簡素化 rule

## 決定

**#D-1 + #D-3 を Bundle a (PR #99 post-merge-feedback 由来) に統合する。**

### Bundle a の最終構成 (4 component)

| # | 役割 | Effort | 出典 |
|---|---|---|---|
| 1 | cli-pr-monitor の rate-limit auto-retry 実装 | M | PR #99 T2-4 |
| 2 | ADR-018 / ADR-009 の rate-limit retry ポリシー明文化 | S | PR #99 T3-5 |
| 3 | **#D-1**: gh CLI 規則を `~/.claude/rules/common/git-workflow.md` に追記 | XS | 計画書 #D-1 |
| 4 | **#D-3**: `check-ci-coderabbit --list-findings` Rust モード (cli-pr-monitor 連携 API) | M | 計画書 #D-3 |

### 取り下げた案

- **#D-2 (pnpm cr:findings wrapper)**: ❌ 取り下げ。#D-3 が機能を内包 (Rust 構造化 findings JSON は wrapper script より widely usable、ADR-022 責務分離原則にも整合)
- **#D-4 (応答スタイル簡素化)**: ⏸️ 保留。**思考連続性低下リスク** (中間出力削減で後段の context 再構築コストが増え、token カテゴリが入れ替わるだけで正味削減が縮む可能性) を考慮、Bundle Z Phase 2/3 完了後の副作用観測手段確立を待つ。再評価条件は「将来の検討事項」参照

## 実装方針 (2 Sub-PR 分割)

### Sub-PR 1: token 削減層 (先行)

- **#D-1**: `git-workflow.md` に gh CLI 使用規則を追記 (XS)
- **#D-3**: `check-ci-coderabbit --list-findings` Rust 実装 (M)

### Sub-PR 2: rate-limit 自動化層 (主軸)

- cli-pr-monitor の rate-limit auto-retry 実装 (Sub-PR 1 の #D-3 findings API を消費)
- ADR-018 / ADR-009 の rate-limit retry ポリシー明文化 (= 本 ADR の改訂版を ADR-018 へ反映)

**分割根拠**: 依存方向 (#D-3 API → cli-pr-monitor 消費) が一方向、検証段階性確保。1 PR で 4 component を land すると CR review iteration が複雑化する (PR #99 でも 4 round)。

## 設計詳細

### rate-limit detection (改善版)

既存の `state.rate_limit` 検出は review state ベースで、`not_found` 時に rate-limit overlay を見逃す gap がある。

**改善**: walkthrough comment (PR の最初の CR comment) の `body` + `updated_at` を直接 polling し、`Rate limit exceeded` パターンを regex 検出する。

参照: memory `project_coderabbit_rate_limit_overlay.md` (PR #99 で実証された CR の rate-limit overlay 仕様)

### auto-trigger 投稿

- body 内 `Please wait N minutes and M seconds` を regex 抽出
- `updated_at` + N min M s = 解除予定時刻
- 解除 + **1 分** の安全マージン後 `gh api -X POST issues/N/comments -f body='@coderabbitai review' > /dev/null 2>&1` を投稿
- 1 分マージンは PR #99 セッション末で実証済 (本セッション内手動再現で確認)

### session 超え recovery

- `.claude/cli-pr-monitor-state.json` schema 拡張:
  - `rate_limit_unlock_at`: 解除予定時刻 (ISO 8601)
  - `scheduled_retry_post`: bool
- SessionStart hook (`hooks-session-start.exe`) が state file を読み、rate-limit 待機中なら cli-pr-monitor を recovery mode で再起動
- 既存の `state.rate_limit_last_retriggered_at` dedup を継承し、複数 session での重複投稿を防止

### gh CLI 使用規則 (#D-1)

`~/.claude/rules/common/git-workflow.md` に以下を追記:

- POST 操作 (作成・更新): 応答 body 破棄 (`> /dev/null 2>&1`)
- GET 操作 (取得): `--jq` で必要 field のみ抽出
- CR walkthrough 除外: `gh pr view --json reviews,comments` の `comments` field に CR walkthrough の base64 internal state が含まれるため `--jq 'del(.comments[].body)'` で除外

### 構造化 findings (#D-3)

`check-ci-coderabbit.exe --list-findings --pr <N>` で以下の JSON を出力:

```json
{
  "findings": [
    {"severity": "major", "file": "...", "line": 415, "summary": "...", "url": "..."}
  ]
}
```

- cli-pr-monitor からも消費可能 (rate-limit auto-retry のロジックに統合)
- Claude が `gh api` 重複取得をせず、1 コマンドで構造化 findings を取得

## 影響

### Positive

- ✅ rate-limit 完全自動回復 (ユーザー手動介入消滅)
- ✅ session 跨ぎ recovery (ユーザーが PC を閉じても OK)
- ✅ CR review listing の構造化 + token 削減 (~150-500K cache_creation tokens、全体の 1-3.7%)
- ✅ Claude のターン消費削減 (rate-limit 関連の対話が消滅)

### Negative

- ⚠️ 旧 Bundle Z2 に対する効果削減 (#D-4 抜きで 25-30% → 1-3.7% に縮小)
- ⚠️ CR 仕様変更 (walkthrough overlay format が変わった等) 時の fragility (regex 依存)。**個人開発向けで仕様変更時に対応する想定** (こちら側で対応する性質ではないため事前ケアしない)

### Trade-off

- 開発体験の質的変化 (rate-limit 手動介入消滅) を **token 削減効果より優先**
- #D-4 (応答スタイル) の保留により、潜在 2.5-4M tokens 削減を見送り

## 別セッションでの実装に必要な情報

本 ADR に基づく実装を別セッションで行う場合、以下を参照:

### 既存の関連コンポーネント

- **`src/cli-pr-monitor/src/stages/poll.rs`**: 現行 `handle_rate_limit_retry` 実装 (PR #97 Phase 4 land 済)、本 ADR で改修
- **`src/cli-pr-monitor/src/state.rs`**: state file schema、`rate_limit_last_retriggered_at` 等の dedup フィールド存在
- **`src/check-ci-coderabbit/`**: Rust 実装、`--list-findings` モード追加先
- **`src/hooks-session-start/`**: SessionStart hook、recovery 起動の起点
- **`~/.claude/rules/common/git-workflow.md`**: #D-1 追記先 (global rule、本リポジトリ外)

### 関連 ADR

- **ADR-018**: cli-pr-monitor takt 化 (本 ADR で部分改訂、rate-limit retry セクション追加)
- **ADR-009**: 旧 Post-PR Monitor 設計 (Superseded by ADR-018 partial、本 ADR で navigation 注記追加)
- **ADR-022**: 自動化コンポーネントの責務分離原則 (#D-3 の Rust 側実装が ADR-022 に整合)
- **ADR-026**: Cargo workspace (`check-ci-coderabbit` は既存 member、`--list-findings` 追加で member 構成変更不要)
- **ADR-030**: Deterministic post-merge-feedback (`.failed` marker パターンを recovery 設計の参考にする)

### 関連 memory

- `project_coderabbit_rate_limit_overlay.md`: rate-limit 検出ロジックの根拠 (PR #99 で実証された walkthrough overlay 仕様)
- `project_coderabbit_auto_resolve.md`: `resolved:` reply での auto-resolve 挙動

### todo.md / todo4.md エントリ

- `docs/todo.md` 推奨実行順序サマリー: 順位 42-45 (Bundle a 4 component)
- `docs/todo4.md`:
  - cli-pr-monitor の rate-limit auto-retry + `@coderabbitai review` auto-trigger 実装 (PR #99 T2-4)
  - ADR-018 / ADR-009 の rate-limit retry ポリシー明文化 (PR #99 T3-5)
  - 本 ADR で追加される #D-1 / #D-3 entry も別セッションで todo4.md に追記が必要

### 新セッションで最初に確認すべきこと

1. `git log --oneline -5` で master の最新状態を確認 (Bundle Z Phase 2/3 が land 済か等)
2. `docs/todo.md` の Bundle a 関連 entry (順位 42-45) を読む
3. `docs/todo4.md` の Bundle a 詳細 entry を読む
4. 本 ADR (ADR-034) を読む
5. memory `project_coderabbit_rate_limit_overlay.md` を読む
6. **どの Sub-PR を実施するか確認**: Sub-PR 1 (#D-1 + #D-3) と Sub-PR 2 (rate-limit auto-retry + ADR-018 改訂) のどちらから着手か (推奨は Sub-PR 1 先行)

### 完了条件

- Sub-PR 1 + Sub-PR 2 が両方 land
- ADR-018 に rate-limit retry ポリシーが明文化される (本 ADR の設計詳細を反映)
- dogfood で 1-2 PR 試験運用、rate-limit 自動回復が観測される
- ユーザー手動介入 (`@coderabbitai review` 投稿等) が 0 になる
- 本 ADR のステータスを「承認済み」に変更

## 将来の検討事項

### #D-4 (Claude 応答スタイル簡素化) の再評価条件

Bundle Z Phase 2/3 (#B-β / #B-γ) 完了後、以下が確立した時点で慎重 pilot を実施:

- **副作用観測手段**: session 比較メトリクス (思考品質 proxy 指標 = 再 grep / 再 read 頻度の変化、修正回数の変化等)
- **段階的展開**: rule を一気に書かず、1 種類ずつ (Insight ブロック → 完了報告 → 分析テーブル) 試す
- **dogfood 比較**: 同種 PR を rule あり / なしで比較し、token 削減量と思考品質の trade-off を定量化

これらが揃わない限り、#D-4 は保留継続。

### Bundle a 着手時の前提条件 reality check

- CR の rate-limit 仕様が変わっていないか (memory `project_coderabbit_rate_limit_overlay.md` の挙動が再現するか) を着手前に dogfood で確認
- `gh api` の rate-limit (CR とは別、GitHub API 側) が干渉しないか観察

## References

- ADR-018: cli-pr-monitor takt 化
- ADR-009: 旧 Post-PR Monitor (Superseded by ADR-018 partial)
- ADR-022: 自動化コンポーネントの責務分離原則
- ADR-026: Cargo workspace
- ADR-030: Deterministic post-merge-feedback
- `docs/pipeline-token-efficiency.md` #D セクション (採用判定改訂 2026-05-02)
- memory `project_coderabbit_rate_limit_overlay.md`
- PR #99 (本 ADR の起源、cli-pr-monitor の rate-limit detection gap が顕在化したセッション)
