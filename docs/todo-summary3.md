# TODO 推奨実行順序サマリー (続き 2、順位 400 以降)

> **本ファイルの位置付け**: [docs/todo-summary2.md](todo-summary2.md) の推奨実行順序 table を docs 50KB 超過解消のため 2026-09-03 に再分割した後半 (順位 400 以降を収容)。**新規行の追加先は本ファイル**。part 1 は [docs/todo-summary.md](todo-summary.md) (順位 219 以前)、part 2 は [docs/todo-summary2.md](todo-summary2.md) (順位 220-399)。
>
> **分割位置の根拠**: 新規順位は常に最後の part に入るため part 3 だけが伸び、part 2 は完了削除で縮む。順位 400 で切ると part 2 = 43.5KB / part 3 = 32.4KB となり、伸びる側に約 17KB (約 45 順位分) の余裕が残る (2026-09-03 実測)。
>
> **順位 500 以降の新規行は、タスク列の冒頭に起票由来のマーカーを付ける** ([ADR-079](adr/adr-079-defect-origin-tagging.md))。`[defect:G1]` (テストを書く場が無かった) / `[defect:G2]` (テストの場はあったが入力空間を覆っていなかった) / `[improvement]` (不具合ではない改善) の 3 択で、迷う defect は G1 に倒す。`[defect:*]` は**詳細エントリに実観測の証拠** (`#123` / `run 32589642740` / `発火: 4 回` のいずれか) が要る — 無ければ `[improvement]` としてしか登録できない。`pnpm lint:docs` が fail-closed で検査する。**順位 499 以前の既存行は対象外**。

## 推奨実行順序サマリー (続き 2、順位 400 以降)

