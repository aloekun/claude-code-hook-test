//! Subprocess utility helpers shared across CLI / hook crates.
//!
//! 順位 173a (todo11.md): combine_output extract — 5 crate horizontal duplication 解消の最初の sub-PR。
//! 順位 173b: wait_with_timeout を polling 系 2 variant (`_safe` / `_basic`) として抽出。
//! 順位 173c: drain_pipe を 3 variant (`_unlimited` / `_capped` / `_capped_reporting`) として抽出。
//! 順位 173d: run_cmd_shell を 2 variant (`_capped` / `_capped_reporting`) として抽出。
//! 2026-07-17: run_cmd_shell に `_unlimited` variant を追加 (出力を control flow 判定に
//! 使う callsite 用。ADR-044 層 2 = callsite ごとに intent が異なるため variant 維持)。
//! 3 variant の共通骨格 (spawn → drain → wait → combine) は `run_cmd_shell_with` に集約し、
//! 各 variant は drain 戦略の違いのみを表す。
//! ADR-026 Cargo workspace + ADR-012 lib-* naming に整合。
//!
//! 順位 173e で variant merge を検討予定。
//! `cli-pr-monitor/src/classifier_runner.rs` の channel-based wait_with_timeout と
//! `cli-pr-monitor/src/runner.rs` の `run_cmd_direct` (direct args / variant B) は
//! signature と設計意図が異なるため本 lib では別 variant として扱わず、必要なら 173e
//! で評価する。

use std::io::{BufRead, BufReader, Read};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// stdout と stderr を結合する。
///
/// 挙動:
/// - どちらか片方が空ならもう片方をそのまま返す
/// - 両方非空で stdout が改行で終わっている場合は separator を挿入しない (二重改行回避)
/// - 両方非空で stdout が改行で終わっていない場合は `\n` で連結
///
/// この `\n` suffix 吸収版は元 `hooks-post-tool-linter` で採用されていた頑健版。
/// 4 crate (cli-*) の basic 版とは `stdout.ends_with('\n')` のときに挙動が分岐するが
/// (basic="out\n\nerr" / robust="out\nerr")、production の呼び出し側はすべて
/// `drain_pipe` 経由で trailing newline を除去済の文字列を渡すため顕在化しない。
/// 既存 4 crate test も `\n` suffix case を含まないため全 case pass。
pub fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.ends_with('\n') {
        format!("{}{}", stdout, stderr)
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

const POLL_INTERVAL_MS: u64 = 100;

/// timeout / wait 失敗の経路で reader thread の回収に許す上限 (順位 323)。
///
/// [`kill_process_tree`] が成功していればパイプは即座に閉じるので、この猶予は
/// **tree kill が失敗したときの backstop** として効く。上限を設けないと、timeout の
/// 保証が「kill が成功すること」に依存した条件付きのものになる ([ADR-043])。
const JOIN_GRACE_MS: u64 = 500;

/// プロセスを**子孫ごと**強制終了する (順位 323)。
///
/// `shell_command` の child はシェルで、実際のコマンド (cargo / jj / ping) は**孫**に
/// なる。`Child::kill` はシェルしか殺さないため、孫はパイプの書き込み端を握ったまま
/// 生き残り、reader thread の `join()` が孫の自然終了までブロックする = timeout が
/// 事実上無効になる (実測: timeout 1s に対し制御が戻るまで 9.59s)。
///
/// 実装は外部コマンド経由で、`libc` 依存を増やさない (`check-ci-coderabbit` の
/// `kill_process_by_id` と同じ方針)。
/// - Windows: `taskkill /T` が子孫まで辿る
/// - Unix: [`shell_command`] が `process_group(0)` で child をプロセスグループの
///   リーダーにしているため、pgid == child の pid。負の pid でグループ全体へ送る
///
/// direct argv で spawn した child (シェルを挟まない callsite) に対しても安全に
/// 呼べる: Unix ではその child はグループリーダーではないので `kill -- -<pid>` は
/// 対象なしで失敗するだけ (pgid `<pid>` は「pid `<pid>` のプロセスがリーダーの
/// 場合」にしか存在せず、それは自分自身なので他人のグループを撃つことはない)。
///
/// kill 自体の失敗は握り潰す — 既に自然終了していれば失敗するのが正常。
pub fn kill_process_tree(pid: u32) {
    #[cfg(windows)]
    {
        let _ = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    #[cfg(not(windows))]
    {
        let _ = Command::new("kill")
            .args(["-9", "--", &format!("-{}", pid)])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// 子プロセスの終了を timeout 付きで待機する。Err 経路 (try_wait 失敗) で
/// child を kill + wait してから Err を返す **safe** variant。
///
/// 戻り値:
/// - `Ok(Some(status))`: 正常終了
/// - `Ok(None)`: timeout (child は kill + wait 済)
/// - `Err(msg)`: try_wait 失敗 (child は kill + wait 済)
///
/// timeout / try_wait エラーの両経路で [`kill_process_tree`] + `child.kill()` +
/// `child.wait()` を実施し、子プロセスと呼び出し側 reader スレッドが zombie 化するのを
/// 防ぐ。subprocess lifecycle の strictness を要求する callsite で使用する。
///
/// **両経路とも孫まで殺す** (PR #436 CodeRabbit Major): 本 variant は
/// `cli-push-runner` の diff stage が `shell_command` の child に対して使うため、
/// try_wait 失敗の経路でもシェルの孫が残り得る。timeout 側だけ孫を殺すと、
/// 「どちらの失敗経路でも child は残らない」という本 variant の売りが片肺になる。
pub fn wait_with_timeout_safe(
    label: &str,
    child: &mut std::process::Child,
    timeout_secs: u64,
) -> Result<Option<ExitStatus>, String> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_process_tree(child.id());
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            Err(e) => {
                kill_process_tree(child.id());
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("Failed to wait for {}: {}", label, e));
            }
        }
    }
}

