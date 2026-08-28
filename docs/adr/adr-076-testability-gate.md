# ADR-076: testability gate — I/O 出力のインライン解釈を push で止める

## ステータス

試験運用 (2026-08-28 導入、warning モード)。**4 週間の発火観測後に本採用 / 条件変更 / 却下を本 ADR へ追記する** (判定は monthly-review、ADR-039 の bounded lifetime)。

## コンテキスト

[defect-convergence-plan.md](../defect-convergence-plan.md) § 根因 の実測では、第 2 バッチの判定層不具合 8 件のうち **6 件が G1** — 「判定ロジックが I/O と同居していて、テストを書く場が最初から無い」形だった。ルール追加では直らないことも同計画で実証済みで、強制点は push ゲートに置くと決めた (2026-08-25 ユーザー決定)。

典型例は 順位 490 (`cli-pr-monitor` の `diff_at_is_empty`) である。

```rust
fn diff_at_is_empty() -> bool {
    let (ok, out) = run_cmd_direct("jj", &[/* ... */], &[], 30);
    if !ok { /* エラー処理 */ return false; }
    out.trim() == "true"     // ← この解釈を単体テストする場所がどこにも無い
}
```

## 決定

**変更された `.rs` の中に「I/O 出力をその場で解釈して bool を返す関数」が新しく入ったら、push 時に報告する。** `syn` による AST 解析を `cli-push-runner` の stage として実装する。

### 検出条件

次の 3 つがそろったときに発火する。

1. 返り値が `bool` / `Option<bool>` / `Result<bool, _>`
2. 関数内に I/O 由来の値がある — I/O 原子 (`Command::new` / `fs::*` / `env::var` / `run_cmd*` 等) の直接呼び出し、または**同一ファイル内で I/O 原子を含む関数**の呼び出し (1 ホップ)
3. **返り値の式そのもの**が、その値からインラインで導かれている

汚染は「同一ファイル内の I/O を持たない関数への呼び出し」で止まる。**そこがテストの場だから**である。外部 crate の呼び出し (`serde_json::from_str` 等) は解釈の場を作らないので汚染を通す。

### 意図的に射程外にしたもの

| 形 | 理由 |
|---|---|
| 分岐して literal を返す (`if x.is_empty() { return false }`) | stage の entry point がすべてこの形で、発火させると FP が支配的になる |
| bool 以外の判定型 (独自 enum / タプル / `Option<Vec<T>>`) | 「読んで parse して返すだけ」の thin wrapper が大量に当たる |
| I/O の成否をそのまま返す (`ok` / `.success()` / `.is_ok()`) | 解釈すべき内容が無い。切り出す先も無い |
| 呼び出し側での解釈 / 別ファイルの I/O ヘルパ経由 (名前が `run_cmd*` 等以外) | 関数単位の局所解析の限界 |

