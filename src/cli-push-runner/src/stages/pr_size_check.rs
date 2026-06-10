//! PR size check stage — 順位 151 (Bundle "既存ルール仕組み化")
//!
//! `jj diff --stat -r '<default_branch>..@'` で PR 範囲の insertions + deletions を計測し、
//! `~/.claude/rules/common/git-workflow.md` § Multi-PR chaining の「1 PR あたり 250-800 lines」
//! 目安を決定論的に維持する。
//!
//! ADR-039 (Experimental feature 標準パターン) 3 点セット準拠:
//! - **Config opt-in**: 試験運用のため default `enabled = false`。`[pr_size_check]` section
//!   不在 / `enabled` 未設定 / `enabled = false` のいずれも skip (= 完全 no-op)。
//! - **Kill-switch**: `enabled = false` (TOML) + env `PR_SIZE_CHECK_OVERRIDE=1` で意図的バイパス。
//!   stop コマンドは `[pr_size_check] enabled = false` を `push-runner-config.toml` に書く。
//! - **Bounded lifetime**: 本リポジトリで 3-5 PR の dogfood 後 (false positive 観測 /
//!   検出効果 / override 使用頻度) に default-ON 昇格 or 却下を判定。
//!
//! 2 段階閾値 (warning / block):
//! - `warning_threshold` (default 800) 超過 → log warning のみ (push は続行)
//! - `block_threshold` (default 1500) 超過 → push を block (override env でバイパス可能)
//!
//! Stage 配置: `run_pre_checks` 内 (quality_gate より前)。検出時は重い AI review を
//! 無駄に走らせず即停止する。
//!
//! revset は `format!("{}..@", default_branch)` 形式 (rule⑫ `no-hardcoded-jj-revset-range` 適用)。

use std::process::{Command, Stdio};

use crate::config::{
    PrSizeCheckConfig, DEFAULT_PR_SIZE_BASE_BRANCH, DEFAULT_PR_SIZE_BLOCK_THRESHOLD,
    DEFAULT_PR_SIZE_WARNING_THRESHOLD,
};
use crate::log::{log_info, log_stage};

const JJ_TIMEOUT_SECS: u64 = 30;
const OVERRIDE_ENV_VAR: &str = "PR_SIZE_CHECK_OVERRIDE";

/// `[pr_size_check]` config の有無に応じて検査を実行し、push を続行してよいか
/// (= 閾値未満 or warning レベル or override active) を返す。
///
/// ADR-039 § 1 Config opt-in 準拠: default `enabled = false` (試験運用)。
/// section 不在 / `c.enabled = None` / `c.enabled = Some(false)` のいずれも skip。
/// 明示的に `c.enabled = Some(true)` のときのみ検査を実行。
///
/// fail-open: jj 不調 (timeout / 起動失敗 / 出力 parse 失敗) 時は warning ログのみで
/// true を返し、push 自体は止めない。
pub(crate) fn run_pr_size_check(config: Option<&PrSizeCheckConfig>) -> bool {
    let enabled = config.and_then(|c| c.enabled).unwrap_or(false);
    if !enabled {
        return true;
    }
    let default_branch = effective_default_branch(config);
    let warning = effective_warning_threshold(config);
    let block = effective_block_threshold(config);
    let revset = format!("{}..@", default_branch);
    let stat_line = match run_jj_diff_stat(&revset) {
        Ok(line) => line,
        Err(e) => {
            log_info(&format!(
                "pr_size_check: jj diff --stat 失敗、検査を skip して push を続行します: {}",
                e
            ));
            return true;
        }
    };
    let total = match parse_total_lines(&stat_line) {
        Some(n) => n,
        None => {
            log_info(&format!(
                "pr_size_check: jj diff --stat 出力を parse できません ({:?}) — 検査を skip",
                stat_line
            ));
            return true;
        }
    };
    let override_active = parse_override_env(std::env::var(OVERRIDE_ENV_VAR).ok().as_deref());
    classify_and_log(total, warning, block, &revset, override_active)
}

fn classify_and_log(
    total: usize,
    warning: usize,
    block: usize,
    revset: &str,
    override_active: bool,
) -> bool {
    if total > block {
        log_stage(
            "pr_size",
            &format!(
                "PR diff {} 行が block_threshold {} を超過 (revset: {})",
                total, block, revset
            ),
        );
        if override_active {
            log_info(&format!(
                "  {}=1 により続行します (意図的バイパス)",
                OVERRIDE_ENV_VAR
            ));
            return true;
        }
        log_info(&format!(
            "  対処:\n  \
             (a) PR を 2 つ以上に分割 (git-workflow.md § Multi-PR chaining 参照)\n  \
             (b) 大型 refactoring 等で意図的なら env {}=1 を設定して再実行\n  \
             (c) `push-runner-config.toml` の `[pr_size_check]` で閾値を恒久的に調整",
            OVERRIDE_ENV_VAR
        ));
        return false;
    }
    if total > warning {
        log_stage(
            "pr_size",
            &format!(
                "PR diff {} 行 > warning_threshold {} (block_threshold {} 未満、push 続行)",
                total, warning, block
            ),
        );
        return true;
    }
    log_stage(
        "pr_size",
        &format!("PR diff {} 行 (閾値 warning={} 内、OK)", total, warning),
    );
    true
}

