# ADR-072 検証記録 — 夜間 todo 消化ループの実走観測

> **本ファイルの位置付け**: [ADR-072](adr-072-nightly-todo-loop.md) の § 検証記録 を 2026-09-03 に分離したもの (順位 513)。ADR 本体が 126KB に達し、50KB の安定読み取り閾値を大きく超えていたため。
>
> **なぜ検証記録を出したか**: ADR 本体が持つべきは**決定とその根拠**で、実走スモークの観測ログは追記され続ける性質が違う。決定を読みたい人が 27KB の観測ログを読み飛ばす必要がなくなる。**決定は分割していない** — ADR は 1 決定 1 ファイルが原則で、決定どうしの参照を切ると読めなくなるため。
>
> **本ファイルは追記先**である。以後の実走観測はここへ足す。


### `cli-nightly-task-select` の unit test (25 件)

台帳パーサ 17 件 / 引数解析 8 件。境界として固定したもの:

- 文書順の最初を選ぶこと、除外順位を飛ばすこと
- 全候補が除外済み / 無人可マークが 1 つも無い → `None` (正常な no-op)
- **無人可 列を持つ表が 1 つも無い → エラー** (no-op ではない)
- 順位の重複 / 未知のマーク表記 (`✅ (条件付き)` など) / 非数値の順位 / 列ずれ / 区切り行の欠落 → すべてエラー
- 無人可 列を持たない表 (棚卸し履歴など) は無視する
- エスケープされたパイプ (`\|`) をセル区切りにしない
- `--exclude-ranks` の空文字は空集合、フラグ欠落は引数不正

### 実データでの選択 (2026-08-06)

