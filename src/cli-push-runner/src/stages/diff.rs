//! Diff stage — `[diff] command` の出力を reviewers 用ファイルに書き出す。
//!
//! 出力は takt の reviewers が Read で参照するレビュー対象そのもののため、
//! **切り詰めない** (`run_diff_cmd` の doc)。実行は timeout 付き (T6)。
//!
//! ## レビュー範囲は PR 全体
//!
//! `[diff] command` の [`DIFF_PR_RANGE_PLACEHOLDER`] は `Config::diff_pr_range()`
//! (= `<base>..@`) に展開される。以前は config に `jj diff -r @` と直書きされており、
//! **tip コミットしかレビュアーに渡らなかった**。祖先コミットは AI レビューを一度も
//! 経ずに merge され、しかもレビュアー側からは「渡された diff が PR 全体か」を
//! 検証できないため誤りが誰にも検知されなかった (todo 順位 288、Severity High で
//! PR #268/#300/#301/#311 と 4 回再発)。
//!
//! 範囲の直書きを禁じるだけでは派生プロジェクトの古い config を救えないため、
//! [`verify_diff_covers_pr_range`] が生成された diff と PR 範囲の変更ファイル集合を
//! 突き合わせ、不足があれば fail-closed で中断する。

use std::path::Path;
use std::process::Stdio;

use lib_subprocess::{drain_pipe_unlimited, shell_command, wait_with_timeout_safe};

use crate::config::{DiffConfig, DEFAULT_DIFF_TIMEOUT_SECS, DIFF_PR_RANGE_PLACEHOLDER};
use crate::log::log_stage;

#[derive(Debug, PartialEq)]
pub(crate) enum DiffResult {
    /// diff に内容があり、ファイルへの書き出しが完了した
    HasContent,
    /// diff 出力が空 (レビュー対象なし、push は続行可能)
    Empty,
    /// diff コマンドの実行またはファイル書き出しに失敗した
    Error,
}

/// diff 取得専用: 出力を切り詰めず、stdout / stderr を分離したまま timeout 付きで取得する。
///
/// 戻り値: `Ok(stdout)` / `Err(stderr | timeout メッセージ | 起動失敗メッセージ)`。
///
/// **stdout と stderr を結合しない**のが本関数の要件で、`lib_subprocess::run_cmd_shell_*`
/// (全 variant が `combine_output` で結合する) を使えない理由でもある。stdout は
/// reviewers が読む diff そのものとしてファイルに書かれるため、jj が stderr に出す警告
/// (並列 workspace 運用時の `Concurrent modification detected` 等) が混入すると
/// レビュー対象を汚す。読み取り戦略は cap なし (diff は全量が必要) で、shell 経由なのは
/// `[diff] command` が config 由来の文字列だから。同型の「全量 + 分離 + timeout」は
/// `bookmark_check::run_jj_bookmark_list` にもあるが、そちらは direct args で
/// signature が非互換のため共通化しない (ADR-044 層 1)。
///
/// timeout (T6): 旧実装は `Command::output()` で**無限待ち**だった。ADR-045 の並列
/// workspace 運用で jj の lock 競合が起きるとパイプラインが無言ハングする
/// (他 stage は全て timeout 付きで、diff だけが穴だった)。timeout 時は Err を返し、
/// 呼び出し側が `DiffResult::Error` = exit 5 で中断する (fail-closed / ADR-043)。
///
/// child の lifecycle: timeout 経路・try_wait 失敗経路とも `wait_with_timeout_safe` が
/// child を kill + reap する (`_basic` ではなく `_safe` を選ぶ理由 = ADR-044 層 2)。
///
/// **child を kill した 2 経路 (timeout / wait 失敗) では reader thread を join しない**。
/// `shell_command` の child はシェル (cmd.exe / sh) で、その孫 (実際の `jj` 等) は
/// kill の対象外になり得る (cmd.exe は常に、sh も複合コマンドを fork した場合)。
/// 孫は pipe の書き込み端を継承したまま生き残るため EOF が来ず、join すると孫が自然終了する
/// までブロックする = timeout が意味を成さない (T6 が直そうとしているハングの再生産)。
/// 実測: 9s 走るコマンドに 1s の timeout を設定し join すると、制御が戻るまで 9.6s 掛かった。
/// よってこの 2 経路では thread を detach して即座に返す (push-runner は直後に exit 5 で
/// 終了するため thread は道連れになる)。出力も不要 (診断は timeout メッセージ自身が持つ)。
/// 子が自力で終了した経路 (exit 0 / 非 0) は pipe が閉じるため join してよい。
fn run_diff_cmd(cmd: &str, timeout_secs: u64) -> Result<String, String> {
    let mut child = shell_command(cmd)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("Failed to execute {}: {}", cmd, e))?;

    let stdout_handle = drain_pipe_unlimited(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle = drain_pipe_unlimited(child.stderr.take().expect("stderr must be piped"));

    let status = wait_with_timeout_safe("diff", &mut child, timeout_secs)
        .map_err(|e| format!("diff コマンドの wait に失敗: {}", e))?;

    let Some(status) = status else {
        return Err(format!(
            "diff コマンドがタイムアウトしました ({}s): {}\n\
             jj の lock 競合 (並列 workspace 実行中の別 jj プロセス) を疑ってください。\
             大 diff で恒常的に超過する場合は `[diff] timeout` を延長してください。",
            timeout_secs, cmd,
        ));
    };

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    if status.success() {
        Ok(stdout)
    } else {
        Err(stderr)
    }
}