fn effective_default_branch(config: Option<&PrSizeCheckConfig>) -> String {
    config
        .and_then(|c| c.default_branch.as_ref())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_PR_SIZE_BASE_BRANCH.to_string())
}

fn effective_warning_threshold(config: Option<&PrSizeCheckConfig>) -> usize {
    config
        .and_then(|c| c.warning_threshold)
        .unwrap_or(DEFAULT_PR_SIZE_WARNING_THRESHOLD)
}

fn effective_block_threshold(config: Option<&PrSizeCheckConfig>) -> usize {
    config
        .and_then(|c| c.block_threshold)
        .unwrap_or(DEFAULT_PR_SIZE_BLOCK_THRESHOLD)
}

/// `jj diff --stat -r '<revset>'` を実行し、末尾の summary 行 (例:
/// `"3 files changed, 311 insertions(+), 0 deletions(-)"`) を返す。
///
/// summary 行が見つからない場合は最後の非空行を返す (parse 側で再判定)。
fn run_jj_diff_stat(revset: &str) -> Result<String, String> {
    let mut child = Command::new("jj")
        .args(["diff", "--stat", "-r", revset])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("jj diff --stat 起動失敗: {}", e))?;

    let stdout_handle = crate::runner::drain_pipe(child.stdout.take().expect("stdout must be piped"));
    let stderr_handle = crate::runner::drain_pipe(child.stderr.take().expect("stderr must be piped"));

    let status = crate::runner::wait_with_timeout("jj diff --stat", &mut child, JJ_TIMEOUT_SECS)
        .map_err(|e| format!("jj diff --stat wait 失敗: {}", e))?;

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();

    match status {
        None => Err(format!("jj diff --stat タイムアウト ({}s)", JJ_TIMEOUT_SECS)),
        Some(s) if s.success() => Ok(extract_summary_line(&stdout)),
        Some(_) => Err(stderr.trim().to_string()),
    }
}

/// `jj diff --stat` 出力の末尾の summary 行 (`N files changed, M insertions(+), K deletions(-)`)
/// を抽出する。
fn extract_summary_line(stdout: &str) -> String {
    stdout
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("")
        .to_string()
}

/// summary 行から `insertions + deletions` を計算する。
///
/// 期待 format: `"3 files changed, 311 insertions(+), 0 deletions(-)"`
/// 部分欠落 (`insertions(+)` のみ / `deletions(-)` のみ) もサポート。
/// parse 不能時は `None` を返す。
fn parse_total_lines(line: &str) -> Option<usize> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    let insertions = extract_number_before(trimmed, "insertion").unwrap_or(0);
    let deletions = extract_number_before(trimmed, "deletion").unwrap_or(0);
    if insertions == 0 && deletions == 0 && !trimmed.contains("files changed") {
        return None;
    }
    Some(insertions + deletions)
}

/// `"<N> <token>...(+)"` のような部分から `<N>` を取り出す。`token` を見つけた直前の数字を返す。
fn extract_number_before(line: &str, token: &str) -> Option<usize> {
    let idx = line.find(token)?;
    let prefix = &line[..idx];
    let digits: String = prefix
        .chars()
        .rev()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    digits.parse().ok()
}

