//! WP-11 決定論層 (ADR-054): fix 後の diff を scope 検証する auto-push gate。
//!
//! CodeRabbit コメント (外部非信頼テキスト) 起点の fix エージェントが、finding 対象外の
//! ファイルを改変する prompt injection を決定論的に block する。fix commit は分離 child
//! commit に隔離されるため、`jj diff --from <pre_takt_cid> --to @ --summary` で「fix が
//! 実際に変更したファイル」を取得し、findings の Location から導出した allowlist と照合する。
//!
//! 試験運用 (ADR-039 準拠): `[fix.scope_guard] enabled = false` (default OFF)。
//! `pnpm deploy:hooks` で派生プロジェクトに配布されるため、意図せぬ有効化を避けて opt-in と
//! する (品質 gate が default ON なのは本リポジトリ固有の pre-existing 契約であり、新規かつ
//! 配布対象の本層には適用しない)。kill-switch: `PR_MONITOR_SCOPE_GUARD_DISABLE=1`。
//!
//! bounded lifetime (decision trigger): 本リポジトリで enabled = true にした dogfood
//! 開始から 3-5 PR 経過後に採否判定する。誤検知 (正当な関連ファイル修正の block) ゼロかつ
//! 注入 fixture の test 緑を維持 → 採用。誤検知頻発 → enabled = false に戻し allowlist 導出を
//! 再設計 (PR diff のファイルも allowlist に含める緩和案) or 却下。

use std::collections::BTreeSet;

use lib_report_formatter::Finding;
use serde::Deserialize;

use crate::log::log_info;
use crate::runner::{run_cmd_direct, JJ_CMD_TIMEOUT_SECS};

/// kill-switch: この環境変数が "1" のとき scope guard を skip する (緊急バイパス用)。
/// 既存の `PR_MONITOR_GATE_DISABLE` とは独立 — 品質 gate と scope guard を別々に停止できる。
pub(crate) const SCOPE_GUARD_DISABLE_ENV: &str = "PR_MONITOR_SCOPE_GUARD_DISABLE";

/// fix step が正当に refresh する中間ファイル (fix.md の許可書き込み対象)。
/// findings 由来 allowlist に加えて常に許可する。
const ALWAYS_ALLOWED: &[&str] = &[".takt/review-diff.txt"];

/// scope guard 設定 (`pr-monitor-config.toml` の `[fix.scope_guard]`)。
/// `crate::config::FixConfig` の field として deserialize される。
#[derive(Deserialize, Clone)]
pub(crate) struct ScopeGuardConfig {
    /// 派生プロジェクト配布を考慮し default OFF (ADR-039 § 1 / ADR-054)。
    /// 本リポジトリで有効化する場合のみ `enabled = true` を明示する。
    #[serde(default)]
    pub(crate) enabled: bool,
    /// `"enforce"` (violation で push 中止) または `"observe"` (記録のみ、push 続行)。
    /// 未知値は enforce 扱い (fail-closed 方向)。
    #[serde(default = "default_mode")]
    pub(crate) mode: String,
}

fn default_mode() -> String {
    "enforce".into()
}

impl Default for ScopeGuardConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mode: default_mode(),
        }
    }
}

/// scope guard の評価結果。
#[derive(Debug, PartialEq)]
pub(crate) enum ScopeGuardOutcome {
    /// config disabled or kill-switch → 何もしない (push 続行)
    SkippedDisabled,
    /// 変更ファイルがすべて allowlist 内 → push 続行
    Passed,
    /// observe モードで violation を検知 (記録のみ、push 続行)
    ObservedViolation { out_of_scope: Vec<String> },
    /// enforce モードで violation を検知 (push 中止)
    BlockedViolation { reason: String },
}

/// scope guard を無効化すべきか。(config の enabled, kill-switch env 値) から判定する。
/// env 読取は呼び出し側で行い、本関数は注入値で判定する (DI over ambient global)。
pub(crate) fn scope_guard_disabled(config_enabled: bool, env_value: Option<&str>) -> bool {
    if env_value == Some("1") {
        return true;
    }
    !config_enabled
}

/// mode 文字列が observe か。未知値・enforce は false (= enforce = block 側、fail-closed)。
fn is_observe_mode(mode: &str) -> bool {
    mode == "observe"
}

/// パスを正規化する (Windows のバックスラッシュ → スラッシュ、前後空白除去)。
/// jj diff --summary (Windows は `\` 区切り) と findings の `file` (`/` 区切り) を揃える。
fn normalize_path(path: &str) -> String {
    path.trim().replace('\\', "/")
}

