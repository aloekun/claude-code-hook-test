# ADR-072: 夜間 todo 消化ループ — 無人実装から draft PR までの決定論経路

## ステータス

試験運用 (2026-08-06)

> [ADR-052](adr-052-autonomy-execution-boundary-classes.md) の自動実行可クラスのうち **draft PR 作成**を、[ADR-066](adr-066-autonomy-global-kill-switch.md) の kill-switch と [ADR-071](adr-071-draft-pr-backpressure.md) の背圧の上に実装する。無人 fix push ([ADR-067](adr-067-phase-b-unattended-fix-push.md)) の次の段で、**自律 actor が初めて「新しい成果物」を作る**経路になる。

## コンテキスト

[ADR-067](adr-067-phase-b-unattended-fix-push.md) の Phase B で、PC 電源オフ中でも PR イベントが処理され、docs 指摘なら無人 fix push まで到達する経路が成立した。ただし同 ADR § 欠点が指摘したとおり、**Phase B の実効価値は発火機会の少なさに縛られている** — 対象が既存 PR への docs 修正に限られるため、`claude/` ブランチの PR が存在しない限り何も起きない。

一方、`docs/todo-summary.md` には着手されないまま滞留するタスクが積み上がっている。そのうち「成功条件が `cargo test --workspace` で検証完結し、着手時の設計判断を含まない」ものは、人間が対話で補助しなくても完結しうる。この 2 つを繋ぐのが本 ADR の対象である。

### なぜ draft PR で止めるのか

[ADR-052](adr-052-autonomy-execution-boundary-classes.md) 原則 2 は、自律 actor が到達してよい境界を **commitment 点の手前**と定める。draft PR は「成果物は出来ているが、まだ誰も採用していない」状態で、ready 化とマージという 2 つの人間の操作が commitment 点として残る。

無人実装の品質を事前に保証する手段はない ([dev-conventions](../dev-conventions.md) § LLM を含む自動化経路は実走でしか検証できない)。保証できないなら、**間違っていた場合のコストを小さくする**方に設計を寄せる。draft PR を閉じるコストはクリック 1 回である。

### 実行主体を GitHub Actions にした経緯

cloud routine 案は [ADR-070](adr-070-weekly-review-cloud-routine.md) § 実現可能性の未検証点の実測で劣後した — routine の `jj git push` はローカル hook (`jj-push-guard`) に阻まれ、例外新設は「自律 push 経路の新設」として採用バーを超える。GitHub Actions は workflow step が push する Phase B と同構造でこの問題が発生せず、`claude/` prefix ブランチは ruleset 除外とも整合する。

## 決定 (試験運用)

### 1. 「何を実装するか」は Rust 分類関数が決める

`cli-nightly-task-select` が台帳 (`docs/claude-code-web-tasks.md`) の markdown table を解釈し、「無人可」マークの付いた行から 1 件を選ぶ。選択を LLM にも shell にも委ねない。

- **LLM に委ねない根拠**: [ADR-052](adr-052-autonomy-execution-boundary-classes.md) は「分類ロジックを Rust 分類関数を用意せず自律 actor の実行時 LLM 判断に委ねる」ことをアンチパターンとして明示している。何を実装するかは自律動作の起点であり、ここが揺れると下流のゲートがいくら堅くても「意図しないタスクを正しく実装した draft PR」が出てくる。
- **shell (awk/grep) に委ねない根拠**: markdown table の境界 (列ずれ・エスケープされたパイプ・無関係な表の混在) に回帰テストを書く場が無い。exe なら unit test で固定できる。

選択は**文書順の最初**とする。台帳の表が工数昇順に並んでいるためで、乱択や最新順にすると run ごとに選択が変わり失敗の再現ができなくなる。

### 2. 台帳の曖昧さはすべて停止側へ

台帳は人間が手で編集する markdown なので、列ずれ・順位の重複・未知のマーク表記が起こる。これらは読み飛ばさず **exit 2** で止める。読み飛ばした行が本来の選択対象だった場合、ループは黙って別のタスクを実装する。

exit コードは 3 種に分ける:

| code | 意味 | 後続 |
|---|---|---|
| 0 | タスクを選んだ | 実装 step へ進む |
| 2 | 引数不正 / 台帳の読み取り・解釈に失敗 | 進まない |
| 3 | 台帳は読めたが該当タスクが無い (正常な no-op) | 進まない |