/// レビュー対象 diff が PR 範囲の全変更ファイルを含むか検査する (fail-closed / ADR-043)。
///
/// **なぜ必要か**: `[diff] command` は config 由来の自由文字列で、`-r @` のように
/// PR より狭い範囲を書けてしまう。狭い範囲を書いても reviewers 側からは「渡された
/// diff が全体か」を検証できず、正しく「docs-only」等と判定してしまうため、
/// 誤りが誰にも検知されないまま merge に至る (todo 順位 288、Severity High で 4 回再発)。
/// 検査の真実源は `jj diff --summary` = docs_only_routing / pr_size_check と同じ経路。
///
/// 判定不能 (summary 取得失敗 / summary の行が 1 つでもパースできない / diff がヘッダを
/// 持たない) は「網羅している」に倒さずエラーにする。「検証できない」を「検証した」と
/// 扱わないための線引き。
///
/// パースの厳格性は [`parse_summary_paths`] が担う: **1 行でも解釈できない行があれば
/// `Err`**。妥当な行だけ拾って未知行を silent drop すると、jj の出力書式が一部だけ
/// 変わったとき (例: 一部行だけ新 status) に、変わっていない行で `expected` が非空になり
/// coverage を通過してしまう — 本 PR が塞いでいる「誤りが誰にも検知されない」構造その
/// ものを gate 自身が再生産する (CodeRabbit #313: per-line fail-open)。
fn verify_diff_covers_pr_range(
    diff_output: &str,
    fetch_summary: impl FnOnce() -> Result<String, String>,
) -> Result<(), String> {
    let summary = fetch_summary().map_err(|e| format!("PR 範囲の summary 取得に失敗: {}", e))?;

    let expected = parse_summary_paths(&summary).map_err(|e| {
        format!(
            "summary をパースできませんでした: {}。jj の出力書式が変わった可能性があります",
            e
        )
    })?;
    if expected.is_empty() {
        return Ok(());
    }

    let covered = parse_git_diff_paths(diff_output);
    if covered.is_empty() {
        return Err(
            "diff 出力に `diff --git` ヘッダが無く、対象ファイルを特定できません".to_string(),
        );
    }

    let missing: Vec<&String> = expected.iter().filter(|p| !covered.contains(*p)).collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "{} ファイルが未収録 (例: {})",
        missing.len(),
        missing
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// `jj diff --summary` の `M path` / `R old new` (rename) / `C old new` (copy)
/// 形式からパス集合を作る。
///
/// **空行を除く 1 行でも解釈できなければ `Err`** を返す (CodeRabbit #313)。妥当な行
/// だけを拾って未知行を silent drop すると、jj の出力書式が一部だけ変わったとき
/// (一部の行だけ新 status / status とパスに分割不能) に、変わっていない行で `expected`
/// が非空になり `verify_diff_covers_pr_range` の coverage 検査を通過してしまう。
/// 全行が未知のときだけ fail-closed する旧実装 (SIM-NEW-diff-rs-L178) の per-line 版。
/// 全行が空 (末尾改行のみ 等) なら `Ok` の空集合を返す (= 変更なし)。
///
/// `R`/`C` は `<status> <old> <new>` の 3 トークン形式 (`lib_docs_policy` の
/// `is_docs_only_summary` テストが同じ `"R docs/a.md docs/b.md"` 形状を実証)。
/// `parse_git_diff_paths` (`diff --git a/old b/new` の new = `b/` 側のみ拾う) と
/// 揃えるため new path 側だけを採用する (SIM-NEW-diff-rs-L146)。
///
/// Windows の jj は `\` 区切りで出すため `/` に正規化して `--git` 側と突き合わせる。
fn parse_summary_paths(
    summary: &str,
) -> Result<std::collections::BTreeSet<String>, String> {
    let mut paths = std::collections::BTreeSet::new();
    for line in summary.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let (status, rest) = line
            .split_once(' ')
            .ok_or_else(|| format!("status とパスに分割できない行: {:?}", line))?;
        let path = summary_line_new_path(status, rest)
            .ok_or_else(|| format!("未知 status / new path 欠落の行: {:?}", line))?;
        paths.insert(path);
    }
    Ok(paths)
}

