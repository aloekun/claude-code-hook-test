//! 順位 323: timeout 時に**孫プロセスが本当に止まる**ことの検証。
//!
//! 経過時間の assert (`tests::rank323_grandchild_outliving_the_shell`) だけでは
//! tree kill の有無を判別できない — `join_within_grace` の上限があれば、孫が生きて
//! いても制御は戻るため。実際、変異テストで `kill_process_tree` を外しても経過時間
//! テストは素通りした。孤児が残るかどうかは**孫が仕事を続けられたか**でしか観測
//! できないので、孫自身にファイルを書き続けさせて判別する。
//!
//! **プローブが本当に動いていることを対照テストで固定する**。最初に書いた
//! 「孫が完了マーカーを書く」版は、ネストした `cmd` のクォートが崩れて完遂時にも
//! 何も書いておらず、変異テストで素通りした (テストが空振りしていた)。

use super::*;
use std::time::Duration;

/// 孫が書く tick の数。timeout (1s) より十分長く、kill されなければ全部書ける長さ。
const EXPECTED_TICKS: usize = 5;

/// 孫が生きていれば書き終えているはずの観測待ち時間 (tick 数より長く取る)。
const OBSERVE_AFTER_SECS: u64 = 8;

/// 各 tick に現れるロケール非依存のトークン。
///
/// Windows の `ping` は応答 1 行ごとに `TTL=` を含む (メッセージ本文は日本語化
/// されるが `TTL=` は変わらない)。Unix 側は同じトークンを明示的に出す。
const TICK_TOKEN: &str = "TTL=";

/// 孫プロセスが 1 秒ごとに出力を書き続けるコマンド。
///
/// **シェル自身ではなく孫に書かせる**のが要点。シェルを kill しただけでは孫は
/// 生き残って書き続ける = 孤児が残った証拠になる。
/// リダイレクトはシェルが開くが、書き込むのは孫。
///
/// リダイレクト先の引用について (PR #436 CodeRabbit Minor):
/// - **Unix**: `'...'` で囲み、パス中の `'` は `'\''` 方式で閉じ直す ([`sh_quote`])
/// - **Windows**: 引用**できない**。`shell_command` は `cmd /c <cmd>` を direct argv で
///   spawn するため、Rust の Windows 用引数エスケープが `"` を `\"` に変換し、
///   cmd.exe はそれを解釈しない。実測で `> "<path>"` にするとコマンドごと起動に
///   失敗した (0.11s で ping が 1 度も走らない)。よって囲まず、代わりに
///   [`assert_marker_path_is_shell_safe`] で前提を明示的に検査する
#[cfg(windows)]
fn orphan_probe_cmd(marker: &str) -> String {
    assert_marker_path_is_shell_safe(marker);
    format!("ping 127.0.0.1 -n {} > {}", EXPECTED_TICKS, marker)
}
#[cfg(not(windows))]
fn orphan_probe_cmd(marker: &str) -> String {
    format!(
        "(i=0; while [ $i -lt {} ]; do echo TTL=; sleep 1; i=$((i+1)); done) > {}",
        EXPECTED_TICKS,
        sh_quote(marker),
    )
}

/// POSIX sh の単一引用符で安全に囲む。`'` は一旦閉じて `\'` を挟み再び開く。
#[cfg(not(windows))]
fn sh_quote(raw: &str) -> String {
    format!("'{}'", raw.replace('\'', "'\\''"))
}

/// Windows のリダイレクト先を引用できない以上、パスに空白やシェル特殊文字が
/// 含まれていれば**テストは黙って空振りする**。それを事故として検出するための前提検査。
///
/// 黙って skip しないのは、空振りしたテストが「検証済み」に見えるのが本 PR で 2 度
/// 踏んだ失敗そのものだから。環境が該当したら loud に落として気付けるようにする。
#[cfg(windows)]
fn assert_marker_path_is_shell_safe(path: &str) {
    const SHELL_SIGNIFICANT: &[char] = &[' ', '"', '&', '|', '<', '>', '^', '(', ')', '%'];
    assert!(
        !path.contains(SHELL_SIGNIFICANT),
        "marker path にシェル特殊文字が含まれており、cmd.exe では引用できないため\n\
         リダイレクトが意図した先に書かれない (テストが空振りする): {:?}\n\
         TEMP を空白やメタ文字を含まないパスに設定して再実行してください",
        path,
    );
}

fn marker_path(kind: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "lib-subprocess-{}-{}.marker",
        kind,
        std::process::id()
    ))
}

fn tick_count(content: &str) -> usize {
    content.matches(TICK_TOKEN).count()
}

/// マーカーはバイト列として読む。`ping` の出力はロケール次第で非 UTF-8 になり、
/// `read_to_string` だと本 PR が直したのと同じ理由 (Err 時に buf を捨てる) で空に
/// なってしまう — テスト側が同じ罠を踏むと検証が無言で空振りする。
fn read_and_remove(marker: &std::path::Path) -> String {
    let bytes = std::fs::read(marker).unwrap_or_default();
    let _ = std::fs::remove_file(marker);
    String::from_utf8_lossy(&bytes).to_string()
}

/// 前提の対照: timeout させなければプローブは tick を全部書き切る。
/// これが落ちるならプローブが空振りしており、下の検証は無意味。
#[test]
fn the_probe_writes_every_tick_when_it_is_not_interrupted() {
    let marker = marker_path("probe");
    let _ = std::fs::remove_file(&marker);
    let cmd = orphan_probe_cmd(&marker.to_string_lossy());
    let (_ok, _out) = run_cmd_shell_unlimited("test", &cmd, 60);
    let content = read_and_remove(&marker);
    assert_eq!(
        tick_count(&content),
        EXPECTED_TICKS,
        "プローブが完遂時にも tick を書き切っていない (テストが空振り): {:?}",
        content,
    );
}

/// incident 再現: timeout 後に孫が仕事を続けられないこと。
#[test]
fn a_grandchild_stops_writing_after_the_timeout() {
    let marker = marker_path("orphan");
    let _ = std::fs::remove_file(&marker);
    let cmd = orphan_probe_cmd(&marker.to_string_lossy());

    let (ok, output) = run_cmd_shell_unlimited("test", &cmd, 1);
    assert!(!ok, "timeout should report failure: {:?}", output);

    std::thread::sleep(Duration::from_secs(OBSERVE_AFTER_SECS));
    let content = read_and_remove(&marker);
    assert!(
        tick_count(&content) < EXPECTED_TICKS,
        "孫プロセスが timeout 後も書き続けた (孤児が残っている): {:?}",
        content,
    );
}