/// 子プロセスの終了を timeout 付きで待機する。Err 経路 (try_wait 失敗) で
/// child を kill せずそのまま Err を返す **basic** variant。
///
/// 戻り値:
/// - `Ok(Some(status))`: 正常終了
/// - `Ok(None)`: timeout (child は kill + wait 済)
/// - `Err(msg)`: try_wait 失敗 (child は kill されない、呼び出し側で扱う)
///
/// timeout 経路のみ [`kill_process_tree`] + `child.kill()` + `child.wait()` を実施する。
/// try_wait 失敗時の child cleanup は呼び出し側が制御する設計の callsite で使用する。
/// `_safe` との 2 variant 並立は 173b 時点での保存対応で、merge 可否は 173e で判断する。
///
/// **Err 経路で孫を殺さないのは意図的** (PR #436 CodeRabbit Major への回答): cleanup を
/// 呼び出し側に委ねるのが本 variant の契約そのもので、ここで殺すと `_safe` との差が
/// 消える。シェル child を渡す唯一の呼び出し元 `run_cmd_shell_with` は Err を
/// [`kill_and_join_err`] で受けて孫まで殺しており、それ以外の呼び出し元はいずれも
/// direct argv (シェルを挟まない = 孫が生まれない) であることを確認済み。
pub fn wait_with_timeout_basic(
    label: &str,
    child: &mut std::process::Child,
    timeout_secs: u64,
) -> Result<Option<ExitStatus>, String> {
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_process_tree(child.id());
                    let _ = child.kill();
                    let _ = child.wait();
                    return Ok(None);
                }
                std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));
            }
            Err(e) => return Err(format!("Failed to wait for {}: {}", label, e)),
        }
    }
}

/// 子プロセスの stdout / stderr パイプを別スレッドで全量読込し、行末空白を trim した
/// 文字列を返す **unlimited** variant。
///
/// 全データを単一バッファに読み込むため出力サイズ制限なし。
/// JSON 等の構造化出力を pipe 経由で完全パースする callsite (例: check-ci-coderabbit
/// の JSON 受け取り) で使用する。出力過大時にメモリ消費が線形成長する点に注意。
///
/// **不正な UTF-8 は置換文字にする (`from_utf8_lossy`)**。旧実装は `read_to_string`
/// で読んでいたが、これは不正な UTF-8 に当たると `Err` を返し、その際 **buf を元の
/// 長さへ戻す** — つまり読めていた分も含めて**全出力が無言で消える**。本 variant は
/// doc が示すとおり「出力を control flow 判定に使う」callsite 用なので、その全損は
/// 判定の破綻に直結する (実測: Windows の `ping` (Shift-JIS 出力) で本関数は 0 バイト、
/// `drain_pipe_capped` は 444 バイトを返した)。capped 系は元から `from_utf8_lossy`
/// なので、本修正は 3 variant の挙動を揃えるものでもある。
pub fn drain_pipe_unlimited(
    pipe: impl Read + Send + 'static,
) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut reader = BufReader::new(pipe);
        let _ = reader.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).trim_end().to_string()
    })
}

