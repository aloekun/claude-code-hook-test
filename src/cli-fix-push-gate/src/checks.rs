//! Phase B fix push ゲートの純粋判定コア (ADR-067)。
//!
//! I/O を持たない。config / env / ファイル読み取りは [`crate::inputs`] が担い、本 module は
//! 読み取り済みの値だけで「この fix push を無人で実行してよいか」を決める。
//!
//! # 4 軸の AND 合成
//!
//! ADR-052 原則 2 は自動実行可クラスを「内容軸 × target 軸の合成で、いずれかがゲート必須なら
//! 操作全体がゲート必須」と定める。本 module はそこへ kill-switch (ADR-052 原則 5 / ADR-066) と
//! scope guard (ADR-054) を加えた 4 軸をすべて AND で評価する。1 つでも欠ければ deny =
//! Phase A 相当 (分析コメントのみ) へ degrade する。

use std::collections::BTreeSet;

use lib_autonomy_policy::{Decision as AutonomyDecision, DenyReason as AutonomyDenyReason};

/// 自律 fix push を許可するブランチ名の prefix (ADR-052 target 軸)。
///
/// trunk ではない隔離 namespace であることが自動実行可の条件。既存のローカル発 PR ブランチ
/// (`feat/` 等) は対象外で、当面の作用対象は WP-18 の夜間ループが作る `claude/` ブランチのみ。
pub(crate) const REQUIRED_BRANCH_PREFIX: &str = "claude/";

/// 判定に必要な事実一式。すべて読み取り済み。
pub(crate) struct GateFacts<'a> {
    /// kill-switch の判定結果 (lib-autonomy-policy が算出済み)。
    pub(crate) autonomy: AutonomyDecision,
    /// fix push 先のブランチ名。
    pub(crate) branch: &'a str,
    /// fix が実際に変更したファイルの `M path` 形式 summary。
    pub(crate) fix_diff_summary: &'a str,
    /// findings 由来の編集許可パス集合 (scope guard の allowlist)。
    pub(crate) allowlist: &'a BTreeSet<String>,
}

/// deny の理由。loud 出力・telemetry・exit コードの根拠になる。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DenyReason {
    /// kill-switch 側で停止 (ADR-066)。
    Autonomy(AutonomyDenyReason),
    /// ブランチが `claude/` prefix ではない (ADR-052 target 軸)。
    BranchNotIsolated(String),
    /// fix が何も変更していない。push するものが無い。
    EmptyFixDiff,
    /// diff summary をパースできない (ADR-054 fail-closed)。
    DiffUnparseable(String),
    /// 変更内容が自動実行可クラスではない (ADR-052 内容軸 / ADR-035 docs-only 基準)。
    ContentNotAutoExecutable,
    /// finding 対象外ファイルへの変更 (ADR-054 scope violation = injection の疑い)。
    OutOfScope(Vec<String>),
}

impl DenyReason {
    /// telemetry / grep 用の安定した短縮コード。可変データは含めない。
    pub(crate) fn code(&self) -> &'static str {
        match self {
            DenyReason::Autonomy(inner) => inner.code(),
            DenyReason::BranchNotIsolated(_) => "branch-not-isolated",
            DenyReason::EmptyFixDiff => "empty-fix-diff",
            DenyReason::DiffUnparseable(_) => "diff-unparseable",
            DenyReason::ContentNotAutoExecutable => "content-not-auto-executable",
            DenyReason::OutOfScope(_) => "scope-violation",
        }
    }

    /// 人間向けの 1 行説明。
    pub(crate) fn describe(&self, env_name: &str, config_path: &str) -> String {
        match self {
            DenyReason::Autonomy(inner) => inner.describe(env_name, config_path),
            DenyReason::BranchNotIsolated(branch) => format!(
                "ブランチ {branch:?} は {REQUIRED_BRANCH_PREFIX} prefix ではありません (ADR-052 target 軸により無人 push 不可)"
            ),
            DenyReason::EmptyFixDiff => {
                "fix による変更がありません (push するものが無いため何もしません)".to_string()
            }
            DenyReason::DiffUnparseable(detail) => {
                format!("fix diff summary をパースできません (fail-closed): {detail}")
            }
            DenyReason::ContentNotAutoExecutable => {
                "変更内容が自動実行可クラス (ADR-035 docs-only 基準) ではありません".to_string()
            }
            DenyReason::OutOfScope(files) => format!(
                "finding 対象外ファイルへの変更を検知 (injection の疑い、ADR-054): {files:?}"
            ),
        }
    }
}

/// 判定結果。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// 4 軸すべてを満たす。workflow は push してよい。
    Allowed { changed_files: Vec<String> },
    Denied(DenyReason),
}

