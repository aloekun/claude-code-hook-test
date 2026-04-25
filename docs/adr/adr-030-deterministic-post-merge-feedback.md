# ADR-030: 決定論的 Post-Merge Feedback — takt 経由の同期実行 + 失敗マーカーによる recovery

## ステータス

試験運用 (2026-04-25)

- **Supersedes ADR-014**: full — `/post-merge-feedback` skill 自体を廃止し、takt workflow に置き換える
- **Supersedes ADR-029**: partial — 層 3-4 (Claude session / skill 起動) を廃止。層 1 の `[[merge_pipeline.post_steps]]` `type = "ai"` スロットは流用するが、出力先を pending file から takt workflow 起動 + report file に変更する

## コンテキスト

### 問題: ADR-029 で実証された silent loss

ADR-029 は `cli-merge-pipeline` → pending file → Stop hook → Claude が next turn で `additionalContext` を読む → skill 起動、という 4 層トリガー機構を採用した。PR #74 マージ後の dogfood で、この機構の **後半 2 層が非決定論的** であることが実証された。

| 層 | 機構 | 決定論性 | PR #74 で何が起きたか |
|---|------|---------|---------------------|
| 1 | `cli-merge-pipeline` が pending file を書き込む | ✅ 決定論的 | 正常動作 (`status=pending`) |
| 2 | Stop hook が pending を読み `additionalContext` を出力 | ✅ 決定論的 | 正常動作 (`status=dispatched`) |
| 3 | Claude が次ターンで `additionalContext` を読む | ❌ **非決定論的** | ユーザー入力先行で次ターン消失 |
| 4 | Claude が skill 起動命令と解釈・実行 | ❌ **非決定論的** | 層 3 で詰まったため未到達 |

層 3 はセッションライフサイクル依存で、ユーザー入力 / VSCode 終了 / Claude Code 再起動で容易に壊れる。層 4 は skill design philosophy が ask-based — `AskUserQuestion` で中止可能、明示命令も無視可能。**must-run 要件に skill を主動線で使うのは設計ミス** と判定する。

PR #74 マージ後は pending file が `dispatched` で stuck した状態で session が終了 → 24h 後に stale 削除 → **フィードバック silent loss** という最悪の経路が再現された。

### 設計上の知見: skill 機構は本質的に "ask-based"

ADR-014 の選択肢 3 (skill による明示呼び出し) は「セッション知見へアクセスできる」点で優れていたが、`/post-merge-feedback` を **必ず** 走らせるための強制力がない。ADR-029 はこのギャップを Stop hook + state file で埋めようとしたが、最終層がやはり Claude の判断 (skill 命令を解釈して実行する) に依存しており、決定論性を担保できなかった。

skill の哲学 (`AskUserQuestion` で中断可能、ユーザー優先) と must-run 要件は **構造的に両立しない**。決定論的な実行を要求するなら、Claude のターン取得や skill 実行に頼らない経路が必要。

### 既存の決定論パターン: takt 経由の同期実行

[ADR-015](adr-015-push-runner-takt-migration.md) (push-runner) と [ADR-018](adr-018-pr-monitor-takt-migration.md) (cli-pr-monitor) で確立済みのパターン:

> 機械的ステップは Rust exe で、AI ステップは takt workflow で同期実行する

このパターンは Claude Code session のライフサイクルに依存しないため、決定論的に AI 処理を走らせることができる。Stage 2 の AI レビュー / fix loop / supervise が `takt-test-vc ADR-0003` の知見通り 97-99% のクリーンパス削減を達成しており、本プロジェクトの先行 2 例で実証済み。

ADR-030 は **同じパターンを post-merge-feedback に適用する 3 例目** として位置付ける。

## 検討した選択肢

### 選択肢 A: ADR-029 を維持 (現状)

pending file + Stop hook + skill。silent loss が再発するため **却下**。

### 選択肢 B: ADR-029 を維持 + skill 強制起動 (Anthropic API 直接呼び出し)

`cli-merge-pipeline` が `claude -p "/post-merge-feedback"` を spawn する案。ADR-014 の選択肢 1 と同じ欠点 (新規 session ゆえセッション知見が失われる) が再発する。本プロジェクトの先行 ADR-015 / 018 が確立した「AI 処理は takt 経由」原則とも乖離する。**却下**。

