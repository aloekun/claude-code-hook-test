# ADR-065: CI matrix による移植退行防止 — 両 OS で同一スイートを回す

## ステータス

試験運用 (2026-08-01 実装。required check 化は安定観測後)

> 本 ADR は 2026-07-04 策定のハーネス改善計画 WP-16 の決定を永続化したものである。
> 前提となる Linux 可搬性レイヤは [ADR-063](adr-063-linux-portability-release-binaries.md)。

## コンテキスト

WP-15 完了時点の CI は `release-binaries.yml` の 1 job のみで、以下の穴があった:

- **Linux only**: Windows 側で回るのはローカル `pnpm push` (push-runner) だけ。人が push する
  ときにしか動かず、PR 単位のゲートが存在しない。
- **master push only**: `pull_request` トリガーが無く、PR の段階では何も検証されない。
- **`--ignored` 統合テストが対象外**: jj を実際に spawn する経路が CI をすり抜ける。

一方、片 OS でしか検出できない欠陥は**両方向で実在が確認されている**:

- **Linux でしか出ない**: `cli-pr-monitor` の lock が同時取得を許すレース (8 スレッド中 6 つが
  取得)。Windows ではスケジューリング差で顕在化しなかっただけで欠陥は同じ
  ([ADR-063](adr-063-linux-portability-release-binaries.md) の副次発見)。
- **Windows でしか出ない**: シェル経由で渡す jj の revset 範囲指定 (`jj -r "<base>..@"`) は
  cmd.exe でクォートが除去されず失敗する。`sh` では通るため Linux 実行では気付けない。

さらに [ADR-063](adr-063-linux-portability-release-binaries.md) の残課題として、
`#[cfg(windows)]` ガードのテスト (`pump_child_io` の deadlock 保護、`run_cmd_capture` の
stdout/stderr 分離) は Linux 実行では skip される。CI が Linux のみだと、これらは
**CI で一度も実行されない**状態だった。

## 決定

### 1. `windows-latest` + `ubuntu-latest` の 2 leg matrix を PR ゲートとして新設する

`.github/workflows/ci.yml`。トリガーは `pull_request` / `master` への push /
`workflow_dispatch`。`fail-fast: false` とし、片方が落ちても他方を完走させる —
本 workflow の目的は「どちらの OS で壊れたか」の切り分けなので、巻き添えキャンセルは
必要な情報を失う。権限は `contents: read`、checkout は `persist-credentials: false`
(`pr-monitor.yml` / `release-binaries.yml` と同じ token 漏洩対策)。

### 2. 各 leg はローカル push-runner と同一のコマンドを回す

`cargo clippy --workspace --all-targets --all-features -- -D warnings` /
`cargo test --workspace` / `cargo test --workspace -- --ignored --test-threads=1`。

CI とローカルでコマンドが違うと、どちらかの緑が嘘になる。`--all-targets` により
test コードも clippy 対象になるため、片 OS でしかコンパイルされない `#[cfg(...)]` 配下の
未使用 import 等もここで露出する。`--ignored` を直列 (`--test-threads=1`) で回すのは、
これらが cwd を書き換えて相互干渉するため ([ADR-041](adr-041-test-isolation-patterns.md))。

### 3. `--ignored` のために jj を固定バージョンで導入し、版一致を fail-closed で検証する

統合テストは実 jj を spawn する。導入しなければこれらは CI をすり抜け、上記の
クォート事故のような jj 呼び出し経路の OS 差が land 前に検出できない。

版は 0.42.0 固定 ([ADR-011](adr-011-jj-push-new-bookmark-strategy.md) /
[ADR-015](adr-015-push-runner-takt-migration.md) /
[ADR-045](adr-045-jj-workspace-parallel-sessions.md) が 0.42 系の挙動に依存)。
**「入っているが別バージョン」は黙って進めない** — テストが緑でも本番の jj 挙動と一致
しなくなる最悪の形なので、`jj --version` の照合に失敗したら step を落とす
([ADR-043](adr-043-security-gates-fail-closed.md))。統合テストは commit を作るため
identity (`user.name` / `user.email`) も CI で明示設定する (未設定だと author が空になり
挙動が環境依存になる)。

なお jj のバージョンは **ローカル検証環境 / `scripts/cloud-setup.sh` / 本 workflow の
3 箇所に論理結合**している。上げるときは必ず 3 箇所を揃えること
([ADR-051](adr-051-cross-system-config-coupling.md))。

### 4. hooks smoke test は「既存 E2E の両 OS 実行」+「block 経路の新規テスト」で構成する