/// 子プロセスの stdout / stderr パイプを別スレッドで最大 `max_lines` 行まで読込し、
/// 改行で結合した文字列を返す **capped** variant (silent truncate)。
///
/// `max_lines` 超過分は読み捨て (パイプバッファの排出は継続) されるため、超過行数は
/// 戻り値に反映されない。ログ表示用の callsite で使用する (cli-push-runner /
/// hooks-stop-quality 等)。
pub fn drain_pipe_capped(
    pipe: impl Read + Send + 'static,
    max_lines: usize,
) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut collected = Vec::with_capacity(max_lines);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < max_lines {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    }
                }
                Err(_) => break,
            }
        }
        collected.join("\n")
    })
}

/// 切り捨てた行数を報告する注記行を作る (`"... (N lines truncated)"`)。
///
/// 出力を「先頭 N 行 + 超過の明示」に切り詰める箇所で表記を揃えるための共有点。
/// 切り詰めの実装自体は共有できない (pipe を streaming しながら数える
/// `drain_pipe_capped_reporting` と、materialize 済み文字列を切る callsite では
/// 構造が異なる) が、**ログ上の見え方は一致させる**。
pub fn truncation_notice(truncated_lines: usize) -> String {
    let suffix = if truncated_lines == 1 { "" } else { "s" };
    format!("... ({} line{} truncated)", truncated_lines, suffix)
}

/// 子プロセスの stdout / stderr パイプを別スレッドで最大 `max_lines` 行まで読込し、
/// 超過があれば末尾に `"... (N lines truncated)"` を付与する **reporting capped** variant。
///
/// `drain_pipe_capped` と同じく行毎読込 + truncate だが、切り捨て行数を最終出力に反映する。
/// マージパイプライン等で「ログは確認したいが過大化は避けたい」 callsite で使用する
/// (cli-merge-pipeline、MAX_LINES=200)。
pub fn drain_pipe_capped_reporting(
    pipe: impl Read + Send + 'static,
    max_lines: usize,
) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut reader = BufReader::new(pipe);
        let mut collected = Vec::with_capacity(max_lines);
        let mut buf = Vec::new();
        let mut truncated = 0usize;
        loop {
            buf.clear();
            match reader.read_until(b'\n', &mut buf) {
                Ok(0) => break,
                Ok(_) => {
                    if collected.len() < max_lines {
                        collected.push(
                            String::from_utf8_lossy(&buf)
                                .trim_end_matches(&['\r', '\n'][..])
                                .to_string(),
                        );
                    } else {
                        truncated += 1;
                    }
                }
                Err(_) => break,
            }
        }
        if truncated > 0 {
            collected.push(truncation_notice(truncated));
        }
        collected.join("\n")
    })
}

/// Kills `child`, waits for it to be reaped, joins `stdout_handle` and `stderr_handle`,
/// then returns `(false, error)`.
///
/// Joining threads without first killing the child would block indefinitely if the child is
/// still running (the reader threads only break on pipe EOF, which arrives when the child
/// exits). Always kill before joining to avoid that deadlock.
fn kill_and_join_err(
    child: &mut std::process::Child,
    stdout_handle: JoinHandle<String>,
    stderr_handle: JoinHandle<String>,
    error: String,
) -> (bool, String) {
    kill_process_tree(child.id());
    let _ = child.kill();
    let _ = child.wait();
    let _ = join_within_grace(stdout_handle, stderr_handle);
    (false, error)
}

/// reader thread 2 本を **上限付き**で回収する (順位 323)。
///
/// [`kill_process_tree`] が成功していれば孫も死んでパイプが閉じるため、実際にはほぼ
/// 即座に返る。上限が要るのは kill が失敗し得るから (権限 / race / `taskkill` 不在):
/// 素の `join()` はその場合に孫の自然終了までブロックし、**timeout の保証が
/// 「kill が成功すること」に依存した条件付き**のものになる ([ADR-043])。
///
/// 猶予は 2 本で共有する ([`JOIN_GRACE_MS`] を全体の deadline とする)。1 本ずつ
/// 上限を与えると最悪 2 倍待つことになり、上限の意味が薄れる。
/// 回収できなかった thread は detach され、プロセス終了時に道連れになる。
fn join_within_grace(
    stdout_handle: JoinHandle<String>,
    stderr_handle: JoinHandle<String>,
) -> (String, String) {
    let deadline = Instant::now() + Duration::from_millis(JOIN_GRACE_MS);
    let stdout_rx = forward_join(stdout_handle);
    let stderr_rx = forward_join(stderr_handle);
    (
        recv_until(&stdout_rx, deadline),
        recv_until(&stderr_rx, deadline),
    )
}

