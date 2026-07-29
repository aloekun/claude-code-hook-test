//! workspace root 発見 (設計決定 2 § 入力)。
//!
//! `jj workspace list` の動的列挙 + config `extra_roots` で集計対象 root を集める。leak 発火は
//! improve workspace に偏在するため、発見漏れ + 発火 0 の組合せは誤 promote に直結する。よって
//! 発見が不完全な場合 (jj list 失敗 / 現 workspace 以外の root 未解決を extra_roots で補えない /
//! extra_roots 到達不能) は degraded を明示し、当該実行では判定候補の promote を抑止する
//! (集計・レポート生成は fail-open で継続する)。
//!
//! パースと degraded 判定は純粋関数 ([`parse_workspace_list`] / [`combine_roots`]) に分離し、
//! jj 実行・canonicalize・存在確認の I/O は [`discover_roots`] が担う。

use std::path::{Path, PathBuf};
use std::process::Command;

/// `jj workspace list` の 1 行 (テンプレート: name \t is_current \t root|<Error...>)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceLine {
    pub name: String,
    pub is_current: bool,
    /// 解決できた root 絶対パス。`self.root()` が `<Error...>` を返した場合は `None`。
    pub root: Option<String>,
}

/// 発見結果。`degraded` が空なら発見は完全 (promote 可)。
#[derive(Debug, Clone)]
pub struct RootDiscovery {
    pub roots: Vec<PathBuf>,
    pub degraded: Vec<String>,
    pub main_root: PathBuf,
}

impl RootDiscovery {
    /// degraded 理由が 1 件でもあれば true。
    pub fn is_degraded(&self) -> bool {
        !self.degraded.is_empty()
    }
}

/// `jj workspace list` テンプレート出力をパースする (pure)。3 タブ列に満たない行は skip。
pub fn parse_workspace_list(output: &str) -> Vec<WorkspaceLine> {
    output
        .lines()
        .filter_map(|line| {
            let mut it = line.splitn(3, '\t');
            let name = it.next()?.trim().to_string();
            let is_current = it.next()?.trim() == "true";
            let root_field = it.next()?.trim();
            if name.is_empty() {
                return None;
            }
            let root = if root_field.starts_with("<Error") || root_field.is_empty() {
                None
            } else {
                Some(root_field.to_string())
            };
            Some(WorkspaceLine {
                name,
                is_current,
                root,
            })
        })
        .collect()
}

/// 発見した情報から root 集合と degraded 理由を導く (pure)。
///
/// - `current_root`: exe 隣接 `.claude/` の親 (常に既知)。
/// - `resolved_workspace_roots`: jj list の非エラー行の root。
/// - `unresolved_non_current`: root 未解決かつ現 workspace でない workspace 数
///   (現 workspace は `current_root` で補えるため除外)。
/// - `jj_list_ok`: jj list コマンド自体が成功したか。
/// - `reachable_extra`: 存在確認できた extra_roots。
/// - `unreachable_extra`: 存在しない extra_roots (degraded 理由になる)。
pub fn combine_roots(
    current_root: PathBuf,
    resolved_workspace_roots: Vec<PathBuf>,
    unresolved_non_current: usize,
    jj_list_ok: bool,
    reachable_extra: Vec<PathBuf>,
    unreachable_extra: &[String],
) -> (Vec<PathBuf>, Vec<String>) {
    let mut roots = vec![current_root];
    roots.extend(resolved_workspace_roots);
    roots.extend(reachable_extra.iter().cloned());
    roots.sort();
    roots.dedup();

    let mut degraded = Vec::new();
    if !jj_list_ok {
        degraded.push(
            "jj workspace list の実行に失敗しました (他 workspace の telemetry を取り逃す可能性があるため degraded)"
                .to_string(),
        );
    }
    for extra in unreachable_extra {
        degraded.push(format!("extra_root が到達不能です: {extra}"));
    }
    if jj_list_ok && unresolved_non_current > reachable_extra.len() {
        degraded.push(format!(
            "現 workspace 以外で root 未解決の workspace が {unresolved_non_current} 件あり、到達可能な extra_roots ({} 件) で補いきれません (発火 0 判定の誤 promote を防ぐため degraded、extra_roots への明示追加を推奨)",
            reachable_extra.len()
        ));
    }
    (roots, degraded)
}

/// 実 I/O: jj を実行し config extra_roots と合わせて root 集合と main root を解決する。
///
/// `current_root` は exe 隣接 `.claude/` の親。`extra_roots` は config `[telemetry_report]` 由来。
pub fn discover_roots(current_root: &Path, extra_roots: &[String]) -> RootDiscovery {
    let current = canonicalize_or_as_is(current_root);
    let main_root = lib_jj_helpers::resolve_main_workspace_root(&current)
        .map(|p| canonicalize_or_as_is(&p))
        .unwrap_or_else(|| current.clone());

    let (jj_list_ok, lines) = run_jj_workspace_list(current_root);
    let parsed = parse_workspace_list(&lines);
    let resolved: Vec<PathBuf> = parsed
        .iter()
        .filter_map(|w| w.root.as_ref())
        .map(|p| canonicalize_or_as_is(Path::new(p)))
        .collect();
    let unresolved_non_current = parsed
        .iter()
        .filter(|w| w.root.is_none() && !w.is_current)
        .count();

    let (reachable_extra, unreachable_extra) = partition_extra_roots(extra_roots);
    let (roots, degraded) = combine_roots(
        current,
        resolved,
        unresolved_non_current,
        jj_list_ok,
        reachable_extra,
        &unreachable_extra,
    );

    RootDiscovery {
        roots,
        degraded,
        main_root,
    }
}

