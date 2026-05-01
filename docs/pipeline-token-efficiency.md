# takt パイプライン トークン効率調査・改善計画

> **動機**: Claude Code Max 5x の 5 時間レートリミットの 90% を 3 時間時点で消費する事象が観測された (PR #97 セッション、2026-04-30 〜 2026-05-01 JST)。本ドキュメントは、その session log (`6cbc5021-...jsonl`、6.18MB / 1180 assistant turns) を分析した結果と、4 つの改善方向 (#A〜#D) の調査結果・改善方針を記録する。
>
> **方針**: 各改善案は「調査結果 → 改善案 → 採用判定」の 3 セクションで管理する。実装決定後は本ドキュメントから該当セクションを削除し、ADR 化または todo3.md/todo4.md にタスク登録する。
>
> **状態**: 試験運用 (本ドキュメントは "計画書" であり、実装が完了したら役割を終える)

---

## 観測データ (PR #97 セッション、2026-04-30 〜 2026-05-01 JST)

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

## #A: post-merge-feedback パイプライン

### 調査結果

`.takt/workflows/post-merge-feedback.yaml` 解析の結果、観測された **「常に 2 iterations」パターンは workflow 設計上の必然** と判明。

- **Step 1 `analyze`**: 3 facets を並列実行 (analyze-pr / analyze-session / analyze-prepush-reports)
- **Step 2 `aggregate-feedback`**: 3 レポートを Plankton 優先度で統合し最終 feedback-report.md を生成

2 step とも必須 (片方を削除すると output が成立しない)。「2nd iter 冗長」仮説は **誤り**。

ただし、別の改善余地が 3 つ見つかった (下記 #A-1, #A-2, #A-3)。

### 改善案

#### #A-1: analyze facets を haiku model 化 (現状 sonnet)

3 facets はすべて `model: sonnet`。analyze 系は「情報源から finding を抽出する分類タスク」で deep reasoning は aggregate 側に任せる方が適切。

**変更**:
```yaml
# .takt/workflows/post-merge-feedback.yaml
# analyze-pr / analyze-session / analyze-prepush-reports の 3 facets
# 変更前: model: sonnet
# 変更後: model: haiku
# aggregate-feedback は sonnet 維持 (品質担保)
```

**期待効果**: 1 run あたり ~2-3 分削減 + token cost 大幅削減 (haiku は sonnet の約 1/3 cost)。4 runs/session で **約 10 分 + 大規模 token 削減**。

**リスク**: haiku が finding を見落とす可能性。dogfood 1-2 PR で品質変化を確認する gating が必要。

#### #A-2: trivial PR の post-merge-feedback skip 条件追加

doc-only PR (`.md` のみ変更) や 1-commit fix PR では post-merge-feedback の ROI が低い。`cli-merge-pipeline` 側で PR diff size を判定して skip する。

**変更箇所**: `src/cli-merge-pipeline/` の merge 後処理ロジック
**判定条件 (案)**:
- diff の changed files が `*.md` のみ
- かつ commit 数 = 1
- かつ +/- 合計 < 50 行

**期待効果**: doc PR (本セッション中の PR #94 等) で post-merge-feedback 9 分丸ごと削減。月数件の doc-only PR があれば効果大。

**リスク**: doc PR でも文書間 reference 整合性等の発見がありうる (Bundle U の起源)。skip 判定は緩めに設定し、誤 skip による学習機会損失を最小化する。

#### #A-3: transcript filter の絞り込み強化

analyze-session が読む `.takt/post-merge-feedback-transcript.jsonl` には **session 全履歴** が入る。PR-related な部分のみ filter すれば input token 削減 (analyze-session の cache_read 削減)。

**変更箇所**: `cli-merge-pipeline` の transcript 生成ロジック (どの範囲を filter するか)
**現状**: 全 session
**改善案**: 当該 PR の作成 commit から merge までの時刻 range で filter

**期待効果**: analyze-session の input token 30-50% 削減 (推定)。dogfood で実測必要。

**リスク**: PR 作成前の議論 (設計判断、却下されたアイデア) が落ちる可能性。post-merge-feedback の知見の質に影響しうる。

### 採用判定

| 改善案 | ROI | 実装コスト | 推奨 |
|---|---|---|---|
| #A-1 haiku 化 | ★★★★★ | XS (yaml 1 行 ×3) | **即実施推奨** |
| #A-2 trivial PR skip | ★★★★ | S (Rust 判定ロジック) | 中期 |
| #A-3 transcript filter | ★★★ | M (cli-merge-pipeline 改修) | dogfood 後 |

---

## #B: pre-push-review パイプライン

> **背景**: 6 iter / 17-18 分の outlier が PR #97 で **2 回**発生 (各 round で個別)。総時間 36 分が突出 (47.9 分中 75% を消費)。

### 調査結果

#### Workflow 構造

[`.takt/workflows/pre-push-review.yaml`](../.takt/workflows/pre-push-review.yaml):

```
reviewers (parallel: simplicity + security) → fix → (loop, threshold 2) → supervise → fix_supervisor → COMPLETE
```

`loop_monitors.threshold: 2` で reviewers→fix サイクル 2 周まで許容。逸脱で supervise → fix_supervisor へエスカレート。**6 iter = 2 cycles + supervise + fix_supervisor の最大 path**。

#### 6-iter run の中身 (詳細解析)

**Run 1 (10:30 UTC, 18m 30s)**: PR #97 Phase 4 初回 push

| Iter | 結果 | 内容 |
|---|---|---|
| 1 | REJECT | What コメント 3 件 (S01, S02, S03) を検出 |
| 2 | REJECT | S01-S03 修正済み、**新たに S04 What コメント発見** (parse_rate_limit) |
| 3 | APPROVE | S04 修正済み |

**Run 2 (14:20 UTC, 17m 22s)**: PR #97 Round 3 push (Result 化対応)

| Iter | 結果 | 内容 |
|---|---|---|
| 1 | REJECT | F-001 What コメント (`// 成功時のみ dedup key と state を更新`) |
| 2 | REJECT | F-001 修正済み、**fix が introduce した F-002 (Nesting depth 5+) 発見** |
| 3 | APPROVE | F-002 修正 (match → if let Err パターン) |

#### 共通する waste 源泉

両 run とも **Iter 1 で全 finding が検出されない / fix が新たな violation を introduce** することで Iter 2 が必要になり、その結果 supervise + fix_supervisor のエスカレートで合計 6 iter に膨らんだ。

**根因 1: simplicity-review iter 1 の検出漏れ** (Run 1 の S04)
- 1000+ 行の diff 全体をスキャンする際、LLM の attention drift で一部の What コメントを見落とす
- `review-simplicity.md` instruction には「ALL comments を enumerate」の指示がない

**根因 2: fix step が新 violation を introduce** (Run 2 の F-002)
- F-001 の修正 (What コメント削除) のため `match Ok(()) => / Err(e) =>` パターンを採用したが、これが nesting depth 7 を導入
- `fix.md` instruction には「自分の変更が他の criteria を violate しないか self-check」の指示がない

**根因 3 (より上流): AI が What/How コメントを **そもそも書く***
- explanatory output style mode が "include in conversation, not in code" と指示しているにも関わらず、私 (Claude) はコメントを書く習性がある
- 簡潔に書かせる Stop hook / lint rule が **不在**

### 再評価 (2026-05-01: PR #98 セッション後)

ユーザーフィードバック (PR #98 セッション末) により、本セクションの当初提案 (旧 #B-1〜#B-4) は **構造的に不適切** と判定し全面再編した。論点は以下:

- **6 iter = worst path は「たまたま」ではなく構造的必然**: `非決定 reviewer × 非制約 fix × 短い loop threshold = 高確率で最大 path 到達`
- **LLM 検証器の追加では収束しない**: LLM review → LLM fix → LLM review の連鎖は完全性保証も再現性もない (ask-based の本質的限界)
- **解くべき問題の捉え直し**: 「iter を減らす」ではなく **「iter を不要にする」** = LLM 通過前に決定論層で止める

旧 #B-1〜#B-4 はすべて「LLM 検証器を増設する案」で、批判は以下:

| 旧案 | 批判 | 移行先 |
|---|---|---|
| #B-1 fix self-check | LLM self-check は信頼できない (検出漏れと同じ問題を内包) | 取り下げ → #B-β (制約付き fix、機械的指標 diff) |
| #B-2 reviewer exhaustive scan | recall は上がるが 100% 保証なし、deterministic lint の下位互換 | 取り下げ → #B-γ (reviewer 役割を異常検知に再定義) |
| #B-3 regex lint hook | regex で「意味」を取るのは危険 (Why/What 区別不能、多言語対応で破綻) | 取り下げ → #B-α (AST/トークンベース、コメント存在自体を禁止) |
| #B-4 coding-style.md 強化 | 「文化」であって「制御」ではない (ルール増 → 読まれない、モデル変わる → 崩れる) | **完全削除** (補助層としても採用しない) |

新案 #B-α / #B-β / #B-γ で「決定論レイヤー → 制約付き修正 → 異常検知レビュアー」のアーキテクチャ 3 層を構築する。

### 改善案

#### #B-α: 決定論 comment lint hook (Rust 限定 PoC)

**設計思想**: regex で "What っぽい文章を検出" するのではなく、**コメントの存在自体を制約する** (例外マーカーのみ許可)。意味解析を回避することで言語非依存な shell を確保する。

**検出ロジック**:

- 原則: Rust ソース内のすべての comment (`//`, `/* */`, `///`) を count
- 例外マーカー (実装 `src/hooks-post-tool-comment-lint-rust/src/main.rs` の `ALLOWED_LINE_PREFIXES` / `ALLOWED_BLOCK_PREFIXES` 定数が **single source of truth**):
  - **line comment**: `///` (rustdoc outer) / `//!` (rustdoc inner) / `// TODO:` / `// FIXME:` / `// SAFETY:` / `// NOTE:` / `// HACK:` / `// XXX:`
  - **block comment**: `/**` (block rustdoc outer) / `/*!` (block rustdoc inner)
- 上記以外のコメントは **REJECT** (count > 0 で hook が block)
- マーカー追加・削除時は実装定数と本箇所を必ず同期させる (docs と実装の乖離は次フェーズの誤誘導要因)

**配置 (ADR-002 / ADR-006 / ADR-007 整合)**:

- 新 crate `src/hooks-post-tool-comment-lint-rust/` (PoC は Rust 限定、将来 ts/py を独立 crate で並列追加)
- 既存の **PreToolUse hooks** (`hooks-pre-tool-validate.exe` 等) とは **別エントリ** として配置 (言語別 plugin の独立性確保、ユーザー指示)
- PostToolUse タイミング (Edit/Write 後) で発火、書かれた直後に block して即時修正させる
- ADR-002 (PostToolUse の Biome + oxlint 二段構成) には統合せず、独立 hook entry として並列
- ADR-007 の **AST 層** に位置づけ (正規表現層ではない)。`tree-sitter` / `tree-sitter-rust` で `(line_comment)` / `(block_comment)` ノードを query で抽出

**期待効果**:

- AI が What/How コメントを書いた瞬間に hook が block → 修正させる
- takt 起動時にはコメント問題が解決済みのため、reviewer は ALL APPROVE に近い動作
- 長期的に **6-iter run を 1-iter に構造的に圧縮**

**リスク**:

- 例外マーカーリストの保守 (新例外 `// LICENSE:` 等を追加するたびに list 更新)
- false positive (合理的な Why コメントが誤 block されると開発体験悪化) → 例外マーカー充実で回避
- 派生プロジェクト展開時の言語拡張作業が **言語別に Effort M** ずつ加算 (本 PoC は Rust のみ)

#### #B-β: 制約付き fix instruction

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

**期待効果**:

- Run 2 タイプ (fix が新 violation を introduce) の **構造的排除**
- LLM 判断ではなく数値比較なので、attention drift や指示読み飛ばしの影響を受けない

**リスク**:

- 「適切な refactor で一時的に行数増加」のケース判定 (関数分割で個別関数長は減るが modified function 範囲では増える)
  → 対策: function 単位ではなく **diff 内の change site 周辺** に scope を絞る
- metric tool の Rust 限定 (PoC は `rust-code-analysis`、将来言語拡張で別 tool 評価)

#### #B-γ: reviewer の役割を「検査」から「異常検知」へ

**設計思想**: 決定論層 (#B-α + #B-β) を通過した状態を前提に、reviewer の責務を **「lint で防げない高次違反のみ flag」** に再定義する。enumerate 義務を削除して attention drift 問題を解消。

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
| **#B-α 決定論 comment lint hook (Rust)** | ★★★★★ | M (新 crate + AST + hook 登録) | **Phase 1 (PoC)** |
| **#B-β 制約付き fix instruction** | ★★★★ | M (helper script + facet update) | **Phase 2** |
| **#B-γ reviewer 役割変更** | ★★★ | S (facet instruction 書き換え) | **Phase 3** |

**Bundle 案 (Bundle Z 再編)**: アーキテクチャ 3 層を **3 Phase 分割** で順次実装し、各 Phase の dogfood で次 Phase 着手判断。

- **Phase 1 — #B-α (Rust 限定 PoC)**:
  - 新 crate `src/hooks-post-tool-comment-lint-rust/` を Cargo workspace (ADR-026) に追加
  - `tree-sitter` / `tree-sitter-rust` で `(line_comment)` / `(block_comment)` ノードを抽出 + 例外マーカー判定
  - PostToolUse hook として独立配置 (既存 PreToolUse hooks とは別エントリ、ADR-006 整合)
  - dogfood 1〜2 PR: 例外マーカー漏れ / false positive を観測 → list 拡充
- **Phase 2 — #B-β (制約付き fix)**:
  - `scripts/fix-metrics-check.ps1` + `rust-code-analysis` 統合
  - `.takt/facets/instructions/fix.md` に deterministic check ブロック追加
  - dogfood 1〜2 PR: 「適切 refactor 誤 reject」の頻度を観測 → scope 調整
- **Phase 3 — #B-γ (reviewer 異常検知化)**:
  - `review-simplicity.md` / `review-security.md` 書き換え
  - dogfood 1〜2 PR: 二重 miss 発生率と 1-iter APPROVE 率を観測 → 本採用判断

**期待累積効果**: pre-push iter 分布 `{1×3, 3×2, 6×1}` (PR #97 ベースライン) → `{1×N}` (1-iter ALL APPROVE 構造化)。outlier 率 1/6 (16.7%) → 0% 達成試算。

---

## #C: post-pr-review パイプライン

> **背景**: pre-push-review と比較すると概ね健全だが、3-iter の outlier が 1 件あり (PR #96 の auto-fix run、10m 19s)。

### 調査結果

#### Workflow 構造

[`.takt/workflows/post-pr-review.yaml`](../.takt/workflows/post-pr-review.yaml):

```
analyze → fix → analyze (loop, threshold 2) → supervise → fix_supervisor → COMPLETE
```

`analyze` step は明示的に `model:` 指定なし (default モデル使用)。fix / supervise / fix_supervisor は `model: sonnet`。

#### ディスク上のメタデータ実測 (8 runs)

| 起動時刻 | iter | 所要時間 | 備考 |
|---|---|---|---|
| 04:06 | **3** | 10m 19s | PR #96 auto-fix (Critical/Major 2 件) |
| 04:23 | 1 | 2m 16s | approved |
| 05:21 | 1 | 1m 30s | approved |
| 11:17 | 1 | 2m 26s | PR #97 round 1 (Minor 1 件のみ → user_decision) |
| 11:33 | 1 | 1m 38s | approved |
| 13:59 | 1 | 1m 40s | approved |
| 14:38 | 1 | 1m 22s | approved |
| 15:30 | 1 | 2m 0s | approved |

**8 runs 中 7 runs が 1-iter で完了**。outlier 1 件のみ (12.5%)。pre-push-review の outlier 率 (1/6 = 16.7%) と比べて同水準だが、3-iter 中央値は post-pr-review の方が低い。

#### 3-iter outlier の中身 (PR #96, 10m 19s)

`coderabbit-analysis.md.20260430T041651Z` (Iter 1) → `coderabbit-analysis.md` (Iter 3) を比較:

| Iter | Step | 結果 |
|---|---|---|
| 1 | analyze | CR Major 2 件検出 (lock.rs `LockResult::Acquired` の I/O 失敗時誤返却 / `parse_iso8601` panic) → needs_fix |
| 2 | fix | 両 finding を修正 (新 variant `Unavailable` 追加 + 範囲チェック追加 + 回帰テスト) |
| 3 | analyze | **同じ `.takt/review-comments.json` snapshot を再分析**、各 finding を current source 読んで verify → "Already fixed" 判定 → approved |

#### 観測された waste 源泉

**根因 1: review-comments.json snapshot が refresh されない**

post-pr-review が起動する時点で `cli-pr-monitor` が CR comments を取得し snapshot 化。fix 後も snapshot は更新されないため、Iter 3 の analyze は **同じ findings を再評価する** 必要がある。

Iter 3 の output 例:
> 「`.takt/review-comments.json` is a snapshot captured before fix iteration 1 ran; CodeRabbit has not re-reviewed yet, so the same 2 findings appear. The previous fix step report indicates both were addressed. **I verified each by reading the current source**...」

つまり Iter 3 は「**fix step が言う通りに本当に修正されているか**」をソースを読んで再確認している。**fix step を信頼すれば不要な作業**。

**根因 2: snapshot refresh しても CR の resolution は遅延**

仮に snapshot を refresh しても、CR が thread を resolved にマークするのは数分〜数時間後 (人間が resolve ボタンを押すか、CR が次の review で確認するまで)。即時 refresh の効果は薄い。

**根因 3: rate-limit 発生時に post-pr-review を skip しない**

本セッションで観測されたとおり、CR rate-limit 中に post-pr-review を起動しても新しい findings は得られない。それでも analyze は実行される (~1-2 分の無駄)。

### 改善案

#### #C-1: analyze step に明示的に `model: haiku` を指定

現在 model 未指定 (default = sonnet)。analyze は CR 既存 findings の **分類タスク** で deep reasoning は不要。haiku で十分。

**変更**:
```yaml
# .takt/workflows/post-pr-review.yaml の analyze step
- name: analyze
  edit: false
  persona: code-reviewer
  model: haiku  # 追加
  ...
```

**期待効果**:
- 1-iter run (7/8 runs): 1.5-2.5 分 → 0.5-1 分 (analyze 自体が短縮)
- 3-iter run (1/8 runs): 10m 19s → 7-8 min (analyze × 2 が短縮)
- **session あたり累積 5-7 分削減 + token cost 大削減**

**リスク**: haiku は finding の severity 判定や applicability filter で精度低下する可能性。dogfood 1-2 PR で精度比較必要。

#### #C-2: fix step 報告に基づく Iter 3 短絡

fix step が "All applicable findings fixed" を report に明記した場合、Iter 3 の analyze 自体を skip して COMPLETE に直行する option を workflow に追加。

**実装案 (案)**:
- `fix.md` instruction の `## Convergence gate` table の `persists` が 0 で `misdirected` が 0 なら "fully resolved" マーカーを report に書く
- workflow rule に `condition: All findings fixed (no persists)` を追加して `next: COMPLETE`

**期待効果**: 3-iter run を 2-iter に圧縮 (~3 分削減)。年に数十回ある仮想シナリオで累積効果

**リスク**:
- fix step の自己評価信頼性 (現状でも `persists: 0` を report しているが、未修正のまま 0 を書く可能性ゼロではない)
- 後続 supervise step でカバーされない場合は安全網が薄くなる

#### #C-3: rate-limit 発生時の post-pr-review skip

`cli-pr-monitor` が rate-limit を検出した場合、post-pr-review takt 起動を skip。

**実装案**: `cli-pr-monitor` 側で `rate_limit.is_some()` の時 takt invoke を skip し、log のみ出力。次のセッションで再起動された時に通常 flow が走る。

**期待効果**: rate-limit 中に post-pr-review が空打ちする 1-2 分を完全削減。本セッションのように rate-limit が頻発する場合 (4 round) で **計 4-8 分削減**

**リスク**: 低い。rate-limit 中に findings は得られないため skip は妥当。

### 採用判定

| 改善案 | ROI | 実装コスト | 推奨 |
|---|---|---|---|
| **#C-1** analyze haiku 化 | ★★★★★ | XS (yaml 1 行) | **即実施** (#A-1 と同 PR) |
| #C-3 rate-limit skip | ★★★★ | S (cli-pr-monitor 1 分岐) | 即実施 |
| #C-2 Iter 3 短絡 | ★★ | M (workflow + instruction 改修) | dogfood 後 |

**Bundle 案**: #A-1 (post-merge-feedback haiku) + #C-1 (post-pr-review haiku) を **同 PR で land** 推奨。共通テーマは "分類・抽出タスクは haiku で十分" で、yaml 4 行の変更で完結。**期待効果合計: session あたり 15-20 分 + 大規模 token 削減**。

---

## #D: CR review query / Claude 応答スタイル

> **背景**: `gh pr` / `gh api` 関連 query 76-82 回 / 303KB chars / max 47KB が Bash tool_result の最大カテゴリ。Claude (私自身) の text-only 応答が 8.5M cache_creation tokens (全体の 62%) を占める。

### 調査結果

#### gh CLI 使用パターン (82 calls 解析)

| パターン | 件数 | 特徴 |
|---|---|---|
| `--jq` filter なし | **44 (54%)** | 生 JSON を全取得後に python pipe で filter |
| `--jq` filter あり | 38 (46%) | 効率的 |

**最大の waste 箇所**:

1. **POST `/replies` の応答破棄漏れ** (9 calls, 53KB stdout)
   - CR thread に `resolved: ...` で reply する POST。応答に full reply object が返る (diff_hunk + URL + node_id + body) が、私は **success/fail だけ知れば十分**
   - 最大単発 24KB
   - 改善: `> /dev/null 2>&1` で出力抑制

2. **List endpoint での `--jq` 未使用** (16 calls, 18KB)
   - `gh api .../comments` で全 metadata を取得 → 後段で python フィルタ
   - 改善: `--jq '.[] | {created_at, body_first: .body[:200]}'` で最初から filter

3. **`gh pr view` で過剰 field 取得** (12 calls)
   - `--json reviews,comments,reviewDecision,statusCheckRollup` の `comments` field に CR walkthrough の **embedded base64 internal state** が混入
   - 1 call で 44KB (うち 80% 以上が base64 noise)

4. **特殊大型 outlier** (1 call, **47.98KB**)
   - `gh api .../pulls/N/comments/N/replies -f body=...` の応答 (CR thread への reply で、応答本体に元 thread の diff_hunk 等を含めて返す)

#### Read tool 使用パターン (94 calls, 266KB)

| 指標 | 値 |
|---|---|
| 全文 read | 26 (28%) |
| offset/limit 付き read | **74 (74%)** |
| 同一ファイル複数回 read 上位 | `main.rs ×24`, `todo3.md ×11`, `poll.rs ×9` |

74% が partial read で、これは健全。`main.rs` の 24 回は調査・修正・確認サイクルで再 read する性質上避けにくい。

#### Text-only assistant turn (cache_creation 占有率 62.3%, 8.5M tokens)

主要発生源:
- CR review listing (round 1-4 で計 4 回、各 2-5KB)
- 完了報告サマリ (push 完了 / merge 完了 / fix 完了 で各 1-3KB)
- 分析テーブル (本セッションの token analysis 等で 3-5KB)
- Insight ブロック (各応答に 1-3 個、計約 1KB ずつ)

これらは **後続全 turn の cache に乗り続ける** ため、初回の出力サイズが最終的に 9x で billable input token に膨らむ (1KB の応答 → 後続 9KB)。

### 改善案

#### #D-1: gh CLI 使用ルールの定型化 — `~/.claude/rules/common/git-workflow.md`

私 (Claude) が gh CLI を使うときの定型パターンをルール化:

```markdown
## gh CLI 使用規則

### POST 操作 (作成・更新)

応答 body は破棄する (success/fail は exit code で判別):

```bash
# BAD: 24KB の reply object が返って context に乗る
gh api repos/.../comments/N/replies -f body='resolved: ...'

# GOOD: 出力を捨てる
gh api repos/.../comments/N/replies -f body='resolved: ...' > /dev/null 2>&1
```

### GET 操作 (取得)

`--jq` で必要 field のみ抽出する:

```bash
# BAD: 44KB JSON 全部取得
gh pr view 97 --json reviews,comments

# GOOD: --jq で構造化抽出
gh pr view 97 --json reviews --jq '.reviews | map({commit: .commit.oid[:8], state})'
```

### CR walkthrough 除外

`gh pr view` の `comments` field には CR walkthrough の base64 internal state が含まれる (1 PR で 30KB+)。確認時は `--jq 'del(.comments[].body)'` 等で除外。
```

**期待効果**:
- gh tool_result 削減: ~70KB (POST replies 53KB + jq 化 18KB)
- 9x 再キャッシュ効果で **~150K cache_creation tokens 削減**
- effort: rule 追記のみ (XS)
- 持続性: ルール化で次セッション以降も継続効果

**リスク**: ルール量が増えると AI が読み込まないリスク。`git-workflow.md` の既存セクションに追記する形で目立たせる工夫必要。

#### #D-2: `pnpm cr:findings <PR>` wrapper script 追加

CR findings を私が読みやすい形で取得する shell/Node script を追加:

```bash
$ pnpm cr:findings 97
PR #97 (state: OPEN, head: badaaf57)
Latest CR review: 2026-04-30T14:06:10Z (commit 79b7c3dd)

Unresolved findings (4):
  Major  src/check-ci-coderabbit/src/main.rs:415  updated_at 基準で計算すべき
  Major  src/cli-pr-monitor/src/stages/poll.rs:183 max_duration を素通り
  Major  src/cli-pr-monitor/src/stages/poll.rs:203 失敗時 perma-skip
  Minor  docs/todo.md:69                         順位 絶対参照
```

**期待効果**:
- gh pr view + jq pipeline を script に隠蔽
- 私の応答で「未対応レビューリスト」を作るときの効率化
- effort: S (Node/Bash script + jq クエリ)
- ROI: 高 (CR review listing の繰り返し作業を 1 コマンド化)

#### #D-3: `check-ci-coderabbit --list-findings` モード追加

Rust 側で構造化 findings JSON を生成 (元案 #7 の再掲):

```bash
$ check-ci-coderabbit.exe --list-findings --pr 97
{
  "findings": [
    {"severity": "major", "file": "src/.../main.rs", "line": 415, "summary": "...", "url": "..."},
    ...
  ]
}
```

**期待効果**:
- `gh api` の生 JSON 取得 → Rust 側で構造化済み JSON を一度で取得
- cli-pr-monitor からも消費可能になり、retrigger 自動化と連携
- effort: M (Rust 実装 + テスト)
- ROI: 大 (#D-1 + #D-2 を deterministic に置き換える)

**リスク**: 既存の cli-pr-monitor / check-ci-coderabbit の責務分離 (ADR-022) に抵触しないか要確認。

#### #D-4: Claude 応答スタイルの簡素化 — `~/.claude/rules/common/coding-style.md` または専用 rule

私自身の text-only 応答パターンを抑制するガイドライン:

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

**期待効果**:
- text-only response 約 30-50% 削減 (推定 **2.5-4M cache_creation tokens 削減**)
- 全 #D 案中 **最大 ROI**

**リスク**:
- ルールベースの行動変容は不安定 (持続性が低い)
- ユーザーへの説明不足で意図が伝わらない可能性
- explanatory mode との緊張 (Insight 削減 vs 教育的応答)

### 採用判定

| 改善案 | ROI | 実装コスト | 推奨 |
|---|---|---|---|
| **#D-1** gh CLI 規則 | ★★★★ | XS (rule 追記) | **即実施** |
| **#D-4** 応答スタイル簡素化 | ★★★★★ | S (rule 追記) | **即実施** |
| #D-2 pnpm cr:findings wrapper | ★★★ | S (script) | 中期 |
| #D-3 Rust findings mode | ★★★ | M (Rust) | 長期 |

**Bundle 案 (Bundle Z2?)**: #D-1 + #D-4 を **1 PR で `~/.claude/rules/` に追加** 推奨。effort 合計 S。

**期待累積効果 (Bundle Y2 + Z2 統合)**:
- #A-1 + #C-1 (haiku 化): session あたり 15-20 分削減 + token 大削減
- #D-1 + #D-4 (gh + 応答ルール): cache_creation **3-4M tokens 削減** (全体の 25-30%)
- Bundle Z 再編 (#B-α + #B-β + #B-γ、3 Phase 分割): pre-push iter 数を **1-iter 固定** に構造化 (outlier 率 1/6 (16.7%) → 0% 試算)
- **合計**: rate-limit 90% 消費が 60-70% に下がる試算 (要 dogfood 確認)

---

## 全体統合: Bundle 群の累積効果見積

> **目的**: 別セッションで改善作業を進める際の **指示・優先順位の根拠** + **改善後の比較ベースライン** として使用する。本見積は本ドキュメント執筆時点 (PR #97 セッション、2026-04-30 〜 2026-05-01 JST) の観測値から導出。

### Bundle 編成

| Bundle | 内容 | effort | 即効性 |
|---|---|---|---|
| **Bundle Y2** | #A-1 + #C-1 (analyze facets を haiku 化、aggregate/fix/supervise は sonnet 維持) | XS (yaml 4 行) | 最即効 |
| **Bundle Z (再編)** | #B-α + #B-β + #B-γ (アーキテクチャ 3 層: 決定論 comment lint / 制約付き fix / 異常検知 reviewer)、Phase 1〜3 で順次 dogfood | M+M+S (3 Phase 分割) | 段階的 (Phase 1 から) |
| **Bundle Z2** | #D-1 + #D-4 (gh CLI 使用規則 + Claude 応答スタイル簡素化 rules) | S (rules 追記) | 即効 |

### 期待効果 (Bundle 別)

| Bundle | 削減対象 | 想定削減量 | 検証指標 |
|---|---|---|---|
| Y2 | analyze step の sonnet 利用 | session あたり 15-20 分 + sonnet → haiku で **当該 step の token cost 1/3** | post-pr-review / post-merge-feedback の avg time、当該 facets の billable input tokens |
| Z (再編) | pre-push-review iter 数を構造的に固定化 | **outlier 0% 達成 + 1-iter ALL APPROVE 90% 超** (旧推定: avg iter 2.5 → 1.5 だったが、決定論層導入で 1-iter 固定が target に格上げ) | pre-push-review iter 分布 (`{1×N}` 集中度)、6-iter outlier 発生率 (target 0%)、1-iter ALL APPROVE 率 |
| Z2 | gh CLI noise + text-only response | cache_creation **3-4M tokens 削減** (現在 13.6M の 25-30%) | gh tool_result avg/max chars、text-only turn の cache_creation 占有率 |

### 統合効果試算

ベースライン (PR #97 セッション):
- 一意 cache_creation: **13.64M tokens**
- takt パイプライン総時間: **114.7 分** (セッション 63%)
- rate-limit 90% を 3 時間で消費

3 Bundle 全実装後の試算:
- 一意 cache_creation: **9-10M tokens** (Y2 + Z2 合計で 25-35% 削減)
- takt パイプライン総時間: **80-95 分** (Z + Y2 で 25-30% 短縮)
- rate-limit 消費: 90% / 3h → **60-70% / 3h** 試算

**注**: 上記は **各効果が独立加法的** との仮定。実際は中間効果が打ち消される可能性あり。dogfood 1-2 セッションで実測必須。

### 検証方法 (別セッションで Bundle 実装後に実施)

実装後セッションを 1 つ完走させた後、以下を本ドキュメントの「観測データ」セクションと比較:

#### ① セッション全体メトリクス比較

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

#### ② takt パイプライン時間比較

各 takt run の `meta.json` から iter / 時間を集計:

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

#### ③ Bash gh CLI tool_result 比較

```python
# Bash gh-pr 関連の tool_result chars を集計 (#D-1 効果検証)
# Top 5 大きい gh 出力サイズを ベースライン値と比較:
# - 47.98KB (POST /replies)
# - 44.08KB (gh pr view --json reviews,comments)
# - 24.10KB / 22.86KB / 19.62KB
```

#### ④ pre-push-review iter 分布比較 (#B 効果検証)

ベースライン: `{1×3, 3×2, 6×1}` = 6 runs (うち 6-iter outlier 1 件、avg iter 2.5)
目標 (Bundle Z 再編後): `{1×N}` (1-iter 固定) で 6-iter outlier 消失 (outlier 率 1/6 (16.7%) → 0%)、avg iter 2.5 → 1.0、1-iter ALL APPROVE 率 90% 超 (L302 / L633 と統一)

#### ⑤ サンプル CR review listing token 量比較 (#D-4 効果検証)

ベースライン: round 1-3 の review listing は各 2-5KB chars
目標: 各 1-2KB chars (file:line + severity + 1 行要約のみ)

### 別セッションでの作業指示 (テンプレート)

別セッションで Bundle Y2 / Z / Z2 のいずれかを実装する際、以下のフォーマットで本ドキュメントを参照する:

```markdown
## 作業概要
本セッションは [Bundle Y2 | Bundle Z | Bundle Z2] の実装を行う。
詳細は docs/pipeline-token-efficiency.md の該当セクションを参照。

## 実装タスク
- [ ] [該当 Bundle の改善案を順に列挙]
- [ ] テスト追加 (該当する場合)
- [ ] 実装 PR を作成
- [ ] merge 後に本セッションを 1 つ完走させる (検証用 dogfood)

## 検証方法
docs/pipeline-token-efficiency.md の「全体統合: Bundle 群の累積効果見積 → 検証方法」を実行。
本ドキュメントのベースライン値と比較し、想定削減量に届いているか測定。

## 完了基準
- 想定削減量の 70% 以上達成 → 本ドキュメントの「進捗管理」table で「採用済」マーク + 完了 PR 番号記録
- 想定削減量に届かず → 原因分析を「全体統合」セクション末尾に追記し、必要なら追加 Bundle 提案
```

---

## 進捗管理

| 改善案 | 状態 | 採用日 | 完了 PR |
|---|---|---|---|
| #A-1 analyze facets haiku 化 | 採用済 (Bundle Y2) | 2026-05-01 | #98 |
| #A-2 trivial PR skip | 計画 | - | - |
| #A-3 transcript filter | 計画 | - | - |
| ~~#B-1 fix self-check~~ | 取り下げ (Bundle Z 再編 → #B-β) | 2026-05-01 | — |
| ~~#B-2 reviewer exhaustive scan~~ | 取り下げ (Bundle Z 再編 → #B-γ) | 2026-05-01 | — |
| ~~#B-3 決定論的 lint hook~~ | 取り下げ (Bundle Z 再編 → #B-α) | 2026-05-01 | — |
| ~~#B-4 coding-style.md 強化~~ | 完全削除 (Bundle Z 再編、補助層としても不採用) | 2026-05-01 | — |
| #B-α 決定論 comment lint hook (Rust 限定 PoC) | 採用済 (Bundle Z Phase 1) | 2026-05-02 | #99 |
| #B-β 制約付き fix instruction | 計画 (Bundle Z Phase 2) | 2026-05-01 | - |
| #B-γ reviewer 役割変更 (異常検知化) | 計画 (Bundle Z Phase 3) | 2026-05-01 | - |
| #C-1 analyze haiku 化 | 採用済 (Bundle Y2) | 2026-05-01 | #98 |
| #C-2 Iter 3 短絡 | 検討 | - | - |
| #C-3 rate-limit skip | 計画 | - | - |
| #D-1 gh CLI 規則 | 計画 | - | - |
| #D-2 pnpm cr:findings wrapper | 検討 | - | - |
| #D-3 Rust findings mode | 検討 | - | - |
| #D-4 応答スタイル簡素化 | 計画 | - | - |

---

## 関連

- 元セッション: `C:\Users\HIROKI\.claude\projects\e--work-claude-code-hook-test\6cbc5021-e5f4-420d-853b-e1b467d45ae4.jsonl`
- 関連 ADR:
  - [ADR-015](adr/adr-015-push-runner-takt-migration.md) (push runner takt 化)
  - [ADR-018](adr/adr-018-pr-monitor-takt-migration.md) (cli-pr-monitor takt 化)
  - [ADR-020](adr/adr-020-takt-facets-sharing.md) (facets 共通化)
  - [ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) (post-merge-feedback 決定論化)
- 関連 workflow: [.takt/workflows/](../.takt/workflows/)
