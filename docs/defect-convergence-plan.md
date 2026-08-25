# 不具合収束計画 — 後追い発覚ループの根治

> **状態**: Phase 0 が進行中 (PR T = 実装済み・未マージ / V・W・U = 未着手)、Phase 1 以降は未着手。**PR 総数は新規 7 本 (機構 4 + ルール撤廃 3)**。既存の第 2 バッチ 11 本 ([bugfix-batch-plan.md](bugfix-batch-plan.md)) と交錯して進める (→ [§ 全体順序](#全体順序))。
>
> **本ファイルは ephemeral な作業計画書**であり、**本ファイルと参照先の repo 内ドキュメントだけで作業に着手できる**ことを編集方針とする (実装セッションは本計画の策定会話を参照できない)。退役条件は [§ 退役手順](#退役手順)。
>
> **目的**: bugfix-batch-plan.md の作業中に新しい不具合が見つかり収束しないループの根治。到達目標は在庫削減ではなく「**実装時に不具合を混入させ、後追いで発覚する**」状態の停止 (2026-08-25 ユーザー決定)。improvement 由来の起票増加は問題にしない。

## 進行表

| # | PR | Phase | 状態 |
|---|---|---|---|
| 機1 | `feat(push-runner): testability gate — I/O 癒着判定の混入を止める` | 1 | 未着手 |
| 機2 | `feat(push-runner): open-questions gate — 未解決の問いが push を止める` | 2 | 未着手 |
| 機3 | `fix(nightly-todo): 掃除ループの判定を exe へ移す` | 3 | 未着手 |
| 機4 | `feat(ledger): 起票由来タグと defect 流入の週次計測` | 4 | 未着手 |
| 撤1 | `feat(lint): workflow/facet/convention のルール 3 件を lint へ移す` | 5 | 未着手 |
| 撤2 | `feat(pre-tool-validate): cargo fmt / Set-Content / jj squash の deny` | 5 | 未着手 |
| 撤3 | `fix(automation): 順位 445 実装 + create-pr --body 削除 + docs-only feedback 接続` | 5 | 未着手 |

## 根因 (実測)

第 2 バッチの判定層不具合 8 件の site 分類 (2026-08-25 実測。テスト数は当日時点の master `dd86b697`):

| 生成源 | 件数 | site (テスト数) |
|---|---|---|
| **G1: 判定が I/O と癒着し、テストの場が最初から無い** | 6 | [runner.rs](../src/cli-pr-monitor/src/runner.rs) `diff_at_is_empty()` (0) / [bookmark_check.rs](../src/cli-push-runner/src/stages/bookmark_check.rs) `run_bookmark_check()` (0) / [lib.rs](../src/lib-subprocess/src/lib.rs) `run_cmd_shell_with` (0) / `.github/workflows/nightly-todo.yml` の shell 判定 3 件 (0) |
| **G2: pure function だがテストが入力空間を覆わない** | 2 | 順位 476 `detect_last_mutating_jj_op` — テスト 11 件あって誤検知 4 回発火 / 順位 477 (site は bugfix-batch-plan.md § PR M) |

**ルール追加では直らないことの実証** (本計画がルールでなく機構を選ぶ根拠):

1. TDD の convention は [dev-conventions.md](dev-conventions.md) に存在しなかった (grep 0 件) — 「無視された」以前に書かれてもいなかった
2. [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 1「回帰テストの場が無い判定を無人経路に置かない」は存在したが、467 D-1 の掃除ループは**決定 1 を引用するコメントの直下で** shell 判定を増やした
3. memory `feedback_no_unenforced_rules`「強制力のないルール追加は即却下」が存在するのに、dev-conventions.md は **`##` 節が 15 個** (`###` 込みで 19 個。2026-08-25 に `grep -c "^## "` で実測) まで成長した — **「ルールを作らないルール」自身が強制されていない**

## 前提 (ユーザー決定)

2026-08-25 のヒアリングで確定。**再ヒアリング不要、この決定に従う**:

- 収束の定義 = 不具合の後追い発覚の停止。既存の未完了 261 件は**触らない**
- 強制点 = **push ゲート** (pnpm push)。書いた直後に止め、後追いにしない
- TDD の機構化は「**テスト可能な形の強制**」に絞る (テスト先行の順序は機械的に見分けられない)
- 設計の穴は「未解決の問い」を成果物にして push を止める形で届ける
- workflow shell の判定は exe へ移してテストの場を作る
- 第 2 バッチの優先枠 T〜W だけ先に片付け、機構を挟んでから通常枠 M〜S
- Phase 5 の PR 粒度は撤1/撤2/撤3 の 3 本で承認済み

## 共通の作業手順

- **bugfix-batch-plan.md の運用ルールを全 PR で踏襲する**: PR をスタックしない (前の PR マージ後に master 起点で作る) / 着手前に [ADR-075](adr/adr-075-verify-premises-before-acting.md) に従い本計画の記述を実測で確かめる / PR 作成前にタイトル・ボディの明示承認 ([ADR-028](adr/adr-028-pnpm-create-pr-gate.md)) / body は `--body-file` 経路
- 新機構 (機1 / 機2 / 機3) は [ADR-039](adr/adr-039-experimental-feature-standard-pattern.md) の標準パターンに準拠する。**各実装 PR は次の 4 点を必ず具体値で固定する** (本計画は ephemeral なので、値の恒久的な置き場は実装 PR と ADR 側である):
  - **config キーと既定値** — 機1 / 機2 は push-runner 側なので [push-runner-config.toml](../push-runner-config.toml)、機3 は workflow から exe を呼ぶ形なので exe の引数と `.claude/hooks-config.toml` のいずれかに置く。導入時の既定は機1 = warning、機2 / 機3 = 有効
  - **kill-switch** — 既存 gate と同じ `enabled = false` で恒久停止できること
  - **decision trigger** — 試験運用の判定時期と判定材料 (機1 は 4 週後の FP 率実測、機2 / 機3 は 3〜5 PR の dogfood)
  - **bounded lifetime の記録先** — 本採用 / 修正 / 却下の判定結果を書く ADR。却下時は機構を物理削除する (ADR-042 § Mechanism graveyard prevention)
- 本計画の PR は**いずれも auto lane (夜間ループ) に載せない**。ただし根拠は 2 種類あり、混同しないこと:
  - **[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 6 の Guard 禁止パス該当** — 載せると実装しても push が拒否され構造的に完了不能になる ([ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 2 クラス 3)。**機3** (`.github/workflows/`) と **機4** (`src/lib-ledger/`) の 2 本が該当する
  - **禁止パス非該当だが方針として載せない** — 機1 / 機2 (`src/cli-push-runner/` = push ゲート自身)、撤2 (`src/hooks-pre-tool-validate/` = hook の判定層)、撤1 / 撤3。無人経路が自分を縛る層を書き換える形になるため

## 全体順序

**Phase 0 (T→V→W→U、既存 4 本) → Phase 1 → 2 → 3 → 4 → 5 (撤1→撤2→撤3) → 通常枠 Q→N→M→P→O→R→S**

- Phase 5 の 3 本は他 Phase と独立。**PR Q の観測窓 (発火率 1.4%) を早く開けたい場合は通常枠と入れ替えてよい**
- Phase 4 は測定の起点なので後ろへずらさない (退出判定の 4 週窓が遅れる)

## Phase 0 — 優先枠 T〜W (既存 4 本)

**進捗 (2026-08-25)**: **T = 実装済み (未マージ)**、V / W / U = 未着手。T は緑/赤分類を新 crate `cli-nightly-outcome` へ移し、`Report outcome` step を exe 呼び出しへ縮退させた。

実施内容・完了基準・後始末は bugfix-batch-plan.md § 優先枠を正とする。ただし**以下の実装方針変更が同計画の記載に優先する**:

| PR | 変更点 |
|---|---|
| T (488) | 緑/赤分類を shell から **exe へ移す** (機3 の先行適用)。ADR-072 決定 10 改訂は元計画どおり |
| V (490) | `diff_at_is_empty()` を **pure function 化**し、比較材料 (`pre_takt_cid`、捕捉済み) を引数で受ける。正しい実装例は同 crate の `stages/scope_guard.rs`。抽出中に浮上した設計の問い (比較対象は何か等) は機2 の最初の入力として記録し、機2 のエントリ形式が実際の問いを表現できるかの検証材料にする |
| W (491) / U (489) | 元計画どおり |

**完了条件の追加**: T と V の pure 化作業で、機1 の検出条件の**境界**を実例で確定させる。**方針そのもの (I/O + 判定の同居) は Phase 1 で確定済みで、ここでは再検討しない** — 実例から決めるのは次の 3 点に限る:

1. どの呼び出しを I/O と数えるか (`Command::new` / `run_cmd_*` / `fs::` / `env::var` の外に何を含めるか)
2. 「出力への分岐」とエラー処理の線引き (エラー処理は同居に数えない)
3. thin I/O wrapper (取得だけして値を返す層) の除外基準

## Phase 1 — 機1: testability gate (新規 1 本)

**何を止めるか**: G1 の新規混入 — 「判定ロジックが I/O と同居していてテストを書く場が無い」形の Rust 関数が push を通ること。

**設置**: [src/cli-push-runner/src/stages/](../src/cli-push-runner/src/stages/) に新 stage `testability_gate.rs`。`lint_screen` / `pr_size_check` と同列の diff スコープ決定論検査 (対象は push 範囲 `<base>..@` で変更された `.rs` ファイルのみ。既存コードは見ない)。stage の配線は `stages/mod.rs` と `push-runner-config.toml` の既存 stage を踏襲。

**検出条件**: 「**本体に I/O 呼び出し (`Command::new` / `run_cmd_*` / `fs::` / `env::var`) と、その出力への分岐 (エラー処理以外) が同居する、判定型 (`bool` / `Option<T>` / `Result<bool, _>`) を返す関数**」。
「引数なし + I/O + 判定型」の 3 条件案は**採用しない** — ダミー引数 1 つで回避できる (2026-08-25 評価で棄却済み。再検討しない)。

**AST 手段の選定 (本 PR 内の設計判断)**: repo に `ast-grep` は未配線、`syn` 依存も無い (2026-08-25 実測)。[ADR-007](adr/adr-007-custom-linter-layer-boundary.md) は「正規表現層 / ast-grep 外部委譲」の 2 層しか想定していないため、**`syn` を Rust exe に組み込む第 3 の形を採る場合は ADR-007 の改訂を本 PR に同乗**させる。

**FP の扱い (段階導入)**:

- 導入時は **warning** (push は通し stderr + telemetry [ADR-055](adr/adr-055-firing-telemetry-collection.md) に発火記録)。試験運用 4 週で FP 率を実測し、**FP < 10% ([ADR-042](adr/adr-042-rule-vs-mechanism-boundary.md) Step 2 の経験則) で deny へ昇格**、超えるなら検出条件を絞って再測。昇格判定は monthly-review に載せる
- allowlist は**既存分の凍結専用**で、新規追加はできない (cargo test が allowlist の中身を既知集合と照合し、**差分が「削除のみ」であることを assert** する ratchet)。初期登録 6 件: `diff_at_is_empty` / `run_bookmark_check` / `working_copy_is_empty` / `head_has_description` / `pipeline_is_running` / `is_kill_switch_enabled` (Phase 0 の V と通常枠 O のマージで減る)
  - **この 6 件は根因表 (§ 根因) の G1 6 件とは別の母集団**である。偶然どちらも 6 件だが、重なるのは `diff_at_is_empty` / `run_bookmark_check` の 2 件だけ。根因表は**第 2 バッチで実際に不具合として発現した site**、こちらは**検出条件に当たる既存関数を 2026-08-25 に grep で洗い出した集合** (発現していないものを含む)。着手時は実測で採り直すこと

**完了基準 (最重要)**: [ADR-049](adr/adr-049-incident-eval-regression-suite.md) の incident→eval 方式で、**修正前の実不具合コードを入力にすると検出が発火する**回帰テストを固定する。fixture の採取元は **2026-08-25 時点の master `dd86b697`** の次の 2 関数 (行番号は当時点、関数名を正とする):

- `src/cli-pr-monitor/src/runner.rs` の `diff_at_is_empty` (191 行付近、順位 490)
- `src/cli-push-runner/src/stages/bookmark_check.rs` の `run_bookmark_check` (83 行付近、順位 484)

採取は `jj file show -r dd86b697 <path>` で行い、good fixture (発火しない側: pure 化後の形) と対で ADR-049 の `tests/fixtures/incidents/{bad,good}/` 配置規則に倣う。**2 件とも発火しなければ本 PR は完了ではない。**

**順位 481 (`run_cmd_shell_with`) は本 gate の射程外**である。同関数の戻り値は `(bool, String)` で上記の判定型に当たらず、欠陥も判定の誤りではなく I/O ヘルパ内の liveness 欠陥 (正常終了経路の reader thread join が上限なし) である。**タプル戻り値を検出条件へ足して射程に入れることはしない** — `(bool, String)` を返す I/O ヘルパは正当な形として多数あり、FP が跳ね上がるため。481 は通常枠 PR N の経過時間 assert 付き決定論テストで塞ぐ。

**G1 の 6 件を誰が直すか** — 機構と修正は別物なので混同しないこと。**機1 は既存 6 件を 1 つも直さない**。機1 がするのは「今後 同型が新しく入るのを止める」ことだけで、既存分は次の 6 本が個別に直す:

| G1 の site | 直す PR |
|---|---|
| `diff_at_is_empty` (490) | Phase 0 の **PR V** |
| `run_bookmark_check` (484) | 通常枠 **PR O** |
| `run_cmd_shell_with` (481) | 通常枠 **PR N** |
| nightly-todo の緑/赤分類 (488) | Phase 0 の **PR T** |
| nightly-todo の master 参照 (487) | 通常枠 **PR S** |
| nightly-todo の掃除ループ (D-1) | **機3** |

**機3 が直すのは掃除ループ (D-1) の 1 件だけ**である (488 は PR T、487 は PR S が担当)。機1 は 490 / 484 の修正前コードを**回帰 fixture として使う**が、修正そのものは PR V / PR O が行う。

**限界の明記 (doc コメントに書く)**: thin I/O wrapper (取得だけして値を返す層) との線引きは曖昧領域として残り、本 gate は完全ではなく ratchet である。だから Phase 4 で測定を続ける。

## Phase 2 — 機2: open-questions gate (新規 1 本)

**何を止めるか**: 実装中に見つけた設計の穴 (「この判定の比較対象は何か」等) が、ユーザーに確認されないまま仮定で実装されて push されること。

**成果物**: `docs/open-questions.md` (新規)。エントリ形式:

- `## Q-<連番>: <問い>` 見出し + `関連:` (ファイルパス) + `仮定:` (回答が得られるまで実装に置いた前提) の 3 要素
- 解消 = ユーザーの回答を得て、回答内容を然るべき場所 (ADR / 対象コードの doc comment / 本計画) に書き、**エントリを削除する**。エントリに回答を書き溜めない (ファイルは常に「未解決の問いだけ」を含む)

**ゲート**: push-runner 新 stage `open_questions_gate.rs`。`docs/open-questions.md` に見出しが 1 つ以上あれば deny し、問いの一覧を表示する。ファイル不在または見出し 0 = pass。判定は pure function (入力: ファイル内容文字列) + unit test。

**境界 (doc コメントに書く)**: 「問うべきなのに書かなかった」は検出不能。本機構の保証は「**書かれた問いは必ず push 前にユーザーへ届く**」まで。問いが浮上すること自体は機1 の pure 化作業に依存する (Phase 0 PR V で実地検証済みであること)。

## Phase 3 — 機3: shell 判定の exe 移送 (新規 1 本)

**何を止めるか**: `.github/workflows/nightly-todo.yml` の掃除ループ (`Clean up branches of settled PRs` step) に shell 判定として実装された 3 分岐が、テスト不能なまま残ること。

**背景 (実測 2026-08-25)**: 掃除ループは 2026-08-22 18:05 UTC の run 32589642740 で初めて実走し (`claude/nightly-228` を削除)、削除経路と lease 一致削除は観測済み。だが「既に消えたブランチで落とさない」skip 2 分岐と障害経路は **TOCTOU レース (実測窓 約 1.3 秒) でしか通らず自然発火を期待できない**。順位 467 D-1 の残観測はこの状態で止まっている。

**実装**: scan → ref 観測 → lease 付き削除 → 結果分類 (ref 不在→skip / ref 移動→中止 / ネットワーク・認証失敗→red) を新 crate `cli-branch-cleanup` へ移し、分類判定を pure function + unit test で固定する。workflow step は exe 呼び出しと結果表示のみに縮退させる。

- 既存の `cli-stale-branch-scan` (判定・列挙) への統合ではなく**新 crate を推奨** — scan = 提案、cleanup = 外部可視の実行、という [ADR-022](adr/adr-022-automation-responsibility-separation.md) の責務分離を保つ。統合するなら ADR-022 との整合を PR 内で説明すること
- App token / lease の意味論は現行 step のコメント (nightly-todo.yml 内) が正。移送で意味を変えない
- **通常枠 PR S (順位 487、master SHA pin) も実装方針を shell 修正から exe 移送へ変更する** (PR 自体は通常枠のまま)

**後始末 (機3 のマージで実施 — 本計画を追加した PR ではない)**: 機3 が exe + unit test で skip 2 分岐と障害経路を固定した時点で順位 467 の残観測は構造的に決着する。**その確認が済んでから** todo24.md の 467 節 + todo-summary2.md の 467 行を削除し、bugfix-batch-plan.md § 残観測トラッキングの 467 項も削除する。テストが未固定の状態で追跡記録を先に消してはならない。

## Phase 4 — 機4: 効果測定 (新規 1 本)

**何を測るか**: 機構導入後も defect 由来の起票が減っているか。減っていなければ機構は儀式であり、G2 対策 (対象外に置いた網羅性強制) の再提案が要る — その判断材料を決定論で取る。

**実装**:

- **由来マーカー**: [todo-summary2.md](todo-summary2.md) 系列の summary 行の内容セル先頭に `[defect:G1]` / `[defect:G2]` / `[improvement]` を付ける。**境界順位以降の新規行のみ必須** (既存行 261 件は触らない — ユーザー決定)。境界順位は本 PR 実装時点の最大採番 +1 を [lib-ledger](../src/lib-ledger/) の summary 検査層 (`summary_gate.rs`) の定数に固定し、境界以降でマーカー欠落なら fail-closed で拒否する
- **週次集計**: `cli-ledger-candidates` に defect 流入集計 (境界順位以降の `[defect:*]` 行を週別に数える) を追加し、weekly-review の決定論 scan から呼べる形にする
**タグの判定契約 (これが無いと退出基準を自分で無効化できる)**: 分類が主観だと、defect を `[improvement]` に付け替えるだけで「減った」を作れてしまう。次を決定論的な検査契約として `summary_gate.rs` に固定する。

- **判定元**: `[defect:*]` を名乗れるのは、**エントリ本文に実観測の証拠 (run ID / PR 番号 / 発火回数のいずれか) を含む行だけ**。証拠が無ければ `[improvement]` としてしか登録できない。これは bugfix-batch-plan.md の選定基準 (2)「実観測された不具合の修正」と同一の線引きである
- **G1 / G2 の別**: G1 = 不具合 site にテストを書く場が無かった (I/O 癒着 / shell 判定)。G2 = テストの場はあったが入力空間を覆っていなかった。**どちらとも言えない defect は G1 に倒す** (fail-closed 側 — 機構の効果を過大評価しない向き)
- **再分類の規則**: 一度付けたタグの変更は、**`[defect:*]` → `[improvement]` の向きだけ根拠を必須**とする (退出基準を緩める向きの変更だから)。変更行に `再分類根拠:` を書き、無ければ検査が落ちる。逆向き (`[improvement]` → `[defect:*]`) は根拠不要
- **fail-closed**: 境界順位以降でタグ欠落・未知タグ・`[defect:*]` なのに証拠なし・根拠なしの緩和方向再分類は、いずれも拒否する

**退出基準 (週次判定の定義)**: 曖昧だと同じ集計でも可否が変わるため、次で確定する。

- **週境界**: ISO 週 (月曜 00:00 UTC 始まり)。起票日は summary 行の日付ではなく**当該行を追加したコミットの author date** を使う (行の日付表記は後から編集されうるため)
- **判定**: 直近 4 週の defect 件数列が**非増加** (`w1 >= w2 >= w3 >= w4`、同値を許す) かつ **`w4 < w1`** であること。0 件が続く週 (`0,0,0,0`) は非増加を満たし `w4 < w1` を満たさないが、**全週 0 件は無条件で充足**とする (これ以上減らせないため)
- **有効性**: 集計対象の PR が 1 本も無い週は「観測なし」として**判定から除外し、その分だけ窓を過去へ伸ばす** (無活動を減少と誤認しないため)

**G2 が減らない場合**は property-based test / 入力空間分割の table test 必須化を再提案する (本計画で対象外としたことの再評価トリガー)。

## Phase 5 — ルール撤廃バッチ (新規 3 本)

ハーネスで強制できるのに文章のままのルールを機構へ置き換える (2026-08-25 の棚卸しで機械化可能なのにルールのみの運用は約 30 件)。撤廃の型: **(A) 検査を足す / (B) footgun 自体を除去 / (C) 既存機構に吸収**。

### 撤1 — lint 系 (型 A)

1. **順位 319 の `-e` convention を機構化**: [dev-conventions.md](dev-conventions.md) §「GitHub Actions の `run:` は常に `-e` 付きで起動する」の要求を [scripts/lint-workflows.mjs](../scripts/lint-workflows.mjs) の契約検査 3 として実装 (検査内容は同節の記述を正とする)。実装後、同節は「機械化: lint-workflows 契約検査 3」の宣言に縮小
2. **takt facet の言語指定検査**: `.takt/facets/instructions/*.md` の各ファイルに出力言語の指定行が存在することを検査 (dev-conventions §「takt facet の出力言語は各 instruction に直書きする」の機構化)。検査パターンは既存 instruction の実態から確定し、非準拠 facet には同 PR で指定行を足す
3. **ルール台帳ゲート (本命)**: [cli-docs-lint](../src/cli-docs-lint/) に「dev-conventions.md の各 `##` 節は `機械化: <機構名>` または `機械化不能: <ADR-042 Step1/2 の判定理由>` の宣言行を持つ」検査を追加。既存の `##` 節 (2026-08-25 時点で 15 個。着手時に実測し直すこと) への宣言付与が導入作業で、その過程が撤廃候補の棚卸しを兼ねる。**これにより「ルールを書いて溜飲を下げる」経路自体が塞がる** (新ルールを書くたび機械化判定が強制される)

### 撤2 — pre-tool-validate 系 (型 A)

[hooks-pre-tool-validate](../src/hooks-pre-tool-validate/) の preset 追加 + [.claude/hooks-config.toml](../.claude/hooks-config.toml) `[pre_tool_validate] blocked_patterns` への登録 (既存 preset `jj-message-required` 等の実装形式を踏襲):

4. **順位 411 の実装**: `cargo fmt` (workspace 全体整形) を deny し正しい対処を提示 — todo21.md の 411 節が仕様。あわせて**既存ファイルへの PowerShell `Set-Content`** を deny (LF→CRLF 化けで全行 diff 化する。根拠: memory `powershell-set-content-crlf`。Edit / sed へ誘導)
5. **`-u` 無し `jj squash` の deny**: source/dest 両方に description があると headless で editor hang する (根拠: memory `jj-squash-editor-hang-headless`)。`--use-destination-message` / `-m` 付きへ誘導

後始末: 対応する memory 2 件は「機構化済み」へ書き換え。順位 411 のエントリ後始末 (todo21.md 節 + summary 行削除)。

### 撤3 — 残り (型 B / A / C)

6. **順位 445 の実装** (型 A): todo preamble と facet routing 記述の整合 lint (todo22.md の 445 節が仕様)。実装後、dev-conventions §「同一事実が複数箇所に分散する場合の変更手順 (順位 445 実装までの暫定 convention)」の節を削除。順位 445 のエントリ後始末も同乗
7. **create-pr の `--body` を物理削除** (型 B): [create_pr.rs](../src/cli-pr-monitor/src/stages/create_pr.rs) の `--body` 受け付け (改行再結合 workaround を含む) を撤去し `--body-file` のみ残す。1 行目切り捨て事故 (memory `create-pr-multiline-body-truncation`) の footgun 除去。呼び出し側の案内 (prepare-pr skill 等) も追随
8. **docs-only PR の feedback skip を機構化** (型 C): [ADR-057](adr/adr-057-docs-only-deterministic-routing.md) の決定論 docs-only 判定を [cli-merge-pipeline](../src/cli-merge-pipeline/) の feedback 起動判定へ接続し、docs-only PR では post-merge feedback を起動しない。根拠: memory `no-feedback-adoption-for-doc-prs` (ユーザー決定済みの運用)。判定ロジックは push-runner 側 `docs_only_routing.rs` と重複させず lib 化を検討 ([ADR-044](adr/adr-044-subprocess-utility-extraction-boundary.md) の境界判定に従う)

**PR 不要の自動達成分**: [ADR-074](adr/adr-074-auto-lane-screening-criteria.md) 決定 6 → 通常枠 PR R (486) が機構化 / ADR-072 決定 1 → 機1+機3 が強制層になる。対応する ADR の格下げ追記 (規範から記述へ) は各該当 PR に同乗する。

**撤廃できないもの**: [ADR-075](adr/adr-075-verify-premises-before-acting.md) / [ADR-067](adr/adr-067-phase-b-unattended-fix-push.md)「実走でしか検証できない」/「複合タスク仕様に除外根拠を書く」等の認識論的ルール約 8 件は ADR-042 Step 1 (regex / AST / runtime check で表現不能) で機構化できない。これらは残す — 目的はルールゼロではなく、**判断が要るルールだけが残る**状態。

## 対象外 (明示)

- **G2 の網羅性強制** — Phase 4 の測定を見て再提案 (ユーザー決定「テスト可能な形の強制に絞る」)
- **既存の未完了 261 件の棚卸し** — 触らない (ユーザー決定)
- **既存 I/O 癒着 6 件の一括改修** — allowlist で凍結。うち 2 件は Phase 0 の V / 通常枠 O が個別に解消する

## 退役手順

1. 新規 7 本 + Phase 0 の 4 本がマージ済みで、bugfix-batch-plan.md の通常枠 7 本も完了していること
2. Phase 4 の退出基準 (defect 由来起票の 4 週連続単調減少) を充足していること
3. 機構の設計判断が ADR 化されていること (機1 = ADR-007 改訂 + 新規 ADR / 撤1-③ = ADR-042 への追記)
4. 次のコマンドで**本ファイル以外からの参照が残っていない**こと (bugfix-batch-plan.md からの相互参照を先に始末する):

   ```sh
   rg -n --glob '!.takt/**' --glob '!.claude/feedback-reports/**' \
      --glob '!docs/defect-convergence-plan.md' "defect-convergence-plan"
   ```

   - **ディレクトリを列挙する形にしない。** `docs/ src/ .github/ CLAUDE.md` のような列挙は、ルート直下の `README.md` など列挙漏れしたファイルの参照を見落とす。リポジトリ全体を対象にし、除外だけを指定する
   - **`git grep` は使えない。** 本リポジトリは jj 運用で、`git` コマンドは pre-tool-validate hook がブロックする
   - `rg` は `.gitignore` を尊重するため `target/` `node_modules/` は自動で外れる。明示除外が要るのは過去ログで数百件当たる `.takt/` と `.claude/feedback-reports/`、および**この手順の行自身**を含む本ファイルの 3 つ (2026-08-23 に bugfix-batch-plan.md で両方の必要性を実測済み)
5. 本ファイルを物理削除する