/// `JoinHandle` を channel に載せ替えて、時間制限付きで待てるようにする
/// (`std` の `JoinHandle` に時限 join が無いため)。
fn forward_join(handle: JoinHandle<String>) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(handle.join().unwrap_or_default());
    });
    rx
}

fn recv_until(rx: &mpsc::Receiver<String>, deadline: Instant) -> String {
    let remaining = deadline.saturating_duration_since(Instant::now());
    rx.recv_timeout(remaining).unwrap_or_default()
}

/// コマンド文字列を OS のシェル経由で実行する `Command` を組み立てる (WP-15)。
///
/// Windows は `cmd /c <cmd>`、それ以外 (Linux / macOS) は `sh -c <cmd>`。
/// **リポジトリ内で「シェル経由の実行」を行う唯一の spawn 起点**であり、新たに
/// `Command::new("cmd")` を書き足してはならない (Linux 移植で無言に壊れるため)。
///
/// `sh` を選ぶ理由: quality gate / push / merge の step は `pnpm test` や
/// `cargo clippy ... -- -D warnings` のような単純なコマンド行であり、bash 固有構文
/// (配列 / `[[ ]]` / process substitution) を必要としない。POSIX sh に限定しておけば
/// bash 不在の最小コンテナ (クラウドの使い捨て環境) でも同じ経路が通る。
///
/// **cmd.exe と sh でシェル構文は互換ではない**点に注意 (`%VAR%` vs `$VAR`、
/// パス区切り、`&` の意味)。config に書く step の `cmd` は両者で解釈が一致する
/// 書き方 (プレーンなコマンド + `/` 区切り相対パス) に揃えること。
///
/// Unix では child を新しいプロセスグループのリーダーにする (`process_group(0)`、
/// 順位 323)。[`kill_process_tree`] が孫まで終了させるのに pgid を必要とするため —
/// Windows の `taskkill /T` に相当する「子孫を辿る」手段が Unix には無い。
/// 副作用として child は端末のジョブ制御から外れるが、本 crate の callsite は
/// すべて非対話 (gate / hook / pipeline) なので影響しない。
pub fn shell_command(cmd: &str) -> Command {
    #[cfg(windows)]
    {
        let mut command = Command::new("cmd");
        command.args(["/c", cmd]);
        command
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::process::CommandExt;
        let mut command = Command::new("sh");
        command.args(["-c", cmd]);
        command.process_group(0);
        command
    }
}

/// `run_cmd_shell_*` 3 variant の共通骨格 (spawn → drain → wait → combine)。
///
/// variant 間の差は `drain` (= どの `drain_pipe_*` を使うか) だけで、それ以外の
/// semantics — timeout メッセージ書式 / child の lifecycle / 戻り値の意味 — は
/// 3 variant で共通。stdout と stderr は型が異なる (`ChildStdout` / `ChildStderr`) ため、
/// 同一の drain 戦略を両方へ適用できるよう `Box<dyn Read + Send>` に統一して渡す。
///
/// child の lifecycle: 待機は `wait_with_timeout_basic` (= try_wait 失敗時に child を
/// kill しない basic semantics) を使うが、その Err 経路は本関数が `kill_and_join_err` で
/// 受け、child の kill + reap と reader thread の join を行う。timeout 経路は
/// `wait_with_timeout_basic` 自身が kill + reap する。結果として **どの失敗経路でも
/// child は残らない**。
///
/// ## 失敗経路で孫まで殺す理由 (順位 323)
///
/// 失敗経路では [`kill_process_tree`] で**子孫ごと**終了させ、reader thread の回収は
/// [`join_within_grace`] で上限付きにする。素朴に `child.kill()` + `join()` すると、
/// シェルの孫がパイプを握ったまま生き残って join がブロックし、timeout が事実上
/// 無効になっていた (実測: timeout 1s に対し 3 variant とも 9.59s)。
///
/// **採らなかった案: 失敗経路で join せず detach する。** 制御は確実に戻るが、
/// 孫は孤児として走り続ける。本 crate の callsite (quality_gate / push / merge) は
/// cargo や jj のような重いコマンドを起動するので、孤児が残ると次の実行と資源を
/// 奪い合い、`.failed` marker を残す orphan takt (#286) と同じクラスの実害を生む。
/// tree kill なら制御が戻る速さ**と**孤児の除去を同時に満たせるため、そちらを採った。
/// detach は「kill が失敗したら」の保険としてのみ [`join_within_grace`] に残っている。
///
/// **テストで固定していない経路**: `kill_process_tree` 自体が失敗したとき
/// ([`join_within_grace`] の猶予が効く経路) は回帰テストで固定できていない —
/// kill の失敗を決定論的に再現する手段が無いため。tree kill が成功する経路は
/// `orphan_tests` が、上限付き join の存在は本 doc が担保する。
fn run_cmd_shell_with<F>(label: &str, cmd: &str, timeout_secs: u64, drain: F) -> (bool, String)
where
    F: Fn(Box<dyn Read + Send>) -> JoinHandle<String>,
{
    let mut child = match shell_command(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return (false, format!("Failed to execute {}: {}", cmd, e)),
    };

    let stdout_handle = drain(Box::new(child.stdout.take().expect("stdout must be piped")));
    let stderr_handle = drain(Box::new(child.stderr.take().expect("stderr must be piped")));

    let exit_status = match wait_with_timeout_basic(label, &mut child, timeout_secs) {
        Ok(status) => status,
        Err(e) => return kill_and_join_err(&mut child, stdout_handle, stderr_handle, e),
    };

    let (stdout, stderr) = match exit_status {
        None => join_within_grace(stdout_handle, stderr_handle),
        Some(_) => (
            stdout_handle.join().unwrap_or_default(),
            stderr_handle.join().unwrap_or_default(),
        ),
    };
    let combined = combine_output(&stdout, &stderr);

    match exit_status {
        None => {
            let mut msg = format!("timed out after {}s", timeout_secs);
            if !combined.is_empty() {
                msg = format!("{}\n{}", msg, combined);
            }
            (false, msg)
        }
        Some(status) => (status.success(), combined),
    }
}