/// 純粋判定コア。
///
/// 判定順は決定論的に「kill-switch → ブランチ → 空 diff → 内容軸 → scope」。
/// 空 diff を内容軸より先に見るのは、`is_docs_only_summary` が空入力へ `false` を返す仕様で、
/// そのままだと「変更なし」が「docs-only ではない」と誤って報告されるため。
pub(crate) fn evaluate(facts: &GateFacts<'_>) -> Verdict {
    if let AutonomyDecision::Denied(reason) = &facts.autonomy {
        return Verdict::Denied(DenyReason::Autonomy(reason.clone()));
    }
    if !facts.branch.starts_with(REQUIRED_BRANCH_PREFIX) {
        return Verdict::Denied(DenyReason::BranchNotIsolated(facts.branch.to_string()));
    }
    let changed = match lib_scope_guard::parse_changed_files(facts.fix_diff_summary) {
        Ok(files) => files,
        Err(detail) => return Verdict::Denied(DenyReason::DiffUnparseable(detail)),
    };
    if changed.is_empty() {
        return Verdict::Denied(DenyReason::EmptyFixDiff);
    }
    if !lib_docs_policy::is_docs_only_summary(facts.fix_diff_summary) {
        return Verdict::Denied(DenyReason::ContentNotAutoExecutable);
    }
    let out_of_scope = lib_scope_guard::find_out_of_scope(&changed, facts.allowlist);
    if !out_of_scope.is_empty() {
        return Verdict::Denied(DenyReason::OutOfScope(out_of_scope));
    }
    Verdict::Allowed {
        changed_files: changed,
    }
}