/// 1 行分の `<status> <rest>` から採用すべきパスを返す (rename/copy は new path のみ)。
/// 受理できない行は `None` を返し、呼び出し側 [`parse_summary_paths`] が `Err` に昇格して
/// fail-closed に倒す。
///
/// `None` になるのは: 未知 status (`jj diff --summary` の M/A/D/R/C 以外) / R・C で
/// new path (末尾トークン) が無い崩れた行 / パスが空。catch-all で任意 status や崩れた
/// R/C を通すと、jj の出力書式が変わった行を「妥当なパス」として取り込み、書式変化を
/// 検知できず gate が沈黙する (SIM-NEW-diff-rs-L178)。旧実装は崩れた R/C の生トークンを
/// 残して coverage 不一致に頼っていたが、CodeRabbit #313 指摘に従い「未知 status も崩れた
/// R/C も明示的に reject」へ統一する (fail-closed の判定を coverage 副作用でなく
/// パース時点に前倒しする)。
fn summary_line_new_path(status: &str, rest: &str) -> Option<String> {
    let trimmed = rest.trim();
    let path = match status {
        "M" | "A" | "D" => trimmed,
        "R" | "C" => trimmed.rsplit_once(' ').map(|(_, new)| new)?,
        _ => return None,
    };
    (!path.is_empty()).then(|| path.replace('\\', "/"))
}

/// unified diff の `diff --git a/X b/X` ヘッダからパス集合を作る。
fn parse_git_diff_paths(diff_output: &str) -> std::collections::BTreeSet<String> {
    diff_output
        .lines()
        .filter_map(|line| line.strip_prefix("diff --git "))
        .filter_map(|rest| rest.split_once(" b/"))
        .map(|(_a_side, b_path)| b_path.trim().replace('\\', "/"))
        .filter(|p| !p.is_empty())
        .collect()
}

