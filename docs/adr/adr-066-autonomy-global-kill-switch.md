# ADR-066: 自律実行の全体 kill-switch — 正極性単一フラグと「欠損 → 安全状態」原則

## ステータス

試験運用 (2026-08-02)

> 本 ADR は [ADR-052](adr-052-autonomy-execution-boundary-classes.md) 原則 5 が呼び手に課した契約（config opt-in + kill-switch の接続を自動実行可クラス有効化の前提条件とする）の実装である。[ADR-039](adr-039-experimental-feature-standard-pattern.md) の試験運用標準パターン（config opt-in + kill-switch + bounded lifetime）に従う。

## コンテキスト

### 問題: 契約はあるが実体が無い

ADR-052 は自律 actor が人間ゲート無しで実行してよい操作（自動実行可クラス）を定義し、その有効化条件として原則 5 で次を課した。

| 項目 | 契約（ADR-052 原則 5） |
|---|---|
| opt-in 既定 | 自動実行可クラスの有効化は config opt-in（既定 OFF）。未設定なら全操作をゲート必須扱い |
| kill-switch | 単一フラグ（リポジトリ内 config + CI variable）で全自律動作を即時停止 |
| 未接続 / 読み取り不能時の既定 | 背圧または kill-switch が未接続・読み取り不能なら自動実行可クラスを無効化しゲート必須へ倒す |
| 停止手順 | フラグを OFF にすると次の自律実行判定から無効化される |

しかしこの契約を満たす実体は存在しなかった。既存の kill-switch はすべて機能個別（`PR_MONITOR_SCOPE_GUARD_DISABLE` / `PR_MONITOR_GATE_DISABLE` / `POST_TAKT_REGATE_DISABLE` / `DOCS_ONLY_ROUTING_DISABLE` / `CLAUDE_TELEMETRY_DISABLE` / `CLOUD_HARNESS`）で、「全自律動作を止める単一フラグ」は無い。

一方で計画上の依存グラフは全体 kill-switch を最後尾（WP-19、WP-18 依存）に置いており、無人 fix push（WP-17）が先に来る順序だった。この順序のまま進むと ADR-052 原則 5 に違反した状態で自動実行可クラスが有効化される。2026-08-02 の着手前レビューでこの食い違いを検出し、全体 kill-switch を WP-17 の先頭 PR へ前倒しすることを決定した。

### 既存 kill-switch の棚卸しで判明したこと

着手時の設計案は「既存はすべて `*_DISABLE=1` で止める負極性。全体 kill-switch は『読めなければ止まる』ので極性が逆になる」というものだったが、棚卸しの結果この前提は不正確だった。本リポジトリには既に **2 つの家系**がある。

| 家系 | 例 | 極性 | フラグ不在時 |
|---|---|---|---|
| opt-in enable（ADR-039 標準） | `[fix.scope_guard] enabled`、`[telemetry] enabled`、`CLOUD_HARNESS=1`（ADR-060） | 正（存在 + truthy で有効） | OFF（安全側） |
| 緊急バイパス env | `PR_MONITOR_SCOPE_GUARD_DISABLE` 等 | 負（存在で停止 / skip） | 効果なし（opt-in 層の判定に従う） |

「config opt-in（正・既定 OFF）+ 緊急 env（負）」の複合が ADR-039 の標準そのものであり、「読めない = 許可しない」は `unwrap_or(false)` の opt-in 家系が既に実践している。全体 kill-switch は**新しい極性の発明ではなく既存 opt-in 家系への合流**である。

## 決定 (試験運用)

### 1. 統一原則は極性ではなく「欠損 → 安全状態」

フラグの極性を全機構で揃えることは目的にしない。守るべき不変条件は次の 1 行である。

> **入力の欠損・読み取り不能・解釈不能は、その機構の安全状態へ解決する。**

安全状態は機構の役割で決まる。ゲートの安全状態は「ゲート有効」（だから緊急バイパス env は不在時に効果なし）、自律 actor の安全状態は「停止」（だから本フラグは不在時に deny）。守るものが違うから安全方向が違うだけで、両家系ともこの不変条件を満たす。極性の混在はこの原則の下で整合しており、運用上の矛盾を生まない。

[ADR-043](adr-043-security-gates-fail-closed.md) の「fail-closed はゲート関数のみ、助言層は fail-open」とも整合する — 本フラグは自律実行の可否を決めるゲート関数側である。

### 2. 正極性の単一フラグ（AND 合成）

自律実行の許可は次の 2 拠点の **AND** で決まる。どちらか一方でも欠損・非 truthy なら停止。

