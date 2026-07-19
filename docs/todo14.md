# TODO (Part 14)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo13.md がファイルサイズ約 171KB (50KB 安定読み取り閾値の約 3.4 倍) に到達したため、新規エントリは本ファイルに記録する (2026-07-19 週次レビュー WR-2026-07-19-T02 採用)。**新規エントリの追加先は本ファイル**。todo.md / todo2.md 〜 todo13.md の既存エントリは引き続き有効、相互に独立。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## 現在進行中

### VSCode 拡張が hook `systemMessage` を UI 描画するかの調査 (ADR-059 dogfood / 削除条件 2)

> **動機**: [ADR-059](adr/adr-059-hook-system-message-visibility.md) (systemMessage 可視化) の dogfood で、2026-07-19 に PR-N1〜N3 を land し reminder 起点で weekly review を実行したが、**VSCode 拡張環境では systemMessage の 1 行が UI に独立描画されたか確証が持てなかった** (観測できたのは additionalContext 経由のモデル言及のみ)。VSCode 拡張が hook の `systemMessage` をターミナル CLI と異なる扱いにしている可能性がある。ADR-059 の bounded-lifetime 判定 (期限 2026-08-16) と `docs/weekly-review-notification-plan.md` 削除条件 2 の前提であり、未確認のままでは段階展開の採否も計画書削除も判断できない。
>
> **対処案**: (1) **ターミナル CLI 版 Claude Code で新セッションを起動**し systemMessage が UI 描画されるか切り分ける (CLI で出るなら実装は正しく、VSCode 固有の表示差と特定できる)、(2) VSCode 拡張での描画有無・スタイルを確認、(3) 結果を ADR-059 § Dogfood 観測 (2026-07-19) に追記し 2026-08-16 判定 (第 2 弾展開 or 却下) の材料にする。描画されない場合も additionalContext 明示指示 (defense-in-depth) が backstop のため**実装は revert しない**。
>
> **参照**: [ADR-059 § Dogfood 観測 (2026-07-19)](adr/adr-059-hook-system-message-visibility.md)、`docs/weekly-review-notification-plan.md` (削除条件 2)、`src/hooks-session-start/src/main.rs` (`build_session_start_json` = systemMessage 出力元)。
>
> **実行優先度**: 🔧 Tier 2 — Severity Medium (ADR-059 bounded-lifetime 判定と計画書削除の blocker) / Frequency Low (一度切り分ければ済む) / Effort S (CLI で新セッション起動 + 目視)。期限 2026-08-16 に間に合うよう実施。

#### 作業計画

- [ ] ターミナル CLI 版 Claude Code で新セッションを起動し systemMessage の描画を確認 (last-run を stale にするか failed marker を置いて reminder を発火させる)
- [ ] VSCode 拡張での描画有無・スタイルを確認し CLI との差を切り分け
- [ ] 結果を ADR-059 § Dogfood 観測 (2026-07-19) に追記 + 削除条件 2 の可否を判定
- [ ] 本エントリ削除 + todo-summary.md 行削除

#### 完了基準

- VSCode 拡張 (と CLI) で hook `systemMessage` が描画されるかが切り分けられ、ADR-059 削除条件 2 の判定 (計画書 `docs/weekly-review-notification-plan.md` の削除可否) が下せること。