/// 全軸の状態を 1 行へ要約する (loud 出力用)。deny 理由を 1 つに絞る [`evaluate`] と違い、
/// こちらは 4 軸すべてを出して「どれを直せば通るか」を 1 run の log で分かるようにする。
pub(crate) fn describe_axes(facts: &GateFacts<'_>) -> String {
    let autonomy = match &facts.autonomy {
        AutonomyDecision::Allowed => "allowed".to_string(),
        AutonomyDecision::Denied(reason) => format!("denied({})", reason.code()),
    };
    let branch = if facts.branch.starts_with(REQUIRED_BRANCH_PREFIX) {
        "isolated"
    } else {
        "not-isolated"
    };
    let parsed = lib_scope_guard::parse_changed_files(facts.fix_diff_summary);
    let content = match &parsed {
        Err(_) => "unparseable",
        Ok(files) if files.is_empty() => "empty",
        Ok(_) if lib_docs_policy::is_docs_only_summary(facts.fix_diff_summary) => "docs-only",
        Ok(_) => "not-auto-executable",
    };
    let scope = match &parsed {
        Err(_) => "unknown".to_string(),
        Ok(files) => {
            let oos = lib_scope_guard::find_out_of_scope(files, facts.allowlist);
            if oos.is_empty() {
                format!("in-scope({} files)", files.len())
            } else {
                format!("violation({} files)", oos.len())
            }
        }
    };
    format!("autonomy={autonomy} branch={branch} content={content} scope={scope}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowlist(paths: &[&str]) -> BTreeSet<String> {
        lib_scope_guard::allowlist_from_paths(paths.iter().copied())
    }

    fn facts<'a>(
        autonomy: AutonomyDecision,
        branch: &'a str,
        summary: &'a str,
        allow: &'a BTreeSet<String>,
    ) -> GateFacts<'a> {
        GateFacts {
            autonomy,
            branch,
            fix_diff_summary: summary,
            allowlist: allow,
        }
    }

    fn allowed_autonomy() -> AutonomyDecision {
        AutonomyDecision::Allowed
    }

    fn denied_autonomy() -> AutonomyDecision {
        AutonomyDecision::Denied(AutonomyDenyReason::ExternalUnset)
    }

    #[test]
    fn allows_when_every_axis_is_satisfied() {
        let allow = allowlist(&["docs/a.md"]);
        let f = facts(allowed_autonomy(), "claude/nightly-1", "M docs/a.md\n", &allow);
        assert_eq!(
            evaluate(&f),
            Verdict::Allowed {
                changed_files: vec!["docs/a.md".to_string()]
            }
        );
    }

    /// kill-switch が最優先。他の 3 軸が揃っていても止まる。
    #[test]
    fn kill_switch_denies_regardless_of_other_axes() {
        let allow = allowlist(&["docs/a.md"]);
        let f = facts(denied_autonomy(), "claude/nightly-1", "M docs/a.md\n", &allow);
        assert_eq!(
            evaluate(&f),
            Verdict::Denied(DenyReason::Autonomy(AutonomyDenyReason::ExternalUnset))
        );
    }

    #[test]
    fn non_claude_branches_are_denied() {
        let allow = allowlist(&["docs/a.md"]);
        for branch in ["master", "feat/x", "claude", "claudex/y", "", "Claude/y"] {
            let f = facts(allowed_autonomy(), branch, "M docs/a.md\n", &allow);
            assert_eq!(
                evaluate(&f),
                Verdict::Denied(DenyReason::BranchNotIsolated(branch.to_string())),
                "branch {branch:?} は隔離 namespace ではない"
            );
        }
    }

    #[test]
    fn empty_diff_is_reported_as_empty_not_as_content_violation() {
        let allow = allowlist(&["docs/a.md"]);
        let f = facts(allowed_autonomy(), "claude/x", "", &allow);
        assert_eq!(evaluate(&f), Verdict::Denied(DenyReason::EmptyFixDiff));
    }

    #[test]
    fn unparseable_diff_is_fail_closed() {
        let allow = allowlist(&["docs/a.md"]);
        let f = facts(allowed_autonomy(), "claude/x", "R docs/a.md docs/b.md\n", &allow);
        assert!(matches!(
            evaluate(&f),
            Verdict::Denied(DenyReason::DiffUnparseable(_))
        ));
    }

    /// コード変更は allowlist に載っていても自動実行可クラスではない (ADR-052 内容軸)。
    #[test]
    fn code_changes_are_denied_even_when_in_allowlist() {
        let allow = allowlist(&["src/main.rs"]);
        let f = facts(allowed_autonomy(), "claude/x", "M src/main.rs\n", &allow);
        assert_eq!(
            evaluate(&f),
            Verdict::Denied(DenyReason::ContentNotAutoExecutable)
        );
    }

    /// `.claude/` / `.takt/` は形式上 md/yaml でも code-equivalent (ADR-035 除外パス)。
    #[test]
    fn harness_config_paths_are_not_docs_only() {
        let allow = allowlist(&[".claude/hooks-config.toml", ".takt/facets/instructions/fix.md"]);
        for path in [".claude/hooks-config.toml", ".takt/facets/instructions/fix.md"] {
            let summary = format!("M {path}\n");
            let f = facts(allowed_autonomy(), "claude/x", &summary, &allow);
            assert_eq!(
                evaluate(&f),
                Verdict::Denied(DenyReason::ContentNotAutoExecutable),
                "{path} は docs-only ではない"
            );
        }
    }

    /// docs-only でも finding 対象外なら scope violation (injection の疑い)。
    #[test]
    fn docs_change_outside_allowlist_is_scope_violation() {
        let allow = allowlist(&["docs/a.md"]);
        let f = facts(allowed_autonomy(), "claude/x", "M docs/a.md\nM docs/evil.md\n", &allow);
        assert_eq!(
            evaluate(&f),
            Verdict::Denied(DenyReason::OutOfScope(vec!["docs/evil.md".to_string()]))
        );
    }

    /// findings が空なら、どんな docs 変更も通さない。
    #[test]
    fn empty_allowlist_denies_any_change() {
        let allow = allowlist(&[]);
        let f = facts(allowed_autonomy(), "claude/x", "M docs/a.md\n", &allow);
        assert_eq!(
            evaluate(&f),
            Verdict::Denied(DenyReason::OutOfScope(vec!["docs/a.md".to_string()]))
        );
    }

    #[test]
    fn describe_axes_reports_all_four_axes() {
        let allow = allowlist(&["docs/a.md"]);
        let f = facts(denied_autonomy(), "feat/x", "M src/main.rs\n", &allow);
        let line = describe_axes(&f);
        assert!(line.contains("autonomy=denied(external-unset)"), "{line}");
        assert!(line.contains("branch=not-isolated"), "{line}");
        assert!(line.contains("content=not-auto-executable"), "{line}");
        assert!(line.contains("scope=violation(1 files)"), "{line}");
    }

    #[test]
    fn deny_codes_are_stable_and_data_free() {
        assert_eq!(DenyReason::BranchNotIsolated("x".into()).code(), "branch-not-isolated");
        assert_eq!(DenyReason::EmptyFixDiff.code(), "empty-fix-diff");
        assert_eq!(DenyReason::DiffUnparseable("d".into()).code(), "diff-unparseable");
        assert_eq!(DenyReason::ContentNotAutoExecutable.code(), "content-not-auto-executable");
        assert_eq!(DenyReason::OutOfScope(vec!["f".into()]).code(), "scope-violation");
        assert_eq!(
            DenyReason::Autonomy(AutonomyDenyReason::ExternalUnset).code(),
            "external-unset",
            "kill-switch 由来の deny は元の理由コードを保つ (原因切り分けのため)"
        );
    }
}