- 既存の exe-spawn E2E ([ADR-049](adr-049-incident-eval-regression-suite.md) の
  `incident_eval`、`hooks-stop-quality`、`hooks-stop-tool-call-leak`) は `cargo test` に
  乗っているため、matrix 化によって**そのまま両 OS 実行になる**。ADR-049 が確立した
  exe-spawn パターンをここで流用するという WP-16 の意図は、この形で満たされる。
- 一方、リポジトリで唯一「Claude の操作を実際に止める」hook である
  `hooks-pre-tool-validate` には exe-spawn テストが無く、stdin JSON parse → config →
  preset/protected → stderr + exit code の経路が無検証だった。
  `src/hooks-pre-tool-validate/tests/smoke.rs` を新設し、**block (exit 2) と pass (exit 0) を
  対で**固定する。assert は exit code と stderr の有無のみに限定し、block メッセージ本文は
  固定しない (ADR-049 と同じ方針。文言修正でテストが壊れない)。
- 不正な stdin が block 側に倒れないことも固定する。PreToolUse は全ツール呼び出しの前段に
  居るため、ここが exit 2 に倒れると Claude の操作が全面的に止まる。
- **exe は temp dir へ staging してから spawn する** (`t7_cwd_independence` と同方式)。
  この hook は config を `current_exe()` の隣から解決するため、`target/debug` の exe を
  直接叩くと**そこに残っている config に verdict が左右される** — 実際、開発機の
  `target/debug/` には過去の作業で置かれた `hooks-config.toml` が残っており、fresh clone の
  CI とは違う config で判定していた。staging により deploy 済 config での判定を固定し、
  同時に `target/debug` を汚して他 crate の exe-spawn テストへ干渉することも防ぐ。
- `cargo test --workspace` に含まれるが、run の一覧で独立に赤/緑が読めるよう CI では
  独立 step としても実行する (コンパイル済みのため追加コストはほぼゼロ)。

### 5. required check 化は段階を分ける

数 run 分の安定性 (実行時間・flake の有無) を観測してから Branch Protection の
Required status checks に登録する。未検証の新規 workflow をいきなり必須にすると、
プロダクトではなく CI 側の不備で全 PR が止まる。
[ADR-043](adr-043-security-gates-fail-closed.md) の fail-closed はゲート**関数**の
振る舞いに関する原則であり、ゲート自体の導入手順を一足飛びにする根拠ではない。

### 6. `release-binaries.yml` の clippy / cargo test は残す (重複は意図的)

あちらは「壊れたバイナリを rolling release に載せない」ための自己完結したゲートで、
`workflow_dispatch` や PR を経ない master push でも単独で成立する必要がある
([ADR-063](adr-063-linux-portability-release-binaries.md) の設計)。本 workflow に依存
させるとその保証が消える。public リポジトリの Actions は無料・無制限のため、
重複実行のコストは受け入れる。

## 検証記録 (実測)

- **jj 取得 step を両 OS で実走**: 実 URL・実 asset を使い、Linux (WSL Ubuntu 24.04、
  musl tarball → `find` → `install`) / Windows (PowerShell、msvc zip → `Expand-Archive` →
  `jj.exe` 探索) の双方で `jj 0.42.0-b8f7c455...` を取得・実行できることを確認。
  版照合 gate が通る形式であることも併せて確認した。
- **ローカル Windows**: `cargo test --workspace` (1,881 tests) / `cargo test --workspace --
  --ignored --test-threads=1` (20 tests) / 新規 `smoke` (2 tests) / `incident_eval` (2 tests)
  が pass。clippy `-D warnings` clean。
- **fresh checkout の擬似再現**: 開発機の `target/debug/hooks-config.toml` (過去の作業で
  残っていた成果物。CI の fresh clone には存在しない) を退避した状態で通常 / `--ignored` の
  両スイートを再実行し、いずれも pass することを確認した。ローカルだけで通る隠れた
  環境依存が無いことの実測的な裏づけ。
- **ubuntu leg の先取り実行 (WSL Ubuntu 24.04)**: CI と同一コマンドで
  clippy `-D warnings` clean / `cargo test --workspace` **1,707 pass, 0 failed**。
  Windows の 1,881 との差 174 は `#[cfg(windows)]` 群であり、両 OS の実行本数差が
  想定どおりであることも併せて確認した (差が説明できない = どちらかの leg が主題を
  検証していない、の検知)。
- **未観測**: GitHub Actions 上での実行は本 workflow を含む PR の run が初回であり、
  run 時間・cache 効率・flake の有無は未観測。§ 決定 5 の段階分けはこの事実に基づく。

