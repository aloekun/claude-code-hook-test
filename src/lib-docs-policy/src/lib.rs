//! lib-docs-policy — ADR-035 (docs-only PR 評価ポリシー) の path 基準の**単一実装**。
//!
//! ADR-035 は「docs-only 判定基準が facet ごとに分散して drift した」ことを問題として
//! 起案された ADR で、その single source of truth を名乗る。したがって判定の実装が
//! 複数箇所に増えることは、ADR-035 が防ごうとした drift の再生産にあたる。
//! 判定を必要とする決定論層は本 crate を経由すること。
//!
//! ## 現在の呼び出し元
//!
//! - `cli-pr-monitor` の auto-push 前 gate (fix diff が docs-only なら gate skip)
//! - `cli-push-runner` の docs_only_routing stage (PR 範囲が docs-only なら
//!   docs で結果が変わり得ない quality_gate group を skip)
//!
//! ## 本 crate が判定する範囲 (path 基準のみ)
//!
//! ADR-035 の判定基準は **path 基準** と **diff 内容基準** (doc comment のみの `.rs`
//! 変更等) の両方から成るが、本 crate は path 基準だけを扱う。内容基準は AST 解析を
//! 要し path 文字列からは判定できないため、該当ケースは docs-only でないと判定される
//! = 呼び出し側でフル実行に倒れるだけで安全側に落ちる (ADR-043 fail-closed)。
//!
//! ADR-035 §Path 基準 の「疑わしきは docs-only ではない」を実装方針として採る。

/// `jj diff --summary` 出力が docs-only か判定する (ADR-035 path 基準)。
///
/// fail-closed: 空出力・パース不能な行 (rename 等の非 M/A/D 行)・除外パス・
/// 非 docs パスのいずれかがあれば false (= source 扱いで呼び出し側はフル実行)。
/// ADR-035 の diff 内容基準 (doc comment のみの .rs 変更等) は path だけでは
/// 判定できないため対象外 — その場合もフル実行されるだけで安全側に倒れる。
pub fn is_docs_only_summary(summary: &str) -> bool {
    let mut saw_any = false;
    for line in summary.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        saw_any = true;
        let Some((status, path)) = line.split_once(' ') else {
            return false;
        };
        if !matches!(status, "M" | "A" | "D") {
            return false;
        }
        if !is_docs_only_path(path) {
            return false;
        }
    }
    saw_any
}

/// 夜間ループ (ADR-072) のタスク台帳。
///
/// `docs/` 配下だが「次に何を実装するか」を決める**実行入力**であり、無人可マークの
/// 付いた行が `cli-nightly-task-select` に読まれ、その内容がそのまま無人 agent の
/// プロンプトへ入る = code-equivalent (ADR-035 除外パス)。
///
/// ADR-072 決定 6 は同じ理由で台帳を Guard step の禁止リストへ入れているが、PR 評価
/// ポリシー側の手当てが抜けており、**台帳だけを変える PR が docs-only の緩い評価経路に
/// 乗る**状態だった。前方一致ではなく完全一致で比較する — `docs/claude-code-web-tasks`
/// で始まる別ファイル (メモ等) まで巻き込むと、逆に正当な docs PR が重い経路へ落ちる。
const NIGHTLY_TASK_LEDGER: &str = "docs/claude-code-web-tasks.md";

/// 単一パスが ADR-035 の docs-only path 基準を満たすか。
///
/// `.takt/` / `.claude/` は形式上 md/yaml でも code-equivalent (ADR-035 除外パス)。
/// [`NIGHTLY_TASK_LEDGER`] も同じ理由で除外する。
/// Windows の `jj diff --summary` はバックスラッシュ区切りで出力するため正規化する。
fn is_docs_only_path(path: &str) -> bool {
    let p = path.trim().replace('\\', "/");
    if p.is_empty() {
        return false;
    }
    if p.starts_with(".takt/") || p.starts_with(".claude/") {
        return false;
    }
    if p == NIGHTLY_TASK_LEDGER {
        return false;
    }
    p.starts_with("docs/") || p.ends_with(".md")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn docs_only_accepts_all_docs_paths() {
        assert!(is_docs_only_summary(
            "M docs/adr/adr-001.md\nA docs/guide.md\nD docs/old.md"
        ));
    }

    #[test]
    fn docs_only_accepts_root_md() {
        assert!(is_docs_only_summary("M README.md"));
    }

    #[test]
    fn docs_only_rejects_source_path() {
        assert!(!is_docs_only_summary("M src/cli-pr-monitor/src/main.rs"));
    }

    #[test]
    fn docs_only_rejects_mixed_docs_and_source() {
        assert!(!is_docs_only_summary("M docs/a.md\nM src/lib.rs"));
    }

    #[test]
    fn docs_only_rejects_excluded_code_equivalent_paths() {
        assert!(!is_docs_only_summary("M .takt/facets/instructions/fix.md"));
        assert!(!is_docs_only_summary("M .claude/hooks-config.toml"));
        assert!(!is_docs_only_summary("M .takt/workflows/post-pr-review.yaml"));
    }

    /// 台帳 (ADR-072) は docs/ 配下でも code-equivalent なので docs-only にしない。
    #[test]
    fn docs_only_rejects_nightly_task_ledger() {
        assert!(!is_docs_only_summary("M docs/claude-code-web-tasks.md"));
    }

    /// 台帳が 1 つ混ざれば、他が純粋な docs でも PR 全体は docs-only ではない。
    #[test]
    fn docs_only_rejects_nightly_task_ledger_mixed_with_plain_docs() {
        assert!(!is_docs_only_summary(
            "M docs/adr/adr-072.md\nM docs/claude-code-web-tasks.md"
        ));
    }

    /// 前方一致で実装すると近い名前の正当な docs まで重い経路へ落ちる (完全一致の担保)。
    #[test]
    fn docs_only_accepts_paths_merely_prefixed_by_the_ledger_name() {
        assert!(is_docs_only_summary("M docs/claude-code-web-tasks-notes.md"));
        assert!(is_docs_only_summary("M docs/claude-code-web-tasks.md.bak"));
        assert!(is_docs_only_summary("M docs/archive/claude-code-web-tasks.md"));
    }

    #[test]
    fn docs_only_rejects_nightly_task_ledger_with_windows_backslash() {
        assert!(!is_docs_only_summary("M docs\\claude-code-web-tasks.md"));
    }

    #[test]
    fn docs_only_rejects_empty_summary() {
        assert!(!is_docs_only_summary(""));
        assert!(!is_docs_only_summary("  \n"));
    }

    #[test]
    fn docs_only_rejects_unparseable_lines() {
        assert!(!is_docs_only_summary("R docs/a.md docs/b.md"));
        assert!(!is_docs_only_summary("docs/a.md"));
    }

    #[test]
    fn docs_only_normalizes_windows_backslash_paths() {
        assert!(is_docs_only_summary("M docs\\notes.md"));
        assert!(!is_docs_only_summary("M .takt\\facets\\instructions\\fix.md"));
    }
}