**解釈を local へ束縛してから返す形 (`let empty = out.trim() == "true"; empty`) は射程内**である (CodeRabbit #456 の指摘で塞いだ)。`let` を 1 行挟むだけで通れては ratchet にならない。

**完全な検査ではなく ratchet である。** 効果は Phase 4 の測定で追う。

### 回避操作が望ましい refactor と一致する

1 行の純関数へ切り出せば gate は通る。それは抜け道ではなく、**作りたかったテストの場そのもの**である。ただし本 gate が保証するのは「テストが書ける形」までで、「テストがあること」は強制しない (テスト先行の順序は機械的に見分けられない、という同計画の前提どおり)。

### 既存分は BASELINE で凍結する

2026-08-28 の実測で **197 ファイル中 8 件**が該当した。これらは `stages/testability_gate/mod.rs` の `BASELINE` に凍結し、**表は増やせない** (`baseline_never_grows` が件数増加を拒否する)。既存分を直したら行を削る。**機1 は既存 8 件を 1 つも直さない** — 機構と修正は別物である。

| ファイル | 関数 |
|---|---|
| `src/cli-pr-monitor/src/fix_commit/abandon.rs` | `parent_commit_id_is` |
| `src/cli-pr-monitor/src/runner.rs` | `diff_is_empty` |
| `src/cli-push-runner/src/stages/push_jj_bookmark.rs` | `working_copy_is_empty` / `head_has_description` |
| `src/hooks-session-start/src/jj_helpers.rs` | `fetch_head_is_recent` |
| `src/hooks-stop-quality/src/takt_subsession.rs` | `meta_status_is_running` / `meta_is_fresh` |
| `src/lib-telemetry/src/lib.rs` | `telemetry_enabled` |

## ADR-039 の 3 点セット

- **Config opt-in**: `push-runner-config.toml` の `[testability_gate]`。template には置かない (派生プロジェクトは default OFF)。本リポジトリのみ `enabled = true` で dogfood する
- **Kill-switch**: `enabled = false` で恒久停止、env `TESTABILITY_GATE_OVERRIDE=1` で個別 push のバイパス。**`mode` の未知の値は config エラーで落とす** — `"denny"` のような typo を warning へ倒すと、deny を意図した設定が黙って無効になる
- **Bounded lifetime**: 導入時は `mode = "warning"` (push を止めず stderr + telemetry [ADR-055](adr-055-firing-telemetry-collection.md) に記録)。**4 週後に FP 率を実測し、10% 未満なら `mode = "deny"` へ昇格**、超えるなら検出条件を絞って再測する。2 回目も超えたら**機構を物理削除**する ([ADR-042](adr-042-rule-vs-mechanism-boundary.md) § Mechanism graveyard prevention)

**「検査できなかった」は緑に潰さない。** `jj diff --summary` の失敗 / 解釈できない status 行 / **個別ファイルの読み込み・parse 失敗**で走査が成立しなかった場合、warning 中は push を止めず telemetry へ `scan-incomplete` を残して頻度を測り、**deny 昇格後は止める** ([ADR-043](adr-043-security-gates-fail-closed.md))。warning 期間に止めると、測りたかった FP と環境要因の停止が混ざる。

**試験運用中に CI 経由で実質 deny にしない。** 実リポジトリ全体を走査する検査 (`repo_has_no_unlisted_firings`) は `#[ignore]` にしてあり、既定の `cargo test` では BASELINE の stale 検査だけを行う。warning と決めた期間に hard fail の経路を残すと、測定したかった FP がそのまま作業の停止になる。

## 検討した選択肢

### A. 正規表現層 (`.claude/custom-lint-rules.toml`)

「返り値が I/O 出力から導かれたか」は複数行のデータフロー判定であり、[ADR-007](adr-007-custom-linter-layer-boundary.md) の判断フローどおり正規表現層では表現できない。ゆるい近似 (I/O 呼び出し + 判定型) では 2026-08-25 実測で 53 件が当たり、その大半が正しく分離済みの関数だった。

### B. ast-grep への外部委譲

ADR-007 が想定していた形だが、repo に未配線であり、パターン言語では「返り値が I/O 由来か」というデータフロー的条件を書けない。Rust だけのために Node 側のツールチェーンを増やす形にもなる。

### C. `syn` を exe に組み込む (採用)

`syn` は既に `Cargo.lock` に居る (serde_derive 経由) ため、**新しい第三者 crate は増えず** `full` + `visit` の feature 追加で済む。判定は決定論的で、テストも Rust ソース文字列を入力に書ける。ADR-007 が想定していない第 3 の形なので、同 ADR に本形式を追記した。

## 帰結

- push 経路に自作の静的解析器を 1 つ抱える。維持コストは [ADR-062](adr-062-monthly-harness-roi-review.md) の月次 ROI 棚卸しの対象が 1 つ増えることを意味する
- 過去実績で本 gate が書いた時点で止められたのは G1 6 件のうち **1 件** (`diff_at_is_empty`) である。**着手前の見積りでは 2 件としていたが、実装して確定した射程では 1 件だった** — `run_bookmark_check` (順位 484) は PR #175 当時の形も「I/O → 純パーサ → 分岐して literal を返す」で、上表の射程外に当たる
- 残り 4 件は shell 判定 3 件 (Rust ではないため **機3 が exe へ移して初めて射程の候補**になる。移した後の形が本条件に当たるかは移してみるまで分からない) と、タプル戻り値のため射程外と決めた 1 件 (順位 481)
- **1/6 は小さい。** それでも入れるのは、本 gate が止めるのが「過去 6 件」ではなく**今後書かれる同型**であり、かつ回避操作が望ましい refactor と一致するためである。この見積りが 4 週間の測定で覆るなら (発火が 0 件、または FP 率 10% 超)、ADR-039 の bounded lifetime に従って**物理削除する**

## 関連

- [defect-convergence-plan.md](../defect-convergence-plan.md) § Phase 1 — 位置づけと完了基準
- [ADR-007](adr-007-custom-linter-layer-boundary.md) — 正規表現層 / AST 層の線引き (本 ADR で第 3 の形を追記)
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用の標準パターン
- [ADR-042](adr-042-rule-vs-mechanism-boundary.md) — ルール vs 仕組み化の境界
- [ADR-049](adr-049-incident-eval-regression-suite.md) — incident→eval 回帰スイート