/// extra_roots を (存在するもの canonical, 存在しないもの) に分ける。
fn partition_extra_roots(extra_roots: &[String]) -> (Vec<PathBuf>, Vec<String>) {
    let mut reachable = Vec::new();
    let mut unreachable = Vec::new();
    for raw in extra_roots {
        let path = Path::new(raw);
        if path.is_dir() {
            reachable.push(canonicalize_or_as_is(path));
        } else {
            unreachable.push(raw.clone());
        }
    }
    (reachable, unreachable)
}

/// `jj workspace list` をテンプレート付きで実行し (成功可否, stdout) を返す。
/// `--ignore-working-copy` で snapshot を回避し read-only・高速化する。
fn run_jj_workspace_list(cwd: &Path) -> (bool, String) {
    let template =
        "self.name() ++ \"\\t\" ++ self.target().current_working_copy() ++ \"\\t\" ++ self.root() ++ \"\\n\"";
    let output = Command::new("jj")
        .args(["workspace", "list", "--ignore-working-copy", "-T", template])
        .current_dir(cwd)
        .output();
    match output {
        Ok(o) if o.status.success() => (true, String::from_utf8_lossy(&o.stdout).into_owned()),
        Ok(_) | Err(_) => (false, String::new()),
    }
}

/// canonicalize して Windows verbatim prefix (`\\?\`) を剥がす。失敗時は入力のまま。
fn canonicalize_or_as_is(path: &Path) -> PathBuf {
    match path.canonicalize() {
        Ok(p) => {
            let s = p.to_string_lossy();
            match s.strip_prefix(r"\\?\") {
                Some(stripped) => PathBuf::from(stripped),
                None => p,
            }
        }
        Err(_) => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_workspace_list_reads_columns() {
        let out = "ccht-improve\ttrue\t<Error: Failed to resolve>\ndefault\tfalse\tC:\\work\\main\n";
        let lines = parse_workspace_list(out);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].name, "ccht-improve");
        assert!(lines[0].is_current);
        assert!(lines[0].root.is_none(), "<Error は未解決");
        assert_eq!(lines[1].name, "default");
        assert!(!lines[1].is_current);
        assert_eq!(lines[1].root.as_deref(), Some("C:\\work\\main"));
    }

    #[test]
    fn parse_workspace_list_skips_malformed() {
        let lines = parse_workspace_list("incomplete-line\nname\ttrue\t/p\n\n");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].name, "name");
    }

    #[test]
    fn combine_roots_complete_when_all_resolved() {
        let (roots, degraded) = combine_roots(
            PathBuf::from("/cur"),
            vec![PathBuf::from("/main")],
            0,
            true,
            Vec::new(),
            &[],
        );
        assert!(degraded.is_empty(), "完全発見なら degraded なし");
        assert!(roots.contains(&PathBuf::from("/cur")));
        assert!(roots.contains(&PathBuf::from("/main")));
    }

    #[test]
    fn combine_roots_dedups_current_and_resolved() {
        let (roots, _) = combine_roots(
            PathBuf::from("/cur"),
            vec![PathBuf::from("/cur"), PathBuf::from("/main")],
            0,
            true,
            Vec::new(),
            &[],
        );
        assert_eq!(roots.len(), 2, "現 root と重複する解決 root は 1 つに");
    }

    #[test]
    fn combine_roots_current_workspace_error_is_not_degraded() {
        let (_, degraded) = combine_roots(
            PathBuf::from("/cur"),
            vec![PathBuf::from("/main")],
            0,
            true,
            Vec::new(),
            &[],
        );
        assert!(
            degraded.is_empty(),
            "現 workspace の root 解決失敗は unresolved_non_current に数えないため degraded にならない"
        );
    }

    #[test]
    fn combine_roots_degraded_when_unresolved_exceeds_extra() {
        let (_, degraded) = combine_roots(
            PathBuf::from("/cur"),
            Vec::new(),
            1,
            true,
            Vec::new(),
            &[],
        );
        assert_eq!(degraded.len(), 1);
        assert!(degraded[0].contains("root 未解決"));
    }

    #[test]
    fn combine_roots_extra_root_covers_unresolved() {
        let (roots, degraded) = combine_roots(
            PathBuf::from("/cur"),
            Vec::new(),
            1,
            true,
            vec![PathBuf::from("/improve")],
            &[],
        );
        assert!(degraded.is_empty(), "到達可能 extra_root が未解決数を補うと degraded 解消");
        assert!(roots.contains(&PathBuf::from("/improve")));
    }

    #[test]
    fn combine_roots_degraded_when_jj_list_failed() {
        let (_, degraded) = combine_roots(
            PathBuf::from("/cur"),
            Vec::new(),
            0,
            false,
            Vec::new(),
            &[],
        );
        assert_eq!(degraded.len(), 1);
        assert!(degraded[0].contains("jj workspace list"));
    }

    #[test]
    fn combine_roots_degraded_lists_unreachable_extra() {
        let (_, degraded) = combine_roots(
            PathBuf::from("/cur"),
            Vec::new(),
            0,
            true,
            Vec::new(),
            &["/nope".to_string()],
        );
        assert_eq!(degraded.len(), 1);
        assert!(degraded[0].contains("到達不能"));
        assert!(degraded[0].contains("/nope"));
    }
}