2 と 3 を分けるのは run log で「何もすることが無かった」と「台帳が壊れている」を切り分けるためで、後続を動かさない点は同じ。**「無人可 列を持つ表が 1 つも無い」は 3 ではなく 2 とする** — 台帳の構成変更でループが静かに死ぬのを防ぐ。

### 3. 毎晩同じタスクを実装し直さない

台帳の行はタスクが**マージされるまで**残る。素朴に「無人可の先頭行」を選ぶと毎晩同じタスクを実装する。

ブランチ名に順位を埋め (`claude/nightly-<順位>`)、同名ブランチが存在する順位を `--exclude-ranks` で除外することで、決定論のまま解いた。

**除外の判定は PR の状態ではなくブランチの存在で行う。** open PR だけを見ると、draft PR がマージされずクローズされた順位が再び選択対象に戻る。ブランチは残っているため push が non-fast-forward で失敗し、**毎晩 agent を 1 回まるごと走らせて最後に落ちる無駄ループ**になる。採用率が低い場合にクローズが起きることは受け入れ基準 (§ 試験運用判断基準) が前提にしている運用なので、この経路は必ず踏む。

取得に `git ls-remote` を使うのは、一致なしでも exit 0 + 空出力になり「0 件」と「取得失敗」を取り違えないため。背圧の実測値 (未マージ draft 数) は別の指標なので、そちらは引き続き `gh pr list` で数える。

`--exclude-ranks` は**空でも省略できない**。空文字は「数えた結果 0 件」、フラグ欠落は「数えられなかった」で意味が違う。省略可能にすると `gh api` が失敗した run が「開いている draft は無い」と解釈して同じタスクを二重実装する ([ADR-071](adr-071-draft-pr-backpressure.md) § 決定 5 と同じ設計)。

### 4. 背圧ゲートは pre-flight と authority の 2 回呼ぶ

`cli-autonomy-gate --operation draft-pr` を、agent 起動前と push 直前の 2 箇所で同じ入力で呼ぶ。

- **pre-flight** は Max 枠の節約。背圧が飽和しているのに agent を起動すると、捨てることが確定した実装のためにサブスク枠を消費する ([ADR-071](adr-071-draft-pr-backpressure.md) § コンテキストの経済的根拠そのもの)。
- **authority** は push の権威。run 中に kill-switch が倒された場合はこちらで止まる ([ADR-066](adr-066-autonomy-global-kill-switch.md) の「停止は次の操作境界で効く」の実装)。

**2 回の呼び出しは同等ではない。** authority 側が読み直すのは kill-switch の 2 拠点 (`vars.AUTONOMY_ENABLED` と master ref の config) だけで、**未マージ draft 数は job 冒頭のスナップショットを使い回す**。実装 step は最大 60 ターン走るため、その間に別経路で draft PR が増えても authority gate は気づかない。結果として閾値を 1 件超えた状態で push が通りうる。

再計数しないのは、超過の実害が「レビュー待ちが 1 件多い」に留まる一方、authority gate の直前にネットワーク I/O を挟むと gate 自身が外部要因で落ちる経路を増やすため。**kill-switch は即時・背圧は run 単位**という粒度差として受け入れる。閾値超過が実運用で問題になれば再計数を入れる (§ 残課題)。

同じ純粋関数を同じ入力源で 2 回呼ぶだけであり、判定の出所は 1 つに保たれている。「背圧の状態を 2 箇所で持たない」([ADR-071](adr-071-draft-pr-backpressure.md) § 決定 2) と矛盾しない — 持っているのは呼び出し回数であって状態ではない。

### 5. agent には Bash を与えない。検証は workflow だけが行う

実装 agent のツールは `Read` / `Edit` / `Write` / `Glob` / `Grep` のみで、`Bash` / `gh` / `git` / `WebFetch` / `WebSearch` は `--disallowedTools` で明示的に落とす。[ADR-067](adr-067-phase-b-unattended-fix-push.md) の Phase B fix agent と同じ姿勢を取る。

当初は「テストを回せない状態で書かせるとコンパイルも通らない diff を毎晩作る」という理由で `Bash(cargo test:*)` / `Bash(cargo build:*)` / `Bash(cargo clippy:*)` を許していた。pre-push security review がこれを REJECT し、Bash を落とす形へ改めた。

#### レビューの指摘理由は誤りだった (2026-08-06 検証)