| 順位 | Tier | タスク | ファイル | 工数 | 依存 |
|---|---|---|---|---|---|
| 402 | 🚀 Tier 1 | **「対処後は効果を観測するまで完了と見なさない」を明文化 (系統 A-1)** | todo21.md | S | なし (2026-08-10 採用。決定 11 は投稿の成否だけ見て 10 時間気づけず、決定 15 は同じ症状が続くか確かめる前に解決済みと記録した。fail-open は効果の観測を別に用意して初めて成立する) |
| 403 | 🚀 Tier 1 | **AI レビューの数値・外部仕様の主張は仮説として扱い実測で二重検証 (系統 A-2)** | todo21.md | S | なし (2026-08-10 採用。組合せ数の指摘は観察は正しいが提示値も誤り (実測 384)、gh のオプション併用提案は実行時エラー、jq 正規表現案はパースエラー。観察と修正手段の確信度は別) |
| 404 | 🔧 Tier 2 | **外部依存の非同期応答待ちに timeout / retry を明記する convention (系統 A-3)** | todo21.md | S | なし (2026-08-10 採用。cli-stale-branch-scan の初版が timeout 無しで、同期実行経路の無診断ハング要因だった) |
| 405 | 🔧 Tier 2 | **新規 crate 実装時に既存同種コンポーネントとの重複を確認 (系統 B-1)** | todo21.md | S | なし (2026-08-10 採用。default_branch 解決ロジックの複製が CodeRabbit 指摘後に同一 PR 内で再発。34 crate 規模では記憶に頼れない) |
| 406 | 🔧 Tier 2 | **旧 API 廃止時に enum / config key / CLI flag の 3 形態すべての reject をテスト固定 (系統 B-2)** | todo21.md | S | なし (2026-08-10 採用。改名時に CLI フラグだけ test suite から漏れ、別名として通れば fail-open になる) |
| 407 | 🔧 Tier 2 | **旧語彙が live code に出現したら reject するカスタムリントルール (系統 B-3)** | todo21.md | S | なし (2026-08-10 採用。132 箇所の改名で CodeRabbit が同一 PR 内だけで 4 箇所の取りこぼしを指摘。ADR-007 の既存 regex 基盤で足り docs は extensions で自然に除外) |
| 408 | 🚀 Tier 1 | **safety-critical な config 比較に shell glob を禁止し exact-match を必須化 (系統 C-1)** | todo21.md | S | なし (2026-08-10 採用。kill-switch 判定の部分一致で fail-closed を謳う step 自身が fail-open だった。該当コメントが無かったため症状が出ず潜伏していた) |
| 409 | 🔧 Tier 2 | **shell の部分一致比較を検出するカスタムリントルール (系統 C-2)** | todo21.md | S-M | 順位 408 (規約側)。検出対象を安全装置の判定に絞れるかが採否の分かれ目。絞れなければ却下も正規の出口 (ADR-042 の mechanizable 判定) |
| 411 | 🚀 Tier 1 | **`cargo fmt` を PreToolUse でブロックし正しい対処を提示 (系統 F、規約ではなく機構)** | todo21.md | S | なし (2026-08-10 ユーザー判断で提案の形を変更。規約は毎セッション読まれコンテキストを圧迫するが hook は発火時のみコストが出る。ADR-042 へこの非対称を追記するのも本エントリの範囲。**反射的に実行されやすく無関係な差分を生む**ため WP-18 とは独立に早期着手する = 2026-08-10 ユーザー判断) |
| 413 | 🔧 Tier 2 | **`CwdRestore` Drop guard が 8 定義 / 6 ファイルに複製。ADR-025 の統合トリガーと再評価期限を超過** | todo21.md | S-M | なし (2026-08-10 PR #385 の pre-push review 指摘。ADR-025 自身が「2 例目で `lib-test-helpers` へ統合」と定め再評価期限 2026-07-31 も過ぎている。抽出するか ADR-025 の status を更新するかの判断が要る) |
| 414 | 🚀 Tier 1 | **「各出力面は新しい perimeter」原則と screening 関数の出口別分離を明文化 (系統 A-1)** | todo22.md | S | なし (2026-08-11 採用。#389 で PR タイトルが 3 つ目の公開面になり本文用 screening を流用できないと判明。3 ソースが独立に同一原則を指摘。ADR-054 へ output surface × wrapping context の対応表を追記) |
| 415 | 🔧 Tier 2 | **PR 検出源を広げる変更の信頼スコープ検査チェックリスト (系統 A-2)** | todo22.md | XS | なし (2026-08-11 採用。#385 の security review が「origin push 権限と同等の信頼度のソースまで検出を拡張する」点を指摘。検出源追加時の確認項目を明文化) |
| 416 | 🔧 Tier 2 | **新規 screening 関数は実 exe を 1 回動かしてから test / doc を書く (系統 A-3)** | todo22.md | XS | なし (2026-08-11 採用。#389 で既存関数の挙動を推測でテストし期待値が実挙動と食い違った。ADR-067 の「実走でしか検証できない」を純関数の挙動確認まで広げる) |
| 417 | 🚀 Tier 1 | **出力契約 3 層 (exe 出力キー ⊆ workflow allowlist ⊆ 検証 step) の同期を CI で検証 (系統 B-1)** | todo22.md | S | なし (2026-08-11 採用。#389 で片方だけ更新すると新出力が黙って捨てられる構造が判明。workflow のコメント自身が警告していた = 機構で守るべき対象。cross-file 検査は ADR-007 の regex 層外のため CI test 形式) |
| 418 | 🔧 Tier 2 | **出力契約 3 層追跡パターンを再利用可能な形で文書化 (系統 B-2)** | todo22.md | S | なし (2026-08-11 採用。同期責務が exe 実装者 / CI テンプレート / 検証 step 実装者に分散。検証 step は値でなく行の存在を見る点も含めて記録。順位 417 と同一 PR) |
| 419 | 🚀 Tier 1 | **takt run の解決規約 (PR 束縛 / status 判定) を全コンポーネント共通 convention 化 (系統 C-1)** | todo22.md | S | なし (2026-08-11 採用。同ロジックが merge-pipeline / orphan reaper / 将来の pr-monitor の 3 箇所以上で必要。ADR-024 と同じ DRY 昇格パターン。ADR-030 へ thread safety の保証範囲も補足) |
| 420 | 🚀 Tier 1 | **run binding が並行起動下で破れないことの integration test (系統 C-2)** | todo22.md | M | なし (2026-08-11 採用。WP-18 dogfood で run 解決のインシデント 2 件が実発生。既存テストは単一セッション想定。並行バグは計装して実測すること = memory verify-concurrency-by-observation) |
| 421 | 💎 Tier 3 | **marker の命名・状態遷移・recovery ポリシーを統一規約にする (系統 C-3)** | todo22.md | XS | なし (2026-08-11 採用。marker 生成が Rust と takt workflow に分散し post-pr-review / post-merge-feedback の 2 系統で形式が揺れている) |
| 422 | 🔧 Tier 2 | **cross-system parity テストの設計原則 (境界最小化の罠 / 方向性の非対称) を文書化 (系統 D-1)** | todo22.md | S | なし (2026-08-11 採用。workflow_awk_parity.rs の実装に 8 反復を要した根本原因。判定そのものを原本に実行させるところまでが要件だと明記) |
| 423 | 🔧 Tier 2 | **CI matrix で `#[cfg(unix)]` テストの実行を保証する (系統 D-2)** | todo22.md | S | なし (2026-08-11 採用。Windows では中身 0 件で走り「0 件実行」と「全件成功」が CI 出力上見分けにくい。ADR-064 の陽性証拠要求と同じ論理) |
| 424 | 🔧 Tier 2 | **UNC パス復元ロジックを Windows 実機で検証する (系統 E-1)** | todo22.md | M | なし (2026-08-11 採用。Linux では検証不可。本プロジェクトは Windows 固有 gotcha を反復して踏んでいる。実機再現が困難なら見送り判断を根拠つきで記録する) |
| 425 | 🔧 Tier 2 | **jj の `git.auto-local-bookmark` 既定値への依存を CI で固定する (系統 E-2)** | todo22.md | M | なし (2026-08-11 採用。順位 397 の対処はこの既定値に依存しており、変わると前提が崩れるが気づく仕組みが無い) |
| 426 | 🔧 Tier 2 | **`lib-jj-helpers` 分割の call site 回帰統合テスト (系統 E-3)** | todo22.md | M | なし (2026-08-11 採用。#385 の module 分割は re-export ファサードで API 互換を維持したが、実 call site の import 挙動を固定するテストが無い) |
| 427 | 🔧 Tier 2 | **`BookmarkSearch::RemoteOnly` への変異操作を検出する (系統 F-1)** | todo22.md | M | なし (2026-08-11 採用。ADR-013 の設計契約では読み取り専用だが呼び出し側の誤用は防げない。lint より型 (newtype) で不可能にする案と比較すること) |
| 428 | 🔧 Tier 2 | **PR 番号を取る CLI の不正値 (`--pr 0` 等) を PreToolUse で検出する (系統 F-2)** | todo22.md | S | なし (2026-08-11 採用。#385 で exe 側は契約を得たが同型 CLI 追加時の漏れは防げない。exe の拒否で足りるなら不採用も正規の出口) |
| 429 | 💎 Tier 3 | **「optional 列」の意味 (ヘッダに無くてよい ≠ 行に無くてよい) を明記 (系統 G-1)** | todo22.md | XS | なし (2026-08-11 採用。#389 で max_index() への反映漏れから index out of bounds panic が発生。語の解釈の齟齬が原因) |
| 430 | 💎 Tier 3 | **CLI フラグ解析の `Mode` enum + validator パターンを convention 化 (系統 G-2)** | todo22.md | XS | なし (2026-08-11 採用。フラグの段階的拡張による分岐の複製は再発パターン。lint 化は false positive リスクで却下済み、doc 化で代替) |
| 431 | 🔧 Tier 2 | **`review-request` の成功判定を初回レビュー取得まで遅らせる** | todo22.md | S-M | なし (2026-08-11 実走で判明。レート制限による拒否も success として記録され、未レビューの自律 PR が信号として残らない。ADR-019 § M5 の方針によりリトライは作らず検出と可視化に留める) |
| 432 | 💎 Tier 3 | **`check_concurrent_run_guard` の `.takt/runs` 全走査コストと保持ポリシー** | todo22.md | S-M | なし (2026-08-11 実測: run 538 件 / 174MB / 最古 46 日前 / クリーンアップ機構なし。現時点で実害は無いが単調増加。案 1 = 名前フィルタで走査を絞る、案 2 = 保持ポリシー) |
| 433 | 🔧 Tier 2 | **cli-telemetry-report コード堅牢化 + 回帰テスト (月次 ROI レビュー PR #336/#337 post-merge feedback 採用)** | todo14.md | S | なし (2026-08-12 採番 — 詳細エントリのみ登録され台帳行が無い孤児状態で約 3 週間滞留していたものを回復。検出機構の欠落は別起票の docs-lint 1:1 検査を参照) |
| 434 | 💎 Tier 3 | **telemetry 時間語義・不変条件・degraded 運用の文書補強 (ADR-062 / CLAUDE.md)** | todo14.md | XS | なし (2026-08-12 採番 — 孤児エントリの回復) |
| 435 | 🔧 Tier 2 | **jj workspace/bookmark semantics の文書化 + pr-monitor 回帰テスト** | todo14.md | S-M | なし (2026-08-12 採番 — 孤児エントリの回復) |
| 436 | 💎 Tier 3 | **開発ワークフロー規約の補強 (polling 禁止 / CodeRabbit→ADR timing)** | todo14.md | XS-S | なし (2026-08-12 採番 — 孤児エントリの回復) |
| 437 | 🚀 Tier 1 | **旧グローバル rules (.claude_old) の採否判断と再配置** | todo22.md | M | なし (2026-08-12 起票。マシン移行で ~/.claude/rules が消失し、ADR 9 本 + Rust ソース + hook メッセージ計 25 箇所超が dead pointer、グローバル文書対象タスク約 16 件の実施先が未定。採否はユーザー判断) |
| 438 | 🚀 Tier 1 | **孤立ブランチの回収と後始末 (nightly 未マージクローズ 3 本 + 実装孤立 2 本)** | todo22.md | M | なし (2026-08-12 scan + gh 突合。⚠ nightly ブランチの先行削除は夜間ループの再選択事故を誘発するため回収 PR マージ後にのみ削除) |
| 439 | 🔧 Tier 2 | **決定論 gate 結果の telemetry 統合 (観測不能の再発防止)** | todo22.md | M | なし (2026-08-12 起票。B1-loop NO-GO 判定が「観測手段の欠落」で立証不能に終わった再発防止。ADR-043 § Amendment 2026-08-12 参照) |
| 440 | 🔧 Tier 2 | **weekly-review 成果物の保存問題 (dead pointer + cloud 移行後の保存先)** | todo22.md | S-M | なし (2026-08-12 起票。last-run の指す 2026-07-27.md が不在、ADR-070 移行後の保存先未確認。jj-robustness facet の bounded-lifetime 判定 = todo13.md の blocker) |
| 442 | 🔧 Tier 2 | **security facet に「新規 fail-closed 検査の抜けを敵対的に探す」観点を追加** | todo22.md | S | なし (2026-08-12 起票。ADR-056 確定判定の二重 miss 分析で最も再現性の高い失敗パターン = PR #313 Critical 3 件) |
| 443 | 💎 Tier 3 | **fix 検証縮小 × re-gate 全 group 再実行の flaky 当たり面の縮小検討** | todo22.md | S-M | なし (2026-08-12 起票。ADR-058 確定判定で唯一の changed_block が flaky 誤 block と判明。negative result の永続化も正規の出口) |
| 445 | 🔧 Tier 2 | **todo preamble と facet routing 記述の整合を lint で機械検証** | todo22.md | S | なし (2026-08-13 起票。PR #395 feedback 採用。dev-conventions の暫定 convention を置換する) |
| 447 | 🚀 Tier 1 | **台帳の `✅無人可` と判断留保キーワードの矛盾を決定論層で検出 (PR #400 T1-2)** | todo23.md | S | なし (2026-08-14 採用。#400 の正準タグ規約は instruction 層のみで機械強制が無い。実装先は custom lint rule か ledger.rs の fail-closed 検査かを着手時に決める) |
| 448 | 🔧 Tier 2 | **判断留保キーワード検査の回帰テスト (canonical / tagged / untagged の 3 分類) (PR #400 T2-1)** | todo23.md | S | 447 (検証対象が 447 の成果物。走査の実体が現状 Rust に無いため単独着手は不可) |
| 450 | 🔧 Tier 2 | **push-runner の bookmark 不在を早期検出し fallback のノイズを除去 (PR #400 T2-3)** | todo23.md | S | なし (2026-08-14 実測。削除済み bookmark への fallback がパースエラーを出してから中断し、対処法が読み取りにくい) |
| 451 | 💎 Tier 3 | **OR 条件の不成立を主張するときは全経路を明示する convention (PR #400 T3-1)** | todo23.md | XS | なし (2026-08-14 採用。#400 の CodeRabbit 指摘 3 件すべてが同一欠陥。対だった順位 449 は検査対象の台帳 § 昇格検査履歴 廃止により 2026-08-16 削除、本 convention は単独で成立) |
| 452 | 💎 Tier 3 | **本リポ instruction とスキルリポ SKILL.md の同時反映チェックリスト (PR #400 T3-2)** | todo23.md | XS | なし (2026-08-14 採用。ADR-051 の具体化。スキルリポ側に約 110 行のコミット漏れが滞留していた検出も含める) |
| 453 | 🔧 Tier 2 | **post-merge-feedback 分析 agent の書き込み先制約 (read-only facet の一時ファイル生成)** | todo23.md | S | なし (2026-08-14 起票。analyze_transcript.py の実観測。weekly の workspace-hygiene-scan が backstop、本タスクは上流修正で緊急度低) |
| 454 | 🚀 Tier 1 | **自律実行ガードレールの 3 点同期を機械検証する (#400-#406 feedback 統合)** | todo23.md | S | なし (2026-08-15 採用。#403/#405 で 3 箇所を手で揃えた。片方漏れで保護が静かに緩み、#403 では実際に抽出で実体が保護外へ出かけた) |
| 455 | 🚀 Tier 1 | **一時ファイルの弱い一意性を検知する lint (#400-#406 feedback 統合)** | todo23.md | S | なし (2026-08-15 採用。#405 で production/test の両方で踏んだ。1 つ直した直後に同型を別箇所で作っており人手の注意では止まらない。regex 層の限界を先に見積もる) |
| 456 | 🚀 Tier 1 | **workflow の guard なし `git commit` を検知する (#400-#406 feedback 統合)** | todo23.md | S | なし (2026-08-15 採用。#406 で Critical を 2 度。レビューが無ければ夜間ループが停止していた) |
| 458 | 🔧 Tier 2 | **`cli-ledger-cleanup` の統合テスト suite (提案 10 件を統合)** | todo23.md | M | なし (2026-08-15 採用。手動実測した安全側 3 ケースの自動化が起点。削除は取り返しがつかないため安全側こそ回り続ける必要がある) |
| 459 | 🔧 Tier 2 | **weekly-review 周辺の決定論層テスト (提案 4 件を統合)** | todo23.md | S-M | なし (2026-08-15 採用。scan 失敗テストは検証対象が未確定 = shell のままか exe 化か。順位 448 と同じ構図) |
| 460 | 💎 Tier 3 | **外部入力の信頼境界と fail-closed の徒定形を ADR 化 (提案 3 件を統合)** | todo23.md | S | なし (2026-08-15 採用。本チェーンの Critical 2 件の根本にある原則。ADR-043 の具体化として位置づける) |
| 461 | 💎 Tier 3 | **開発 convention の一括追記 — 本チェーンの手順レベル教訓 (提案 12 件を統合)** | todo23.md | S | 460 (設計原則は ADR 側へ寄せるため先に確定させる。finding_id 埋込の方針が未決) |
| 464 | 🔧 Tier 2 | **`review-todo-whole` facet が読む台帳の事実を `cli-ledger-candidates` の出力へ寄せる** | todo24.md | S | なし (2026-08-17 に再 rescope。Criterion 3-2 は決定論 exe へ置換済み・3-3 のブランチ走査は消滅。残るのは 3-1 の逆向き差集合と `✅` 行の特定で、同 exe に出力を足すだけで足りる) |
| 465 | 🔧 Tier 2 | **docs 整合性と output-contract の drift を機械検証する (#409-#414 feedback 系統 A+B を統合)** | todo24.md | S-M | なし (旧依存だった順位 441 は 2026-08-26 に `cli-docs-lint` の `entry_pairing` として実装済み。実装先が同じなので、同 module へ相乗りするか独立 validator にするかを着手時に判断) |
| 466 | 💎 Tier 3 | **出力先と検証設計の convention を明文化する (#409-#414 feedback 系統 C+E を統合)** | todo24.md | S | なし (2026-08-17 採用。docs のみ。出力の visible paths / fixture と実データの対 / step outcome の組み合わせ の 3 点) |
| 468 | 🔧 Tier 2 | **post-merge-feedback の takt run が起動直後に死ぬ経路 — 終了理由が記録されない** | todo24.md | S | なし (2026-08-18 起票。PR #417 の調査で判明。142 run 中 2 件が analyze 起動 34 秒以内に成果物ゼロで死亡。順位 444 は回復層の修正で死因には触れていない。まず終了コード / シグナルの観測を足す) |
| 470 | 🚀 Tier 1 | **誤帰属と副作用フラグ欠如を決定論ルールで弾く (`..` 混入検出 / `jj workspace list` の `--ignore-working-copy` 欠如検出、#417+#421 feedback 採用、系統 A)** | todo24.md | S | なし (両者とも実 incident 実績あり。`.claude/custom-lint-rules.toml` の正規表現層で完結) |
| 471 | 🔧 Tier 2 | **cross-crate 定数 pin と reaper 回帰テストの残片を埋める (#417+#420 feedback 採用、系統 B 実装 + C)** | todo24.md | XS-S | なし (元 3 提案のうち 1 件は起票時点で実装済みと判明。着手時に再確認する) |
| 472 | 🔧 Tier 2 | **語彙・テスト作法・判断規律の convention を dev-conventions.md に明文化 (8 項目、#418 / #419 / #420 / #421 / #423 feedback 採用、系統 B 規約 + D + E + F 規約)** | todo24.md | S-M | なし (docs のみ。分量次第で 3 セクションに PR 分割可) |
| 473 | 💎 Tier 3 | **テスト用 staging ロックの 2 crate 重複を共有化するか再評価する (#423 feedback 採用、系統 F 実装)** | todo24.md | S | なし (ADR-044 層 1 の再評価。#423 の「3 つ目が出たら」判断の見直し) |
| 474 | 🔧 Tier 2 | **夜間 auto lane とユーザー割当 PR の同一ファイル競合を自動検知する (#424 feedback 採用、系統 G)** | todo24.md | S | なし (ADR-074 は lane 割当基準のみで並行競合検知は範囲外) |
| 475 | 💎 Tier 3 | **`resolve_project_dir` の case-sensitive FS 複数一致が無言で 1 件に縮退する (bugfix-batch-plan.md 退役準備中に発見、2026-08-19)** | todo24.md | S | なし (WSL Ubuntu-24.04 / ext4 で 5 回試行し毎回 1 件のみ返ることを確認。発現経路は未確認だが bugfix-batch-plan.md 削除後も記録を残すため起票) |
| 476 | 🚀 Tier 1 | **jj-op-verify が commit message 内の文言を実行と誤認して警告する** | todo24.md | S | なし (本セッション 4 回実観測。`detect_last_mutating_jj_op` が command 全体を split_whitespace して quote 内を除外しないため、`jj describe -m "... jj abandon ..."` を実行と誤認。助言層だが狼少年化で本物の op-log divergence を見逃すリスク。quote 除外は pure function に閉じ決定論的。Severity Medium + Frequency High + Effort S) |
| 477 | 🚀 Tier 1 | **Git Bash 経由の複数行 `node -e` が silent no-op になるのを PreToolUse でブロックする** | todo24.md | S | なし (本セッション 2 回実観測。終了コード 0・出力なしで未実行になり「修正したつもり」を作る。dev-conventions への明文化後に再発したため助言層では不十分で、決定論層 (hook) が本命。Severity High (silent failure) + Frequency Medium + Effort S) |
| 478 | 🔧 Tier 2 | **jj 出力の path separator 前提を regression test で固定する (Windows は `\` 区切り)** | todo24.md | S | なし (PR #432 で CodeRabbit が「POSIX は `/`」を根拠に `\` 判定の削除を提案したが、実測では Windows jj 0.42 は `\` 区切り出力。外すと Windows で誤検知。現在 module doc の記述のみで test 未固定のため再提案の余地が残る。Severity Medium + Frequency Low + Effort S + Risk None) |
| 479 | 🔧 Tier 2 | **手書きの「公開 API 一覧」doc が re-export とずれる — 決定論的な一致検査を入れる** | todo24.md | S | なし (実測で 22 件中 7 件が未記載。うち 3 件は PR #431 以前からの漏れで、単発ではなく継続的にずれる構造。doc を手で直す案は方針 (決定論的で積み上げる) に反するため lint 化で採用。Severity Low + Frequency Medium + Effort S + Risk None) |
| 480 | 🔧 Tier 2 | **`owns()` が false を返す原因 (Owned / TakenOver / Unreadable) をログで区別する** | todo24.md | S | なし (lock 競合 (#364 型) の調査時にログから経路を再構成できない。判定は pure function に切り出せ unit test で固定可能。Severity Medium + Frequency Medium + Effort S + Risk None) |
| 481 | 🚀 Tier 1 | **`lib-subprocess` の失敗経路を塞ぎ切る (PR #436 post-merge-feedback T1-1 + T2-1/T2-3 採用)** | todo25.md | S | なし (正常終了経路の join だけ `join_within_grace` を経由せず無制限のまま残存 = 同 PR が直した Major と同型のギャップ。Severity High / Effort XS。あわせて既存 timeout テストへの elapsed assert 追加と非 UTF-8 テストの変異確認) |
| 482 | 🚀 Tier 1 | **外部コマンド呼び出しの落とし穴を lint で塞ぐ (PR #435 T1-1 + PR #437 T1-3 採用)** | todo25.md | M | なし (`gh pr view --json files` の 100 件無言切り捨てと、ref を破壊する push の lease 欠落。**後者は削除系 (`--delete` / `:refs/...`) と非 fast-forward 更新系 (`--force` / `+refs/...`) の 2 種類**で同じパターンでは捕まらない — PR L で実際に踏んだのは削除系。**規約でなく lint で塞ぐ**判断 = 規約追記は再発を防げなかった実証あり。ADR-007 の層判定を先に行う) |
| 483 | 🔧 Tier 2 | **エラーメッセージの無制限 debug 補間を lint で検出する (PR #437 T1-1 採用)** | todo25.md | S | なし (`clip_for_message` を導入したのに順位セル `{raw:?}` だけ経由せず切り詰め保証が崩れていた。検出範囲の絞り込み方が設計の肝で、全 `{:?}` を禁じると誤検知だらけになる) |
| 484 | 🔧 Tier 2 | **push stage の bare push フォールバック不変条件を seal する (PR #434 T2-1 採用)** | todo25.md | M | なし (fail-closed の判定結果である空リストが上流 fallback に無視される execution-contract 違反が PR #434 の根因。修正済みだが不変条件はテスト未固定。**非空を型で表現できるなら型が良い**) |
| 485 | 🔧 Tier 2 | **PR L で追加した実装のテスト補強 (PR #437 T2-1 + T2-2 採用)** | todo25.md | S | なし (`warn_when_unresolved` の false 側テストが無い + `clip_for_message` がタイトル列でしかテストされず順位セル経由の穴を見逃した。どちらも「追加した機能の一部の経路しかテストしていない」形) |
| 486 | 🚀 Tier 1 | **auto lane の対象ファイルが Guard 禁止パスに当たる行を決定論的に弾く (夜間ループ停止調査 2026-08-22 由来)** | todo25.md | S | なし (2026-08-20 の run が順位 383 を選び `src/lib-ledger/src/lib.rs` の変更で `[NIGHTLY_DENY]` 停止。auto lane 22 行の全件照合で 5 行が deny リスト該当 (383 / 454 / 368 / 360 / 361)。ADR-074 決定 2 クラス 3 の判定を決定論化する — 同 ADR 決定 6 が「決定論だが未実装」と自認している穴。**実装先が deny リスト配下のため auto lane に載せない**) |
| 487 | 🚀 Tier 1 | **nightly-todo の master 参照を SHA で pin する (夜間ループ停止調査 2026-08-22 由来)** | todo25.md | S | なし (2026-08-21 の run で master-ref=`7539551f` / work=`868c9316` と 31 秒差の別コミットを読み、その間に順位 228 の実装 PR #422 がマージされて変更 0 件で停止。master を 3 回別々に読むのに pin が無い。**`.github/workflows/` が deny リスト該当のため auto lane に載せない**) |
| 492 | 🔧 Tier 2 | **agent プロンプトの禁止パス列挙から台帳が欠落している** | todo26.md | XS | なし (ADR-072 決定 6 の禁止パスは 3 箇所に写しがあり、Guard 正規表現と ADR は 9 件だが agent プロンプトの列挙だけ 8 件で `docs/claude-code-web-tasks.md` を欠く。2026-08-25 に 3 箇所を突き合わせて実測。**強制層 = Guard 正規表現はずれていないため fail-closed は成立**しており、実害は agent が台帳を触って Guard deny に当たり run を 1 回捨てること。順位 486 と同じクラスの損失を別の入り口から作る。順位 454 の 3 点同期検査が最初に検出するはずの現存ずれで、454 と同一 PR にしてもよい。`.github/workflows/` が Guard 禁止パスのため auto lane 不可) |
| 493 | 🚀 Tier 1 | **jj materialize による mtime リセットで「最近 fetch した」「書き込み中」判定が壊れる** | todo25.md | S | なし (週次レビュー WR-2026-08-22-J01 / J02、severity=high、facet=jj-robustness。`fetch_head_is_recent()` と `holder_still_writing()` の 2 件は「jj が working copy を materialize すると全ファイルの mtime が checkout 時刻へ書き換わる」という同一根因。bugfix-batch-plan.md の PR P が担当) |
| 494 | 🚀 Tier 1 | **ADR-032 の「永久欠番」決定が CLAUDE.md の ADR index へ未反映** | todo25.md | XS | なし (週次レビュー WR-2026-08-22-A01、severity=high、facet=architecture、category=adr-alignment) |
| 495 | 🔧 Tier 2 | **`lib-*` crate の責務分類基準が ADR-012 に無い** | todo25.md | S | なし (週次レビュー WR-2026-08-22-A04、severity=medium、facet=architecture、category=module-boundary) |
| 496 | 🔧 Tier 2 | **docs の 50KB 超過 3 ファイルを物理分割する** | todo25.md | M | なし (2026-08-22 週次レビューの決定論 scan 由来。`todo-summary2.md` は優先度表 1 枚のため節ではなく順位で切る必要がある) |
| 497 | 🔧 Tier 2 | **PostToolUse で docs ファイルの 50KB 超過を即時ブロックする** | todo25.md | S | なし (2026-08-22 週次レビューの決定論 scan 由来。現在 file-length の検査は週次レビューの報告のみで、超過しても何も止まらない。順位 496 と対) |
| 498 | 🔧 Tier 2 | **非主要拡張子の coverage を拡張子ごとに要求する (`other_ext_tests` の map 化)** | todo25.md | M | なし (PR #461 の CodeRabbit 指摘由来。現行契約は「rule あたり 1+ test」で、その契約自体は `non_main_extension_coverage_is_per_rule_not_per_extension` が固定済み) |
| 499 | 🚀 Tier 1 | **takt の verdict を push-runner が読み、REJECT のまま push される経路を塞ぐ** | todo25.md | S-M | なし (2026-08-30 に PR #463 の作業中で実測。defect-convergence-plan の前提「強制点 = push ゲート」を崩す穴のため Tier 1) |
| 500 | 🚀 Tier 1 | **[defect:G2] `cfg(test)` 判定を ident 単位にし testability gate の fail-closed 経路を固定する** | todo26.md | S | なし (PR #456 feedback。文字列マッチが `#[cfg(test_util)]` にも当たる。判定層にテストはあったが ident 境界の軸が未カバー) |
| 501 | 🚀 Tier 1 | **[defect:G2] 由来タグ判定の単語境界と rustdoc 相対リンクの段数を検査する** | todo26.md | S | なし (PR #472 / #463 feedback。`rerun` が run ID に当たる = 証拠検査が緩む向きの誤り。段数ずれは cross-ref も通る) |
| 502 | 🚀 Tier 1 | **[defect:G1] silent drop / `gh --repo` 欠落 / `spawnSync` timeout 未指定を lint で塞ぐ** | todo26.md | M | なし (PR #454 / #470 feedback。3 件とも実際に踏んだ欠陥で、検査の場が無かった。`extensions` への `mjs` 追加が前提) |
| 503 | 🔧 Tier 2 | **[improvement] doc と実装の同期を検査する (exit code 一覧 / 依存者リスト)** | todo26.md | M | なし (PR #456 / #464 feedback。実際に壊れた観測はまだ無く、予防のための検査) |
| 504 | 🔧 Tier 2 | **[defect:G2] 台帳検査の入力空間を埋める** | todo26.md | M | なし (PR #457 / #458 / #460 feedback。`cfg(test)` 宣言形の全パターンが未カバー。ADR-049 への case 追加を同乗) |
| 505 | 🔧 Tier 2 | **[defect:G2] telemetry の id 契約と TOML 構造の回帰を足す** | todo26.md | S | なし (PR #463 / #456 feedback。セクション分断を実際に起こした。ADR-055 への識別子判定基準の追記を同乗) |
| 506 | 🔧 Tier 2 | **[defect:G2] 夜間ループと Node script 層の境界をテストで固定する** | todo26.md | M | なし (PR #466 / #469 / #470 / #471 feedback。B4 の 4 件は実測済みで固定するだけ、合成ブランチの CI 化のみ新規) |
| 507 | 🚀 Tier 1 | **[defect:G1] 夜間 agent の `Edit(work/**)` がドット始まりディレクトリを覆わない** | todo26.md | M | なし (run 33665621808 で permission_denials_count: 2。順位 281/455 が構造的に完了不能で human lane へ退避済み。着手時判断: 原因の実測確認と `.github/` を allow に含めるかの整理が要る) |
| 508 | 🔧 Tier 2 | **[improvement] 台帳追加候補の除外クラスを決定論で機械適用する** | todo26.md | M | なし (2026-09-03 weekly-review で 238 件を人手選別した。ADR-072 決定 18 の読み替えと skill 制約の改訂を伴う。着手時判断: 順位 486/447 の検査と判定ロジックを共通化するか) |
| 509 | 🚀 Tier 1 | **[defect:G1] `cli-merge-pipeline` の gh 呼び出しが非 colocated workspace で解決に失敗する** | todo26.md | S | なし (weekly-review WR-2026-09-03-J01。順位 467 F-2 / PR #470 と同型で 3 度目。着手時判断: `detect_owner_repo` は `--repo` が循環するため代替経路の選択が要る。順位 502 の lint との前後関係も決める) |
| 510 | 🚀 Tier 1 | **[defect:G1] 夜間ループの稼働状況を週次レビューで見張る** | todo26.md | M | なし (直近 8 晩で 5 晩 red・直近 4 晩連続なのに 2026-09-03 の findings 8 件に言及 0 件。gh が要るため L3 の決定論 scan に置く。着手時判断: ログをどこまで読むか = 停止段まで出すか conclusion だけか) |
| 511 | 🔧 Tier 2 | **[improvement] `todo-summary2.md` を 3 分割し明示列挙の呼び出し元を追随させる** | todo26.md | M | なし (79KB。機構は F1 で 3 分割対応済みだが `--repo` ならぬ `--summary-file` の明示列挙が package.json と nightly-todo.yml に残る。workflow を触るため auto lane 不可。着手時判断: どの順位で切るか) |
| 512 | 💎 Tier 3 | **[improvement] 50KB 超の詳細エントリファイル (`todo14.md` / `todo22.md`) を分割する** | todo26.md | M | なし (61KB / 59KB。移動したエントリの順位 table「ファイル」列の追随が必須で entry_pairing が強制する。着手時判断: 分割か孤児削除かを先に測る) |
| 513 | 💎 Tier 3 | **[improvement] 50KB 超の恒久ドキュメント (ADR-072 / 台帳 / workflow 2 件) の扱いを決める** | todo26.md | L | なし (126KB / 60KB / 67KB / 64KB。watchlist の走査範囲が `docs/todo*.md` に限られ構造的に見逃していた。着手時判断: 分割の可否をファイルごとに決め、走査範囲の拡張方針も併せて決める) |


**戦略**: Tier 1 を 2〜3 セッションで片付け → Tier 2 で計測基盤 (gate telemetry / weekly-review 保存) + rate-limit + convergence cost 削減を進める → Tier 3 でドキュメント整備。Tier 4-5 は cleanup / 外部展開で daily efficiency への直接効果は小さい。(2026-08-12 更新: 旧記述の ADR-032 は ADR-057 置換で欠番)

**Bundle 履歴**: 完了済 Bundle / post-merge-feedback 反映の経緯詳細は [docs/bundle-history.md](bundle-history.md) を参照 (2026-05-25 分離、本ファイルの index 責務集中のため)。
