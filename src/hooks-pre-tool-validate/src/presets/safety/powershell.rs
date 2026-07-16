//! powershell-destructive-write-block プリセット。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

pub(crate) const POWERSHELL_DESTRUCTIVE_WRITE_MSG: &str = r#"**PowerShell からの破壊的ファイル書込がブロックされました**

PowerShell の `[System.IO.File]::WriteAllText` / `WriteAllBytes` / `WriteAllLines` / `Out-File` /
`Set-Content -Value` は **null / 空文字列を渡された場合に対象ファイルを 0 byte に消去** します。

過去事故 (PR #213): `IndexOf` で marker を探したが文字コードズレで -1 が返り、
`Substring(0, -1)` が例外を投げて `$newContent = $null` のまま `WriteAllText` を実行、
src/check-ci-coderabbit/src/main.rs (2369 行) を消失させた。

**代替方法 (安全な書込手段):**
- 単純な置換: `Edit` tool で old_string / new_string 指定
- 新規ファイル作成: `Write` tool で content 指定
- in-place 編集が必要なら: `Bash` tool で `sed -i 's/PATTERN/REPL/' file`
- jj の lifecycle に乗せたいなら: 編集後に `jj describe` で commit

**例外 (allow される case):**
- `__*` prefix の scratch ファイル ('__dump.json' 等) への書込は規約 (CLAUDE.md § Scratch / 一時ファイル命名規約)
  により VCS 管理外のため許可

設計判断 (順位 212): memory `feedback_no_powershell_inplace_edit` の機械強制層。"#;

/// プリセット: powershell-destructive-write-block (PowerShell からの破壊的ファイル書込防止)
///
/// 順位 212 (PR #213 post-merge-feedback feedback-T1-1 + session 派生統合採用):
/// PR #213 (refactor PR A) 作業中、PowerShell スクリプトで `check-ci-coderabbit/src/main.rs`
/// (2369 行) を **0 byte に消去** する事故が発生した。連鎖失敗の構造:
///   ① `IndexOf` の検索 marker に CRLF (`` `r`n ``) を埋め込んだが file は LF only
///   ② `IndexOf` が `-1` を返すも `Substring(0, -1)` 直接呼び出し → `MethodInvocationException`
///   ③ PowerShell default `$ErrorActionPreference = Continue` で script 続行
///   ④ `$newContent = $null` のまま `[System.IO.File]::WriteAllText($path, $null)` → 空ファイル
///
/// memory `feedback_no_powershell_inplace_edit.md` で人間 / AI 規範として codify 済だが、
/// memory は揮発する懸念があり mechanical defense として本 preset を併設する。
///
/// 検出対象 (5 patterns):
/// - `[System.IO.File]::WriteAllText(` (今回事故の直接因)
/// - `[System.IO.File]::WriteAllBytes(`
/// - `[System.IO.File]::WriteAllLines(`
/// - `Out-File` (redirect 系 cmdlet)
/// - `Set-Content -Value` (cmdlet 版書き込み)
///
/// Exception: 文字列リテラル中に `__` prefix (scratch ファイル規約) を含む場合は allow。
/// 例: `[System.IO.File]::WriteAllText("__dump.json", ...)` は scratch ファイル明示のため通す。
pub(crate) fn preset_powershell_destructive_write() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?i)\[System\.IO\.File\]::WriteAllText\s*\(").unwrap(),
            exception: Some(
                Regex::new(r#"(?i)\[System\.IO\.File\]::WriteAllText\s*\(\s*['"]__"#).unwrap(),
            ),
            message: POWERSHELL_DESTRUCTIVE_WRITE_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)\[System\.IO\.File\]::WriteAllBytes\s*\(").unwrap(),
            exception: Some(
                Regex::new(r#"(?i)\[System\.IO\.File\]::WriteAllBytes\s*\(\s*['"]__"#).unwrap(),
            ),
            message: POWERSHELL_DESTRUCTIVE_WRITE_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)\[System\.IO\.File\]::WriteAllLines\s*\(").unwrap(),
            exception: Some(
                Regex::new(r#"(?i)\[System\.IO\.File\]::WriteAllLines\s*\(\s*['"]__"#).unwrap(),
            ),
            message: POWERSHELL_DESTRUCTIVE_WRITE_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)\bOut-File\b").unwrap(),
            exception: Some(Regex::new(r#"(?i)\bOut-File\b\s+(?:-FilePath\s+)?['"]?__"#).unwrap()),
            message: POWERSHELL_DESTRUCTIVE_WRITE_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)\bSet-Content\b[^|]*-Value").unwrap(),
            exception: Some(
                Regex::new(
                    r#"(?i)\bSet-Content\b(?:\s+['"]?__|[^|]*\s-(?:Literal)?Path\s+['"]?__)"#,
                )
                .unwrap(),
            ),
            message: POWERSHELL_DESTRUCTIVE_WRITE_MSG,
        },
    ]
}

#[cfg(test)]
mod tests {
    use crate::blocked_patterns::{build_blocked_patterns, validate_command, SourcedPattern};
    use crate::config::{Config, PreToolValidateConfig};

    fn patterns_with_presets(presets: &[&str]) -> Vec<SourcedPattern> {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(presets.iter().map(|s| s.to_string()).collect()),
                extra_protected_files: None,
                todo_staleness: None,
            }),
        };
        build_blocked_patterns(&config)
    }

    fn is_blocked_with(command: &str, presets: &[&str]) -> bool {
        let patterns = patterns_with_presets(presets);
        validate_command(command, &patterns).is_some()
    }

    const PS_DESTRUCTIVE: &[&str] = &["powershell-destructive-write-block"];

    #[test]
    fn powershell_blocks_writealltext_to_production_file() {
        assert!(is_blocked_with(
            r#"[System.IO.File]::WriteAllText("src/main.rs", $content)"#,
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_blocks_writeallbytes_to_production_file() {
        assert!(is_blocked_with(
            r#"[System.IO.File]::WriteAllBytes("data.bin", $bytes)"#,
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_blocks_writealllines_to_production_file() {
        assert!(is_blocked_with(
            r#"[System.IO.File]::WriteAllLines("log.txt", $lines)"#,
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_blocks_out_file_redirect() {
        assert!(is_blocked_with(
            "Get-Process | Out-File processes.txt",
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_blocks_set_content_with_value() {
        assert!(is_blocked_with(
            r#"Set-Content -Path src/config.toml -Value $newConfig"#,
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_allows_writealltext_when_path_argument_is_scratch_prefix() {
        assert!(!is_blocked_with(
            r#"[System.IO.File]::WriteAllText("__dump.json", $json)"#,
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_allows_out_file_when_path_argument_is_scratch_prefix() {
        assert!(!is_blocked_with(
            r#"Get-Process | Out-File "__processes.log""#,
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_allows_get_childitem_read_only_cmdlet() {
        assert!(!is_blocked_with("Get-ChildItem ./src", PS_DESTRUCTIVE));
    }

    #[test]
    fn powershell_allows_where_object_pipeline_read_only() {
        assert!(!is_blocked_with(
            r#"Get-Process | Where-Object {$_.CPU -gt 100}"#,
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_allows_set_content_bare_invocation_without_value_flag_to_avoid_false_positive() {
        assert!(!is_blocked_with("Set-Content", PS_DESTRUCTIVE));
    }

    #[test]
    fn powershell_destructive_write_is_default_on_in_default_config() {
        let patterns = build_blocked_patterns(&Config::default());
        assert!(
            validate_command(
                r#"[System.IO.File]::WriteAllText("foo.rs", $null)"#,
                &patterns
            )
            .is_some(),
            "default fallback should include powershell-destructive-write-block (順位 212 で default-on)"
        );
    }

    #[test]
    fn powershell_blocks_writealltext_with_lowercase_dot_net_method_name() {
        assert!(
            is_blocked_with(
                r#"[system.io.file]::writealltext("src/main.rs", $content)"#,
                PS_DESTRUCTIVE
            ),
            "PowerShell の .NET method 名は case-insensitive のため lowercase でも block すべき (W-002 SEC fix)"
        );
    }

    #[test]
    fn powershell_blocks_writeallbytes_with_lowercase_dot_net_method_name() {
        assert!(is_blocked_with(
            r#"[system.io.file]::writeallbytes("data.bin", $bytes)"#,
            PS_DESTRUCTIVE
        ));
    }

    #[test]
    fn powershell_blocks_writealltext_when_scratch_string_is_in_value_position_not_path() {
        assert!(
            is_blocked_with(
                r#"[System.IO.File]::WriteAllText("prod.rs", "__placeholder content")"#,
                PS_DESTRUCTIVE
            ),
            "path が prod.rs で value が \"__...\" の場合は block すべき (W-001 SEC fix: exception は path 位置にのみ scope)"
        );
    }

    #[test]
    fn powershell_blocks_writealltext_when_command_reads_scratch_but_writes_to_production() {
        assert!(
            is_blocked_with(
                r#"$x = [System.IO.File]::ReadAllText("__scratch.txt"); [System.IO.File]::WriteAllText("prod.rs", $null)"#,
                PS_DESTRUCTIVE
            ),
            "ReadAllText から __ scratch を読んでも、WriteAllText が prod.rs に書くなら block (W-001 SEC fix: exception scope)"
        );
    }

    #[test]
    fn powershell_blocks_set_content_with_value_in_reversed_parameter_order() {
        assert!(
            is_blocked_with("Set-Content -Value $x -Path src/main.rs", PS_DESTRUCTIVE),
            "Set-Content は -Value と -Path の順序非依存で block すべき (W-001 SIMP fix)"
        );
    }

    #[test]
    fn powershell_allows_set_content_with_scratch_path_in_reversed_parameter_order() {
        assert!(
            !is_blocked_with(r#"Set-Content "__file.txt" -Value $x"#, PS_DESTRUCTIVE),
            "scratch path で positional argument 指定は allow"
        );
        assert!(
            !is_blocked_with(
                r#"Set-Content -Value $x -Path "__file.txt""#,
                PS_DESTRUCTIVE
            ),
            "-Value が -Path より先でも scratch path は allow (PR #215 CR Minor #1 fix: exception の order-independence を main pattern と整合)"
        );
        assert!(
            !is_blocked_with(
                r#"Set-Content -Value $x -LiteralPath "__file.txt""#,
                PS_DESTRUCTIVE
            ),
            "-Value 先行 + -LiteralPath でも scratch path は allow"
        );
    }

    #[test]
    fn powershell_allows_out_file_with_unquoted_scratch_path() {
        assert!(
            !is_blocked_with("Out-File __output.txt", PS_DESTRUCTIVE),
            "unquoted な __ prefix scratch path も allow (W-002 SIMP fix)"
        );
    }

    #[test]
    fn powershell_blocks_set_content_when_scratch_string_in_value_position_not_path() {
        assert!(
            is_blocked_with(
                r#"Set-Content -Path "prod.rs" -Value "__placeholder""#,
                PS_DESTRUCTIVE
            ),
            "path が prod.rs で value が \"__...\" の場合は block すべき (Set-Content 版 W-001 SEC)"
        );
    }
}