## 帰結

### 利点

- PR の段階で両 OS の退行が止まる。従来 CI では一度も実行されていなかった
  `#[cfg(windows)]` テスト群 (`pump_child_io` の deadlock 保護等) が Windows leg で
  CI 実行対象になる。
- jj を CI に入れたことで、jj 呼び出し経路 (シェル引数の扱い・revset 指定) の OS 差が
  land 前に露出する。
- ローカル push-runner と同一コマンドのため、「ローカルは緑・CI は赤」の原因が
  コマンド差ではなく環境差に絞られる。

### 欠点 / 留意点

- `--ignored` を直列実行するため 1 leg あたりの所要時間が増える。cold cache の
  Windows leg が最長になる見込み。
- rust toolchain を pin していない (runner のプリインストール stable を使う)。
  runner 更新で新しい clippy lint が入ると、コード無変更で赤になり得る。
  `release-binaries.yml` が既に受け入れている性質と同じ。
- **jj アーカイブの取得に checksum 検証を付けていない** (受容したリスク)。jj の Release は
  チェックサム asset を公開しておらず、検証するにはハッシュを workflow へ直書きして
  バージョン固定の 3 箇所結合 (§ 決定 3) にもう 1 つ更新箇所を足すことになる。
  `scripts/cloud-setup.sh::install_jj` も同じ前提で、本 workflow で新たに開く穴ではない。
  `permissions: contents: read` かつ secrets を持たない job のため、影響範囲は使い捨ての
  非特権 runner に閉じる。checksum asset が公開されたら追随する。

### 副次発見: 理由が stale 化した `#[cfg(windows)]` ガード

本 ADR の PR (#342) で CodeRabbit が `hooks-stop-quality` の
`run_quality_steps_parallel_collects_failures_in_step_order` の `#[cfg(windows)]` 除去を
指摘し、**指摘が正しかった**。当該テストの step は `exit 0` / `exit 1` のみで、実行経路の
`run_cmd_shell_capped` は [ADR-063](adr-063-linux-portability-release-binaries.md) で
`shell_command` (cmd /c ↔ sh -c) に抽象化済み。ガードと「`cmd /c` 依存だから Windows 限定」と
いう doc コメントは、いずれも ADR-063 以前の記述が残ったものだった。実 Linux (WSL) で
ガード除去後に pass することを実測し、除去した。

留意すべきは、**ローカルの post-PR レビュー層はこれを「false positive」と判定していた**点
(理由として「`run_cmd_shell_capped` は cmd /c 依存」という、まさに stale なコメントの主張を
そのまま採用していた)。コメントが実装から乖離すると、レビュー層はその乖離を増幅する。
`#[cfg(...)]` ガードには**理由を書くだけでなく、その理由が今も成立するかを疑う**必要がある。
同型の点検として `t7_cwd_independence` のガード理由も実態 (再現対象の incident が
`.\.claude\probe.cmd` という cmd.exe 固有のルート相対パス解決そのもの) に書き直した
— こちらはガード自体が正当である。

### 残課題

- [ADR-063](adr-063-linux-portability-release-binaries.md) の残課題のうち
  **「Linux 上で `pump_child_io` の deadlock 保護と `run_cmd_capture` の stdout/stderr 分離が
  無検証」は本 ADR では閉じない**。matrix はこれらを Windows leg で CI 実行対象にするが、
  該当テストは cmd.exe / PowerShell を子プロセスに使うため module ごと Windows 限定であり、
  Linux 側の同等検証には POSIX 版テストの追加が必要になる。
- required check 化 (§ 決定 5) と、観測結果に基づく cache 戦略の調整。

## 関連

- [ADR-063](adr-063-linux-portability-release-binaries.md) — Linux 可搬性レイヤ / nightly
  release。本 ADR が閉じる穴の由来と、両 OS 実行の価値を裏づけた実例
- [ADR-049](adr-049-incident-eval-regression-suite.md) — exe-spawn E2E パターン
  (hooks smoke test の設計元)
- [ADR-041](adr-041-test-isolation-patterns.md) — `--ignored` を直列実行する理由
- [ADR-043](adr-043-security-gates-fail-closed.md) — jj 版照合を fail-closed にする根拠と、
  その原則を導入手順へ拡大解釈しない線引き
- [ADR-051](adr-051-cross-system-config-coupling.md) — jj バージョンの 3 箇所結合
- `.github/workflows/ci.yml` — workflow 本体
- `src/hooks-pre-tool-validate/tests/smoke.rs` — hooks smoke test 本体