security review の主張は「**`--allowedTools` の `Bash(cmd:*)` は文字列の前方一致でシェルを解釈しないため `cargo test; curl ...` が許可を通過する**」というものだった。この前提は**公式ドキュメントで否定される**:

> Claude Code is aware of shell operators, so a rule like `Bash(safe-cmd *)` won't give it permission to run the command `safe-cmd && other-cmd`. The recognized command separators are `&&`, `||`, `;`, `|`, `|&`, `&`, and newlines. **A rule must match each subcommand independently.**

`--allowedTools` も同じルール体系に属する (「a managed settings deny can't be overridden by `--allowedTools`」)。したがって主張された経路は成立しない。

**それを検証せずに設計変更し、ADR の決定として記録したのは誤りだった。** この失敗は指摘の内容とは別に記録する価値がある — 静的レビューは*誤ることがある*のに、REJECT という形式が検証を省く圧力として働いた。さらにこの誤った前提のまま「同じ形が `pr-monitor.yml` にもある」と横展開の警告まで出しており、実在しない穴を production に見つけたと報告しかけた。

#### それでも Bash を落とす判断は維持する

理由は指摘されたものとは別で、こちらは**構造から言える**:

`cargo test` は `build.rs` とテストバイナリ、すなわち**その agent 自身が直前に書いたコード**を実行する。agent のターン中はプロセス env に action の資格情報が載っている。つまり agent に cargo を与えることは「自分で書いたコードを、資格情報のある環境で、いかなるゲートより前に実行させる」ことに等しい。

一方 `Verify deterministically` step は同じコードを実行するが `env:` を持たない。cargo の実行を agent のターンの外へ出すだけで、資格情報への露出は構造的に狭まる。

ただし **claude-code-action が子プロセスへ資格情報を渡すか自体は未検証**であり、実走スモークの観測項目に入れてある (§ 実走スモーク)。したがって現時点の判断は「**測るまでは保守側に置く**」であって、確定した脅威への対処ではない。スモークで露出が無いと確認できれば、Bash の再付与 (agent が自分の変更を検証できる利点) を再検討してよい。

代償として agent は自分の変更をコンパイル・テストできない。ブラインドで書くぶん失敗率は上がるが、失敗した run は `Verify deterministically` step で落ちて draft PR が作られないだけである。**間違った成果物を作るコストより、無駄な run 1 回のコストの方が小さい。**

**agent の自己申告は元から採否の根拠にしていない。** workflow step が `cargo test --workspace` と `cargo clippy --workspace --all-targets -- -D warnings` を回し直した結果を使う。この step は `env:` を持たないため、`work/` 側に仕込まれた `build.rs` が実行されてもトークンには到達しない。

### 6. ガードレールの自己改変は人間に見せる前に止める

draft PR は commitment 点の手前なので人間のレビューが最終防衛になるが、「自分を縛る仕組みを自分で書き換えた diff」だけは例外とし、決定論的な path 禁止リストで push を止める。

対象: `autonomy-config.toml` / `.github/workflows/**` / **`docs/claude-code-web-tasks.md`** / `src/cli-autonomy-gate/**` / `src/lib-autonomy-policy/**` / `src/cli-fix-push-gate/**` / `src/cli-nightly-task-select/**`。

**台帳 (`docs/claude-code-web-tasks.md`) を含めるのは、それが「次に何を実装するか」を決める仕組みだから。** config や gate exe と同じく*自分を縛る側*にある。agent が台帳を書き換えた diff が draft PR に紛れ、人間が見落としてマージすると、以後のタスク選択が静かに汚染される。初版はここを落としていた (§ 静的レビューが捕捉した件)。

**列挙の基準は「危険か」ではなく「将来の無人 run のゲートを緩めるか」である。** security review は `Cargo.toml` / `Cargo.lock` の欠落を warning として挙げたが（agent が追加した build-script 依存が Guard を素通りする）、**採らない** — これらは通常の diff であり、人間の PR レビューとマージという既存の防衛線が効く。一方リストに載せた 7 つは、書き換わると*それ以降の run*がゲートを経ずに走るようになる。この基準を持たないと禁止リストは「怪しいもの全部」へ膨らみ、正当なタスクを弾き始める。

**許可リストではなく禁止リストにした**のは、台帳の「対象ファイル」列が自由記述の markdown (バッククォート・括弧・複数パスの列挙) で、path allowlist に落とすと正当なタスクまで弾くため。allowlist 化は台帳を機械可読にしてからの課題とする (§ 残課題)。

空 diff も deny する — agent が実装を見送った場合と失敗した場合の両方をここで捕まえる。

禁止リストが構造的に見えない面 (`work/.git/`) については決定 9 を参照。

### 7. ゲート資産の改ざんを authority gate の実行前に検知する

決定 6 の禁止リストは `work/` の diff しか見ない。ところが実装 agent は `$GITHUB_WORKSPACE` 全体に `Read` / `Edit` / `Write` を持ち、そこには **`master-ref/`**（ゲート exe と `autonomy-config.toml` の調達元）も含まれる。プロンプトに書いた「対象は `work/`」は強制力を持たない。したがって次の自己解除経路が成立する:

1. agent が `master-ref/target/release/cli-autonomy-gate` または `master-ref/autonomy-config.toml` を書き換える
2. `Guard the guardrails` は `git -C work diff` しか見ないので検知しない
3. `Gate draft PR creation (authority)` が改ざん後の exe / config をそのまま push の権威として実行する

**これは本 workflow のヘッダコメントが「塞いだ」と主張している経路そのものである**（決定 1 の master ref 契約は、調達元が実行中に書き換わらないことを暗黙の前提にしていた）。

対処として、`Build deterministic gates from master` 段で `sha256sum` の基準値を採り、authority gate の直前で照合する。不一致なら gate を実行せずに停止する。基準値は `$RUNNER_TEMP` に置く — ワークスペース外であり、[ADR-067](adr-067-phase-b-unattended-fix-push.md) 段 2 で実測された「Claude Code は作業ディレクトリ外を読めない」制約が、ここでは防御側に働く。

**これは検知であって防止ではない。** より強い対処は `master-ref/` を agent のファイルシステムから完全に外す（別 job + artifact 受け渡し）ことだが、workflow を 2 job へ割る構造変更になる。まず検知で塞ぎ、実走スモークで agent の実挙動を観測してから分離の要否を判断する（§ 残課題）。

### 8. draft PR は App token で作る — CI を PR に紐づけるため

**`GITHUB_TOKEN` で作成した PR には CI が自動で走らない。** GitHub 公式ドキュメントの記述:

> When a pull request is created or updated by a workflow using `GITHUB_TOKEN`, `pull_request` events with the `opened`, `synchronize`, or `reopened` activity types create workflow runs that **require approval**.
>
> events triggered by the `GITHUB_TOKEN` will not create a new workflow run, with the following exceptions […] you can use a **GitHub App installation access token or a personal access token** instead of `GITHUB_TOKEN` to trigger events that require a token.

無人ループでこれは致命的である。夜間に作られた draft PR が「CI 未実施のままマージ待ちに見える」状態になり、人間が Approve を押すまで検証が始まらない。本プロジェクトは **Windows を主開発環境**とし、[ADR-065](adr-065-ci-matrix-cross-os-regression.md) の 2 OS matrix は初回観測期間に実バグを 1 件捕捉している。Windows 検証を人間の操作待ちにする設計は採れない。

#### PAT ではなく App を選ぶ理由

公式は App installation token と PAT を等価に挙げるが、本リポジトリでは **PAT を採れない**。[ADR-067](adr-067-phase-b-unattended-fix-push.md) 段 0 の ruleset `phase-b-backstop-restrict-non-claude-push` は `~ALL` ブランチの `creation` / `update` を禁止し `refs/heads/claude/**` を除外、bypass を **Repository admin ロール**にのみ与えている。オーナーの PAT はそのオーナーとして動くため admin 判定になり、**5 層目の防波堤を素通りする**。App installation は独立した actor で admin ではないため、backstop がそのまま効く。

#### 権限と発行タイミング

App の権限は **Contents: write / Pull requests: write / Metadata: read** のみ。**Workflows は付けない** — `.github/workflows/**` を含む push が権限側でも通らなくなり、決定 6 の禁止リストと二重になる。

installation token の発行は **publish の直前**に置く。寿命が 1 時間で、実装 agent は最大 60 ターン走るため、job 冒頭で取ると publish に届く前に失効しうる。

App token を使うのは push と PR 作成の 2 操作だけで、`gh pr list` による背圧の計数は `GITHUB_TOKEN` のままにする。読み取りに triggering の含意は無く、App token の露出面を広げる理由がない。

#### 副次効果: job の `GITHUB_TOKEN` から write を落とせた

App token の導入は資格情報を 1 つ増やす一方で、**job 自身の権限を縮められる**。push を App token が行うため、workflow の `permissions` は `contents: read` / `pull-requests: read` で足りる。

これは単なる最小権限の整理ではない。`GITHUB_TOKEN` は `claude-code-action` の `github_token` 入力として **agent (未信頼) が触れる唯一の GitHub 資格情報**であり、write を持たせたままだと「agent は push できない」という決定 6 / 7 の前提を token 側から崩しうる。read-only にすることで、その経路が権限層で閉じる。

前例は [ADR-067](adr-067-phase-b-unattended-fix-push.md) の Phase A (analyze job) で、`contents: read` のまま `claude-code-action` を本番運用できている。Phase B (fix job) が `contents: write` へ昇格しているのは workflow step 自身が `GITHUB_TOKEN` で push するためで、本 workflow はその必要がない。

つまり **App token の導入で、Phase B より弱い権限で同じことができる**構成になった。

#### 内部 Verify は品質保証ではない

この決定に伴い、workflow 内の `Verify deterministically` の役割を**コストフィルタ**として再定義する。ubuntu 単独・`--ignored` 無し・hooks smoke 無しで `ci.yml` の真部分集合でしかなく、目的は「コンパイルも通らない変更で draft PR を作り、背圧の枠と人間のレビュー時間を浪費しない」ことに限る。

**品質の保証は draft PR に紐づく `ci.yml` が担う。** 2 OS matrix を本 workflow へ複製しないのは、検証の定義が 2 箇所へ分かれて drift するため — 単一の真実源は `ci.yml` 側に置く。この区別は workflow のコメントと PR 本文の両方に明記する (読み手が「workflow が検証済み」を品質保証と誤読しないため)。

### 9. git 操作は agent が触れていない作業ツリーで行う

決定 6 の禁止リストは `git diff --cached --name-only` で実装しており、**`.git/` 内部を構造的に見ない**。一方 `git add` / `commit` / `push` は多数の設定経路から外部プログラムを起動する — `core.hooksPath`、`filter.*`、`core.fsmonitor`、`credential.helper`、`core.sshCommand`、`diff.*.textconv`、`core.pager` など。agent は `$GITHUB_WORKSPACE` 全体に Write を持つため `work/.git/` を書き換えられる。

**刺さる先は決定 8 で導入した App token である。** これらが発火するのは `Push branch and open draft PR` step で、そこは Contents:write / PR:write を持つ live な資格情報が env にある唯一の場所になった。CI を PR へ紐づけるために置いた資格情報が、そのまま最も価値の高い標的になっていた。

#### 危険なキーの列挙では終わらない

最初の対処は deny-list だった (`core.hooksPath` / `filter.*` を検知 + `.git/hooks` の非 sample ファイルを検知)。しかし**次のレビューが `alias.*` の欠落を指摘し、その次に `core.fsmonitor` / `credential.helper` が挙がった**。2 回連続で、著者ではなくレビュアーが漏れを見つけている。git の設定空間は広く、列挙で追随する限りこの往復は終わらない。

なお指摘のうち `alias.*` の想定経路 (`alias.push` が組み込みの `push` を差し替える) は**成立しない**。git 公式ドキュメント:

> To avoid confusion and troubles with script usage, aliases that hide existing Git commands are ignored except for deprecated commands.

指摘の具体例は誤りだったが、**指摘が示した「列挙が漏れる」という構造は正しい**。誤りの部分だけを見て退けると、正しい部分を捨てることになる。

#### 構造で断つ

`publish/` を **Implement ステップの終了後に新規 clone** し、agent の成果物は**作業ツリーのファイルだけ**を `rsync -a --delete --exclude '.git/'` で運ぶ。以後の `add` / `diff` / `commit` / `push` はすべて `publish/` に対して行う。

agent はそのターンが終わった時点で何も書けないので、**その後に作られた `.git` は定義上 agent の手が届かない**。危険なキーを 1 つも列挙せずに、クラスごと消える。

clone は shallow にしない。新規ブランチの push で shallow update が拒否される経路を避けるためで、[ADR-067](adr-067-phase-b-unattended-fix-push.md) § 残課題が未検証としていた懸念をここでは持ち込まない。

#### clone 先は master ではなく agent の base commit へ固定する

`publish/` を素朴に `--branch master` のまま使うと、**この決定自体が新しいバグを生む**。`work/` の checkout から `publish/` の clone までの間に実装 agent (最大 60 ターン) と `Verify` が走るため、その間に master が進みうる。進んだ master の上へ agent の古い作業ツリーを `rsync --delete` で被せると、**その間に master へ着地した無関係な変更を静かに revert / delete する**。

そこで agent 起動前に `git -C work rev-parse HEAD` を記録し、clone 後に `git -C publish checkout <その sha>` で base を揃える。commit の親が agent の base になるので、PR の merge-base も正しく解決し、diff には agent の変更だけが出る。

この欠陥は pre-push simplicity review が捕捉した。**セキュリティのために入れた構造変更が、別の正しさを壊しうる**という実例で、決定 9 の対処が「clone するだけ」では足りない理由でもある。

**この決定は決定 7 とは非対称である。** `master-ref/` は「実行する exe と config」なので作り直せず sha256 照合 (検知) にとどまるが、`publish/` は「これから作る成果物の入れ物」なので作り直せる。作り直せるものは検知ではなく**排除**する。

### 10. 「設計された停止」と「インフラ障害」を run の色で区別する

停止点のうち **設計上の正常な結末**（背圧 deny / 該当タスク無し / guard deny / 空 diff）は `continue-on-error` で受け、green + `[NIGHTLY_SKIP]` として終える。一方 **インフラ障害**（`gh` や network の失敗、clone 失敗）は `continue-on-error` を付けず red のまま落とす。

両者を同じ扱いにすると、run 一覧から「本当に壊れた夜」と「何もすることが無かった夜」の区別が消える。毎晩回る無人ループでは、この 2 つが混ざった時点で run 一覧が読まれなくなる。

| 結末 | 色 | 根拠 |
|---|---|---|
| 背圧 deny / タスク無し / guard deny / 空 diff | green + `[NIGHTLY_SKIP]` | 設計された結末。毎晩起こりうる |
| `gh` / network / clone の失敗 | **red** | インフラ障害。設計された結末ではない |
| **ゲート資産の改ざん検知 (決定 7)** | **red** | 「何かがゲートを無効化しようとした」— この系が出しうる最も大きい信号 |

改ざん検知を red にするのは初版で落としていた。`continue-on-error: true` + 下流の `if: steps.integrity.outcome == 'success'` で push は止まる (fail-closed は成立している) が、**green で終わるため run 一覧上は「何もすることが無かった夜」と区別が付かない**。決定 10 を書いたことで、その分類にこの結末が入っていないことが露出した (§ 静的レビューが捕捉した件 #10)。

具体的には `Count open claude/ drafts and in-flight ranks` に `continue-on-error` を**意図的に付けていない**。この step が失敗するのは gh API か network の問題であって、設計された結末ではない。なお `Report outcome` は `if: '!cancelled()'` なので red の場合も 1 行サマリは出る — 診断情報は失われない。

pre-push simplicity review はここを「他の停止点と同様に graceful degradation すべき」と指摘したが、上記の理由で**現状を維持する**。指摘が再発しないよう決定として記録しておく。

## 試験運用判断基準 (ADR-039)

| 項目 | 内容 |
|---|---|
| **Config opt-in** | 3 つの独立した条件がすべて要る: Actions variable `AUTONOMY_ENABLED` = `'true'`、`autonomy-config.toml` の `[autonomy] enabled = true`、同 `max_open_draft_prs` の設定。どれか 1 つでも欠ければ draft PR は作られない |
| **Kill-switch** | 夜間ループだけ止めるなら `max_open_draft_prs = 0`。全自律動作を止めるなら `enabled = false` または Actions variable の削除。**新しい停止操作を増やしていない** — 既存 2 拠点 + 背圧の閾値だけで止まる |
| **Bounded lifetime** | decision trigger: **2 週間の試験運用で無人 draft PR の採用率 (人間がマージした割合) を測定し、50% 超で継続・拡大、未満なら対象クラスを絞って再試行**。あわせて (a) 選択が意図どおりの順位に当たること、(b) ガードレール禁止リストが誤検知しないこと、(c) 背圧飽和で実際に停止すること、を観測する。**2026-11-06 までに判定材料が集まらなければ延長 / 却下を判断する** |

採用率の測定は weekly-review の自律アクション棚卸し (WP-19 ステップ 3) に載せて仕組み化する。

## 検証記録

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

### 実走スモーク — **未実施**

**本 ADR の受け入れ基準のうち実走スモークは、この PR の時点では満たしていない。** 実行には (a) 本 workflow が master に存在するか対象ブランチ ref へ `workflow_dispatch` できること、(b) 台帳に無人可マークがあること (= WP-18 PR 2 のマージ) の両方が要る。

反復は [ADR-067](adr-067-phase-b-unattended-fix-push.md) § 段 2 の知見 2 に従い、**マージせずブランチ ref への `workflow_dispatch`** で行う。`dry_run` 入力でゲート通過まで走らせて push を止められるようにしてある。

スモークで同梱観測する項目:

| 観測項目 | 出所 |
|---|---|
| WP-17 残課題: Phase B の自動起動経路が成立するか | [ADR-067](adr-067-phase-b-unattended-fix-push.md) § 検証記録 |
| WP-17 残課題: `coderabbitai[bot]` allowlist の要否 | 同上 |
| **`cargo` サブプロセスから `CLAUDE_CODE_OAUTH_TOKEN` / `GITHUB_TOKEN` が見えるか** | pre-push security review の warning |
| 決定 7 の照合が実 runner 上でも通ること (誤検知で毎晩止まらないこと) | 本 ADR § integrity 機構の drill |
| **App token で作った draft PR に `ci.yml` の 2 OS run が紐づくこと** | 決定 8 (仕様は公式で確認済み、実環境での成立は未観測) |
| Actions variable `AUTONOMY_ENABLED` が `true` ちょうどで設定されており job が起動すること | ADR-066 § 決定 2 (完全一致要件) |
| `claude/nightly-*` の **ref 作成**が App token で通ること (ruleset の除外が creation にも効くこと) | ADR-067 段 0 の ruleset。Phase B が観測したのは既存ブランチへの update のみ |
| `publish/` の clone + rsync が実 runner で成立し、`work/` の変更が過不足なく運ばれること | 決定 9 (`--delete` による削除の反映を含む) |

3 行目は決定 5 で Bash を落とした後も残る経路の確認である。agent は `cargo` を直接叩けないが、`work/` へ書いた `build.rs` は `Verify deterministically` step の `cargo test` が実行する。同 step に `env:` を置いていないことがトークン非露出の根拠なので、**その前提が実 runner で成立するか**を使い捨ての `build.rs` から `env | grep -i token` を出して実測する。

この観測は 2 つの判断に効く。露出があれば `Verify` step の env に明示的な scope 制限が要る。露出が無ければ、決定 5 で保守側に置いた **Bash の再付与 (agent が自分の変更を検証できる利点) を再検討してよい** — 決定 5 の根拠は「資格情報のある環境で agent の書いたコードを走らせない」ことなので、そもそも資格情報が届いていないなら前提が変わる。

## 帰結

### 利点

- Phase B の発火機会の少なさ ([ADR-067](adr-067-phase-b-unattended-fix-push.md) § 欠点) が解消される。夜間ループが `claude/` ブランチの PR を作り始めれば、Phase B はその PR の CodeRabbit 指摘を拾って動く。
- 滞留タスクの消化が人間の着手時間から切り離される。失敗しても draft PR を閉じるだけで済む。
- 停止操作を増やしていない。緊急時の反射は [ADR-066](adr-066-autonomy-global-kill-switch.md) から変わらない。
- 選択が決定論なので、失敗した run を同じ入力で再現できる。

### 欠点 / 留意点

- **禁止リストは許可リストより弱い**。列挙し忘れたガードレールは守られない。台帳を機械可読にして allowlist へ移行するまでの過渡的な設計である。
- **agent は自分の変更を検証できない**。決定 5 で Bash を落としたため、コンパイルも通らない diff を書く run が一定数出る。無駄な run 1 回で済む設計にしてあるが、採用率 (§ 試験運用判断基準) はこのぶん下がる。
- **`work/` に書かれた `build.rs` は `Verify` step で実行される**。agent 自身は Bash を持たないが、`cargo test` はビルドスクリプトを走らせる。同 step は `env:` を持たないためトークンには到達せず、`master-ref/` への書き込みは決定 7 の照合が後段で捕捉する (照合を verify の**後**に置いてあるのはこのため) が、ネットワーク送信自体は塞いでいない。public リポジトリの内容しか送れないことを受容の根拠としている。
- **agent の file tools が `work/` へ scope されていない**。`--allowedTools` は `Read,Edit,Write,Glob,Grep` を無制限に与えており、`work/` 限定はプロンプトの文言にすぎない。決定 7 の照合はゲート資産の改ざんを**検知**するが、`master-ref/` への書き込み自体を**防止**しない。`cargo test` の build script 経由で書き込む経路も同様に検知側で受けている (照合は verify step の後に置いてある)。
- **App token の秘密鍵が新しい保護対象になった**。`NIGHTLY_APP_PRIVATE_KEY` が漏れると、`claude/**` への push と PR 作成が任意に行える (ruleset により `claude/**` 以外へは push できず、マージもできない)。GITHUB_TOKEN より寿命の長い資格情報をリポジトリに置くことになる点は、CI を PR へ紐づける対価として受け入れた。
- **App token の導入は publish step を最も価値の高い標的に変えた**。以前はどの step も write 資格情報を持たなかったが、いまは publish step の env に live な Contents:write / PR:write がある。決定 6 § `work/.git/` の hook / filter 検査はこの step で発火しうる実行面を塞ぐために置いたもので、**資格情報を足すと、その step で何が実行されうるかを洗い直す必要がある**という一般則の実例になっている。
- **pr-monitor の Phase A は夜間 draft PR で自動起動しない可能性がある**。決定 8 で `ci.yml` は走るようになったが、Phase A の起動条件は `issue_comment` / `pull_request_review` であり、CodeRabbit のコメントが来て初めて起動する。CodeRabbit は GitHub App なので App token 作成の PR にも反応するはずだが、これは実走で確認する。
- **採用率 50% は根拠のある閾値ではない**。「半分が使い物にならないなら対象クラスの選び方が間違っている」という直感でしかなく、2 週間のサンプル数 (最大 14 件、背圧で実際にはもっと少ない) では統計的な意味を持たない。

### 残課題

- **実走スモークの完走** (§ 検証記録)。本 ADR の受け入れ基準の中核であり、未実施。
- **外部設定 (GitHub App / repository variables・secrets) の実体が未記録**。決定 8 は「なぜ App token か」を厚く残す一方、App 名・インストール範囲・付与権限の実際・`NIGHTLY_APP_ID` (variable) / `NIGHTLY_APP_PRIVATE_KEY` (secret) / `AUTONOMY_ENABLED` (variable) の登録先と欠落時の倒れ方を 1 行も書いていない。[ADR-051](adr-051-cross-system-config-coupling.md) が内部設定と外部 SaaS 設定の論理結合に課す 3 点 (相互参照コメント / 期待値の組み合わせ表 / 両側同一 PR) が未実施の状態にあたる。実走スモークで GitHub UI を触る際に**設定メタデータ** (名前・登録先の別・付与権限のスコープ・所有者・ローテーション方針・欠落時の挙動) を確認し、§ 外部設定の実体 として本 ADR へ追記する。**秘密値そのもの (`NIGHTLY_APP_PRIVATE_KEY` の鍵本文や発行済み token) は ADR にも git 履歴にも残さない** — ADR-051 が記録を課すのは「結合の存在」と「期待値の組み合わせ」であって、秘密の実値ではない。
- **`master-ref/` を agent のファイルシステムから外す**。決定 7 は検知どまりで、防止には別 job + artifact 受け渡しへの構造変更が要る。実走スモークで agent が実際にワークスペース外へ手を伸ばすか観測してから判断する。
- **authority gate の直前で draft 数を再計数するか**。現状は job 冒頭のスナップショットを使い回す (§ 決定 4)。閾値を 1 件超えて push される事象が実運用で観測されたら入れる。
- **ガードレール禁止リストの allowlist 化**。台帳の「対象ファイル」列を機械可読にする (別列に正規化パスを持つ等) のが前提。
- **禁止リストが YAML に埋まっている**。`cli-nightly-task-select` や専用 exe へ移せば unit test で固定できるが、現状は workflow step の `grep` で、回帰テストが無い。リストが育つようなら extract する ([ADR-044](adr-044-subprocess-utility-extraction-boundary.md) 層 1 の判断基準に従う)。
- **失敗した run の学習が無い**。同じタスクで 3 晩失敗しても 4 晩目に同じことを試す。連続失敗の検出と自動除外は WP-19 ステップ 3 の監査ループで扱う。
