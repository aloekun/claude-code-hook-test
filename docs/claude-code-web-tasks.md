# Claude Code Web 対応可能タスクリスト

> **状態**: 試験運用 (本ドキュメントは「Claude Code Web セッションで着手するタスクのピックアップ scope」を切り出した ephemeral artifact であり、列挙された全タスクが land したら役割を終える)
>
> **作成経緯**: [docs/todo-summary.md](todo-summary.md) のタスク数増加に伴い、Linux 環境の Claude Code Web でも着手できるタスク（= Windows ベースの hooks/パイプラインへの実行依存がないドキュメント修正系）を抽出するため、2026-05-16 に作成。
>
> **scope 境界**: リポジトリ内のファイル編集に閉じ、Rust ビルド/テスト/Windows hook 実行が成功条件にならないタスクのみを採用。

## 採用タスク

判定基準:

1. 編集対象がリポ内ファイル（`docs/` 配下 / `.claude/custom-lint-rules.toml` 内コメント / Rust ソースのコメント）
2. Rust ビルド / Windows hook / pnpm パイプラインの実行が成功条件に **ならない**
3. [docs/todo-summary.md](todo-summary.md) の表で採用判定済み（`feedback_no_unenforced_rules.md` 例外として既存実践の明文化に該当）

| 順位 | Tier | 内容 | 編集ファイル | 工数 |
|---|---|---|---|---|
| 116 | T3 | ADR-040 `step_timeout` 説明に sublinear / KV cache locality clarification を 2-3 行追記し、reference table 600s と formula 720s の数値整合化 (PR #145 T3-#1) | [docs/adr/adr-040-local-llm-context-size.md](adr/adr-040-local-llm-context-size.md) | XS |
| 120 | T3 | `takt-workflow-persona-without-model` rule コメント拡張（field 拡張手順 4-5 行）+ ADR-007 case study 追記（enumeration-based 正規表現層、Rust regex lookahead 非対応の pragmatic 対処）(PR #150 T1-#1、実体 Tier 3) | [.claude/custom-lint-rules.toml](../.claude/custom-lint-rules.toml) ルール⑨ + [docs/adr/adr-007-custom-linter-layer-boundary.md](adr/adr-007-custom-linter-layer-boundary.md) | XS |
| 127 | T3 | extensions 拡張時の test 追加 pattern を Rust ソース内コメントで明文化 (PR #151 T3-#2、順位 124 と同 PR 推奨) | Rust ソース内コメント（test location を正確に参照） | XS |
| 134 | T3 | ADR-035 に docs-only PR 評価の適用外基準リスト追加（mutation / error handling / DRY / YAGNI / function length / test coverage / magic-number 等）(PR #156 T3 #2) | [docs/adr/adr-035-doc-evaluation-policy.md](adr/adr-035-doc-evaluation-policy.md) | S |

### 着手フロー

1. Claude Code Web で本ファイルを起点に対象タスクを 1 つ選ぶ
2. 該当ファイルを Read で確認し、編集内容を [docs/todo-summary.md](todo-summary.md) と該当 `docs/todoN.md` の詳細エントリに照らして固める
3. 編集後、本ファイルの該当行と [docs/todo-summary.md](todo-summary.md) の該当順位行を削除する（todo-summary.md の table 更新方針に従う）
4. 詳細エントリが置かれた `docs/todoN.md` の該当 section も削除する
5. PR を作成（commit 単位は task 単位、複数 task を 1 PR に束ねる場合は理由を PR description に明記）

---

## 周辺情報

### 採用しなかったタスク群 (1): グローバル `~/.claude/*` 編集が必要なタスク

[docs/todo-summary.md](todo-summary.md) で採用判定済みかつ純 docs 修正だが、編集対象が **ユーザーグローバル設定**（`~/.claude/rules/common/*.md` や `~/.claude/CLAUDE.md`）であるため、本リストには含めない。

**理由**:

- ローカル PC と Claude Code Web の作業環境が異なり、`~/.claude/` ディレクトリは Web の per-repo workspace には含まれない
- グローバル `CLAUDE.md` / `~/.claude/rules/*` をバージョン管理する仕組みを本リポジトリでは用意していないため、Web 側で編集しても本リポの PR には反映できない
- ローカル PC 側で着手するのが構造的に妥当

該当する順位（参考、本リストでは取り扱わない）: 44, 66, 79, 84, 93, 100, 105, 107, 108, 110, 111, 117, 122, 128, 133

### 採用しなかったタスク群 (2): 実装系 / CI/script 系 / 判断作業混在系

以下は本リストの対象外。Windows ローカル環境または別途調整が必要。

- **Rust 実装系**: 順位 1, 2, 5, 8, 11, 16, 17, 19, 39, 42–46, 49, 51, 52, 57, 81, 83, 91, 92, 97, 121, 124, 125, 130, 131, 132 等
  - Windows 上での Rust ビルド / hook 実行 / Rust test 検証が成功条件
- **CI/script 実装系**: 順位 6, 10, 95, 96
  - `gh` CLI / GitHub Actions workflow 整備で Web からも実行可能だが、本リポ初の `.github/workflows/*` 追加など影響範囲があり、ローカル dogfood と組み合わせる方が安全
- **判断作業混在系**: 順位 118
  - rule⑧ の paths filter 検討は ADR amendment との整合判断を含み、純 docs 修正には閉じない

---

## ライフサイクル

- 採用タスクが全て land したら本ファイルを retire する（`~/.claude/rules/common/docs-governance.md` § Retirement Workflow に従う、global path のため markdown link なし）
- retire 時の手順:
  1. 採用タスク欄が空になっていることを確認
  2. permanent value の移管は不要（本ファイルは scope 整理のための作業表で、永続価値となる decision はない）
  3. リポ内で本ファイルを参照する箇所を `grep -rn "claude-code-web-tasks.md"` で洗い出し、参照を除去
  4. 本ファイルを物理削除
