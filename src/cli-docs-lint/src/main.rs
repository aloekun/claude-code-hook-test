//! cli-docs-lint — docs/ 整合性チェッカー CLI
//!
//! 使い方:
//!   cli-docs-lint                           全 check 実行 (登録簿 `CHECKS` の順)
//!   cli-docs-lint --check <name>            単一 check のみ (name は `--help` 参照)
//!   cli-docs-lint --docs-dir <path>         検査対象 docs/ ディレクトリ (default: ./docs)
//!
//! 終了コード:
//!   0 - 違反なし (または kill-switch 発動で skip)
//!   1 - 違反あり (stderr に詳細出力)
//!   2 - 引数エラーまたは I/O エラー
//!
//! # 試験運用ステータス (ADR-039 標準パターン適用)
//!
//! 本 binary は新規 lint として導入されたため、ADR-039 の試験運用標準パターン
//! (config opt-in + kill-switch + bounded lifetime) を適用する。
//!
//! - **Config opt-in**: 派生 repo の `templates/push-runner-config.toml` では
//!   `pnpm lint:docs` を `quality_gate.lint` commands から除外 (= default OFF)。
//!   本リポジトリの `push-runner-config.toml` で明示的に追加して dogfood を開始。
//! - **Kill-switch**: 環境変数 `CLI_DOCS_LINT_DISABLE=1` を設定すると検査を
//!   skip して exit code 0 で終了する (= 個別 push の意図的バイパス)。永続的な
//!   無効化は `push-runner-config.toml` の `quality_gate.lint` commands から
//!   `pnpm lint:docs` を削除する revert PR で行う。
//! - **Bounded lifetime**: 本リポジトリで 3-5 PR の dogfood (false positive 観測 /
//!   検出効果 / override 使用頻度) 後に、`templates/push-runner-config.toml` への
//!   default-ON 昇格 or 却下を判定する。判定結果は本 module doc と
//!   `push-runner-config.toml` の `[cli_docs_lint]` section コメントに反映する。

use cli_docs_lint::{cross_ref, entry_pairing, preamble, priority_inversion, Violation};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// 1 つの check の定義。
///
/// **新しい check を足すときに触るのは [`CHECKS`] の 1 行だけ**にする。統合前は
/// `CheckMode` の variant / `parse_args` の match / エラーメッセージ / `print_help` の
/// Usage 行 / 同 Checks 一覧 / `run` の if 節 / `describe_mode` の arm の **7 箇所**へ
/// 同じ事実を書き写す必要があり、実際に `entry-pairing` は `print_help` の Checks
/// 一覧だけ書き漏れて help から消えていた (defect-convergence-plan.md § Phase F の F1)。
struct CheckSpec {
    /// `--check` に渡す名前。
    name: &'static str,
    /// `--help` に出す 1 行説明。
    summary: &'static str,
    /// 実行本体。全 validator が同じ署名を持つ。
    run: fn(&Path) -> Result<Vec<Violation>, String>,
}

/// 実行順に並べた全 check。**ここが唯一の登録簿**。
const CHECKS: &[CheckSpec] = &[
    CheckSpec {
        name: "preamble",
        summary: "TODO 系 markdown の preamble 数詞 vs 実ファイル数",
        run: preamble::check,
    },
    CheckSpec {
        name: "cross-ref",
        summary: "docs/**/*.md の relative link validator (directory-aware)",
        run: cross_ref::check,
    },
    CheckSpec {
        name: "priority-inversion",
        summary: "todo-summary*.md table の Tier N→Tier N+k 依存を検知",
        run: priority_inversion::check,
    },
    CheckSpec {
        name: "entry-pairing",
        summary: "順位 table 行 ⇄ todoN.md 詳細エントリの 1:1 対応 (順位 441)",
        run: entry_pairing::check,
    },
];

#[derive(Debug, PartialEq, Eq)]
enum CheckMode {
    All,
    /// [`CHECKS`] の index。
    Single(usize),
}

#[derive(Debug)]
struct CliArgs {
    mode: CheckMode,
    docs_dir: PathBuf,
}

/// `--check` に指定できる名前の一覧 (`all` を含む)。
fn check_names(separator: &str) -> String {
    let mut names: Vec<&str> = CHECKS.iter().map(|c| c.name).collect();
    names.push("all");
    names.join(separator)
}