/// findings の `file` 列から編集許可ファイル集合 (allowlist) を導出する。
/// 空パスは除外する。
fn allowlist_from_findings(findings: &[Finding]) -> BTreeSet<String> {
    findings
        .iter()
        .map(|f| normalize_path(&f.file))
        .filter(|p| !p.is_empty())
        .collect()
}

/// `jj diff --summary` 出力から変更ファイルパスを抽出する。
///
/// fail-closed: パース不能な行 (rename `R` 等の非 M/A/D 行) は Err に倒し、
/// 呼び出し側で block させる (判定不能を通過させない)。
fn parse_changed_files(summary: &str) -> Result<Vec<String>, String> {
    let mut files = Vec::new();
    for line in summary.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Some((status, path)) = line.split_once(' ') else {
            return Err(format!("パース不能な diff summary 行: {line:?}"));
        };
        if !matches!(status, "M" | "A" | "D") {
            return Err(format!("未対応の diff status (fail-closed): {line:?}"));
        }
        files.push(normalize_path(path));
    }
    Ok(files)
}

/// 変更ファイルのうち allowlist にも ALWAYS_ALLOWED にも含まれないものを返す。
fn find_out_of_scope(changed: &[String], allowlist: &BTreeSet<String>) -> Vec<String> {
    changed
        .iter()
        .filter(|f| !allowlist.contains(*f) && !ALWAYS_ALLOWED.contains(&f.as_str()))
        .cloned()
        .collect()
}

/// violation を mode に応じて Observed / Blocked に振り分ける。
fn decide_violation(config: &ScopeGuardConfig, reason: &str, out_of_scope: Vec<String>) -> ScopeGuardOutcome {
    if is_observe_mode(&config.mode) {
        log_info(&format!("[scope_guard] OBSERVE (block せず記録): {reason}"));
        ScopeGuardOutcome::ObservedViolation { out_of_scope }
    } else {
        log_info(&format!("[scope_guard] BLOCK (enforce、fail-closed): {reason}"));
        ScopeGuardOutcome::BlockedViolation {
            reason: reason.to_string(),
        }
    }
}

/// fix diff (pre_cid → @) の summary を取得する。jj 失敗は Err に倒す (fail-closed)。
fn fetch_diff_summary(pre_cid: &str) -> Result<String, String> {
    let (ok, out) = run_cmd_direct(
        "jj",
        &["diff", "--from", pre_cid, "--to", "@", "--summary"],
        &[],
        JJ_CMD_TIMEOUT_SECS,
    );
    if !ok {
        return Err(out.trim().to_string());
    }
    Ok(out)
}

/// fix diff の変更ファイル一覧を解決する。判定不能 (pre_cid 不明 / jj 失敗 /
/// パース失敗) はすべて fail-closed で violation の `Err(ScopeGuardOutcome)` に倒す。
fn resolve_changed_files(
    config: &ScopeGuardConfig,
    pre_cid: Option<&str>,
) -> Result<Vec<String>, ScopeGuardOutcome> {
    let Some(pre) = pre_cid else {
        return Err(decide_violation(
            config,
            "pre_takt commit id が不明 (fail-closed で block)",
            Vec::new(),
        ));
    };
    let summary = fetch_diff_summary(pre).map_err(|e| {
        decide_violation(config, &format!("fix diff summary 取得失敗 (fail-closed): {e}"), Vec::new())
    })?;
    parse_changed_files(&summary).map_err(|e| {
        decide_violation(config, &format!("diff summary パース失敗 (fail-closed): {e}"), Vec::new())
    })
}