WP-18 PR 2 (#362) の台帳に対し release build の実 exe を走らせた。

| 入力 | 結果 |
|---|---|
| 除外なし | `rank=203 branch=claude/nightly-203` (Batch 1 の先頭 ✅ 行) |
| `--exclude-ranks 203` | `rank=240` |
| 無人可 7 件すべて除外 | exit 3 (no-op) |
| **PR 2 未マージの旧台帳** | **exit 2** — 無人可 列が無いため loud に停止 |

最後の 1 行が重要で、台帳が旧構成のまま夜間ループが動いても「該当タスク無し」として静かに no-op せず、理由を出して止まる。棚卸し履歴の表と「無人可としなかった理由」の表 (どちらも順位 列を持つが 無人可 列を持たない) が正しく無視されることも実データで確認した。

### workflow の構文検証

js-yaml で 17 step 構成を確認した。

### 静的レビューが著者の見落としを 10 件捕捉した (2026-08-06〜07)

pre-push review を 12 サイクル通す過程で、blocking な欠陥 10 件が見つかった (うち 1 件は non-blocking warning からの拾い上げ)。**いずれも著者 (Claude) が設計時に気づけなかったもの**で、記録しておく価値がある。共通するのは「守っているつもりの範囲」と「実際に守れている範囲」のずれである。

なお 10 件のうち 2 件 (#3 / #6) は**指摘の具体例そのものが誤っていた**。#3 は誤りに気づかず設計を動かし、#6 は誤りの中にある正しい構造を拾って設計を変えた。差は前提を検証したかどうかだけである。

| # | 指摘 | 何を見落としていたか |
|---|---|---|
| 1 (simplicity) | `Pre-flight gate` だけが `id` / `continue-on-error` を持たず、背圧 deny が job 全体の failure になる | 4 つある停止点のうち 3 つを graceful に設計しておきながら、**最も高頻度で踏まれる最初の 1 つ**だけ落としていた。背圧が効くたびに毎晩赤い × が出て、`Report outcome` も停止段を特定できない |
| 2 (security) | agent の unscoped な file tools が `master-ref/` に届き、ゲート exe / config を改ざんできる | 決定 6 の禁止リストを「ガードレール保護」として設計した時点で、**保護対象を `work/` の diff だけに限定していた**。調達元そのものが書き換え可能である経路を見ていなかった (→ 決定 7 を新設) |
| 3 (security) | `Bash(cargo test:*)` は前方一致でシェルを解釈しないため `cargo test; curl ...` が通り、diff に痕跡を残さず任意コマンド実行できる | **この指摘は誤りだった** (2026-08-06 に公式ドキュメントで検証)。Claude Code は shell operator を解釈し各サブコマンドの独立一致を要求する。ただし著者はそれを検証せず設計変更し ADR の決定として記録した — **静的レビューも誤りうるのに、REJECT という形式が検証を省く圧力として働いた**。決定 5 自体は別の根拠 (cargo が agent 自身の書いたコードを資格情報のある環境で実行する) で維持したが、根拠は差し替えた |
| 4 (security) | job の `GITHUB_TOKEN` が write のままで、agent が触れる `github_token` 入力から「agent は push できない」前提を崩せる | 決定 8 で App token を**足す**ことだけ考え、元の write 権限を**落とす**発想が無かった。資格情報を追加したら既存の権限を見直す、が抜けていた |
| 5 (security) | agent が `work/.git/hooks` / `.git/config` を書けば、App token を持つ publish step で任意コマンドが走る | 決定 6 の禁止リストを `git diff` で実装した時点で、**`.git/` 内部が構造的に見えない**ことに気づいていなかった。しかも決定 8 で同じ step に live な資格情報を置いており、自分で作った穴に自分で価値を積んでいた |
| 6 (security) | 上記の deny-list が `alias.*` / `core.fsmonitor` / `credential.helper` を漏らしている | **列挙で守ろうとしたこと自体が誤り**だった。2 回連続でレビュアーが漏れを見つけた時点で、列挙の追随ではなく構造で断つべきだと気づくべきだった (→ 決定 9)。なお指摘の具体例 (`alias.push` が組み込みを差し替える) は公式ドキュメント上**成立しない**が、示された構造は正しい — 誤りの部分だけを見て退けると正しい部分を捨てることになる |
| 7 (simplicity) | 決定 9 の `publish/` が `--branch master` 固定で、agent 実行中に master が進むと無関係な変更を静かに revert / delete する | **セキュリティのために入れた構造変更が、別の正しさを壊していた**。`work/` の checkout と `publish/` の clone の間に最大 60 ターン + Verify が挟まることを、`rsync --delete` を書いた時点で考えていなかった (→ base commit への固定を追加) |
| 8 (simplicity) | 決定 6 の禁止リストが**台帳自身**を守っていない | 「自分を縛る仕組み」として config と gate exe は列挙したのに、**選択元である台帳**を同じクラスだと認識していなかった。決定 1 で「台帳は master ref から読む」と信頼境界を引いておきながら、その台帳への書き込み経路を塞いでいなかった |
| 9 (simplicity) | job に `timeout-minutes` が無く既定 360 分まで走りうる | 同リポジトリの他 workflow は**例外なく明示済み** (`ci.yml`=60 / `pr-monitor.yml`=15, 20 / `release-binaries.yml`=30)。とくに同じ `claude-code-action` を使う `pr-monitor.yml` の fix job は 30 turns に対し 20 分を課しているのに、本 job は turns 2 倍 (60) で上限なし。決定 4 で Max 枠の節約のために gate を二重に呼ぶ設計にしておきながら、**ハングした run が枠を焼き続ける経路**を空けていた。`timeout-minutes: 60` を明示 |
| 10 (simplicity) | 改ざん検知が決定 10 の色分けのどちらにも入らないまま green 側に落ちていた | 決定 10 を書いた**その push で**露出した。分類を明文化すると、分類に入らない結末が可視になる — 暗黙のまま放置していれば誰も気づかなかった。fail-closed は成立していたので、見落としていたのは*安全性*ではなく**可観測性**の側だった |

修正後、fix step は指摘 1 に対して `id` / `continue-on-error` の追加に加え、`Select task` へ `if: steps.preflight.outcome == 'success'` を足している。これは指摘の Fix Suggestion には無かったが**必要な追加**である — `continue-on-error` を付けただけでは deny 後も後続段が走ってしまう。

### integrity 機構の drill 4 シナリオ (2026-08-06)

決定 7 の照合が「改ざんを検知する」だけでなく「改ざんが無ければ通る」ことも確認した。後者を確認しないと、毎晩必ず止まる誤検知に気づけない。

| # | 状況 | 期待 | 結果 |
|---|---|---|---|
| 1 | agent が `work/` だけを変更 | 通過 (exit 0) | 一致 |
| 2 | `autonomy-config.toml` を改ざん (閾値の自己引き上げ) | 検知 (exit 1) | 一致 |
| 3 | ゲート exe を改ざん | 検知 (exit 1) | 一致 |
| 4 | 基準値ファイルごと削除 (検知器への攻撃) | 検知 (exit 1) | 一致 |

#4 は `sha256sum -c` が基準値ファイルを読めない時点で失敗するため、fail-closed が自然に成立する。加えて基準値は `$RUNNER_TEMP` = ワークスペース外にあり agent のサンドボックスが届かないため、防御は 2 層になっている。

### 実走スモーク — allow 経路は **schedule の初回実走で成立** (2026-08-08)

**スモークは `workflow_dispatch` で意図的に始める前に、schedule (03:00 JST) の初回実走が先に成立した。** 2026-08-07 18:19 UTC の run が台帳の順位 203 を選び、draft PR [#365](https://github.com/aloekun/claude-code-hook-test/pull/365) (`claude/nightly-203`) の作成まで完走した。

**この順序は褒められたものではない。** 受け入れ基準の中核である実走検証を、人間が観測装置を用意した dispatch ではなく**本番の無人 run が先に消化した**形になっている。結果的に成功したが、失敗していれば観測の準備が無いまま夜間に壊れた成果物が出ていた。ここでの教訓は、`AUTONOMY_ENABLED` を立てた時点で schedule も同時に有効になるという事実が、スモーク計画に織り込まれていなかったこと (§ 残課題)。

観測項目は **10 件**である (起票時の 8 件 + [#364](https://github.com/aloekun/claude-code-hook-test/pull/364) で追加した停止側 1 件 + 順位 379 で追加した tool scope deny 1 件)。以降の集計はこの 10 件を母数にする。

| 観測項目 | 出所 | 結果 (2026-08-08) |
|---|---|---|
| Actions variable `AUTONOMY_ENABLED` が `true` ちょうどで設定されており job が起動すること | ADR-066 § 決定 2 (完全一致要件) | **充足** — job が起動し完走 |
| `claude/nightly-*` の **ref 作成**が App token で通ること (ruleset の除外が creation にも効くこと) | ADR-067 段 0 の ruleset。Phase B が観測したのは既存ブランチへの update のみ | **充足** — `claude/nightly-203` が新規作成された |
| **App token で作った draft PR に `ci.yml` の 2 OS run が紐づくこと** | 決定 8 (仕様は公式で確認済み、実環境での成立は未観測) | **充足** — PR の author は `nightly-todo-aloekun[bot]`、`rust (ubuntu-latest)` / `rust (windows-latest)` がいずれも success。承認待ちにならなかった |
| 決定 7 の照合が実 runner 上でも通ること (誤検知で毎晩止まらないこと) | 本ファイル § integrity 機構の drill | **充足 (1 run)** — 誤検知せず publish へ到達。毎晩の安定性は継続観測 |
| `publish/` の clone + rsync が実 runner で成立し、`work/` の変更が過不足なく運ばれること | 決定 9 (`--delete` による削除の反映を含む) | **充足** — commit は 1 ファイル 18 行追加・削除ゼロで、順位 203 の指定範囲と完全に一致 |
| WP-17 残課題: Phase B の自動起動経路が成立するか | [ADR-067](adr-067-phase-b-unattended-fix-push.md) § 検証記録 | **経路は生存 (2026-08-09 訂正)** — 2026-08-08 時点では「不成立」と記帳したが誤り。起動契機のコメントが無かっただけで、CodeRabbit がコメントした時点で `issue_comment` 経路は発火した (#373 で 04:12:15 に **Phase A が夜間 PR 上で自動起動**)。**Phase B 本体 (無人 fix push) の到達はなお未観測** — docs 指摘が出る PR に当たっていない |
| WP-17 残課題: `coderabbitai[bot]` allowlist の要否 | 同上 | **未判定 (2026-08-10 に前提が整った)** — 決定 16 で夜間 PR へ**レビュー要求が毎回届く**ようになり、判定に必要な起動契機が供給されるようになった (要求と実レビュー取得は別で、レート制限で弾かれる夜がある — 順位 431)。次に **docs 指摘の出る夜間 PR で実レビューが取得できたとき**に Phase A/B の起動可否と併せて見る |
| **`cargo` サブプロセスから `CLAUDE_CODE_OAUTH_TOKEN` / `GITHUB_TOKEN` が見えるか** | pre-push security review の warning | **未観測 (意図的に保留)** — 観測には使い捨ての `build.rs` を仕込む専用 run が要り、初版の probe は public CI ログへ広く env 名を出す設計欠陥で撤去した (§ 残課題)。決定 5 で agent に Bash を与えない判断は**保守側**のため、未観測でも安全側に倒れている。確実に 1 つずつ可観測性を積む方針 (2026-08-08 ユーザー確認) に従い、安全な probe を設計できるまで保留する |
| **停止側: `AUTONOMY_ENABLED` が `'false'` / 未設定で何も作られないこと** | ADR-066 の 3 状態。#364 で受け入れ基準へ追加 | **充足** (2026-08-08、ユーザー実測) — `'false'` と未設定の 2 状態で `workflow_dispatch` (`dry_run` オフ = push / PR 作成をする設定) を実行し、**2 回とも job が skip**。ブランチ・draft PR・App token のいずれも作られなかった。確認後 `'true'` へ復旧済み |
| **tool scope の deny が効くこと (agent が `master-ref/` へ書けない)** | 決定 12 (順位 379) | **充足** (2026-08-08、ローカル CLI 実測) — 同じ `--allowedTools` / `--disallowedTools` フラグで `master-ref/PROBE.txt` への Write を試させると `File is in a directory that is denied by your permission settings.` で拒否され、ファイルは作られず config も無傷。対照で `work/` への Write は成功。あわせて実 dispatch run で agent が対象 1 ファイルのみ編集し `guard=success` = allow 側も成立 |

**停止側は `dry_run` をオフにして検証した。** `AUTONOMY_ENABLED` が `'true'` でなければ job の `if:` で止まるため `dry_run` の値は判定に関与しないが、**あえて「push も PR 作成もする設定」で実行**することで「dry_run だから作られなかったのでは」という解釈の余地を消している。

**10 件中 7 件が充足、残る 3 件が未確定。** 未確定は (a) Phase B 本体 (無人 fix push) への到達、(b) `coderabbitai[bot]` allowlist の要否、(c) トークン露出 (安全な probe を設計できるまで保留)。

> **(a)(b) の前提は 2026-08-10 に整った。** 両者は「CodeRabbit が夜間 PR にコメントすること」を起動契機とするため、レビューが一度も付かない間は**観測機会そのものが無かった**。決定 16 で**レビュー要求が毎回届く**ようになり、docs 指摘の出る夜間 PR で実レビューが取得できれば判定できる。**未確定の理由が「機構が無い」から「事象待ち」へ変わった**点が進捗である。
>
> **要求が届くことと実レビューが取得できることは別である** (2026-08-11 追記)。決定 16 が保証するのは要求の発行と反応の確認までで、レート制限で弾かれる夜がある (#387 で実測)。観測機会の供給頻度は「毎晩」ではなく「枠が空いている夜」が正しい。
>
> **集計の訂正 (2026-08-09)**: 従前は「8 件が充足、1 件が不成立、残る未確定は 2 件」と書いていたが、合計が 11 件で母数の 10 件と合っておらず、充足数も表と 1 件ずれていた (表の充足は 7 行)。**表が正**であり、上記へ改めた。

**トークン露出の観測は意図的に保留する。** 初版の probe は (1) `build.rs` が draft PR の git 履歴に残り、(2) 名指しの 4 変数を超えて `TOKEN`/`SECRET`/`KEY` に一致する全 env 名 (`ACTIONS_RUNTIME_TOKEN` 等) を public CI ログへ出す設計欠陥があり、pre-push security review が REJECT して撤去した。安全に観測するには最低限 (a) `build.rs` を Guard の deny 配下パスに置いて commit 混入を防ぐ、(b) 出力を名指しの変数のみに絞る、(c) `if: github.event_name == 'workflow_dispatch'` で dispatch 限定にする、の 3 点が要る。決定 5 の Bash 非付与が保守側に倒れているため未観測でも安全側であり、不確実な追加 dispatch を急がず、設計を固めてから 1 回で観測する (2026-08-08 ユーザー方針)。

### 定常運用 2 巡目の実走観測 — 全ゲートが設計どおり働いた (2026-08-11)

schedule の実走 ([run 31418341378](https://github.com/aloekun/claude-code-hook-test/actions/runs/31418341378)) が順位 339 を選び、PR [#387](https://github.com/aloekun/claude-code-hook-test/pull/387) (`claude/nightly-339`) の作成まで完走した。**決定 15-17 を入れた後の経路を通しで観測した最初の run** であり、以下を実測で確認した。

| 時刻 (UTC) | 出来事 |
|---|---|
| 18:17:41 | `nightly-todo` が schedule 起動 |
| 18:18:14 | kill-switch 通過・背圧 ok・順位 339 を選択 |
| 18:22:26 | PR #387 作成 (author `app/nightly-todo-aloekun`、`draft=false`) |
| 18:22:29 | `review-request` が自動起動 |
| 18:22:40 | `@coderabbitai review` を **`aloekun` (人間 identity)** で投稿 |
| 18:22:46 | CodeRabbit が **6 秒後**に反応 |
| 18:23:11 | 検証 step が反応を確認して success |

| 設計 | 根拠 | 実測 |
|---|---|---|
| kill-switch は config + variable の AND | ADR-066 決定 2 | `AUTONOMY_ENABLED=enabled repo_config=enabled` |
| config は **master ref の写し**を読む | ADR-066 決定 3 | `config=master-ref/autonomy-config.toml` |
| 背圧は未マージ `claude/` PR 数 | [ADR-071](adr-071-draft-pr-backpressure.md) | `backpressure(autonomous-pr)=ok(1/3)` |
| 着手済みはブランチ存在で除外 | 決定 3 | `着手済み順位=[203,228,240]` を除外 |
| 台帳も master ref から読む | 決定 3 の信頼境界 | `ledger=master-ref/docs/claude-code-web-tasks.md` |
| 停止点は通常 PR | 決定 15 | `draft=false`、OPEN のまま停止 |
| レビュー要求は人間 identity | 決定 16 | 投稿者 `aloekun`、6 秒で反応 |
| 台帳は bot が書き換えない | 決定 6 | 変更は `decide.rs` 1 ファイルのみ |
| 宣言した対象ファイルの外へ出ない | 決定 12 の tool scope | 台帳の対象 `src/check-ci-coderabbit/src/decide.rs` と完全一致、+102/-0 (テストのみ) |

CI は両 OS pass。**逸脱は 1 つも無かった。**

#### ただしレビューは取得できていない — 成功判定が「反応の有無」で止まっている

CodeRabbit の反応の中身は `Review limit reached` (レート制限、`Next review available in: 27 minutes`) だった。`review-request` の検証は**要求後に CodeRabbit のコメントが 1 件以上付いたか**だけを見るため、**拒否も success として記録される**。

これは決定 11 の失敗 (10 時間の無反応に気づけなかった) への対策としては契約どおりで、workflow のコメントにも「投稿の成否ではなく CodeRabbit の反応を待つ」と明記してある。**設計の欠陥ではない。**

ただし結果として、**レビュー未取得が独立した状態として分類されていない**。痕跡自体は残っている — PR には `Review limit reached` コメントが付き、run log にも反応確認の記録がある。問題は**それらが success と同じ色で終わる**ことで、run 一覧や PR 一覧を見ただけでは「レビュー済み」と「レート制限で未取得」を区別できない。§ M5 の方針でリトライ機構を持たないため、解除後に自動で再要求されることもない。

**扱いは未決** — 初回レビューを取得するまで success 判定を遅らせる案をユーザーが検討中 (2026-08-11)。本節は観測事実の記録に留める。

#### レート制限の競合は順位 401 の予測どおりに起きた

枠を消費したのは**同日の人間の作業**である。[ADR-019](adr-019-coderabbit-review-hybrid-policy.md) § 無料枠の窓は固定時刻ではなく直近の消費に追随する に記録した「決定 16 で自律 PR が毎晩 1 レビュー消費するため、人間の作業が集中する日に競合する」が、**記録の翌日に実地で再現した**。

| 時刻 (UTC) | レビュー消費 |
|---|---|
| 15:48:25 | PR [#385](https://github.com/aloekun/claude-code-hook-test/pull/385) (人間) |
| 17:50:53 | PR [#386](https://github.com/aloekun/claude-code-hook-test/pull/386) (人間) |
| 18:22:26 | PR #387 (**自律**) → 枠切れ |

想定外の挙動ではなく、**想定した副作用が想定どおり出た**という位置づけになる。

### 外部設定の実体 (2026-08-08 確認)

[ADR-051](adr-051-cross-system-config-coupling.md) が内部設定と外部 SaaS 設定の論理結合に課す 3 点のうち、**(2) 期待値の組み合わせ表**を本節が担う。**秘密値そのもの (`NIGHTLY_APP_PRIVATE_KEY` の鍵本文や発行済み token) は記録しない** — 記録するのは結合の存在と期待値であって、秘密の実値ではない。

| 項目 | 実体 |
|---|---|
| App 名 | `nightly-todo-aloekun` (PR の author は `nightly-todo-aloekun[bot]` として現れる) |
| 作成日 | **未確認** — App の設定ページに表示が見当たらなかった。監査で必要になった時点で GitHub の Audit log から引く |
| インストール範囲 | **Only select repositories** = `claude-code-hook-test` のみ |
| 付与権限 | Contents: **Read and write** / Pull requests: **Read and write** / Metadata: **Read-only** / Workflows: **No access** |
| `NIGHTLY_APP_ID` | repository **variable** (Actions → Variables) |
| `NIGHTLY_APP_PRIVATE_KEY` | repository **secret** (Actions → Secrets) |
| `AUTONOMY_ENABLED` | repository **variable** (Actions → Variables)。現在値 `'true'` |
| `CODERABBIT_TRIGGER_PAT` | repository **secret** (Actions → Secrets)。2026-08-10 登録。**fine-grained PAT / 対象リポジトリ 1 つ (`claude-code-hook-test`) / `Pull requests: Read and write` のみ / 期限あり**。用途は [`review-request.yml`](../../.github/workflows/review-request.yml) からのレビュー要求コメント投稿だけ (決定 16) |

**PAT は push もマージもできない。** どちらも `Contents: write` が必要で、本 PAT には付与していない。決定 8 が PAT を却下した理由 ([ADR-067](adr-067-phase-b-unattended-fix-push.md) の ruleset backstop 迂回) は、**push に使う PAT** に対するものであって権限を絞った PAT には当たらない。この非対称が決定 16 を成立させている。

**期限切れは沈黙しない。** PAT が失効すると `review-request.yml` の投稿 step が失敗し、workflow が red になる。投稿できても CodeRabbit が反応しなければ検証 step が red になる (決定 16)。どちらの経路でも run 一覧から気づける。

**付与権限は決定 8 の設計意図と完全に一致していた。** 同決定は「Contents: write / Pull requests: write / Metadata: read のみ。**Workflows は付けない**」と書いており、実体もそのとおりだった。Workflows が No access であることは、`.github/workflows/**` を含む push が権限層でも通らないことを意味し、決定 6 の禁止リストと二重の防御になっている。

**既存の Claude GitHub App とは別物である。** あちらは Workflows を含む広い権限を持ち、Claude Code の cloud 経路が使う。混同して「もう入っているから不要」と判断しないこと。

#### 値が欠けたときにどう倒れるか

| 欠落 | 挙動 | 根拠 |
|---|---|---|
| `AUTONOMY_ENABLED` が `'false'` / 未設定 | **job ごと起動しない**。ブランチ・draft PR・App token のいずれも作られない | **実測済** (2026-08-08、§ 実走スモーク)。job の `if: vars.AUTONOMY_ENABLED == 'true'` |
| `autonomy-config.toml` の `enabled = false` | `cli-autonomy-gate` が deny。pre-flight で agent 起動前に止まり、green + `[NIGHTLY_SKIP]` で終わる | exe 単体の drill 8 シナリオ ([ADR-066](adr-066-autonomy-global-kill-switch.md)) |
| `max_open_autonomous_prs` に到達 | 同上 (背圧 deny) | drill 12 シナリオ ([ADR-071](adr-071-draft-pr-backpressure.md)) |
| `NIGHTLY_APP_ID` / `NIGHTLY_APP_PRIVATE_KEY` が欠落 | `Mint App token` が失敗 → **run は red**。`publish` は `if: steps.app-token.outcome == 'success'` で skip されるため、ブランチも draft PR も作られない | **設計上の期待値 (未実測)**。同 step は決定 10 に従い `continue-on-error` を持たない |

最終行は**未実測**である。資格情報を意図的に壊す run は、復旧を伴うため実施していない。fail-closed の側 (欠けても成果物が出ない) は step の `if:` 連鎖から構造的に言えるが、**run が red で終わるか green で終わるか**は実際に落としてみないと確定しない。

#### 再構築手順 (鍵ローテーション / 派生プロジェクトへの展開)

1. GitHub App を作成する。権限は上表のとおり (**Workflows は付けない**)
2. インストール先を対象リポジトリ 1 つに絞る (Only select repositories)
3. App ID を repository **variable** `NIGHTLY_APP_ID` へ登録
4. 秘密鍵を生成し repository **secret** `NIGHTLY_APP_PRIVATE_KEY` へ登録 (鍵本文はリポジトリにも ADR にも残さない)
5. `AUTONOMY_ENABLED` を variable として `'true'` に設定する。**これを立てると schedule も同時に有効になる** (§ 残課題)

上記手順とは別に、**決定 5 で Bash を落とした後も残る経路**の確認が要る。agent は `cargo` を直接叩けないが、`work/` へ書いた `build.rs` は `Verify deterministically` step の `cargo test` が実行する。同 step に `env:` を置いていないことがトークン非露出の根拠なので、**その前提が実 runner で成立するか**を使い捨ての `build.rs` から実測する。**値は出さない** — `env | grep -i token` は `NAME=value` を丸ごと public CI ログへ出すため、上記 (b)「出力を名指しの変数のみに絞る」に反する。名指しの変数について**設定の有無だけ**を出す (例: `println!("cargo:warning=GITHUB_TOKEN set={}", std::env::var("GITHUB_TOKEN").is_ok())`)。

この観測は 2 つの判断に効く。露出があれば `Verify` step の env に明示的な scope 制限が要る。露出が無ければ、決定 5 で保守側に置いた **Bash の再付与 (agent が自分の変更を検証できる利点) を再検討してよい** — 決定 5 の根拠は「資格情報のある環境で agent の書いたコードを走らせない」ことなので、そもそも資格情報が届いていないなら前提が変わる。