fn parse_args(args: &[String]) -> Result<CliArgs, String> {
    let mut mode = CheckMode::All;
    let mut docs_dir = PathBuf::from("docs");
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--check" => {
                i += 1;
                let raw = args.get(i).ok_or("--check には引数が必要です")?;
                mode = parse_check_mode(raw)?;
            }
            "--docs-dir" => {
                i += 1;
                let raw = args.get(i).ok_or("--docs-dir には引数が必要です")?;
                docs_dir = PathBuf::from(raw);
            }
            "--help" | "-h" => {
                return Err("HELP".to_string());
            }
            other => return Err(format!("不明な引数: {}", other)),
        }
        i += 1;
    }
    Ok(CliArgs { mode, docs_dir })
}

fn parse_check_mode(raw: &str) -> Result<CheckMode, String> {
    if raw == "all" {
        return Ok(CheckMode::All);
    }
    CHECKS
        .iter()
        .position(|c| c.name == raw)
        .map(CheckMode::Single)
        .ok_or_else(|| {
            format!("--check は {} のいずれか (got: {})", check_names(" / "), raw)
        })
}

fn help_text() -> String {
    // 列幅は登録簿から取る (名前の最長に合わせる)。固定値だと check 追加で崩れる。
    let width = CHECKS.iter().map(|c| c.name.len()).max().unwrap_or(0) + 2;
    let checks: Vec<String> = CHECKS
        .iter()
        .map(|c| format!("  {:<width$}{}", c.name, c.summary, width = width))
        .collect();
    format!(
        "cli-docs-lint — docs/ 整合性チェッカー\n\n\
         Usage:\n  \
           cli-docs-lint [--check {}] [--docs-dir <path>]\n\n\
         Checks:\n{}",
        check_names("|"),
        checks.join("\n")
    )
}

fn print_help() {
    eprintln!("{}", help_text());
}

fn run(args: &CliArgs) -> Result<Vec<Violation>, String> {
    let mut violations = Vec::new();
    for spec in selected_checks(&args.mode) {
        violations.extend((spec.run)(&args.docs_dir)?);
    }
    Ok(violations)
}

/// mode が選ぶ check 群 (`All` は登録順に全件)。
fn selected_checks(mode: &CheckMode) -> Vec<&'static CheckSpec> {
    match mode {
        CheckMode::All => CHECKS.iter().collect(),
        CheckMode::Single(i) => vec![&CHECKS[*i]],
    }
}

const KILL_SWITCH_ENV: &str = "CLI_DOCS_LINT_DISABLE";

fn is_kill_switch_value(raw: Option<&str>) -> bool {
    match raw {
        Some(v) => v == "1" || v.eq_ignore_ascii_case("true"),
        None => false,
    }
}

fn is_kill_switch_enabled() -> bool {
    is_kill_switch_value(std::env::var(KILL_SWITCH_ENV).ok().as_deref())
}

fn main() -> ExitCode {
    if is_kill_switch_enabled() {
        eprintln!(
            "[cli-docs-lint] SKIP — kill-switch env var {}=1 detected (ADR-039 試験運用 bypass)",
            KILL_SWITCH_ENV
        );
        return ExitCode::from(0);
    }

    let args: Vec<String> = std::env::args().collect();
    let parsed = match parse_args(&args) {
        Ok(p) => p,
        Err(e) if e == "HELP" => {
            print_help();
            return ExitCode::from(0);
        }
        Err(e) => {
            eprintln!("[cli-docs-lint] 引数エラー: {}", e);
            print_help();
            return ExitCode::from(2);
        }
    };

    match run(&parsed) {
        Ok(violations) if violations.is_empty() => {
            eprintln!("[cli-docs-lint] OK ({})", describe_mode(&parsed.mode));
            ExitCode::from(0)
        }
        Ok(violations) => {
            eprintln!(
                "[cli-docs-lint] {} violation(s) found:",
                violations.len()
            );
            for v in &violations {
                eprintln!("  {}", v);
            }
            ExitCode::from(1)
        }
        Err(e) => {
            eprintln!("[cli-docs-lint] 実行エラー: {}", e);
            ExitCode::from(2)
        }
    }
}

fn describe_mode(mode: &CheckMode) -> String {
    match mode {
        CheckMode::All => check_names_of(CHECKS).join(" + "),
        CheckMode::Single(i) => format!("{} only", CHECKS[*i].name),
    }
}