/// auto-push 直前の scope 検証 (副作用: env / jj diff)。
///
/// 判定順:
/// 1. kill-switch env or config disabled → SkippedDisabled
/// 2. pre_cid 不明 / jj diff 失敗 / summary パース失敗 → fail-closed で violation
/// 3. 変更ファイル ⊆ allowlist → Passed / それ以外 → mode で Observed / Blocked
pub(crate) fn evaluate_scope_guard(
    config: &ScopeGuardConfig,
    pre_cid: Option<&str>,
    findings: &[Finding],
) -> ScopeGuardOutcome {
    let env_value = std::env::var(SCOPE_GUARD_DISABLE_ENV).ok();
    if scope_guard_disabled(config.enabled, env_value.as_deref()) {
        log_info("[scope_guard] 無効化されている (config or kill-switch)、scope 検証なしで push 続行");
        return ScopeGuardOutcome::SkippedDisabled;
    }

    let changed = match resolve_changed_files(config, pre_cid) {
        Ok(c) => c,
        Err(outcome) => return outcome,
    };

    let allowlist = allowlist_from_findings(findings);
    let out_of_scope = find_out_of_scope(&changed, &allowlist);
    if out_of_scope.is_empty() {
        log_info(&format!(
            "[scope_guard] PASS: 変更 {} ファイルはすべて allowlist 内",
            changed.len()
        ));
        return ScopeGuardOutcome::Passed;
    }
    decide_violation(
        config,
        &format!("finding 対象外ファイルへの変更を検知 (injection の疑い): {out_of_scope:?}"),
        out_of_scope,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding_at(file: &str) -> Finding {
        Finding {
            severity: "Major".into(),
            file: file.into(),
            line: "1".into(),
            issue: "issue".into(),
            suggestion: "fix".into(),
            source: "CodeRabbit".into(),
        }
    }

    fn enforce_config() -> ScopeGuardConfig {
        ScopeGuardConfig {
            enabled: true,
            mode: "enforce".into(),
        }
    }

    fn observe_config() -> ScopeGuardConfig {
        ScopeGuardConfig {
            enabled: true,
            mode: "observe".into(),
        }
    }

    #[test]
    fn config_defaults_off_with_enforce_mode() {
        let cfg = ScopeGuardConfig::default();
        assert!(!cfg.enabled, "default OFF (opt-in、ADR-054 / 派生プロジェクト配布考慮)");
        assert_eq!(cfg.mode, "enforce");
    }

    #[test]
    fn config_parses_enabled_and_mode() {
        let cfg: ScopeGuardConfig = toml::from_str("enabled = true\nmode = \"observe\"\n").unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.mode, "observe");
    }

    #[test]
    fn config_missing_mode_defaults_to_enforce() {
        let cfg: ScopeGuardConfig = toml::from_str("enabled = true\n").unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.mode, "enforce", "mode 未指定は enforce (fail-closed 方向)");
    }

    #[test]
    fn scope_guard_disabled_by_kill_switch_env() {
        assert!(scope_guard_disabled(true, Some("1")));
        assert!(scope_guard_disabled(false, Some("1")));
    }

    #[test]
    fn scope_guard_enabled_when_config_on_and_env_absent_or_other() {
        assert!(!scope_guard_disabled(true, None));
        assert!(!scope_guard_disabled(true, Some("0")));
        assert!(!scope_guard_disabled(true, Some("")));
    }

    #[test]
    fn scope_guard_disabled_when_config_off() {
        assert!(scope_guard_disabled(false, None));
    }

    #[test]
    fn is_observe_mode_only_for_exact_observe() {
        assert!(is_observe_mode("observe"));
        assert!(!is_observe_mode("enforce"));
        assert!(!is_observe_mode("Observe"));
        assert!(!is_observe_mode("unknown"), "未知値は enforce (block 側、fail-closed)");
    }

    #[test]
    fn normalize_path_converts_windows_separators() {
        assert_eq!(normalize_path("src\\cli\\main.rs"), "src/cli/main.rs");
        assert_eq!(normalize_path("  src/a.rs  "), "src/a.rs");
    }

    #[test]
    fn allowlist_collects_finding_files_and_drops_empty() {
        let findings = vec![finding_at("src/a.rs"), finding_at("src\\b.rs"), finding_at("")];
        let allow = allowlist_from_findings(&findings);
        assert!(allow.contains("src/a.rs"));
        assert!(allow.contains("src/b.rs"), "バックスラッシュは正規化される");
        assert_eq!(allow.len(), 2, "空パスは除外される");
    }

    #[test]
    fn parse_changed_files_extracts_mad_paths() {
        let summary = "M src/a.rs\nA src/b.rs\nD src/c.rs\n";
        let files = parse_changed_files(summary).unwrap();
        assert_eq!(files, vec!["src/a.rs", "src/b.rs", "src/c.rs"]);
    }

    #[test]
    fn parse_changed_files_rejects_rename_and_unparseable_lines() {
        assert!(parse_changed_files("R old.rs new.rs").is_err(), "rename は fail-closed");
        assert!(parse_changed_files("weird-line-without-status").is_err());
    }

    #[test]
    fn find_out_of_scope_flags_files_outside_allowlist() {
        let allowlist: BTreeSet<String> = ["src/a.rs".to_string()].into_iter().collect();
        let changed = vec!["src/a.rs".to_string(), ".claude/settings.json".to_string()];
        let oos = find_out_of_scope(&changed, &allowlist);
        assert_eq!(oos, vec![".claude/settings.json"]);
    }

    #[test]
    fn find_out_of_scope_allows_review_diff_refresh() {
        let allowlist: BTreeSet<String> = ["src/a.rs".to_string()].into_iter().collect();
        let changed = vec!["src/a.rs".to_string(), ".takt/review-diff.txt".to_string()];
        assert!(
            find_out_of_scope(&changed, &allowlist).is_empty(),
            ".takt/review-diff.txt は fix の正当な refresh 対象として常に許可"
        );
    }

    /// WP-11 受け入れ基準 (synthetic injection scenario): finding は src/main.rs のみを
    /// 指すのに、fix が finding 対象外の設定ファイルを改変した場合、enforce モードで
    /// BlockedViolation となり auto-push が止まることを machine-enforce する。
    #[test]
    fn enforce_blocks_when_fix_touches_file_outside_allowlist() {
        let findings = vec![finding_at("src/main.rs")];
        let allowlist = allowlist_from_findings(&findings);
        let changed = vec![
            "src/main.rs".to_string(),
            ".coderabbit.yaml".to_string(),
        ];
        let out_of_scope = find_out_of_scope(&changed, &allowlist);
        let outcome = decide_violation(&enforce_config(), "test", out_of_scope);
        match outcome {
            ScopeGuardOutcome::BlockedViolation { .. } => {}
            other => panic!("enforce は allowlist 外変更で block すべき: {other:?}"),
        }
    }

    /// observe モードでは同じ violation でも block せず ObservedViolation として記録する。
    #[test]
    fn observe_records_violation_without_blocking() {
        let outcome = decide_violation(&observe_config(), "test", vec![".coderabbit.yaml".to_string()]);
        match outcome {
            ScopeGuardOutcome::ObservedViolation { out_of_scope } => {
                assert_eq!(out_of_scope, vec![".coderabbit.yaml"]);
            }
            other => panic!("observe は記録のみで block しない: {other:?}"),
        }
    }

    /// 変更が allowlist 内に収まる正当な fix は violation にならない (false-positive ガード)。
    #[test]
    fn in_scope_fix_produces_no_violation() {
        let findings = vec![finding_at("src/main.rs"), finding_at("src/lib.rs")];
        let allowlist = allowlist_from_findings(&findings);
        let changed = vec!["src/main.rs".to_string(), "src/lib.rs".to_string()];
        assert!(find_out_of_scope(&changed, &allowlist).is_empty());
    }

    struct CwdRestore {
        original: std::path::PathBuf,
    }

    impl Drop for CwdRestore {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    /// 実 jj repo を用意し、base commit の後に settings.json を改変した fix child commit を
    /// 積む。base の commit_id (= pre_takt_cid 相当) を返す。
    fn setup_repo_with_tampered_fix(repo: &std::path::Path) -> String {
        use std::process::Command as StdCommand;

        let run = |args: &[&str]| {
            assert!(
                StdCommand::new("jj")
                    .args(args)
                    .current_dir(repo)
                    .status()
                    .expect("jj spawn")
                    .success(),
                "jj {args:?} failed"
            );
        };
        run(&["git", "init"]);
        std::fs::write(repo.join("main.rs"), "fn main() {}\n").expect("write main.rs");
        std::fs::write(repo.join("settings.json"), "{}\n").expect("write settings.json");
        run(&["describe", "-m", "base"]);
        let out = StdCommand::new("jj")
            .args(["log", "-r", "@", "--no-graph", "-T", "commit_id"])
            .current_dir(repo)
            .output()
            .expect("log");
        let pre_cid = String::from_utf8_lossy(&out.stdout).trim().to_string();
        run(&["new"]);
        std::fs::write(repo.join("settings.json"), "{\"tampered\":true}\n").expect("tamper");
        pre_cid
    }

    /// 統合: 実 jj repo で fix commit が allowlist 外ファイルを変更したとき、
    /// evaluate_scope_guard が enforce で BlockedViolation を返すことを確認する。
    #[test]
    #[ignore = "integration: requires jj in PATH; run via `cargo test -- --ignored --test-threads=1`"]
    fn integration_evaluate_blocks_out_of_scope_fix() {
        let temp = tempfile::tempdir().expect("tempdir");
        let repo = temp.path();
        let pre_cid = setup_repo_with_tampered_fix(repo);

        let original_cwd = std::env::current_dir().expect("cwd");
        std::env::set_current_dir(repo).expect("cd");
        let _guard = CwdRestore {
            original: original_cwd,
        };

        let findings = vec![finding_at("main.rs")];
        let outcome = evaluate_scope_guard(&enforce_config(), Some(&pre_cid), &findings);
        match outcome {
            ScopeGuardOutcome::BlockedViolation { reason } => {
                assert!(reason.contains("settings.json"), "reason: {reason}");
            }
            other => panic!("allowlist 外の settings.json 改変を block すべき: {other:?}"),
        }
    }
}