### 選択肢 C: takt workflow + 失敗マーカー + UserPromptSubmit recovery (採用)

`cli-merge-pipeline` が takt workflow を **同期実行** する。失敗時は `<pr>.md.failed` marker を残し、`UserPromptSubmit` hook が後続 prompt 入力時に検出して再実行を促す。

- L1 (takt 経由実行) は決定論的: takt は Rust 実装で session lifecycle 非依存
- session 知見は **transcript 抽出** で取り戻す: `~/.claude/projects/<id>/*.jsonl` を commit 時刻で range filter (Phase 0 で実証済み)
- L2 (UserPromptSubmit hook) は best-effort だが、L1 が既に成功している場合の話なので silent loss にはつながらない
- **採用**

### 選択肢 D: 旧 skill enrichment 層 (L3) を残す

旧計画の L3 として「Claude がレポートを読んで対話的に enrichment」する skill 層を追加する案。ask-based の弱点を再導入してしまう (skill 哲学と must-run 要件の構造的不整合) ため **却下**。L1 の `aggregate-feedback` facet 内で必要な対話的判断は完結させる。

## 決定

**選択肢 C を採用する。**

### アーキテクチャ: 2 層構成

| 層 | 機構 | 保証レベル | 失敗時 |
|---|------|-----------|--------|
| **L1 Floor** (決定論) | `cli-merge-pipeline` → takt workflow `post-merge-feedback` を **同期実行** | Deterministic invocation: 成功 → at-most-once でレポート生成、失敗 → `.failed` marker で retryable | soft: merge 成功扱い、`.claude/feedback-reports/<pr>.md.failed` marker 残存 |
| **L2 Recovery** (safety net) | `hooks-user-prompt-feedback-recovery` が `*.md.failed` を検出 → `additionalContext` で再実行指示 | At-least-once (ユーザーが何か入力すれば必ず発火) | hook 自体は決定論的、Claude の応答は best-effort (ただし floor は既存) |

### 全体フロー

```text
pnpm merge-pr (cli-merge-pipeline, ADR-013)
  ├─ ... (マージ本体 + ローカル同期)
  ├─ post_steps: type="ai" 分岐
  │    ├─ takt workflow `post-merge-feedback` を同期 spawn
  │    ├─ 成功: .claude/feedback-reports/<pr>.md 生成
  │    └─ 失敗: .claude/feedback-reports/<pr>.md.failed marker (soft fail)
  └─ exit 0
       │
       ▼ (任意のタイミングで Claude session が走るとき)
UserPromptSubmit hook (hooks-user-prompt-feedback-recovery, 新規)
  ├─ .claude/feedback-reports/*.md.failed を glob 検索
  ├─ 不在: silent exit
  └─ 存在: additionalContext で「未完了 feedback あり、再実行: pnpm feedback-retry <pr>」
```

### takt workflow 構成 (4 facets)

[ADR-020](adr-020-takt-facets-sharing.md) の facets 共通化原則に倣う。本 workflow は以下 4 facet を順次 chain する:

