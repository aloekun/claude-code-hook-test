# TODO (Part 27)

> **運用ルール** ([docs/todo.md](todo.md) と同一): 各タスクには **やろうとしたこと / 現在地 / 詰まっている箇所** を必ず書く。完了タスクは ADR か仕組みに反映後、このファイルから削除する。過去の経緯は git log で追跡可能。
>
> **本ファイルの位置付け**: `docs/todo14.md` (61KB) と `docs/todo22.md` (59KB) が 50KB の安定読み取り閾値を超えたため、2026-09-03 に**両ファイルの大きいエントリ 10 件をここへ移した** (順位 512)。**新規エントリの追加先ではない** — 新規は `docs/todo26.md` に記録する。本ファイルは移送したエントリの編集・完了削除専用。
>
> **移送の基準**: サイズ上位から選んだ。移動した各エントリは順位 table (`docs/todo-summary*.md`) の「ファイル」列も本ファイル名へ更新してあり、`cli-docs-lint` の entry-pairing 検査が 1:1 対応を強制する。件数がそのまま編集量になるため、**同じバイト数を動かすのに最も件数が少ない選び方**を採った。
>
> **移送元の内訳**: `todo14.md` から順位 345 / 354 / 357 / 359 / 364 / 433、`todo22.md` から順位 414 / 431 / 432 / 445。
>
> **推奨実行順序**: 全タスク横断のサマリーは [docs/todo-summary.md](todo-summary.md#recommended-order-summary) を参照。

---
### 順位 359: WP-16 系 post-merge feedback 文書系 10 件の docs バッチ

> **動機**: PR #342 (CI matrix / ADR-065)・#343 (監視 CI 観測修正)・#344 (pipeline_lock レース修正) の post-merge feedback で採用確定した文書系 10 件を、1 本の docs バッチ PR に集約する (2026-08-02 方針決定。per-PR の細切れ doc PR を避け milestone でまとめる運用)。全件が当時の convention 集への追記を想定していたが、同集は 2026-09-13 に廃止した (順位 445)。**行き先は着手時に決める**。GitHub 仕様の gotcha など Severity High 2 件を含む。
>
> **内容 (10 件)**:
>
> 1. `[workspace] default-members` 不在で `cargo test` = `cargo test --workspace` という暗黙不変条件の明記 + `push-runner-config.toml` への inline comment (#342 T3-1。順位 360 のテストと対)
> 2. ADR-051 (cross-system config coupling) をチェックリストに登録 — 3 系統以上のインフラ設定を跨ぐ変更時の確認項目 (#342 T3-3)
> 3. ambient/auto-detect 環境状態 (git ブランチ名・`GH_REPO`・cwd) に依存せず明示引数を使う設計原則 — PR #238/#247/#343 の 3 例目で systemic (#343 T3-1)
> 4. 並行バグ調査の標準テストパターン — 決定論再現テスト + stress の aggregate 計測 + low-core CI でのみ再現するレースの扱い (#344 T2-1)
> 5. TOCTOU + 2 層防御 (verify-before-destroy + deferred cleanup、最終 fallback は `create_new` 排他へ収束) の設計原則 (#344 T3-1)
> 6. CI 失敗の introduced-by-this-change / pre-existing を diff で切り分けるチェックリスト (#344 T3-2)
> 7. `paths:` フィルタ付き check を required 化すると skip が pending 扱いで PR が永久ブロックされる GitHub gotcha + early-success 代替 (#342 T3-2、Severity High)
> 8. exe-spawn テストは exe + deploy 済 config を temp dir へ staging する規約 — 既存 bounded wait 規約と対 (#342 T3-4、Severity High)
> 9. cross-platform matrix の `fail-fast: false` 既定 (#342 T3-5)
> 10. shell 抽象化確認済み cfg ガードの除去可否ガイダンス + レビュー層がコード内コメント根拠で false positive 判定する際の前提再検証手順 (#342 T3-7)
>
> **参照**: `.claude/feedback-reports/342.md` / `343.md` / `344.md` (各 Rationale)、[ADR-065](adr/adr-065-ci-matrix-cross-os-regression.md)、[ADR-051](adr/adr-051-cross-system-config-coupling.md)、[ADR-063](adr/adr-063-linux-portability-release-binaries.md)。
>
> **種類**: Tier 3 — docs のバッチ更新。工数 XS-S。

#### 作業計画

- [ ] **10 件のうち #7 (`paths:` フィルタ付き required check が skip=pending で PR を永久ブロックする GitHub gotcha、Severity High) のみ ADR 化**し、残り 9 件は落とす (2026-09-08 に出口を再設計。dev-conventions.md へは書かない)
- [ ] `push-runner-config.toml` へ #1 対応の inline comment を追加
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- #7 (`paths:` フィルタ付き required check の GitHub gotcha) が ADR に記録され、残り 9 件を落とした判断が本エントリの履歴から追えること。

---

### 順位 364: ADR-054 scope guard の pre-push 展開 — fix diff の allowlist 照合 (ADR-068 残課題)

> **動機**: pre-push の takt fix step には「finding 由来 allowlist との fix diff 照合」の決定論層が無く、instruction (fix.md の scope allowlist) 頼み。2026-08-02 の WP-17 PR 2a incident (fix が finding 対象外の lib crate 2 つを丸ごと削除し gate 全 PASS で push) で顕在化した。[ADR-068](adr/adr-068-fix-step-authority-boundary.md) の後退検知 backstop は削除系 (ファイル脱落 / 追加行削減) のみ検知する 80/20 の暫定で、**追加系の injection (finding 対象外ファイルへの書き込み・config 書き換え) は検知できない**。PR #348 security review の non-blocking 注記 (fix step が push-runner-config.toml の `max_added_line_shrink_pct` / `enabled` を書き換えて backstop 自体を自己弱体化できる経路が instruction 頼み) もこれで閉じる。
>
> **対処案**: `cli-push-runner` の post_takt_regate 段 (または直前の専用 stage) で、`.takt/runs/` の最新 findings レポートから `Location` 列を抽出して allowlist を導出し、takt 前後の diff 差分の変更ファイルを照合する。判定コアは `lib-scope-guard` (WP-17 再分割 PR で land 予定) を再利用し、cli-pr-monitor の post-pr 経路と判定の同一性を保つ (ADR-054 の drift 防止)。violation は ADR-068 の `[FIX_REGRESSION]` と同様の loud block + 独立 kill-switch。findings レポートのパース失敗は fail-closed。
>
> **参照**: [ADR-068](adr/adr-068-fix-step-authority-boundary.md) § 決定 3 / 残課題、[ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) § 欠点 (pre-push 展開の予告元)、`src/cli-pr-monitor/src/stages/scope_guard.rs` (post-pr 側の先行実装)、PR #348 security review 注記。依存: WP-17 再分割 PR (lib-scope-guard の land) 後が効率的。
>
> **種類**: Tier 1 — push 前の検査 (scope guard) の展開。工数 M。

#### 作業計画

- [ ] findings レポート (.takt/runs/ 最新 run) から Location 列を抽出する parser (fail-closed)
- [ ] lib-scope-guard で allowlist 照合、violation は loud block + 独立 kill-switch
- [ ] 設定系パス (`push-runner-config.toml`、`.claude/`、`.takt/`、`.github/`) を finding allowlist より優先して常時 deny にする。`push-runner-config.toml` の quality gate `cmd` は再ゲートでシェル経由で実行されるため、finding が指していても fix に書き換えさせない (週次レビュー WR-2026-11-10-C01 で追記、`.claude/weekly-reviews/2026-11-10.md`)。`ALWAYS_ALLOWED` の `.takt/review-diff.txt` は deny より優先する。着手時判断: 設定系パスを直す正当な finding は block して人間に回す形でよいかを決める
- [ ] incident 再現テスト: (a) finding 対象外ファイルへの変更が**変更種別 3 種 (追加 = 新規ファイル作成 / 書き換え = 既存ファイル編集、config 自己弱体化含む / 削除) のいずれでも** block されること、(b) `ALWAYS_ALLOWED` 対象ファイル (`.takt/review-diff.txt` 等) への変更は finding allowlist 外でも block されないこと、の両方を固定する (完了基準の「追加・書き換え・削除いずれも」に対応)
- [ ] fix.md / fix-supervisor.md の「pre-push は後退検知のみ」記述を更新
- [ ] 本エントリ削除 + todo-summary2.md 行削除

#### 完了基準

- fix step が finding 対象外ファイルを変更 (追加・書き換え・削除いずれも) した push が、決定論的に block されること。ADR-068 の後退検知では通ってしまう「追加系 injection」ケースがテストで固定されていること。
- `ALWAYS_ALLOWED` (post-pr 側の先行実装 `src/cli-pr-monitor/src/stages/scope_guard.rs` で定義済み、現状 `.takt/review-diff.txt` のみ) は、fix step が本 instruction (fix.md 「Pre-completion diff refresh」) に従って正当に書き換える中間ファイルの例外リストである。pre-push 側の実装も finding allowlist に加えてこのリストを常に許可し、post-pr 側と同一のリストを共有すること (ADR-054 の drift 防止)。この例外により (a) の block 判定が誤って `.takt/review-diff.txt` 自体の正当な refresh まで block しないことをテストで固定する。
- finding が設定系パスを指していても、fix による変更が block されることをテストで固定する。

### 順位 414: 系統 A-1: 「各出力面は新しい perimeter」原則と screening 関数の出口別分離を明文化する

> **動機**: PR [#389](https://github.com/aloekun/claude-code-hook-test/pull/389) で PR タイトルという 3 つ目の公開面が増えた際、既存の `screen_for_public_output` を流用できないことが判明した。あちらの無害化は**「workflow がコードスパンで囲む」ことが前提**で `@mention` と markdown を verbatim に残す設計だったが、**タイトルはコードスパンにできない**。
>
> **systemic pattern である**: 3 ソース (PR diff / セッション / pre-push security review) が独立に同じ原則を指摘した。過去にも同型が起きている — [ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 14 の初版は「公開面 = PR 本文」と狭く見て **step ログを見落とし**、`tee` 経由の露出を後から塞いだ。**公開面は塞ぐたびに次が見つかる**。
>
> **対処案**: (a) 「新しい出力面を足すときは、既存 screening を流用してよいかを**囲いの有無**から判断する」を convention として明文化、(b) [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md) へ **output surface × wrapping context の対応表**を追記する (どの出口がどんな囲いを持ち、それゆえ何を追加処理すべきか)。
>
> **参照**: [ADR-054](adr/adr-054-prompt-injection-trust-boundary-defense.md)、[ADR-072](adr/adr-072-nightly-todo-loop.md) 決定 14 § 3 つ目の公開面、[screening.rs](../src/lib-ledger/src/screening.rs) (2 関数の対照が実装済み)。
>
> **種類**: Tier 3 — 原則の明文化。工数 S。

#### 作業計画

- [ ] 「出力面ごとの screening」の判断手順 (囲いの有無から必要処理を導く) も **ADR-054 へ同時に書く** (2026-09-08 に出口を再設計。dev-conventions.md へは書かない)
- [ ] ADR-054 へ output surface × wrapping context の対応表を追記する
- [ ] 既存の出力面 (PR 本文 / step ログ / PR タイトル / marker 本文) を棚卸しし、表の初期値を埋める

#### 完了基準

- 新しい出力面を足す人が、既存 screening を流用してよいかを**表を見るだけで判断できる**こと。

### 順位 432: `check_concurrent_run_guard` の `.takt/runs` 全走査コストと保持ポリシー

> **動機**: PR [#388](https://github.com/aloekun/claude-code-hook-test/pull/388) の pre-push review (simplicity、non-blocking) の指摘。`check_concurrent_run_guard` は呼ばれるたびに `.takt/runs/*` を**全ディレクトリ走査し各 `meta.json` を JSON としてパース**する。旧実装は `context.json` の mtime を 1 回読む O(1) だった。
>
> **実測 (2026-08-11)**:
>
> | 項目 | 値 |
> |---|---|
> | run ディレクトリ数 | **538** |
> | 内訳 | pre-push-review 268 / post-pr-review 147 / その他 |
> | 最古の run | 2026-06-26 (46 日分が蓄積) |
> | `.takt/runs` の総容量 | **174 MB** |
> | クリーンアップ機構 | **無し** |
>
> **現時点で実害は無い** (538 ファイルのパースは 1 秒未満、マージは 1 日数回)。問題は**増加が単調で削除する仕組みが無い**こと。
>
> **対処案** (独立に進められる 2 方向):
>
> 1. **走査を絞る** — dir 名に workflow 名が含まれる規約 ([ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § task labeling convention) を使い、`meta.json` を読む前に名前でフィルタする。数行で定数が 1/4 以下になる。**局所改善で低リスク**
> 2. **保持ポリシーを作る** — 週次レビュー等で古い run を畳む。174MB の削減にもなるが、run log は障害調査の資料でもあるため**保持期間の判断が要る** (どこまで遡って調査するかの実績を先に見るべき)
>
> **参照**: [markers.rs](../src/cli-merge-pipeline/src/feedback/markers.rs) (`check_concurrent_run_guard`)、[run_registry.rs](../src/cli-merge-pipeline/src/feedback/run_registry.rs) (`collect_feedback_runs`)、[ADR-030](adr/adr-030-deterministic-post-merge-feedback.md) § task labeling convention、[ADR-031](adr/adr-031-weekly-review-pipeline.md) (週次の棚卸し先)。
>
> **種類**: Tier 2 — exe の走査の絞り込みと保持ポリシーの実装。工数 S (案 1) - M (案 2)。

#### 作業計画

- [ ] 案 1 (名前フィルタ) を先に入れる。実測で効果を確認する (パース回数の削減)
- [ ] 案 2 の保持ポリシーは、run log を実際に何日前まで遡ったかの実績を確認してから期間を決める
- [ ] `.takt/runs` の容量を定期的に見る仕組みが要るかを判断する (週次レビューへ載せるか)

#### 完了基準

- 次のいずれかが達成され、どちらを採ったかが根拠つきで記録されていること。
  - **案 1**: `meta.json` の**パース件数**が post-merge-feedback の run 数まで減っていること (実測で確認)。ディレクトリの列挙自体は残るため **O(n) は解消しない** — 削減できるのは読み取り / パース回数である
  - **案 2**: 保持ポリシーにより `.takt/runs` の run 数に上限が定まっていること

---