/// `shell_command` 経由で shell コマンドを実行し timeout 付きで結果を返す **silent capped** variant。
///
/// 戻り値: `(success, combined_output)`。
/// - 起動失敗 / try_wait 失敗 → `(false, error_message)`
/// - timeout → `(false, "timed out after Ns\n<combined>")`
/// - exit 正常 → `(status.success(), combined)`
///
/// 内部で `drain_pipe_capped(max_lines)` を使用するため stdout / stderr は `max_lines` 行で
/// silent truncate。**出力を control flow 判定に使う callsite で本 variant を使ってはならない**
/// (判定対象の行が cap の外に落ちても戻り値からは判別できず、無言で誤判定する)。
/// その場合は `run_cmd_shell_unlimited` を使う。
///
/// child の lifecycle は 3 variant 共通 (`run_cmd_shell_with` の doc を参照)。
pub fn run_cmd_shell_capped(
    label: &str,
    cmd: &str,
    timeout_secs: u64,
    max_lines: usize,
) -> (bool, String) {
    run_cmd_shell_with(label, cmd, timeout_secs, |pipe| {
        drain_pipe_capped(pipe, max_lines)
    })
}

/// `shell_command` 経由で shell コマンドを実行し timeout 付きで結果を返す **reporting capped** variant。
///
/// `run_cmd_shell_capped` と同 signature だが内部で `drain_pipe_capped_reporting(max_lines)`
/// を使用、stdout / stderr 超過時は末尾に `"... (N lines truncated)"` が追加される。
/// merge log 等で truncate されたか reviewer に明示したい callsite で使用する
/// (cli-merge-pipeline、MAX_LINES=200)。
pub fn run_cmd_shell_capped_reporting(
    label: &str,
    cmd: &str,
    timeout_secs: u64,
    max_lines: usize,
) -> (bool, String) {
    run_cmd_shell_with(label, cmd, timeout_secs, |pipe| {
        drain_pipe_capped_reporting(pipe, max_lines)
    })
}

/// `shell_command` 経由で shell コマンドを実行し timeout 付きで結果を返す **unlimited** variant。
///
/// `run_cmd_shell_capped` から `max_lines` を除いたもので、内部で `drain_pipe_unlimited`
/// を使用するため出力は truncate されない。**出力を control flow 判定に使う callsite**
/// (例: cli-push-runner の push stage が jj の "Refusing to ..." を検知する) はこの variant を
/// 使うこと。capped variant では判定対象の行が cap の外に落ちて silent failure になる。
///
/// 出力が過大な場合にメモリ消費が線形成長する点は `drain_pipe_unlimited` と同じ trade-off。
/// ログ表示量を絞りたい場合は、判定は全量に対して行い cap は表示側にのみ掛ける。
pub fn run_cmd_shell_unlimited(label: &str, cmd: &str, timeout_secs: u64) -> (bool, String) {
    run_cmd_shell_with(label, cmd, timeout_secs, drain_pipe_unlimited)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "non_utf8_tests.rs"]
mod non_utf8_tests;

#[cfg(test)]
#[path = "orphan_tests.rs"]
mod orphan_tests;
