# takt パイプライン トークン効率調査・改善計画

> **動機**: Claude Code Max 5x の 5 時間レートリミットの 90% を 3 時間時点で消費する事象が観測された (PR #97 セッション、2026-04-30 〜 2026-05-01 JST)。本ドキュメントは、その session log (`6cbc5021-...jsonl`、6.18MB / 1180 assistant turns) を分析した結果のうち、**未実施の改善方向のみ** を記録する。
>
> **方針**: 各改善案は「調査結果 → 改善案 → 採用判定」の 3 セクションで管理する。実装決定後は本ドキュメントから該当セクションを削除し、ADR 化または todoX.md にタスク登録する。
>
> **状態**: 試験運用 (本ドキュメントは "計画書" であり、残作業を消化したら役割を終える)
>
> **完了済 Bundle (履歴・調査内容は本ドキュメントから削除済)**:
>
> | Bundle | 内容 | 完了 PR |
> |---|---|---|
> | Bundle Y2 | #A-1 + #C-1 (analyze facets を haiku 化) | #98 |
> | Bundle Z Phase 1 | #B-α (Rust comment lint hook、決定論レイヤー) | #99 |
> | Bundle a Sub-PR 0 | #D-1 (gh CLI 規則 → `~/.claude/rules/common/git-workflow.md`) + ADR-034 起案 | #100 |
> | Bundle a Sub-PR 1 | #D-3 (`check-ci-coderabbit --list-findings` モード) | #101 |

---

## 観測データ (PR #97 セッション、2026-04-30 〜 2026-05-01 JST)

> **用途**: 残作業の改善効果検証時の比較ベースライン。

### セッション全体

| 指標 | 値 |
|---|---|
| Assistant turns | 1,181 |
| 一意トークン (uncached input + cache_creation) | 13.64M |
| Cache read 累積 | 350.9M |
| Output tokens | 833K |
| Cache 再生成倍率 | 約 9x |
| takt パイプライン総時間 | **114.7 分** (セッション全体の 63%) |

### takt パイプライン別

| パイプライン | Runs | 総 iter | avg iter | 総時間 | avg 時間 | iter 分布 |
|---|---|---|---|---|---|---|
| pre-push-review | 6 | 15 | 2.5 | 47.9 分 | 8.0 分 | {1×3, 3×2, **6×1**} |
| post-merge-feedback | 4 | 8 | 2.0 | 35.5 分 | 8.9 分 | {2×4} |
| post-pr-review | 6 | 9 | 1.5 | 31.3 分 | 5.2 分 | {1×4, 2×1, 3×1} |

### Bash tool_result サイズ (主な context bloat 源泉)

| Bash サブカテゴリ | calls | chars total | avg | max |
|---|---|---|---|---|
| `gh pr` / `gh api` (CR query) | 76 | 303,697 | 4,000 | 47,980 |
| grep/head (log inspect) | 59 | 79,806 | 1,352 | 7,280 |
| tail (push log) | 35 | 71,472 | 2,042 | 7,252 |
| jj | 78 | 68,048 | 872 | 8,918 |
| cargo-test | 27 | 43,184 | 1,599 | 12,044 |

---

## #A: post-merge-feedback パイプライン (残作業)

### #A-2: trivial PR の post-merge-feedback skip 条件追加

doc-only PR (`.md` のみ変更) や 1-commit fix PR では post-merge-feedback の ROI が低い。`cli-merge-pipeline` 側で PR diff size を判定して skip する。

**変更箇所**: `src/cli-merge-pipeline/` の merge 後処理ロジック
**判定条件 (案)**:

- diff の changed files が `*.md` のみ
- かつ commit 数 = 1
- かつ +/- 合計 < 50 行

**期待効果**: doc PR で post-merge-feedback 9 分丸ごと削減。月数件の doc-only PR があれば効果大。

**リスク**: doc PR でも文書間 reference 整合性等の発見がありうる。skip 判定は緩めに設定し、誤 skip による学習機会損失を最小化する。

### #A-3: transcript filter の絞り込み強化

analyze-session が読む `.takt/post-merge-feedback-transcript.jsonl` には **session 全履歴** が入る。PR-related な部分のみ filter すれば input token 削減 (analyze-session の cache_read 削減)。

**変更箇所**: `cli-merge-pipeline` の transcript 生成ロジック
**現状**: 全 session
**改善案**: 当該 PR の作成 commit から merge までの時刻 range で filter

**期待効果**: analyze-session の input token 30-50% 削減 (推定)。dogfood で実測必要。

**リスク**: PR 作成前の議論 (設計判断、却下されたアイデア) が落ちる可能性。post-merge-feedback の知見の質に影響しうる。

### 採用判定

| 改善案 | ROI | 実装コスト | 推奨 |
|---|---|---|---|
| #A-2 trivial PR skip | ★★★★ | S (Rust 判定ロジック) | 中期 |
| #A-3 transcript filter | ★★★ | M (cli-merge-pipeline 改修) | dogfood 後 |

---

## #B: pre-push-review パイプライン (Bundle Z Phase 2 / 3)

> **背景**: 6 iter / 17-18 分の outlier が PR #97 で 2 回発生 (各 round で個別)。総時間 36 分が突出 (47.9 分中 75% を消費)。
>
> **アーキテクチャ 3 層**: 「決定論レイヤー (#B-α、PR #99 で完了) → 制約付き修正 (#B-β、Phase 2) → 異常検知レビュアー (#B-γ、Phase 3)」を Phase 1〜3 で順次実装する設計。各 Phase 完了後の dogfood で次 Phase 着手判断。

### Phase 2/3 着手前に把握すべき構造的問題

#### Workflow 構造

[`.takt/workflows/pre-push-review.yaml`](../.takt/workflows/pre-push-review.yaml):

```text
reviewers (parallel: simplicity + security) → fix → (loop, threshold 2) → supervise → fix_supervisor → COMPLETE
```

`loop_monitors.threshold: 2` で reviewers→fix サイクル 2 周まで許容。逸脱で supervise → fix_supervisor へエスカレート。**6 iter = 2 cycles + supervise + fix_supervisor の最大 path**。

#### 観測された 3 つの根因 (6-iter run の解析より)

両 run とも Iter 1 で全 finding が検出されない / fix が新たな violation を introduce することで Iter 2 が必要になり、結果として supervise + fix_supervisor のエスカレートで 6 iter に膨らんだ。

| 根因 | 例 | 解消手段 |
|---|---|---|
| 1. simplicity-review iter 1 の検出漏れ | LLM の attention drift で What コメント S04 を見落とし | **#B-γ で解消対象** |
| 2. fix step が新 violation を introduce | F-001 修正で `match Ok / Err` 採用 → nesting depth 7 を導入 | **#B-β で解消対象** |
| 3. AI が What/How コメントをそもそも書く | explanatory output style の指示があっても Claude の習性として残る | **#B-α (PR #99) で解消済** |

### #B-β: 制約付き fix instruction (Phase 2)

**設計思想**: fix step の self-check を LLM 判断ではなく **機械的指標の diff 比較** に置き換える。指標増加で fix 自身がやり直しを self-trigger。

**実装方法**:

- `.takt/facets/instructions/fix.md` の Completion criteria に追加:

  ```markdown
  ## Pre-completion deterministic check

  Compare metrics between pre-fix and post-fix file states for each modified file:
  - max nesting depth (within modified function or change site)
  - function length (lines)
  - non-doc comment count (excluding `///`, `// TODO:`, `// SAFETY:` 等の例外マーカー)

  Run helper script:

      scripts/fix-metrics-check.ps1 <file_path> <pre_state_sha> <post_state_sha>

  If any metric increased, REJECT own fix and try alternative approach
  (e.g., extract function, use early return, simplify match arm).
  ```

- helper script `scripts/fix-metrics-check.ps1`: `rust-code-analysis` (Rust) を呼び出し metrics を JSON 出力 → diff 計算 → 増加 detected なら exit 非ゼロ
- `.takt/runs/<ts>/fix-metrics.log` に記録 (audit 用)

**PR #99 (Phase 1) との同期要件**:

コメント数の例外マーカーリストは PR #99 で実装した [`src/hooks-post-tool-comment-lint-rust/src/main.rs`](../src/hooks-post-tool-comment-lint-rust/src/main.rs) の `ALLOWED_LINE_PREFIXES` / `ALLOWED_BLOCK_PREFIXES` 定数を **single source of truth** として参照すること。fix.md と lint hook で例外マーカー定義が乖離すると、lint で許容されたコメントが fix の non-doc comment count を増やして誤 reject となる。

**期待効果**:

- 根因 2 (fix が新 violation を introduce) の **構造的排除**
- LLM 判断ではなく数値比較なので、attention drift や指示読み飛ばしの影響を受けない

**リスク**:

- 「適切な refactor で一時的に行数増加」のケース判定 (関数分割で個別関数長は減るが modified function 範囲では増える)
  → 対策: function 単位ではなく **diff 内の change site 周辺** に scope を絞る
- metric tool の Rust 限定 (PoC は `rust-code-analysis`、将来言語拡張で別 tool 評価)

### #B-γ: reviewer の役割を「検査」から「異常検知」へ (Phase 3)

**設計思想**: 決定論層 (#B-α PR #99 + #B-β) を通過した状態を前提に、reviewer の責務を **「lint で防げない高次違反のみ flag」** に再定義する。enumerate 義務を削除して attention drift 問題を解消。

**前提条件**: Phase 2 (#B-β) 完了後に着手する。決定論層が未完成のまま reviewer 役割を絞ると、二重 miss で違反が pre-push を素通りする。

**変更内容**:

- `.takt/facets/instructions/review-simplicity.md` を書き換え:
  - 旧: 「6 criteria を順次チェック + ALL findings を列挙」
  - 新: 「決定論層 (#B-α + #B-β) で防げない **異常パターンのみ flag**」
    - 例: 巨大な closure / 不自然な抽象化 / 命名 ambiguity / 設計上の concern
    - 例外: comment count / nesting depth / function length は **lint が保証済み** として skip
- `.takt/facets/instructions/review-security.md` も同様の方針 (security lint で防げる範囲は skip、higher-order な脅威のみ flag)

**期待効果**:

- attention drift 問題が消滅 (検出対象が absolute に narrow に)
- 1 iter ALL APPROVE 率が **90% 超** に到達 (決定論層で大半が intercept されるため)
- review 所要時間も短縮 (現 baseline 1m 30s〜3m → 30s〜1m 期待)

**リスク**:

- deterministic 層 (#B-α / #B-β) の coverage gap で漏れた違反が reviewer もスルー → 二重 miss の可能性
  → 対策: reviewer の異常検知 instruction に「過度に narrow にしすぎない」ガイドラインを残す
- 「異常」の定義が LLM 主観で false positive が出る場合、決定論層 update で対応 (lint rule 追加)

### 採用判定

| 改善案 | ROI | 実装コスト | 推奨 |
|---|---|---|---|
| **#B-β 制約付き fix instruction** | ★★★★ | M (helper script + facet update) | **Phase 2** (PR #99 後の dogfood 観測完了後着手) |
| **#B-γ reviewer 役割変更** | ★★★ | S (facet instruction 書き換え) | **Phase 3** (Phase 2 dogfood 観測完了後着手) |

**期待累積効果** (Phase 1 完了済 + Phase 2/3 完了後): pre-push iter 分布 `{1×3, 3×2, 6×1}` (PR #97 ベースライン) → `{1×N}` (1-iter ALL APPROVE 構造化)。outlier 率 1/6 (16.7%) → 0% 達成試算。

---

## #C: post-pr-review パイプライン (残作業)

> **背景**: pre-push-review と比較すると概ね健全だが、3-iter の outlier が 1 件あり (PR #96 の auto-fix run、10m 19s)。

### #C-2: fix step 報告に基づく Iter 3 短絡

**根因**: post-pr-review の 3-iter 解析より、`review-comments.json` snapshot は fix 後も refresh されないため、Iter 3 の analyze は **同じ findings を再評価する**。Iter 3 output 例:

> 「`.takt/review-comments.json` is a snapshot captured before fix iteration 1 ran; CodeRabbit has not re-reviewed yet, so the same 2 findings appear. The previous fix step report indicates both were addressed. **I verified each by reading the current source**...」

つまり Iter 3 は「fix step が言う通りに本当に修正されているか」をソースを読んで再確認している。**fix step を信頼すれば不要な作業**。

**実装案**:

- `fix.md` instruction の `## Convergence gate` table の `persists` が 0 で `misdirected` が 0 なら "fully resolved" マーカーを report に書く
- workflow rule に `condition: All findings fixed (no persists)` を追加して `next: COMPLETE`

**期待効果**: 3-iter run を 2-iter に圧縮 (~3 分削減)。年に数十回ある仮想シナリオで累積効果

**リスク**:

- fix step の自己評価信頼性 (現状でも `persists: 0` を report しているが、未修正のまま 0 を書く可能性ゼロではない)
- 後続 supervise step でカバーされない場合は安全網が薄くなる

### #C-3: rate-limit 発生時の post-pr-review skip

**前提となる完了 PR**: PR #97 (cli-pr-monitor の rate-limit 自動検出 + 再トリガー) で rate-limit 検出機構は実装済。本作業はその検出結果を post-pr-review takt invoke の skip 判定に流用する。

**実装案**: `cli-pr-monitor` 側で `rate_limit.is_some()` の時 takt invoke を skip し、log のみ出力。次のセッションで再起動された時に通常 flow が走る。

**期待効果**: rate-limit 中に post-pr-review が空打ちする 1-2 分を完全削減。本セッションのように rate-limit が頻発する場合 (4 round) で **計 4-8 分削減**

**リスク**: 低い。rate-limit 中に findings は得られないため skip は妥当。

### 採用判定

| 改善案 | ROI | 実装コスト | 推奨 |
|---|---|---|---|
| #C-3 rate-limit skip | ★★★★ | S (cli-pr-monitor 1 分岐) | 即実施 |
| #C-2 Iter 3 短絡 | ★★ | M (workflow + instruction 改修) | dogfood 後 |

---

## #D: Claude 応答スタイル (保留)

> **完了済の関連項目**: gh CLI 使用パターン最適化は Bundle a (PR #100 + #101) で対応済。`check-ci-coderabbit --list-findings` で構造化取得が可能になっている (#C-3 でも活用余地あり)。

### #D-4: Claude 応答スタイルの簡素化 ⏸️ 保留 (2026-05-02)

**背景**: Claude (私自身) の text-only 応答が 8.5M cache_creation tokens (全体の 62%) を占める。これらは **後続全 turn の cache に乗り続ける** ため、初回の出力サイズが最終的に 9x で billable input token に膨らむ (1KB の応答 → 後続 9KB)。

**保留理由** (PR #99 セッション末のユーザー判断、ADR-034 参照):

- **思考連続性低下リスク**: 中間出力 (Insight ブロック / 完了報告 / 分析テーブル) は後続 turn の cache に乗り、Claude が「これまでの判断」を参照するソース。削減すると後段で context 再構築 (再 grep / 再 read) を招き、token カテゴリが入れ替わるだけで正味削減が縮む可能性
- **副作用観測手段が未確立**: ルール導入で実際にどれだけ削減 / どれだけ思考品質低下するかの定量比較が困難
- **再評価条件**: Bundle Z Phase 2/3 (#B-β / #B-γ) 完了後、副作用観測手段 (例: session 比較メトリクス、思考品質 proxy 指標) が確立してから慎重 pilot

**ガイドライン案** (採用時に `~/.claude/rules/common/coding-style.md` または専用 rule に追加):

```markdown
## トークン効率優先の応答スタイル (rate-limit 不安定期は特に)

### CR review listing
- 本文・suggestion・修正案の quote を **含めない** (file:line + severity + 1 行要約のみ)
- 「outdated 解釈」セクションは指摘がある場合のみ
- 各 finding 末尾の判定 (✅推奨 / ⚠️任意 / ❌不採用) は 3 文字記号のみで詳細根拠は省略

### 完了報告
- PR push 完了: PR URL + commit hash + テスト結果のみ
- merge 完了: PR URL + 主要数値 (commits / iterations) のみ
- 「次のアクション」提案は user が要求した場合のみ

### Insight ブロック
- 1 ターンに最大 1 つまで
- 真に非自明 (調査結果・予想外の挙動) のみ。一般的な感想は省略
```

**期待効果**: text-only response 約 30-50% 削減 (推定 **2.5-4M cache_creation tokens 削減**)

---

## 全体統合: 残作業の PR 計画

> **方針**: 「少ない PR で的確に + フィードバックループを活かす」を満たすため、3 PR の依存順実行とする。Bundle Z Phase 2/3 は dogfood signal の純度確保のため分割必須、その他は即時 skip 系 / fix-trust 系で thematic にバンドル。

### PR 計画 (3 PR、依存順)

#### PR 1: 即時 skip バンドル (即実施可)

| 含む項目 | 変更箇所 | 内容 | effort |
|---|---|---|---|
| #C-3 | `cli-pr-monitor` | rate-limit 検出時に post-pr-review takt invoke を skip (PR #97 の `rate_limit.is_some()` を流用) | S |
| #A-2 | `cli-merge-pipeline` | doc-only かつ commit 数=1 かつ +/-<50 の trivial PR で post-merge-feedback skip | S |

**バンドル根拠**: 異なる crate だが両方とも「条件検出 → パイプライン skip」の単純 guard 追加。発火する PR の種類が異なる (rate-limit 中の PR vs doc-only PR) ため、後続 dogfood で誤発火が起きても帰属が明確。

**期待効果**: rate-limit 頻発セッションで 4-8 分削減 + doc PR 月次発生分の post-merge-feedback コスト削減

#### PR 2: Bundle Z Phase 2 — #B-β 単独 (PR #99 dogfood 完了後)

| 含む項目 | 変更箇所 | 内容 | effort |
|---|---|---|---|
| #B-β | `.takt/facets/instructions/fix.md` + `scripts/fix-metrics-check.ps1` (新規) | 制約付き fix instruction (deterministic check)。例外マーカーは PR #99 の `ALLOWED_LINE_PREFIXES` / `ALLOWED_BLOCK_PREFIXES` を import | M |

**単独 PR にする根拠**: 決定論メトリクス層は novel で、合理的 refactor の誤 reject リスクが未知。Phase 3 (#B-γ) は Phase 2 が信頼できることを前提に reviewer 役割を絞るため、**Phase 2 単独 dogfood で誤 reject 率を計測しないと Phase 3 の二重 miss リスクが評価不能**。

**期待効果**: pre-push-review 根因 2 (fix が新 violation を introduce) の構造的排除

#### PR 3: Phase 3 + fix-trust 連帯 (PR 2 dogfood 完了後)

| 含む項目 | 変更箇所 | 内容 | effort |
|---|---|---|---|
| #B-γ | `.takt/facets/instructions/review-{simplicity,security}.md` | reviewer enumerate 義務を削除、決定論層が intercept する metric (comment count / nesting / function length) は skip、異常検知のみ flag | S |
| #C-2 | `.takt/workflows/post-pr-review.yaml` + `fix.md` の Convergence gate | fix step が `persists: 0 / misdirected: 0` を report したら Iter 3 analyze を skip して COMPLETE 直行 | M |

**バンドル根拠**: 両方とも **「LLM が出した結果を後段で再検証しない」** という設計哲学の応用。失敗モードも共通 (fix step が誤って "fully resolved" を report → 後段でカバーされない)。同 PR で land すると「決定論層 + fix step trust」のフルシフトが 1 セッションで観測でき、効果計測も統合的。

**期待効果**: pre-push iter 分布 → `{1×N}` 構造化 (1-iter ALL APPROVE 90% 超) + post-pr-review 3-iter outlier 消失

### 実行順序

```text
時間 →

PR 1 (skip バンドル)        ━━━╋━━━━━━━━━━━━━━━━━ [merge → 通常 dogfood で観測]
                              ↓
PR 2 (Bundle Z Phase 2)         ━━━╋━━━━━━━━━━━━━━ [merge → 1-2 PR で誤 reject 率計測]
                                    ↓
PR 3 (Phase 3 + fix-trust)            ━━━╋━━━━━━━━ [merge → outlier 0% 検証]

任意: PR 1 と PR 2 は並列着手可 (依存なし)
必須: PR 3 は PR 2 merge + dogfood 1-2 PR 後
```

### 各 PR 着手前チェックリスト

| PR | 着手前に確認すべきこと |
|---|---|
| PR 1 | なし (即着手可) |
| PR 2 | PR #99 の `ALLOWED_LINE_PREFIXES` 定数が安定していること (PR #99 merge 後の dogfood で例外マーカー追加が落ち着いていること) |
| PR 3 | PR 2 merge 後 1-2 PR で **適切な refactor が誤 reject されていないこと** を確認 (`.takt/runs/<ts>/fix-metrics.log` を観測) |

### 番外: #A-3 transcript filter (任意タイミング)

**判断**: PR 1〜3 のいずれにも組み込まない。**スキマ時間の単独 PR** か **#D-4 再評価セッション** に同梱が妥当。

**理由**:

- 他のどの PR とも依存も共通テーマもない (analyze-session の input range filter という独立 infra 作業)
- effort M でテスト追加が要る → PR 1 に混ぜると規模が膨らむ
- Phase 3 dogfood の signal 純度を保ちたいので PR 3 にも混ぜない
- ROI ★★★ で優先度が中程度

### 期待効果 (残作業)

| 項目 | 削減対象 | 想定削減量 | 検証指標 |
|---|---|---|---|
| Bundle Z Phase 2 + 3 | pre-push-review iter 数 | **outlier 0% 達成 + 1-iter ALL APPROVE 90% 超** | pre-push-review iter 分布 (`{1×N}` 集中度)、6-iter outlier 発生率 |
| #A-2 trivial PR skip | doc-only PR の post-merge-feedback 起動 | doc PR ごとに 9 分削減 | post-merge-feedback runs 数 |
| #A-3 transcript filter | analyze-session の input token | 30-50% 削減 (推定) | analyze-session の billable input tokens |
| #C-2 Iter 3 短絡 | post-pr-review iter 3 | 3-iter run を 2-iter に圧縮 (~3 分削減/run) | post-pr-review の avg iter 数 |
| #C-3 rate-limit skip | rate-limit 中の空打ち | 計 4-8 分削減/session (rate-limit 頻発時) | rate-limit 検出時の post-pr-review skip 率 |
| #D-4 (保留) | Claude text-only response | 潜在 2.5-4M tokens 削減 (18-29%)、要副作用観測 | session 比較メトリクス (要設計) |

### 検証方法 (Bundle 実装後に実施)

実装後セッションを 1 つ完走させた後、以下を「観測データ」セクションと比較。

#### ① セッション全体メトリクス比較 (全残作業共通)

```bash
# 別セッションの jsonl path を取得 (例: ~/.claude/projects/<project>/<session-id>.jsonl)
python3 - <<'EOF'
import json
totals = {'cache_creation': 0, 'cache_read': 0, 'output': 0}
turns = 0
with open('<NEW_SESSION_JSONL_PATH>') as f:
    for line in f:
        try: obj = json.loads(line)
        except: continue
        usage = obj.get('message', {}).get('usage')
        if not usage: continue
        totals['cache_creation'] += usage.get('cache_creation_input_tokens', 0)
        totals['cache_read'] += usage.get('cache_read_input_tokens', 0)
        totals['output'] += usage.get('output_tokens', 0)
        turns += 1
print(f'Turns: {turns}')
print(f'Cache creation: {totals["cache_creation"]:,}')
print(f'Cache read: {totals["cache_read"]:,}')
print(f'Output: {totals["output"]:,}')
EOF
```

**比較値 (ベースライン)**:

- Turns: 1,181
- Cache creation: 13,638,572
- Cache read: 350,868,831
- Output: 833,825

#### ② takt パイプライン時間比較 (#A / #C 系の検証用)

```bash
for d in .takt/runs/<NEW_SESSION_DATE>-*-{pre-push-review,post-pr-review,post-merge-feedback}*; do
  iter=$(grep -o '"iterations":\s*[0-9]*' $d/meta.json | head -1 | grep -o '[0-9]*')
  start=$(grep -o '"startTime":\s*"[^"]*"' $d/meta.json | grep -o '[0-9TZ:.-]\{20,\}')
  end=$(grep -o '"endTime":\s*"[^"]*"' $d/meta.json | grep -o '[0-9TZ:.-]\{20,\}')
  echo "$(basename $d): iter=$iter start=$start end=$end"
done
```

**比較値 (ベースライン)**:

| パイプライン | Runs | 総 iter | 総時間 |
|---|---|---|---|
| pre-push-review | 6 | 15 | 47.9 分 |
| post-merge-feedback | 4 | 8 | 35.5 分 |
| post-pr-review | 6-8 | 9-15 | 22-31 分 |

#### ③ pre-push-review iter 分布比較 (Bundle Z Phase 2 / 3 検証専用)

ベースライン: `{1×3, 3×2, 6×1}` = 6 runs (うち 6-iter outlier 1 件、avg iter 2.5)

目標 (Phase 2/3 完了後): `{1×N}` (1-iter 固定) で 6-iter outlier 消失 (outlier 率 1/6 (16.7%) → 0%)、avg iter 2.5 → 1.0、1-iter ALL APPROVE 率 90% 超

### 別セッションでの作業指示 (テンプレート)

```markdown
## 作業概要
本セッションは docs/pipeline-token-efficiency.md の [PR 1 | PR 2 | PR 3 | 番外 #A-3] を実装する。
含む項目と変更箇所は同ドキュメントの「PR 計画」セクションを参照。

## 着手前チェック
docs/pipeline-token-efficiency.md の「各 PR 着手前チェックリスト」を確認し、前提が満たされていることを確認。

## 実装タスク
- [ ] PR 計画に列挙された全項目の実装
- [ ] テスト追加 (該当する場合)
- [ ] 実装 PR を作成
- [ ] merge 後に本セッションを 1 つ完走させる (検証用 dogfood)

## 検証方法
docs/pipeline-token-efficiency.md の「検証方法」を実行。
ベースライン値と比較し、想定削減量に届いているか測定。

## 完了基準
- 想定削減量の 70% 以上達成 → 本ドキュメントから該当 PR セクション削除 + 進捗管理に PR 番号記録
- 想定削減量に届かず → 原因分析を「全体統合」セクション末尾に追記し、必要なら計画再編
```

---

## 進捗管理

| 改善案 | 配置 | 状態 | 採用日 | 完了 PR | 備考 |
|---|---|---|---|---|---|
| #C-3 rate-limit skip | **PR 1** | 計画 | - | - | PR #97 の rate-limit 検出を流用 |
| #A-2 trivial PR skip | **PR 1** | 計画 | - | - | 単独実施可 |
| #B-β 制約付き fix instruction | **PR 2** (Bundle Z Phase 2) | 計画 | 2026-05-01 | - | PR #99 の例外マーカー定数と同期必須 |
| #B-γ reviewer 役割変更 | **PR 3** (Bundle Z Phase 3) | 計画 | 2026-05-01 | - | PR 2 dogfood 完了が前提 |
| #C-2 Iter 3 短絡 | **PR 3** (fix-trust 連帯) | 計画 | - | - | PR 3 で #B-γ と同梱 |
| #A-3 transcript filter | **番外** | 計画 | - | - | スキマ時間の単独 PR か #D-4 再評価時に同梱 |
| #D-4 応答スタイル簡素化 | (保留) | 保留 (ADR-034) | 2026-05-02 | - | Bundle Z (PR 2 + PR 3) 完了後再評価 |

---

## 関連

- 元セッション: `C:\Users\HIROKI\.claude\projects\e--work-claude-code-hook-test\6cbc5021-e5f4-420d-853b-e1b467d45ae4.jsonl`
- 前提となる完了 PR (残作業の依存先):
  - **PR #97** (cli-pr-monitor rate-limit 自動検出): #C-3 の前提 — 検出機構を流用して skip 判定を追加
  - **PR #99** (Bundle Z Phase 1 — `src/hooks-post-tool-comment-lint-rust/`): #B-β / #B-γ の前提 — 例外マーカー定数 (`ALLOWED_LINE_PREFIXES` / `ALLOWED_BLOCK_PREFIXES`) を single source of truth として参照
  - **PR #100** (Bundle a Sub-PR 0 — gh CLI 規則 + ADR-034 起案): #D-4 保留判断の根拠
  - **PR #101** (Bundle a Sub-PR 1 — `check-ci-coderabbit --list-findings`): rate-limit 関連で構造化 findings 取得が可能
- 関連 ADR:
  - [ADR-015](adr/adr-015-push-runner-takt-migration.md) (push runner takt 化)
  - [ADR-018](adr/adr-018-pr-monitor-takt-migration.md) (cli-pr-monitor takt 化)
  - [ADR-020](adr/adr-020-takt-facets-sharing.md) (facets 共通化)
  - [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) (post-merge-feedback 決定論化)
  - [ADR-034](adr/adr-034-coderabbit-auto-monitoring.md) (#D-4 保留判断 + Bundle a 設計根拠)
- 関連 workflow: [.takt/workflows/](../.takt/workflows/)