/// takt 実行後の diff snapshot を取得する (T12 post-takt re-gate の変化検出用)。
///
/// Stage 1.5 と同じ `[diff] command` を再実行し stdout を返す。呼び出し側 (re-gate) は
/// takt 起動前に保持した snapshot と本値を**前後比較**し、一致すれば「fix はコードを
/// 書き換えていない」= re-gate skip に倒す。取得失敗 (jj 失敗 / timeout) は `None` を返し、
/// 呼び出し側が fail-closed (= 変化ありとみなし re-gate 実行) に扱う (ADR-043)。
///
/// `run_diff` と違いファイルには書かない (比較のためメモリ上で保持するだけ)。timeout /
/// stderr 分離の要件は `run_diff_cmd` と同一 (同 doc 参照)。範囲カバレッジ検査も
/// 行わない (前後比較が目的で、レビュー入力にはならないため)。
pub(crate) fn capture_diff_snapshot(config: &DiffConfig, pr_range: &str) -> Option<String> {
    let timeout = config.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS);
    run_diff_cmd(&resolve_diff_command(&config.command, pr_range), timeout).ok()
}

/// `[diff] command` の [`DIFF_PR_RANGE_PLACEHOLDER`] を PR 範囲 revset に展開する。
pub(crate) fn resolve_diff_command(command: &str, pr_range: &str) -> String {
    command.replace(DIFF_PR_RANGE_PLACEHOLDER, pr_range)
}

pub(crate) fn run_diff(config: &DiffConfig, pr_range: &str) -> DiffResult {
    run_diff_with(config, pr_range, || {
        super::docs_only_routing::run_jj_diff_summary(pr_range)
    })
}

/// `run_diff` の本体。PR 範囲の summary 取得を注入可能にして、範囲カバレッジ検査を
/// jj 実行なしでテストできるようにする (`post_takt_regate::decide_regate` と同じ流儀)。
fn run_diff_with(
    config: &DiffConfig,
    pr_range: &str,
    fetch_summary: impl FnOnce() -> Result<String, String>,
) -> DiffResult {
    let command = resolve_diff_command(&config.command, pr_range);
    log_stage("diff", &format!("実行: {}", command));

    let timeout = config.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS);
    let output = match run_diff_cmd(&command, timeout) {
        Ok(output) => output,
        Err(err) => {
            log_stage("diff", "diff コマンド失敗");
            if !err.is_empty() {
                eprintln!("{}", err);
            }
            return DiffResult::Error;
        }
    };

    if let Err(reason) = verify_diff_covers_pr_range(&output, fetch_summary) {
        report_coverage_failure(pr_range, &reason);
        return DiffResult::Error;
    }

    if output.is_empty() {
        log_stage(
            "diff",
            "diff 出力が空です。レビューをスキップして push に進みます。",
        );
        return DiffResult::Empty;
    }

    write_diff_output(&config.output_path, &output)
}

/// 範囲検査に落ちたときの fail-closed 通知 (ADR-043)。
///
/// 「レビュー範囲が PR より狭い」ことは検知できても自動修復はできない
/// (どこまで広げるべきかは config の意図次第) ため、push を止めて人間に返す。
fn report_coverage_failure(pr_range: &str, reason: &str) {
    log_stage("diff", &format!("レビュー範囲の検査に失敗: {}", reason));
    eprintln!(
        "[push-runner] [diff] レビュー対象 diff が PR 範囲 ({}) を網羅していません: {}\n\
         このまま進めると祖先コミットが AI レビューを経ずに merge されます (todo 順位 288)。\n\
         `[diff] command` が `{}` を使っているか、出力が unified diff (--git) 形式かを確認してください。",
        pr_range, reason, DIFF_PR_RANGE_PLACEHOLDER
    );
}

fn write_diff_output(output_path: &str, output: &str) -> DiffResult {
    let path = Path::new(output_path);
    if let Some(parent) = path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            log_stage("diff", &format!("ディレクトリ作成失敗: {}", e));
            return DiffResult::Error;
        }
    }

    match std::fs::write(path, output) {
        Ok(()) => {
            let line_count = output.lines().count();
            log_stage(
                "diff",
                &format!("書き出し完了: {} ({} 行)", output_path, line_count),
            );
            DiffResult::HasContent
        }
        Err(e) => {
            log_stage("diff", &format!("ファイル書き出し失敗: {}", e));
            DiffResult::Error
        }
    }
}

#[cfg(test)]
mod tests;
