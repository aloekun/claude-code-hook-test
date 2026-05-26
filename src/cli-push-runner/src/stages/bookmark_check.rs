//! Bookmark check stage — 順位 2 (PR #85 T1-3)
//!
//! `jj git push` は bookmark が必要だが、jj 環境では新規ブランチで bookmark を
//! 作成し忘れる落とし穴がある (PR #85 で初回 `pnpm push` が bookmark 未設定 →
//! `Nothing changed` で終了し、158s かけた quality_gate + takt review が無駄に
//! なった実証ベース)。本 stage は pipeline 最早期 (`scratch_file_warning` の前)
//! で `jj bookmark list` を確認し、非 trunk bookmark が無ければ即 error 終了して
//! 後続 stage の無駄実行を防ぐ。
//!
//! Stage 配置: `run_pipeline` の最早期 (scratch_file_warning の前)。bookmark 不在
//! は push 自体が不可能な状態のため、最優先で fail-fast する。
//!
//! fail-open: `jj bookmark list` 実行失敗 (timeout / 起動失敗) 時は warning ログ
//! のみで push を続行する。jj 不調で push 自体を止めない設計。
//!
//! 設計上の non-config: `jj git push` は bookmark を必須とする仕様で、本 stage を
//! バイパスする正当な use case は存在しない。よって `[bookmark_check]` config
//! section は追加せず、常に有効。

use std::process::Command;

use lib_jj_helpers::is_trunk_bookmark;

use crate::log::{log_info, log_stage};

const JJ_TIMEOUT_SECS: u64 = 30;

/// `jj bookmark list` で非 trunk なローカル bookmark の存在を確認し、
/// push を続行してよいか (= 非 trunk bookmark が 1 件以上存在) を返す。
///
/// fail-open: jj 実行失敗時は warning ログのみで true を返し、push 自体は止めない。
pub(crate) fn run_bookmark_check() -> bool {
    let raw = match run_jj_bookmark_list() {
        Ok(output) => output,
        Err(e) => {
            log_info(&format!(
                "bookmark_check: jj bookmark list 失敗、検査を skip して push を続行します: {}",
                e
            ));
            return true;
        }
    };
    let bookmarks = parse_non_trunk_bookmarks(&raw);
    if bookmarks.is_empty() {
        log_stage("bookmark", "ローカル bookmark (非 trunk) が見つかりません");
        log_info(
            "  push 不可: `jj git push` は bookmark が必要です。\n  \
             対処: `jj bookmark create <name> -r @` で bookmark を作成して再実行してください\n  \
             例: `jj bookmark create feat/my-feature -r @`",
        );
        return false;
    }
    log_stage(
        "bookmark",
        &format!(
            "非 trunk bookmark 検出 ({} 件): {}",
            bookmarks.len(),
            bookmarks.join(", ")
        ),
    );
    true
}

fn parse_non_trunk_bookmarks(raw: &str) -> Vec<String> {
    raw.lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with('\t'))
        .filter_map(|line| line.split(':').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && !is_trunk_bookmark(s))
        .collect()
}

fn run_jj_bookmark_list() -> Result<String, String> {
    use std::process::Stdio;

    let mut child = Command::new("jj")
        .args(["bookmark", "list"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj bookmark list 起動失敗: {}", e))?;

    let stdout_handle =
        crate::runner::drain_pipe(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle =
        crate::runner::drain_pipe(child.stderr.take().expect("stderr must be piped"));

    let status = crate::runner::wait_with_timeout("jj bookmark list", &mut child, JJ_TIMEOUT_SECS)
        .map_err(|e| format!("jj bookmark list wait 失敗: {}", e))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!("jj bookmark list タイムアウト ({}s)", JJ_TIMEOUT_SECS)),
        Some(s) if s.success() => Ok(stdout),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_non_trunk_typical_output() {
        let output = "\
feat/xyz: abc1234 add feature
  @origin: abc1234 add feature
main: def5678 initial
  @origin: def5678 initial
";
        assert_eq!(parse_non_trunk_bookmarks(output), vec!["feat/xyz"]);
    }

    #[test]
    fn parse_non_trunk_multiple_feature_bookmarks() {
        let output = "\
feat/a: 111 desc
feat/b: 222 desc
main: 333 desc
";
        assert_eq!(parse_non_trunk_bookmarks(output), vec!["feat/a", "feat/b"]);
    }

    #[test]
    fn parse_non_trunk_only_trunk_returns_empty() {
        let output = "main: abc123 desc\nmaster: def456 desc\n";
        assert!(parse_non_trunk_bookmarks(output).is_empty());
    }

    #[test]
    fn parse_non_trunk_empty_output_returns_empty() {
        assert!(parse_non_trunk_bookmarks("").is_empty());
    }

    #[test]
    fn parse_non_trunk_skips_indented_remote_lines() {
        let output = "\
feat/xyz: abc1234 desc
  @origin: abc1234 desc
  @upstream: abc1234 desc
";
        assert_eq!(parse_non_trunk_bookmarks(output), vec!["feat/xyz"]);
    }

    #[test]
    fn parse_non_trunk_filters_out_master_and_main() {
        let output = "\
feat/branch1: abc desc
master: def desc
feat/branch2: ghi desc
main: jkl desc
";
        assert_eq!(
            parse_non_trunk_bookmarks(output),
            vec!["feat/branch1", "feat/branch2"]
        );
    }

    #[test]
    fn parse_non_trunk_handles_single_feature_bookmark() {
        let output = "feat/single: abc desc\n";
        assert_eq!(parse_non_trunk_bookmarks(output), vec!["feat/single"]);
    }
}