fn parse_override_env(raw: Option<&str>) -> bool {
    let Some(value) = raw else {
        return false;
    };
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_total_lines_typical_jj_output() {
        let line = "3 files changed, 311 insertions(+), 0 deletions(-)";
        assert_eq!(parse_total_lines(line), Some(311));
    }

    #[test]
    fn parse_total_lines_with_both_nonzero() {
        let line = "5 files changed, 100 insertions(+), 50 deletions(-)";
        assert_eq!(parse_total_lines(line), Some(150));
    }

    #[test]
    fn parse_total_lines_empty_diff() {
        let line = "0 files changed, 0 insertions(+), 0 deletions(-)";
        assert_eq!(parse_total_lines(line), Some(0));
    }

    #[test]
    fn parse_total_lines_insertions_only_no_deletions_section() {
        let line = "2 files changed, 50 insertions(+)";
        assert_eq!(parse_total_lines(line), Some(50));
    }

    #[test]
    fn parse_total_lines_deletions_only_no_insertions_section() {
        let line = "2 files changed, 30 deletions(-)";
        assert_eq!(parse_total_lines(line), Some(30));
    }

    #[test]
    fn parse_total_lines_returns_none_for_empty() {
        assert_eq!(parse_total_lines(""), None);
        assert_eq!(parse_total_lines("   "), None);
    }

    #[test]
    fn parse_total_lines_returns_none_for_unrelated_text() {
        assert_eq!(parse_total_lines("Some unrelated jj output"), None);
    }

    #[test]
    fn extract_summary_line_picks_last_non_empty() {
        let stdout = "docs/foo.md | 5 +\nsrc/bar.rs  | 10 ++++\n2 files changed, 13 insertions(+), 2 deletions(-)\n";
        assert_eq!(
            extract_summary_line(stdout),
            "2 files changed, 13 insertions(+), 2 deletions(-)"
        );
    }

    #[test]
    fn extract_summary_line_skips_trailing_blank_lines() {
        let stdout = "summary\n\n\n";
        assert_eq!(extract_summary_line(stdout), "summary");
    }

    #[test]
    fn extract_summary_line_empty_stdout() {
        assert_eq!(extract_summary_line(""), "");
    }

    fn fixture_revset(default_branch: &str) -> String {
        format!("{}..@", default_branch)
    }

    #[test]
    fn classify_within_warning_threshold_passes() {
        let r = fixture_revset("master");
        assert!(classify_and_log(100, 800, 1500, &r, false));
    }

    #[test]
    fn classify_at_warning_threshold_passes() {
        let r = fixture_revset("master");
        assert!(classify_and_log(800, 800, 1500, &r, false));
    }

    #[test]
    fn classify_above_warning_below_block_passes_with_warning() {
        let r = fixture_revset("master");
        assert!(classify_and_log(900, 800, 1500, &r, false));
    }

    #[test]
    fn classify_at_block_threshold_still_passes() {
        let r = fixture_revset("master");
        assert!(classify_and_log(1500, 800, 1500, &r, false));
    }

    #[test]
    fn classify_above_block_threshold_blocks_without_override() {
        let r = fixture_revset("master");
        assert!(!classify_and_log(1600, 800, 1500, &r, false));
    }

    #[test]
    fn classify_above_block_with_override_passes() {
        let r = fixture_revset("master");
        assert!(
            classify_and_log(1600, 800, 1500, &r, true),
            "override flag should bypass block"
        );
    }

    #[test]
    fn classify_with_alternative_default_branch_works() {
        let r = fixture_revset("main");
        assert!(classify_and_log(100, 800, 1500, &r, false));
    }

    #[test]
    fn effective_default_branch_uses_default_when_none() {
        assert_eq!(effective_default_branch(None), "master");
    }

    #[test]
    fn effective_default_branch_uses_config_when_present() {
        let c = PrSizeCheckConfig {
            enabled: Some(true),
            default_branch: Some("main".to_string()),
            warning_threshold: None,
            block_threshold: None,
        };
        assert_eq!(effective_default_branch(Some(&c)), "main");
    }

    #[test]
    fn effective_default_branch_falls_back_when_blank_string() {
        let c = PrSizeCheckConfig {
            enabled: Some(true),
            default_branch: Some("   ".to_string()),
            warning_threshold: None,
            block_threshold: None,
        };
        assert_eq!(effective_default_branch(Some(&c)), "master");
    }

    #[test]
    fn effective_warning_threshold_default() {
        assert_eq!(effective_warning_threshold(None), 800);
    }

    #[test]
    fn effective_warning_threshold_from_config() {
        let c = PrSizeCheckConfig {
            enabled: Some(true),
            default_branch: None,
            warning_threshold: Some(500),
            block_threshold: None,
        };
        assert_eq!(effective_warning_threshold(Some(&c)), 500);
    }

    #[test]
    fn effective_block_threshold_default() {
        assert_eq!(effective_block_threshold(None), 1500);
    }

    #[test]
    fn effective_block_threshold_from_config() {
        let c = PrSizeCheckConfig {
            enabled: Some(true),
            default_branch: None,
            warning_threshold: None,
            block_threshold: Some(3000),
        };
        assert_eq!(effective_block_threshold(Some(&c)), 3000);
    }

    #[test]
    fn run_pr_size_check_skips_when_section_absent() {
        assert!(run_pr_size_check(None));
    }

    #[test]
    fn run_pr_size_check_skips_when_enabled_none() {
        let c = PrSizeCheckConfig {
            enabled: None,
            default_branch: None,
            warning_threshold: None,
            block_threshold: None,
        };
        assert!(run_pr_size_check(Some(&c)));
    }

    #[test]
    fn run_pr_size_check_skips_when_enabled_false() {
        let c = PrSizeCheckConfig {
            enabled: Some(false),
            default_branch: None,
            warning_threshold: None,
            block_threshold: None,
        };
        assert!(run_pr_size_check(Some(&c)));
    }

    #[test]
    fn parse_override_env_truthy_variants() {
        for v in ["1", "true", "TRUE", "yes", "on", " true "] {
            assert!(parse_override_env(Some(v)), "'{}' should be truthy", v);
        }
    }

    #[test]
    fn parse_override_env_falsy_variants() {
        for v in ["0", "false", "no", "", "   ", "maybe"] {
            assert!(!parse_override_env(Some(v)), "'{}' should be falsy", v);
        }
    }

    #[test]
    fn parse_override_env_none_is_false() {
        assert!(!parse_override_env(None));
    }
}