| 拠点 | 実体 | 役割 |
|---|---|---|
| リポジトリ内 config | `autonomy-config.toml` の `[autonomy] enabled`（bool、既定 OFF） | リポジトリ側の意思表示。commit で追跡・レビューできる |
| 外部フラグ | env `AUTONOMY_ENABLED`。CI では Actions variable を workflow が env へ写す | 実行環境側の許可。CI では admin のみ書き込み可 |

ADR-052 契約表の 4 行（opt-in 既定 / kill-switch / 未接続時の既定 / 停止手順）はこの単一フラグで**すべて**満たされる。opt-in と kill-switch は別機構ではなく、同一フラグの既定状態と停止操作である。

**負極性（`AUTONOMY_KILL=1` で停止）は採らない。** 理由は 2 つある。

- **GitHub Actions の未定義評価**: `vars` コンテキストは未定義 variable を空文字列に評価する。負極性条件（`vars.AUTONOMY_KILL != '1'`）では変数の削除・typo・リポジトリ移行時の引き継ぎ漏れがすべて**無音で「許可」**に倒れ、ADR-052 原則 5 に正面から違反する。workflow 式は必ず未定義が停止側へ評価される向きで書く:

  ```yaml
  if: ${{ vars.AUTONOMY_ENABLED == 'true' }}  # 未定義 → '' → false → 停止（安全側）
  ```

  **設定値は `true`（小文字ちょうど）でなければならない**。この式は文字列の完全一致であり、`True` / `TRUE` / `1` / `yes` はすべて偽 = 停止側へ倒れる。安全側の失敗ではあるが「variable を設定したのに fix job が起動しない」の実際の原因になるため、設定時に確認すること（Settings → Secrets and variables → Actions → Variables）。なお exe 側（`cli-autonomy-gate`）の外部フラグ判定は `1` 等も真として受けるため**両層で受理する値が異なる**。厳しい側（workflow 式）に合わせて `true` を使う。

- **緊急停止の操作反射の統一**: ADR-060 の `CLOUD_HARNESS` は「変数を削除すれば止まる」を運用者に訓練済み。本フラグも削除 = 停止で同方向に揃う。負極性だと「削除しても止まらない、設定しないと止まらない」という逆反射が同居し、インシデント対応時の人為ミスに直結する。

### 3. 実行コンテキスト別の権威マッピング

`autonomy-config.toml` は checkout した内容がそのまま読めるため、**どの ref の写しを読むか**が信頼境界になる。

| コンテキスト | config の出所 | 外部フラグの出所 |
|---|---|---|
| GitHub Actions（Phase B 等） | **master ref から取り出した写し**。PR ブランチの checkout を使ってはならない | Actions variable `AUTONOMY_ENABLED`（admin のみ書き込み可） |
| ローカル自律 actor（夜間ループ等） | 作業ツリーの `autonomy-config.toml` | 実行環境の env |
| interactive セッション | 対象外（ADR-052 原則 1 により本フラグは自律 actor 限定。人間ゲートは ADR-028） | 対象外 |

CI で PR ブランチの config を読むと、自律 actor 自身（または prompt injection を受けた fix エージェント）が `claude/` ブランチ上で `enabled = true` に書き換えて自己解除できる（[ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) が塞いだ信頼境界と同型）。

`cli-autonomy-gate` は渡されたパスの出所を検証できないため、これは**呼び手の契約**である。履行の監査は allow / deny 行に必ず出力される `config=<実パス>` で行う。`--config` に既定値を設けず省略を引数不正としているのは、呼び手にパスの出所を必ず意識させるためである。

### 4. 純粋判定コアへの集約

判定は `src/cli-autonomy-gate/src/decision.rs` の `evaluate` 1 関数に閉じ込め、欠損 / ゴミ値 / false / true の全組み合わせを unit test で固定する（external 9 種 × repo config 3 種 × operation 2 種 = 54 組合せを走査し、「許可されるのは 3 条件が揃った場合だけ」を網羅的に確認）。既存の `gate.rs` の `gate_disabled` / `post_takt_regate` の判定コアと同スタイル。

**呼び手が env / config を直読みして独自に真偽を組み立てることを禁止する。** 片方の呼び手だけが `unwrap_or(true)` を書いた瞬間に無音で fail-open へ反転するためで、`lib-docs-policy` が ADR-035 の path 基準を単一実装へ集約しているのと同じ drift 防止である。呼び手はすべて `cli-autonomy-gate` の exit コードを経由する。