fn check_names_of(specs: &[CheckSpec]) -> Vec<&'static str> {
    specs.iter().map(|c| c.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(extra: &[&str]) -> Vec<String> {
        let mut v = vec!["cli-docs-lint".to_string()];
        v.extend(extra.iter().map(|s| s.to_string()));
        v
    }

    #[test]
    fn default_mode_is_all() {
        let parsed = parse_args(&args(&[])).unwrap();
        assert_eq!(parsed.mode, CheckMode::All);
        assert_eq!(parsed.docs_dir, PathBuf::from("docs"));
    }

    /// 登録簿の全 check が `--check <name>` で選べる。**新しい check を足しても
    /// このテストは書き換え不要**で、登録漏れ側だけが落ちる。
    #[test]
    fn every_registered_check_is_selectable_by_name() {
        for (i, spec) in CHECKS.iter().enumerate() {
            let parsed = parse_args(&args(&["--check", spec.name])).unwrap();
            assert_eq!(parsed.mode, CheckMode::Single(i), "{}", spec.name);
        }
    }

    /// `--help` の Checks 一覧に全 check が出る。**`entry-pairing` が help から
    /// 漏れていた実害 (F1) の回帰テスト。**
    #[test]
    fn help_lists_every_registered_check() {
        let help = help_text();
        for spec in CHECKS {
            assert!(help.contains(spec.name), "{} が help に無い:
{help}", spec.name);
            assert!(help.contains(spec.summary), "{} の説明が help に無い", spec.name);
        }
    }

    /// `all` は登録簿の全件を実行する (実行漏れの検知)。
    #[test]
    fn all_mode_selects_every_registered_check() {
        let selected = selected_checks(&CheckMode::All);
        assert_eq!(selected.len(), CHECKS.len());
        let described = describe_mode(&CheckMode::All);
        for spec in CHECKS {
            assert!(described.contains(spec.name), "{} が describe_mode に無い", spec.name);
        }
    }

    /// 登録名の重複は `--check` の解決を先勝ちで壊す。
    #[test]
    fn registered_names_are_unique() {
        let mut names = check_names_of(CHECKS);
        names.sort_unstable();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count, "{names:?}");
    }

    /// `lib.rs` に生えた check module が [`CHECKS`] へ登録されないと、その検査は
    /// **静かに実行されなくなる** (false-green)。module 宣言と登録簿の対応をここで固定する。
    ///
    /// module 名 → check 名は `_` → `-` の機械変換で、この対応自体もここが唯一の規定。
    #[test]
    fn every_check_module_is_registered() {
        /// check を持たない共有 module (登録簿に載らないのが正しいもの)。
        /// **追加は意図的な判断**であり、素通しさせないためここへ明示する。
        const NON_CHECK_MODULES: &[&str] = &["docs_files"];

        let registered = check_names_of(CHECKS);
        let modules = include_str!("lib.rs")
            .lines()
            .filter_map(|l| l.trim().strip_prefix("pub mod ").and_then(|r| r.strip_suffix(';')))
            .filter(|m| !NON_CHECK_MODULES.contains(m));
        for module in modules {
            let name = module.replace('_', "-");
            assert!(
                registered.contains(&name.as_str()),
                "module {module} が CHECKS に未登録です (期待する check 名: {name})"
            );
        }
    }

    #[test]
    fn parses_docs_dir_override() {
        let parsed = parse_args(&args(&["--docs-dir", "some/other"])).unwrap();
        assert_eq!(parsed.docs_dir, PathBuf::from("some/other"));
    }

    #[test]
    fn rejects_unknown_check() {
        let err = parse_args(&args(&["--check", "spelling"])).unwrap_err();
        assert!(err.contains("preamble"));
    }

    #[test]
    fn rejects_unknown_flag() {
        let err = parse_args(&args(&["--no-such"])).unwrap_err();
        assert!(err.contains("不明な引数"));
    }

    #[test]
    fn help_is_signaled_separately() {
        let err = parse_args(&args(&["--help"])).unwrap_err();
        assert_eq!(err, "HELP");
    }

    #[test]
    fn kill_switch_value_one_enables() {
        assert!(is_kill_switch_value(Some("1")));
    }

    #[test]
    fn kill_switch_value_true_case_insensitive() {
        assert!(is_kill_switch_value(Some("true")));
        assert!(is_kill_switch_value(Some("TRUE")));
        assert!(is_kill_switch_value(Some("True")));
    }

    #[test]
    fn kill_switch_value_unset_means_disabled() {
        assert!(!is_kill_switch_value(None));
    }

    #[test]
    fn kill_switch_value_empty_or_zero_means_disabled() {
        assert!(!is_kill_switch_value(Some("")));
        assert!(!is_kill_switch_value(Some("0")));
        assert!(!is_kill_switch_value(Some("false")));
    }
}
