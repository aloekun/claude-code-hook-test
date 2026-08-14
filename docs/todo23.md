# TODO (Part 23)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: docs/todo22.md がファイルサイズ 66528 B (2026-08-13 時点、50KB = 51200 B の安定読み取り閾値超過) に到達したため、新規エントリは本ファイルに記録する (2026-08-13 新設、週次レビュー WR-2026-08-13-M01 採用)。**新規エントリの追加先は本ファイル**。todo.md / todo3.md 〜 todo22.md の既存エントリは引き続き有効、相互に独立。
>
> **サイズ表記について**: todo22.md のサイズは記録時点で異なる (週次レビュー検出時 54203 B → 本ファイル新設時 66528 B → その後の追記でさらに増加)。各記載は**その時点の計測値**であり、現在値と一致しないことがある。現在値が必要なら計測すること。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---

## post-merge feedback 採用分 (#400、2026-08-14 採否確定)

> **由来**: PR [#400](https://github.com/aloekun/claude-code-hook-test/pull/400) (weekly-review 台帳昇格機構の転記規則・基準番号書式の明文化) の post-merge feedback ([ADR-030](adr/adr-030-deterministic-post-merge-feedback.md)) が挙げた提案を、ユーザーが採否確定した (2026-08-14)。
>
> | レポートの推奨 | 件数 | 本バッチでの扱い |
> |---|---|---|
> | ✅ 採用候補 | 6 | **全件採用** |
> | 🤔 様子見 | 3 | action なし (OR 条件検出 regex の誤検出率検証待ち / `skill-sync-check` との重複調査待ち / bookmark チェックリストは自動化の可否と合わせて再評価) |
> | ❌ 却下推奨 | 0 | — |
>
> **登録時に判明した前提の欠落 (重要)**: レポートの Tier2 #1 / #2 は「キーワード走査」「昇格 OR ロジック」への回帰テストを新規テストファイルに書く前提だが、**どちらも Rust 実装が存在しない**。2026-08-14 に `src/` 全体を検索して確認した — 4 語キーワード (「再選定」「着手時判断」「見積り」「検討」) を走査するコードも、昇格経路を評価するコードも無い。条件 1 の判定も昇格判定も instruction 層 (人間 + facet LLM) にあり、`cli-nightly-task-select` の台帳パーサは `無人可` マーク列を読むだけで `注意` 列を照合しない。したがって該当 2 件は**「何を検証対象にするか」を決める作業から始まる**。レポートの記述をそのまま着手すると、存在しない対象のテストを書こうとして詰まる。

### 台帳の `✅無人可` と判断留保キーワードの矛盾を決定論層で検出する (出典: PR #400 Tier1 #2)

> **動機**: CodeRabbit 指摘 (#400 の 2 件目) — 無人可判定の条件 1 は `注意` 欄の 4 語走査だけを見るため、同義表現 (「未定」「複数案あり」) はすり抜ける。#400 で正準タグ `「着手時判断: <原文>」` を規約化したが、**規約は instruction 層にあり機械強制が無い**。人間がタグを付け忘れれば従来どおりすり抜ける。
>
> **本タスクの位置づけ**: 台帳 (`docs/claude-code-web-tasks.md`) と facet instruction に入れた転記規約の決定論的な裏付け。「判断留保キーワード検査の回帰テスト」の前提タスクでもある (あちらの検証対象が本タスクの成果物になる)。
>
> **検出ロジック**: 「`無人可=✅` の行の `注意` セルに 4 語のいずれかが含まれる」を矛盾として検出する。正準タグ規約により同義表現もこの検査に落ちるため、**synonym の列挙に依存しない**のが利点。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 1 #2、[ADR-043](adr/adr-043-security-gates-fail-closed.md)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 1、[docs/claude-code-web-tasks.md](claude-code-web-tasks.md) § 自律実行可否の 2 段階分類
>
> **実行優先度**: 🚀 **Tier 1** — Severity High / Frequency Medium / Effort S / Adoption Risk None。夜間ループが人間の意図と違うタスクを実装する経路を塞ぐ。

#### 設計決定 (案)

実装先の候補が 2 つあり、着手時に選ぶ (両方入れる選択もある):

- **(a) custom lint rule** (`.claude/custom-lint-rules.toml`、`paths=["docs/claude-code-web-tasks.md"]`) — 台帳を編集した人へ即時フィードバック。既存 12 rule の確立パターンに乗る
- **(b) 台帳パーサの fail-closed 検査** (`src/cli-nightly-task-select/src/ledger.rs`) — 夜間ループがタスクを選ぶ瞬間に停止する。同 module の設計方針「曖昧さはすべて停止側へ」および [ADR-043](adr/adr-043-security-gates-fail-closed.md) と整合し、**マークが誤っていても自動実行に到達しない**

(a) は書き手への予防、(b) は自動実行の直前での遮断で、守る対象が違う。

#### 作業計画

- [ ] 実装先を (a) / (b) / 両方 から決める (夜間ループの fail-closed 経路に置くかが判断の軸)
- [ ] 検査を実装する
- [ ] good / bad fixture を追加する (実装先が lint rule なら `rule_test_coverage_check` / `incident_eval.rs` の 3 点セット)
- [ ] 台帳の現行 12 行が検査を通ることを確認する (dogfood)

#### 完了基準

- `無人可=✅` かつ `注意` に判断留保キーワードを含む行が、人手のレビューを介さず検出される

#### 詰まっている箇所

なし

### 判断留保キーワード検査の回帰テスト (出典: PR #400 Tier2 #1)

> **動機**: CodeRabbit 指摘 (#400 の 2 件目) の回帰テスト。canonical 4 語 / 正準タグ付き同義語 / タグなし同義語の 3 分類で、検出・非検出の境界を固定する。
>
> **本タスクの位置づけ**: 「台帳の `✅無人可` と判断留保キーワードの矛盾を決定論層で検出する」の**後続タスク**。検証対象はあちらの成果物であり、**単独では着手できない** (現時点で走査の実体が Rust に無いため、テストの置き場所も決まらない)。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 2 #1
>
> **実行優先度**: 🔧 **Tier 2** — Severity High / Frequency Medium / Effort S / Adoption Risk None (前提タスク依存)。

#### 設計決定 (案)

先行タスクの実装先に応じてテストの置き場所が決まる:

- 実装先が **custom lint rule** の場合 → `rule_tests_extras.rs` + `tests/fixtures/incidents/{bad,good}/` + `tests/incident_eval.rs` の 3 点セット (既存 rule の確立パターン)
- 実装先が **台帳パーサ** の場合 → `ledger.rs` の `mod tests` に追加

入力の 3 分類と期待結果:

| 分類 | 例 | 期待 |
|---|---|---|
| canonical | 「再選定する」「着手時判断」「見積り」「検討」 | 検出 |
| tagged synonym | 「着手時判断: 実装方式が未定」 | 検出 (タグが canonical 語を含むため) |
| untagged synonym | 「未定」「複数案あり」 | **非検出** |

3 行目は**仕様であって欠陥ではない**。タグ無し同義語を機械検出しようとすると synonym 列挙に戻り、[ADR-007](adr/adr-007-custom-linter-layer-boundary.md) の regex 層の限界に当たる。テストはこの境界を明示的に固定し、「ここから先は運用規律が担保する」と線を引くために書く。

#### 作業計画

- [ ] 先行タスクの実装先が決まるのを待つ
- [ ] 3 分類の fixture を作る
- [ ] 境界 (特に untagged synonym の非検出) を assert し、意図的な仕様であるとコメントで残す

#### 完了基準

- 3 分類すべてが自動テストで固定され、`cargo test` で回帰検出できる

#### 詰まっている箇所

前提タスク (決定論層の実装先確定) 待ち

### 昇格不適格判定の「両経路記載」を決定論化するかを判断する (出典: PR #400 Tier2 #2)

> **動機**: CodeRabbit 指摘 (#400 の 1 件目 / 3 件目) — 採用は docs-only / cargo-test の **2 経路 OR** なので、片経路の失敗だけでは不適格を証明できない。#400 で「両経路分の基準番号または非適用理由」を必須化したが、これも instruction 層の規約で機械強制が無い。§ 昇格検査履歴 は一度記帳すると基準変更まで再評価されないため、**片経路だけの理由で記帳された順位は恒久除外**になる。
>
> **本タスクの位置づけ**: レポートは新規テストファイルでの「OR/AND ロジック網羅テスト」を提案しているが、**昇格判定を行う Rust 実装は存在しない** (facet LLM + 人間の判断)。したがって「判定ロジックのテスト」は書けない。決定論化しうる対象は判定そのものではなく**記帳の形式** — § 昇格検査履歴 の各行の理由が両経路に言及しているかの検査である。これはレポート Tier1 #1 (様子見、誤検出率の検証待ち) と同じ対象であり、**両者は統合して扱う**。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 2 #2 および Tier 1 #1、[docs/claude-code-web-tasks.md](claude-code-web-tasks.md) § 昇格検査履歴
>
> **実行優先度**: 🔧 **Tier 2** — Severity High / Frequency Medium / Effort S-M / Adoption Risk None。

#### 設計決定 (案)

- 検査対象は § 昇格検査履歴 の table 行の「対象外の理由」セル。両経路 (`docs-only` / `cargo-test`) への言及が揃っているかを見る
- 誤検出率が読めないため、**実装前にサンプリング検証を行う** (レポート Tier1 #1 自身が前提として挙げている)
- **「決定論層を作らない」も正規の出口**。その場合は negative result を `docs/dev-conventions.md` へ永続化する (spike 見送りの永続化 convention に従う)。§ 昇格検査履歴 はまだ 1 行も記帳されていないため、初回記帳の実物を見てから判断する方が精度が高い

#### 作業計画

- [ ] 次回 `/weekly-review` の初回記帳を待ち、実際の理由セルの書かれ方をサンプルとして得る
- [ ] 両経路言及の検出を regex で書いた場合の誤検出率を、そのサンプルで見積もる
- [ ] 採否を決める (実装する / negative result として見送る)
- [ ] 実装する場合は fixture テストを同時に置く

#### 完了基準

- 実装するか見送るかが根拠つきで決まり、いずれの場合も記録が残る (実装 = 検査 + fixture、見送り = dev-conventions.md への negative result)

#### 詰まっている箇所

初回の記帳実績待ち (次回 `/weekly-review` で発生する)

### push-runner の bookmark 不在を早期検出し fallback のノイズを除去する (出典: PR #400 Tier2 #3)

> **動機**: #400 のセッションで実際に発生 — 新規ブランチの初 push で `pnpm push` が exit 7 (`push 可能な bookmark がありません`) で中断した。パイプラインは pre_checks 段階で止まるため実害は小さいが、ユーザーが受け取るのは「push が失敗した」という結果である。
>
> **実測した挙動 (2026-08-14)**: bookmark が無いと push-runner は fallback として**削除済み bookmark** (`claude/lib-ledger-extract (deleted)`) を `@` へ前進させようとし、jj のパースエラー (`Failed to parse bookmark name`) を出力してから「bookmark がありません」で中断する。最終的なエラーメッセージ自体は対処法 (`jj bookmark create <name> -r @`) を提示しており親切だが、**その手前に無関係なパースエラーが挟まる**ため、何が問題なのかが読み取りにくい。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 2 #3、[ADR-011](adr/adr-011-jj-push-new-bookmark-strategy.md)、`src/cli-push-runner`
>
> **実行優先度**: 🔧 **Tier 2** — Severity Medium / Frequency Low / Effort S / Adoption Risk None。開発体験の劣化であり機能欠陥ではない。

#### 設計決定 (案)

- **(a)** 削除済み bookmark (`(deleted)` サフィックス) を fallback 候補から除外する — パースエラーの直接原因を潰す
- **(b)** pre_checks で「非 trunk bookmark が 0 件」を先に判定し、fallback を試みずに対処法だけを出す

(a) は最小修正、(b) はメッセージの見通しが良くなる。両立可能。

#### 作業計画

- [ ] fallback 候補の絞り込みロジックを確認する
- [ ] 削除済み bookmark を除外する (or 早期判定に変える)
- [ ] bookmark 0 件 / 削除済みのみ / 正常 の 3 ケースで挙動を固定する回帰テストを追加する

#### 完了基準

- 新規ブランチの初 push で、無関係なパースエラーを挟まずに対処法だけが提示される

#### 詰まっている箇所

なし

### OR 条件の不成立を主張するときは全経路を明示する convention (出典: PR #400 Tier3 #1)

> **動機**: #400 の CodeRabbit 指摘は 3 箇所すべてが**同じ欠陥**を指していた — 複数経路の OR で「どれも駄目」と結論するのに、片方の経路しか検査していない。台帳側には書式規約を入れたが、それは台帳固有の記述であり、同型の判断は他の場所でも起きうる。
>
> **本タスクの位置づけ**: 「昇格不適格判定の『両経路記載』を決定論化するかを判断する」の自動検出が様子見の間、**人手ガイドとして機能する**。自動検出を見送った場合は本 convention が恒久的な担保になる。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 3 #1
>
> **実行優先度**: 💎 **Tier 3** — Severity Medium / Frequency Medium / Effort XS / Adoption Risk None。

#### 設計決定 (案)

`docs/dev-conventions.md` に良い例・悪い例つきで追加する:

- ✗ 不良: `cargo-test 基準 2 不適合` (もう一方の経路が未検査。他経路で適格なら誤って恒久除外する)
- ✓ 良好: `docs-only 基準 1 不適合 (Rust 実装で docs 編集に閉じない)・cargo-test 基準 2 不適合 (Windows hook 発火が成功条件)`

#### 作業計画

- [ ] `docs/dev-conventions.md` へ節を追加する (良い例・悪い例の対照つき)
- [ ] 適用範囲を書く — 複数経路の OR で不成立を主張する判断すべて (台帳の昇格判定に限らない)

#### 完了基準

- OR 条件の不成立を書く人が、全経路を明示すべきことと書式を convention から判断できる

#### 詰まっている箇所

なし

### 本リポ instruction とスキルリポ SKILL.md の同時反映チェックリスト (出典: PR #400 Tier3 #2)

> **動機**: #400 では 1 つの規約変更を **3 面** (本リポの台帳 + facet instruction / スキルリポの `SKILL.md` / デプロイ先 `~/.claude/skills/`) へ同時反映する必要があった。今回は手動確認で漏れなく済んだが、[ADR-051](adr/adr-051-cross-system-config-coupling.md) (クロスシステム設定 coupling) が扱う型そのもので、concrete checklist が無い。
>
> **同セッションで判明した別の腐り方**: スキルリポの working tree に、過去のデプロイ時にコミットし忘れた差分**約 110 行**が滞留していた。デプロイ先とはハッシュ一致していたため実害が出ず、「リポだけが遅れている」状態が検出されないまま継続していた。**同期の確認をデプロイ先との一致だけで行うと、この状態を見逃す**。チェックリストにはこの検出を含める。
>
> **参照**: `.claude/feedback-reports/400.md` Tier 3 #2、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、`/skill-sync-check` skill
>
> **実行優先度**: 💎 **Tier 3** — Severity Medium / Frequency Medium / Effort XS / Adoption Risk None。

#### 設計決定 (案)

`docs/dev-conventions.md` へ手順を追加する:

1. 3 面 (本リポ / スキルリポ / デプロイ先) の対象ファイルを列挙する
2. **着手前に**スキルリポの `git status` が clean であることを確認する (過去のコミット漏れの検出)
3. スキルリポへ commit → デプロイ先へコピー → ハッシュ一致を確認
4. 既存 `/skill-sync-check` skill との役割分担を明記する (あちらは同期状態の診断、本チェックリストは変更時の手順)

#### 作業計画

- [ ] `docs/dev-conventions.md` へチェックリストを追加する
- [ ] `/skill-sync-check` が「リポ未コミット差分」を検出できるか確認し、できるなら手順 2 をその呼び出しに置き換える

#### 完了基準

- 本リポとスキルリポに跨る規約変更で、3 面の反映漏れとスキルリポ側のコミット漏れの両方がチェックリストで防げる

#### 詰まっている箇所

なし
