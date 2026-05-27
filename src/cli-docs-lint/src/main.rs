//! cli-docs-lint — docs/ 整合性チェッカー CLI
//!
//! 使い方:
//!   cli-docs-lint                           全 check (preamble + cross-ref) 実行
//!   cli-docs-lint --check preamble          preamble 検査のみ
//!   cli-docs-lint --check cross-ref         cross-reference 検査のみ
//!   cli-docs-lint --docs-dir <path>         検査対象 docs/ ディレクトリ (default: ./docs)
//!
//! 終了コード:
//!   0 - 違反なし
//!   1 - 違反あり (stderr に詳細出力)
//!   2 - 引数エラーまたは I/O エラー

use cli_docs_lint::{cross_ref, preamble, Violation};
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, PartialEq, Eq)]
enum CheckMode {
    All,
    Preamble,
    CrossRef,
}

#[derive(Debug)]
struct CliArgs {
    mode: CheckMode,
    docs_dir: PathBuf,
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
                mode = match raw.as_str() {
                    "preamble" => CheckMode::Preamble,
                    "cross-ref" => CheckMode::CrossRef,
                    "all" => CheckMode::All,
                    other => {
                        return Err(format!(
                            "--check は preamble / cross-ref / all のいずれか (got: {})",
                            other
                        ))
                    }
                };
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

fn print_help() {
    eprintln!(
        "cli-docs-lint — docs/ 整合性チェッカー\n\n\
         Usage:\n  \
           cli-docs-lint [--check preamble|cross-ref|all] [--docs-dir <path>]\n\n\
         Checks:\n  \
           preamble   TODO 系 markdown の preamble 数詞 vs 実ファイル数\n  \
           cross-ref  docs/**/*.md の relative link validator (directory-aware)"
    );
}

fn run(args: &CliArgs) -> Result<Vec<Violation>, String> {
    let mut violations = Vec::new();
    if matches!(args.mode, CheckMode::All | CheckMode::Preamble) {
        violations.extend(preamble::check(&args.docs_dir)?);
    }
    if matches!(args.mode, CheckMode::All | CheckMode::CrossRef) {
        violations.extend(cross_ref::check(&args.docs_dir)?);
    }
    Ok(violations)
}

fn main() -> ExitCode {
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

fn describe_mode(mode: &CheckMode) -> &'static str {
    match mode {
        CheckMode::All => "preamble + cross-ref",
        CheckMode::Preamble => "preamble only",
        CheckMode::CrossRef => "cross-ref only",
    }
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

    #[test]
    fn parses_preamble_mode() {
        let parsed = parse_args(&args(&["--check", "preamble"])).unwrap();
        assert_eq!(parsed.mode, CheckMode::Preamble);
    }

    #[test]
    fn parses_cross_ref_mode() {
        let parsed = parse_args(&args(&["--check", "cross-ref"])).unwrap();
        assert_eq!(parsed.mode, CheckMode::CrossRef);
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
}
