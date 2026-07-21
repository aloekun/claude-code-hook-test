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

(削除済) `docs/pipeline-token-efficiency.md` #D セクションで 4 案を検討:

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
- **#D-4 (応答スタイル簡素化)**: ❌ **不採用** (2026-05-04 ユーザー判断、Bundle Z Phase 2/3 完了後の再評価で確定)。**思考連続性低下リスク** (中間出力削減で後段の context 再構築コストが増え、token カテゴリが入れ替わるだけで正味削減が縮む可能性) と **副作用観測手段の継続的不在** を考慮した最終判断。潜在 2.5-4M tokens 削減は採用見送り

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

**改善**: walkthrough comment (PR の最初の CR comment) の `body` + `updated_at` を直接 polling し、`RATE_LIMIT_MARKERS` (multi-variant) のいずれかに match するかで rate-limit を判定する (実装: `src/check-ci-coderabbit/src/main.rs` の `is_rate_limit_comment` + `extract_wait_time`、順位 167-169 で multi-variant 対応済)。

参照: memory `project_coderabbit_rate_limit_overlay.md` (PR #99 で実証された CR の rate-limit overlay 仕様) + 下記 § 既知 CR rate-limit format 一覧

### 既知 CR rate-limit format 一覧 (順位 169 採用、2026-05-29 追加)

CR は format を時間経過で変更するため、本リポジトリの実装は multi-variant marker 配列 + 複数 regex pattern で対応する。発見時期昇順:

| 発見時期 | body marker | wait time regex |
|---|---|---|
| ~2026 年初頃 (旧 format) | `Rate limit exceeded` (本文先頭、heading なし) | `Please wait \*?\*?(\d+) minutes? and (\d+) seconds?` / 短縮形 `Please wait \*?\*?(\d+) minutes?` |
| 2026-05 観測 (新 format) | `rate limited by coderabbit.ai` (HTML コメント `<!-- ... -->` 内、`## Review limit reached` heading 併設) | `More reviews will be available in (\d+) minutes? and (\d+) seconds?` / 短縮形 `More reviews will be available in (\d+) minutes?` |
| 2026-07-20 観測 (第 3 世代、PR #309) | 変わらず `rate limited by coderabbit.ai` (`## Review limit reached` heading も維持) | `Next review available in[:*\s]*(\d+) minutes?...` (`**Next review available in:** **57 minutes**` の形。ラベルと数値の間に markdown 強調が挟まるため区切りを文字クラスで吸収) |

**未知書式の fail-closed fallback (2026-07-20 追加、WP-15 追補 R2)**: marker は一致したが wait time regex がどれも一致しない場合、旧実装は `parse_rate_limit` が `None` を返し「rate-limit ではない」= 検知の沈黙に倒れていた。書式追随は本質的に後追いになるため、**marker 一致を制限の根拠として採用し、待機時間だけを既定値 (`UNKNOWN_FORMAT_FALLBACK_WAIT_MINUTES` = 30 分) で埋める**方式に変更した。既定値が実際の reset より短い場合は wakeup 後に再検出されて再 park されるだけで、retry は `max_retries` で有界。既定値適用時は checker が stderr に警告を出し (cli-pr-monitor がログ転送)、書式再変更の検知シグナルを兼ねる。これにより「書式変更 → 即 silent success」の経路は構造的に閉じ、書式追随 (下記手順) は待機時間精度の改善に格下げされる。

**HTML マーカー優先**: walkthrough body の HTML コメント (`<!-- ... rate limited by coderabbit.ai -->`) は heading 文言や本文より stable な可能性が高いため、新 format 検出の優先 source とする (CR 側で UI 文言は変えても internal marker は維持する傾向、本リポジトリ未検証だが post-merge-feedback で再評価)。

### 検出 logic 更新手順 (CR が format を変更した場合)

format drift で `is_rate_limit_comment` が常時 false を返す symptom (= 30+ 分 polling 継続、`RateLimitOutcome::Parked` 経路に乗らない、PR #182 で実観測) を発見した場合の標準対応:

1. **観測**: 該当 PR で `gh api repos/.../issues/<N>/comments --jq '.[] | select(.user.login == "coderabbitai[bot]")' | head` で walkthrough body を確認
2. **grep**: 新 marker 候補 (HTML コメント / heading / 本文文言) を特定
3. **marker 配列 append**: `src/check-ci-coderabbit/src/markers.rs` の `RATE_LIMIT_MARKERS` 配列に新 marker を追加
4. **regex 追加**: `src/check-ci-coderabbit/src/rate_limit.rs` の `extract_wait_time` から呼ばれる `extract_<format-name>_format_wait_time` ヘルパーを新規追加 (既存世代との共存パターン)
5. **fixture 追加**: 同 `#[cfg(test)]` mod に新 format fixture を 2-3 variant 追加 (順位 168 と同 pattern)。既存 fixture は backward compat のため維持。**実 incident の comment body を出典付きで fixture 化する** (ADR-049)
6. **ADR-034 update**: 本 ADR の § 既知 CR rate-limit format 一覧 table に新 format 行を append

### auto-trigger 投稿

- body 内 wait time 表現を regex 抽出 (`extract_wait_time` が multi-variant 自動判定)
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
- #D-4 (応答スタイル) の **不採用** (2026-05-04 確定) により、潜在 2.5-4M tokens 削減は永続的に見送り

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

### Bundle a component の land 状況 (2026-05-29 update)

Bundle a の 4 component は以下のように消化された:

| component | 状態 | land 経路 |
|---|---|---|
| rate-limit auto-retry 実装 (旧 順位 42) | ✅ land 済 | **PR #113 (Bb-1)** で実装、PR #115 dogfood で実証 |
| ADR-018 / ADR-009 retry policy 明文化 (旧 順位 43) | ✅ 部分達成 | ADR-018 追記 (2026-05-06)、残作業は ADR-009 navigation 注記 |
| #D-1 (gh CLI 規則を `~/.claude/rules/common/git-workflow.md` に追記) | 別経路で完了 | 計画書消化、本 ADR 当初 entry は retire 済 |
| #D-3 (`check-ci-coderabbit --list-findings` Rust モード) | 状況依存 | todo entry 化されている場合 land 状況を確認 (本 ADR では trackable な PR # を直接保持しない方針) |

詳細な進捗 trackable な permanent reference は本 ADR § "Bundle b との関係" 表 (line 188+) を参照。`docs/todo*.md` の `順位 N` entry は ephemeral artifact であり、entry 削除時に本セクションの言及が dead-pointer 化しないよう PR # primary 引用を優先する。

### 新セッションで最初に確認すべきこと

1. `git log --oneline -5` で master の最新状態を確認 (Bundle Z Phase 2/3 / Bundle b / Bundle CR-RL 等の land 済 PR を把握)
2. 本 ADR (ADR-034) を読む — 特に "Bundle b との関係" 表で Bundle a の現状を把握
3. ADR-018 (cli-pr-monitor takt 移行) を読む — rate-limit retry policy の現状実装
4. memory `project_coderabbit_rate_limit_overlay.md` を読む
5. **未着手 component** を確認: 旧 順位 46 (integration test) / 旧 順位 49 (parse_findings error-path test infra) が `docs/todo*.md` 系列に entry を持つか `grep "整合性 test\|parse_findings" docs/todo*.md` で確認
6. **どの Sub-PR を実施するか確認**: Bundle a 残作業 (Sub-PR 2 縮小版) または Bundle CR-RL (順位 167-169、本 ADR 採用ロジックと隣接領域) のどちらから着手か (Bundle CR-RL は format drift 修正の優先度が高い)

### 完了条件

- Sub-PR 1 + Sub-PR 2 が両方 land
- ADR-018 に rate-limit retry ポリシーが明文化される (本 ADR の設計詳細を反映)
- dogfood で 1-2 PR 試験運用、rate-limit 自動回復が観測される
- ユーザー手動介入 (`@coderabbitai review` 投稿等) が 0 になる
- 本 ADR のステータスを「承認済み」に変更

## 将来の検討事項

### Bundle a 着手時の前提条件 reality check

- CR の rate-limit 仕様が変わっていないか (memory `project_coderabbit_rate_limit_overlay.md` の挙動が再現するか) を着手前に dogfood で確認
- `gh api` の rate-limit (CR とは別、GitHub API 側) が干渉しないか観察

## Bundle b との関係 (2026-05-06 追記、`docs/coderabbit-monitoring-efficiency.md` retire 時に集約)

Bundle b (Bb-1/Bb-2/Bb-3、PR #113-115、2026-05-05〜06 land) は本 ADR とは別領域 (Cron 機構 vs structured findings 消費) を扱うが、**Sub-PR 2 の主機能 = cli-pr-monitor rate-limit auto-retry + `@coderabbitai review` auto-trigger は Bb-1 で実質達成済**。Bundle a Sub-PR 2 の scope を再整理する。

### Sub-PR 2 の構成変化 (Bundle b land 後)

| component | 旧 (本 ADR 当初) | 新 (Bundle b land 後) |
|---|---|---|
| rate-limit auto-retry 実装 (順位 42) | Sub-PR 2 で着手予定 | ✅ **Bb-1 (PR #113) で land 済**。CronCreate park モデルで `[rate_limit] reset 後の即時 retrigger → @coderabbitai review 自動投稿` が PR #115 dogfood で実証 |
| ADR-018 / ADR-009 retry policy 明文化 (順位 43) | Sub-PR 2 で着手予定 | ✅ **ADR-018 追記 (2026-05-06) で部分達成**。CronCreate 特性 + park モデル + Bundle b で再導入された経緯を記載済。残作業は ADR-009 navigation 注記 (Superseded by ADR-018 partial の link) |
| integration test (順位 46) | Sub-PR 2 で着手予定 | 未着手。Bb-1/Bb-2 の unit test (poll.rs / state.rs sibling parity) は存在するが、CR rate-limit 検出 → backoff → retry サイクルの **end-to-end test** はまだない |
| `parse_findings` error-path test infra (順位 49) | Sub-PR 2 で着手予定 | 未着手。Bundle b は `parse_findings` を touch していない別 area |

### Sub-PR 2 着手時の推奨 scope

旧 4 component → 残 3 component に縮小:

- 順位 43 残 (ADR-009 navigation 注記、XS)
- 順位 46 (integration test、M)
- 順位 49 (parse_findings error-path test infra、M)

順位 42 は Bb-1 で達成済のため Sub-PR 2 から除外、structured findings 駆動化 (= `check-ci-coderabbit --list-findings` 消費に切り替え) は残課題として 順位 46 の integration test 設計時に併せて検討する。

### Bundle b との並行進行性 (旧 `coderabbit-monitoring-efficiency.md` から引き継ぎ)

- Bundle b と Bundle a Sub-PR 2 は **別領域** (Cron 機構 vs structured findings 消費) で並行進行可能と当初設計されたが、結果的に Bundle b 先行 land で Sub-PR 2 scope が縮小した
- 今後同種の bundle 並行設計を行う際の教訓: **「別領域」と判断した bundle が実装段階で重なる可能性がある** (Bb-1 の rate-limit auto-retry と Sub-PR 2 の rate-limit auto-retry が同 cli-pr-monitor を touch するため)。bundle 設計時に touched modules ベースで重複可能性を確認すべき

## References

- ADR-018: cli-pr-monitor takt 化
- ADR-009: 旧 Post-PR Monitor (Superseded by ADR-018 partial)
- ADR-022: 自動化コンポーネントの責務分離原則
- ADR-026: Cargo workspace
- ADR-030: Deterministic post-merge-feedback
- (削除済) `docs/pipeline-token-efficiency.md` #D セクション (採用判定改訂 2026-05-02、本 ADR で経緯保存)
- memory `project_coderabbit_rate_limit_overlay.md`
- PR #99 (本 ADR の起源、cli-pr-monitor の rate-limit detection gap が顕在化したセッション)