| facet | 役割 | 共有/専用 |
|---|---|---|
| `analyze-pr` | PR diff + reviews を分析。`E:\work\claude-code-skills\analyze-pr\SKILL.md` から port | 専用 (新規) |
| `analyze-session` | transcript range filter で抽出した user/assistant 履歴から実装時の学び・トラブル修正・ユーザー指示を抽出 | 専用 (新規) |
| `analyze-prepush-reports` | `.takt/runs/<latest>/reports/*.md` (pre-push-review の simplicity / security レポート) を集約 | 専用 (新規) |
| `aggregate-feedback` | 上記 3 facets の出力を [Plankton 優先度](adr-014-post-merge-feedback.md#plankton-優先度テーブル) で統合 → ADR 提案 / 仕組み改善案を生成。旧 `/post-merge-feedback` skill の Phase 4 ロジックから port | 専用 (新規) |

skill ベースで運用していた analyze-pr / post-merge-feedback Phase 4 のロジックは facet 化することで、takt の loop / supervise / fix 機構の上に乗せられる。fix loop 自体は本 workflow では不要 (修正対象がコードではなくレポート生成) のため、シンプルな chain 構造になる。

### 入力源

| 入力源 | 取得方法 | 用途 |
|---|---|---|
| PR data | `gh pr view <pr> --json ...` + `gh api .../pulls/<pr>/comments` + `.../pulls/<pr>/reviews` | `analyze-pr` |
| transcript | `~/.claude/projects/<project-id>/*.jsonl` を commit 時刻で range filter | `analyze-session` |
| pre-push reports | `.takt/runs/<latest>/reports/*.md` | `analyze-prepush-reports` |

### 出力

- 成功: `.claude/feedback-reports/<pr>.md` (Markdown レポート、ADR 提案 / 仕組み改善案を含む)
- 失敗: `.claude/feedback-reports/<pr>.md.failed` marker (内容は失敗理由 + 復旧手順)

両方とも repository には含めない (`.gitignore` で除外、内部 artifact)。

### transcript 抽出戦略 (Phase 0 調査結果反映)

```text
入力: <pr_number>
1. gh pr view <pr> --json commits,mergedAt → first_commit_time, end_time 取得
2. ~/.claude/projects/<project-id>/*.jsonl の全ファイルを mtime ∈ [first_commit_time, end_time + 1day buffer] で粗フィルタ
3. 該当 file 内で entry.timestamp ∈ [first_commit_time, end_time] かつ type ∈ {user, assistant} を抽出
4. 合成 in-memory log を analyze-session facet に渡す
```

#### transcript の制約 (Phase 0 で確認済)

| 観察 | 影響 |
|---|---|
| `timestamp` は ISO 8601 ms 精度 | commit 時刻からの逆引き filter が容易 |
| `thinking` content は encrypted (`signature` のみ可視、`thinking` field は空) | chain-of-thought は抽出不可。user/assistant text + tool calls/outputs で十分 |
| `gitBranch` は `HEAD` 固定 (jj detached state のため) | branch 名 filter は **使えない**。**時刻 range で filter する必要がある** |
| 1.7 MB / 621 行 (現セッション例) | takt context window 圧迫の可能性。filter 後の絞り込みが必須 |
| `type: queue-operation` はノイズ | parsing で skip すべき |

具体的なファイル所在: `~/.claude/projects/<project-id>/<session-id>.jsonl` (1 session = 1 file、UUID 命名)。本プロジェクトでは `%USERPROFILE%\.claude\projects\e--work-claude-code-hook-test\` 配下。

### 失敗ポリシー: soft

`takt` 失敗時の挙動:

- merge は **成功扱い** で進める (PR は既にマージ済みなので巻き戻せない)
- `.claude/feedback-reports/<pr>.md.failed` marker を残す
- L2 recovery (UserPromptSubmit hook) が後続 prompt で発火 → ユーザーが `pnpm feedback-retry <pr>` で再実行

**採用根拠**: hard fail (merge を失敗扱いにする) は既にマージ済みの PR を取り消せないため不可能。retry 機構を Floor の外側に持つことで、Floor 自体は exactly-once を保証しつつ failure 時の人手介入経路を確保する。

### レイテンシ

`pnpm merge-pr` の所要時間が takt workflow 実行分 (数分) 増加する。ユーザー判断 (作業計画策定時に合意) として **数分の追加レイテンシは許容**。`pnpm merge-pr` は同期実行で待つ前提とする ([ADR-016](adr-016-long-running-command-strategy.md) の長時間コマンド戦略に該当)。

### task labeling convention (Phase B dogfood で確立)

PR #77 の dogfood で、Rust 側の `find_latest_run_dir` が takt の run dir を見つけられず `.failed` marker が誤って書かれる事象が発生した。原因は task label と run dir の命名規則の不整合。

#### takt の run dir 命名

takt は run dir を `<timestamp>-<sanitized-task-label>` 形式で生成する (workflow 名ではなく **task label** を suffix に使う)。task label の sanitization は概ね「lowercase + 空白/特殊文字 → `-`」だが内部仕様で、Rust 側で再現するのは脆い。

#### 採用する規約

**task label は workflow 名を必ず prefix として含む `"<workflow-name> [<context>]"` 形式とする。**

| workflow | task label の例 | 結果の dir suffix |
|---|---|---|
| `pre-push-review` | `"pre-push-review"` | `<ts>-pre-push-review` |
| `post-pr-review` | `"post-pr-review"` | `<ts>-post-pr-review` |
| `post-merge-feedback` | `"post-merge-feedback for #77"` | `<ts>-post-merge-feedback-for-77` |

すべての run dir 名に `-<workflow>` という連続部分文字列が必ず現れる。Rust 側のマッチングは `name.contains(&format!("-{}", workflow))` の 1 行で完結し、context suffix の有無に関わらず一律にマッチする。

#### 制約

workflow 名同士が部分文字列関係になってはいけない。「部分文字列関係」とは `-<workflow-A>` が `-<workflow-B>...` の中に含まれること、すなわち `name.contains(&format!("-{}", workflow))` で取り違えが起きる関係を指す (実装は [`feedback.rs`](../../src/cli-merge-pipeline/src/feedback.rs) の `find_latest_run_dir`)。

例:

- **NG**: `merge` ⇄ `post-merge-feedback` — workflow=`merge` の needle `-merge` は `<ts>-post-merge-feedback-...` の中央に出現するため誤マッチ
- **NG**: `post-merge` ⇄ `post-merge-feedback` — 同様に `-post-merge` が dir 末端に出現
- **OK**: `build` ⇄ `post-merge-feedback` — `-build` が他の dir 名のどこにも現れない

現存 3 workflow (`pre-push-review` / `post-pr-review` / `post-merge-feedback`) は問題なし。新 workflow 追加時はこの制約を確認する。

#### 採用根拠

- **invariant に応じた選択**: 「最新 run dir = 自分のもの」という同期実行 invariant に依存する代替案 (Option C) よりも、命名規約による直接対応のほうが並行 takt 実行・将来の非同期化に対して頑健
- **既存 (`pre-push-review`) との後方互換**: pre-push-review の現行 task はすでに workflow 名と一致するため、規約を後付けで導入しても何も変える必要がない
- **post-pr-review の latent bug を予防**: 旧 task `"analyze PR review comments"` は workflow 名と無関係で、「post-pr-review の最新 run を Rust から探す」コードを書けば即破綻する。本 ADR で揃える

### Supersede 範囲

#### ADR-014 (full supersede)

`/post-merge-feedback` skill 自体を廃止する。理由:

- skill 機構は ask-based で must-run 要件と構造的に不整合 (本 ADR コンテキスト参照)
- skill が担っていた Phase 1-5 (PR 特定 → analyze-pr 呼び出し → セッション振り返り → 統合 → ユーザー承認) は、takt workflow の 4 facets + L2 recovery の組み合わせで再実装される
- セッション知見へのアクセスは「skill がメイン会話内で動く」ことではなく「transcript 抽出」で達成する。Phase 0 で実現可能性を確認済

ADR-014 のステータスを `Superseded by ADR-030` に更新する (Phase E で実施)。

#### ADR-029 (partial supersede)

- **廃止**: 層 3 (Claude が次ターンで additionalContext を読む) と層 4 (skill 起動)
- **流用**: 層 1 の `[[merge_pipeline.post_steps]]` `type = "ai"` スロット — ADR-013 で予約された拡張ポイントを引き続き使う
- **置換**: pending file 機構 (`hooks-stop-feedback-dispatch` / `lib-pending-file` / `.claude/post-merge-feedback-pending.json`) は廃止し、takt workflow 起動 + report file ベースに置き換える

ADR-029 のステータスを `Superseded by ADR-030` に更新する (Phase E で実施)。

### ADR-022 (責務分離原則) との整合性

L1 takt 経由の決定論実行は ADR-022 の以下の原則に整合:

- **原則 1**: 全副作用は許可側に収まる
  - `.claude/feedback-reports/<pr>.md` の新規書き込み → **新規 artifact への自己記述**
  - `.claude/feedback-reports/<pr>.md.failed` marker → 同上
  - `additionalContext` 出力 → 現セッション内 Claude への指示 (草案生成に類する)
  - commit description / bookmark 名 / PR title/body への介入は一切なし
- **対称性の回復**: ADR-029 設計では「Claude session が必要」という非対称が残っていたが、L1 takt 経由により **Claude 不在でも動く** 対称性が回復する。これは ADR-022 の自動化原則 (人間介入が optional) と整合

### ADR-028 (外部可視成果物ゲート) との関係

本 ADR は **内部 artifact のみ生成**:

- `.claude/feedback-reports/<pr>.md` — local 専用、`.gitignore` で除外
- `.claude/feedback-reports/<pr>.md.failed` — 同上

GitHub 上に観測可能な成果物 (PR / tag / commit description) は一切生成・改変しないため、ADR-028 の `permissions.ask` ゲートの **対象外**。

`pnpm merge-pr` 自体は ADR-028 の対象 (PR マージは外部可視) だが、これは既存ゲートで管理済み。本 ADR で追加するのは merge **後** の post_steps のみ。

## 実装タスク

詳細な実装手順は [`docs/todo.md`](../todo.md) の「マージ後フィードバック機構の決定論化」セクション Phase B-F を参照。本 ADR は仕様のみを規定する。

- **Phase A**: 本 ADR 起案 (PR 1) — 設計のみ
- **Phase B**: takt workflow + 4 facets — L1 Floor (PR 2)
- **Phase C**: UserPromptSubmit hook — L2 Recovery (PR 3)
- **Phase D**: 廃止 (skill enrichment 不要、本 ADR 「検討した選択肢 D」参照)
- **Phase E**: 旧機構廃止 (PR 4 — Phase B/C dogfood 数回後)
- **Phase F**: dogfood 検証 (PR 4 マージ後 / 継続観察)

## 影響

### Positive

- **silent loss 0**: L1 が takt 経由の決定論実行になるため、セッションライフサイクル非依存で feedback report が生成される
- **session 知見の維持**: transcript 抽出により skill 経由と同等の情報源にアクセス可能
- **既存パターンの再利用**: ADR-015 / 018 で確立した「機械的 = Rust、AI = takt」原則の 3 例目として、保守者の認知負荷を増やさない
- **責務分離の明確化**: ADR-022 の原則 1 (新規 artifact への自己記述) の枠内で完結し、Claude 不在でも動く対称性を回復

### Negative

- **新規 takt workflow + 4 facets の追加保守コスト**: pre-push-review / post-pr-review に続く 3 つ目の workflow となる
- **`pnpm merge-pr` の所要時間が増える**: 数分の追加レイテンシ (ユーザー合意済)
- **派生プロジェクトへのバックポート工数**: takt-test-vc など派生 repo に展開する際は workflow + facets + UserPromptSubmit hook の 3 セットを移植する必要がある (Phase F dogfood 完了後の検討事項)

### 将来の展望

- **Phase F dogfood 安定後の本採用化**: ステータスを `承認済み` に更新
- **派生プロジェクトへのバックポート**: takt-test-vc / techbook-ledger 等に同機構を展開
- **取りこぼし時の user-side recovery**: 現状は L2 で `pnpm feedback-retry` を促すが、Plankton 化 (CLAUDE.md / hook で自動再実行) も検討可能 (YAGNI で Phase F 後)

## References

- [ADR-013: Merge Pipeline](adr-013-merge-pipeline.md) — `[[merge_pipeline.post_steps]]` `type = "ai"` スロットの提供元
- [ADR-014: Post-Merge Feedback](adr-014-post-merge-feedback.md) — 本 ADR で **full supersede**。Plankton 優先度テーブルは継承
- [ADR-015: Push Pipeline takt 移行](adr-015-push-runner-takt-migration.md) — 「機械的 = Rust、AI = takt」原則の先行事例 (1 例目)
- [ADR-016: 長時間コマンド実行戦略](adr-016-long-running-command-strategy.md) — `pnpm merge-pr` の所要時間延伸の取り扱い根拠
- [ADR-018: cli-pr-monitor takt 移行](adr-018-pr-monitor-takt-migration.md) — 同原則の 2 例目 (本 ADR は 3 例目)
- [ADR-020: takt facets 共通化戦略](adr-020-takt-facets-sharing.md) — 4 facets 分離方針の根拠
- [ADR-022: 自動化コンポーネントの責務分離原則](adr-022-automation-responsibility-separation.md) — L1 takt 経由は本原則に整合
- [ADR-026: Cargo workspace](adr-026-cargo-workspace.md) — 新 crate `hooks-user-prompt-feedback-recovery` 追加手順
- [ADR-028: pnpm create-pr ゲート](adr-028-pnpm-create-pr-gate.md) — 外部可視成果物ゲートとの軸別境界 (本 ADR の射程外)
- [ADR-029: Post-Merge Feedback の自動起動](adr-029-post-merge-feedback-auto-trigger.md) — 本 ADR で **partial supersede** (層 1 流用、層 3-4 廃止)