truthy の受理集合（`1|true|yes|on`、前後空白・大小無視）は `lib_telemetry::is_truthy` を共有する。再実装すると kill-switch ごとに truthy 解釈が drift する。

### 5. 背圧契約は操作クラス別

ADR-052 原則 5 は背圧の未接続も fail-closed 条件に含めるが、背圧の指標「未マージ draft 数」は draft PR 作成にしか意味を持たない。よって**背圧の接続は操作クラス別の前提条件**と定義する。

| 操作クラス | 背圧 | 現在の状態 |
|---|---|---|
| `fix-push`（既存 PR ブランチへの fix push） | cli-pr-monitor の有界 retry（`max_retries` 系） | 接続済み |
| `draft-pr`（draft PR 作成） | 未マージ draft 数の監視 | **未接続**（WP-18 で実装） |

`draft-pr` は kill-switch が両面とも有効でも常に deny する。これは未実装の placeholder ではなく現時点の正しい fail-closed 状態であり、「kill-switch だけ有効化して draft の山を積む」経路（ADR-052 原則 5 が警告するアンチパターン）を構造的に塞ぐ。WP-18 で背圧を実装する PR がこの判定を反転させる。

### 6. 判定タイミングは操作直前・毎回

run 冒頭で 1 回だけ判定するのではなく、各 commitment 操作（push / draft PR 作成）の直前に毎回呼ぶ。ADR-052 停止手順「フラグを OFF にすると次の自律実行判定から自動実行可が無効化される（既に起動済みの単一操作は対象外）」を満たすため、長時間 run の途中でフラグを倒しても次の操作境界で効く。

### 7. deny は loud（無音 no-op 禁止）

判定結果は必ず出力する。allow は stdout の `[AUTONOMY_ALLOW]`、deny は stderr の `[AUTONOMY_OFF]`。どちらも全ソースの状態と読み取り先 config パスを 1 行に含める。

```text
[AUTONOMY_OFF] operation=fix-push reason=external-unset config=autonomy-config.toml \
  AUTONOMY_ENABLED=unset repo_config=disabled backpressure(fix-push)=connected
```

deny 理由を 1 つに絞る `evaluate` と違い、状態行は 3 ソースすべてを出す。「フラグを 1 つ直したのにまだ止まる」の切り分けを 1 run の log だけで完結させるためである。deny は [ADR-055](adr-055-firing-telemetry-collection.md) テレメトリへも理由コードのみ記録する（config パス・env 生値は記録しない）。allow は firing ではないため記録しない。

ADR-060 の `CLOUD_HARNESS` は「無効時は無音」を選んだが、あれはローカル常時発火のノイズ対策という個別事情である。CI run 上の deny は「何もしなかった run」の原因切り分けに直結するため loud が正しい（[ADR-064](adr-064-monitor-success-positive-evidence.md) の silent-success 排除と同じ論理）。

### 8. exit コード契約

| コード | 意味 |
|---|---|
| 0 | 許可 |
| 1 | 拒否（ポリシー判定） |
| 2 | 引数不正 |

**呼び手は非ゼロをすべて拒否として扱う。** `1` だけを拒否とみなして `2` を通すと、引数を間違えた瞬間に fail-open する。この契約は `main.rs` の module doc にも永続記録する。

### 9. `autonomy-config.toml` を専用ファイルとする

`pr-monitor-config.toml` への section 追加ではなく独立ファイルにした。

1. cli-pr-monitor 専用ではない横断フラグである（Phase B / 夜間ループ / cloud routine が共有）。
2. CI では master ref の写しを読ませる必要があり、小さい専用ファイルの方が workflow 側の抽出が 1 パスで済み監査しやすい。
3. toml の parse 失敗は本 exe では fail-closed（自律停止）に倒れる。相乗りすると無関係な section の typo が自律動作を止めるため、blast radius を分離する。

派生プロジェクトへは配布しない（`templates/autonomy-config.toml` はコピー任意の参考）。ファイルが無ければ常に deny = 安全側の既定になるため、`deploy-hooks.ts` の「見つからなければ作成を促す」警告対象にもしない。

## フラグ台帳

混在を暗黙の慣習ではなく文書化された在庫にする。新しい switch を追加したら本表を更新すること。

| フラグ | 層 | 極性 | 未設定 / 読み取り不能時 | 停止操作 |
|---|---|---|---|---|
| `autonomy-config.toml` `[autonomy] enabled` | 自律ゲート | 正 | 停止（deny） | `enabled = false` |
| env / CI var `AUTONOMY_ENABLED` | 自律ゲート | 正 | 停止（deny） | 変数を削除 |
| `CLOUD_HARNESS`（ADR-060） | クラウド dispatcher | 正 | 全 hook no-op | 変数を削除 |
| `[telemetry] enabled`（ADR-055） | 観測 | 正 | 記録しない | `enabled = false` |
| `[fix.scope_guard] enabled`（ADR-054） | ゲート | 正 | 無効（opt-in 既定 OFF） | `enabled = false` |
| `PR_MONITOR_SCOPE_GUARD_DISABLE` | 緊急バイパス | 負 | 効果なし（opt-in 判定に従う） | 変数を設定 |
| `PR_MONITOR_GATE_DISABLE` | 緊急バイパス | 負 | 効果なし | 変数を設定 |
| `POST_TAKT_REGATE_DISABLE`（ADR-058） | 緊急バイパス | 負 | 効果なし | 変数を設定 |
| `DOCS_ONLY_ROUTING_DISABLE`（ADR-057） | 緊急バイパス | 負 | 効果なし | 変数を設定 |
| `CLAUDE_TELEMETRY_DISABLE`（ADR-055） | 緊急バイパス | 負 | 効果なし | 変数を設定 |
| `STOP_TOOL_CALL_LEAK_OVERRIDE`（ADR-053） | 緊急バイパス | 負 | 効果なし | 変数を設定 |
| `CLI_DOCS_LINT_DISABLE` | 緊急バイパス | 負 | 効果なし | 変数を設定 |

読み方: **正極性 = その機構の「有効化」を表し、未設定は安全側（停止 / 無効）**。**負極性 = 既に有効な機構の「緊急バイパス」を表し、未設定は現状維持**。負極性フラグは単独で機構を有効化できないため、未設定が「許可」に倒れる経路は存在しない。

## ADR-039 3 点セットの適用

| 項目 | 内容 |
|---|---|
| **Config opt-in** | `autonomy-config.toml` の `[autonomy] enabled`（既定 OFF）。ファイルを置かなければ OFF。本リポジトリでは PR 1 時点で `false`（呼び手が PR 2 で実装されるため、マージしても運用挙動が変わらない） |
| **Kill-switch** | 恒久停止は `enabled = false`。緊急停止は Actions variable `AUTONOMY_ENABLED` の削除。**単一フラグの停止操作であり、別建ての負極性 env は設けない**（決定 2） |
| **Bounded lifetime** | decision trigger: **Phase B（WP-17 PR 2）が稼働してから自律 fix push が発生した 3〜5 run で、(a) 有効時に意図どおり通ること、(b) いずれかのフラグを倒すと次の操作境界で止まること、(c) deny 理由が run log だけで切り分けられること、を確認したら本採用**（本 ADR の status 更新）。**2026-11-02 までに判定に至らなければ、自律化ロードマップの生死を判断して延長 / 却下する**。trigger の永続記録は本 ADR + `main.rs` module doc の 2 箇所 |

`scope_guard` では「ADR-039 の opt-in 既定 OFF」と「ADR-043 の fail-closed」が試験運用期間中は緊張関係にあった（既定 OFF = ガードが働かない）。本フラグにはこの緊張が無い — **既定 OFF がそのまま fail-closed 方向**であり、両原則が同じ向きを指す。

## 検証記録

PR 1 時点で実 exe による kill-switch drill 8 シナリオを実施し、全て設計どおりを確認した。

| # | 入力 | 期待 | 結果 |
|---|---|---|---|
| 1 | 外部フラグ未設定 / config false | deny `external-unset`、exit 1 | 一致 |
| 2 | 外部フラグ `1` / config false | deny `repo-config-disabled`、exit 1 | 一致 |
| 3 | 外部フラグ `maybe` / config false | deny `external-not-truthy`、exit 1 | 一致 |
| 4 | 外部フラグ `1` / config ファイル不在 | deny `repo-config-unavailable`、exit 1 | 一致 |
| 5 | 外部フラグ `1` / config true / `fix-push` | allow、exit 0 | 一致 |
| 6 | 外部フラグ `1` / config true / `draft-pr` | deny `backpressure-unavailable`、exit 1 | 一致 |
| 7 | `--config` 省略 | 引数不正、exit 2 | 一致 |
| 8 | `pnpm autonomy-status` | deny 行が全ソース状態付きで出る | 一致 |

unit test 21 件（判定コア 9 / sources 6 / 引数解析 6）。うち 1 件が 54 組合せの網羅走査。

### 実走観測 1 run 目（[ADR-067](adr-067-phase-b-unattended-fix-push.md) 段 2、2026-08-04）

Phase B の稼働により bounded lifetime の decision trigger 観測が始まった。**自律 fix push が発生した run は 1 回目**であり、本採用判定に要する 3〜5 run には達していない。

| trigger 項目 | 観測 | 判定 |
|---|---|---|
| (a) 有効時に意図どおり通ること | `AUTONOMY_ENABLED=true` + config `enabled=true` で `[FIX_PUSH_ALLOW] ... autonomy=allowed` → push 成立 | 充足 |
| (b) いずれかのフラグを倒すと次の操作境界で止まること | `AUTONOMY_ENABLED` を削除した状態で dispatch → **fix job 自体が skip**（workflow 式層で停止） | variable 側は充足。**config 側（`enabled = false`）の実走観測は未実施**（exe 単体は drill #2 で確認済み） |
| (c) deny 理由が run log だけで切り分けられること | 段 2 の 3 回目で `reason=empty-fix-diff` が 4 軸すべての状態とともに 1 行で出力され、原因が上流の `Apply fixes` にあると判別できた | 充足 |

§ 欠点の 1 点目に挙げた master ref 契約も実走で確認できた — gate 呼び出しの `--config master-ref/autonomy-config.toml` が run log に出ており、PR ブランチ側の同名ファイルは判定に使われていない。

## 帰結

### 利点

- ADR-052 原則 5 の契約が機械判定可能な実体を得た。Phase B（PR 2）は本 exe を呼ぶだけで契約を満たせる。
- 未定義 variable・config 欠落・parse 失敗・型違い・ゴミ値のすべてが停止へ倒れる経路を、1 つの純粋関数と 54 組合せテストで固定した。
- `draft-pr` の構造的 deny により、背圧（WP-18）より先に draft PR 自動作成が有効化される順序事故が起きない。
- フラグ台帳により、今後 switch を追加する際の極性判断が「どちらの家系か」の 1 問に還元された。

### 欠点 / 留意点

- **master ref 契約を exe は強制できない**。CI で PR ブランチの config を渡す誤りは workflow レビューと `config=` 出力の監査でしか捕捉できない。PR 2 で workflow 側に master ref 抽出を実装する際、この 1 行が信頼境界の要であることをコメントで明示する。
- **2 拠点 AND は有効化の手間を倍にする**。「config を true にしたのに動かない」は起こりうるが、状態行が 3 ソースすべてを出すため切り分けは 1 run で済む。
- 本 PR 時点で呼び手は無い（`pnpm autonomy-status` と drill のみ）。これは ADR-052 原則 5 が「自動実行可クラス有効化の**前提条件**」として kill-switch の先行を要求しているためで、意図的な順序である。

### 残課題

- PR 2（Phase B）で workflow 式（`vars.AUTONOMY_ENABLED == 'true'`）と exe 呼び出しの二層を接続し、master ref からの config 抽出を実装する。
- WP-18 で未マージ draft 数の背圧を実装し、`Operation::DraftPr` の `backpressure_connected` を反転させる。
- WP-19 の残り（自主減速・監査ループ）は本 ADR の範囲外。本フラグは「全停止」だけを担い、「いつ自主減速するか」は別機構が担う（ADR-052 原則 5 の役割分担）。

## 関連

- [ADR-052](adr-052-autonomy-execution-boundary-classes.md) — 自律実行境界の 2 クラス分類。本 ADR は原則 5 の契約の実装
- [ADR-039](adr-039-experimental-feature-standard-pattern.md) — 試験運用標準パターン。本フラグは opt-in 家系への合流
- [ADR-043](adr-043-security-gates-fail-closed.md) — fail-closed 原則。「欠損 → 安全状態」の上位根拠
- [ADR-060](adr-060-cloud-harness-sessionstart-dispatcher.md) — `CLOUD_HARNESS`。正極性 + 「変数削除 = 停止」の先行例で、操作反射を揃える相手
- [ADR-054](adr-054-prompt-injection-trust-boundary-defense.md) — prompt injection 信頼境界。master ref 契約が同型の穴を塞ぐ
- [ADR-055](adr-055-firing-telemetry-collection.md) — 発火テレメトリ。deny の記録先
- [ADR-064](adr-064-monitor-success-positive-evidence.md) — silent-success 排除。loud deny の論理的な親
- [ADR-028](adr-028-pnpm-create-pr-gate.md) — interactive セッションの人間ゲート。本フラグの対象外を定める境界
