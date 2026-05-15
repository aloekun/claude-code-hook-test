//! PostToolUse リンターフック (設定駆動型)
//!
//! Write/Edit ツール使用後にファイルに対してリンター/フォーマッターを実行し、
//! 診断結果を additionalContext として Claude にフィードバックします。
//!
//! .claude/hooks-config.toml の [post_tool_linter] セクションから
//! 拡張子ごとのパイプラインを読み込みます。

use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

// --- 入力 ---

#[derive(Deserialize)]
struct HookInput {
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    file_path: Option<String>,
    path: Option<String>,
}

// --- 出力 ---

#[derive(Serialize)]
struct HookOutput {
    #[serde(rename = "hookSpecificOutput")]
    hook_specific_output: HookSpecificOutput,
}

#[derive(Serialize)]
struct HookSpecificOutput {
    #[serde(rename = "hookEventName")]
    hook_event_name: String,
    #[serde(rename = "additionalContext")]
    additional_context: String,
}

// --- 設定 ---

#[derive(Deserialize, Default)]
struct Config {
    post_tool_linter: Option<PostToolLinterConfig>,
}

#[derive(Deserialize, Default)]
struct PostToolLinterConfig {
    pipelines: Option<Vec<PipelineConfig>>,
}

#[derive(Deserialize, Clone)]
struct PipelineConfig {
    extensions: Vec<String>,
    steps: Vec<StepConfig>,
}

#[derive(Deserialize, Clone)]
struct StepConfig {
    cmd: String,
    args: Vec<String>,
    fix: bool,
}

// --- カスタムルール設定 (custom-lint-rules.toml) ---

#[derive(Deserialize, Default)]
struct CustomRulesConfig {
    rules: Option<Vec<CustomRule>>,
}

/// `custom-lint-rules.toml` の `[[rules]]` エントリ。
///
/// **サポート field 一覧** (rule author 向け reference、`.claude/custom-lint-rules.toml` 冒頭コメントと整合):
///
/// | field | 必須 | semantics |
/// |---|---|---|
/// | `id` | ✅ | ルール一意 identifier |
/// | `pattern` | ✅ | 検出する正規表現 (case-insensitive にしたい場合は `(?i)` prefix を pattern 内に明示) |
/// | `severity` | ✅ | `"error"` or `"warning"` |
/// | `message` | ✅ | 違反時のメッセージ |
/// | `extensions` | ✅ | 対象拡張子の list (例: `["rs", "toml"]`)。空配列を使うと全 file が対象になる anti-pattern なので避ける |
/// | `why` | optional | ルールの根拠 (ADR 参照 / PR 由来等)。省略可だが post-merge-feedback 由来は明記推奨 |
/// | `paths` | optional | glob pattern による file path filter (順位 102 land 済)。指定時は `extensions` との **AND** 結合で評価。例: `paths = ["docs/**/*.md"]` で docs/ 配下のみ対象。未指定 (None) または空配列は「path filter なし」(= `extensions` のみで判定) |
/// | `fix` | optional | `CustomRuleFix` (strategy + steps) |
/// | `example` | optional | `CustomRuleExample` (bad + good) |
///
/// **glob syntax** (`globset` crate 準拠):
///
/// - `*` = 同階層の 0+ 文字 (path separator は含まない)
/// - `**` = 任意階層の recursive match (`docs/**/*.md` は `docs/a.md` / `docs/adr/b.md` 両方マッチ)
/// - `?` = 単一文字
/// - `[abc]` = 文字 class
///
/// **`extensions` × `paths` の AND 結合の意義**: `extensions` は file 種別 (rust / toml / md) を絞る軸、
/// `paths` は file 位置 (docs/ 配下 / tests/ 配下) を絞る軸で直交。両方マッチで初めて rule 対象とすることで、
/// rule scope を明示的に二次元で表現できる (ADR-007 amendment 順位 104 で codify 予定)。
#[derive(Deserialize, Clone)]
struct CustomRule {
    id: String,
    pattern: String,
    severity: String,
    message: String,
    #[serde(default)]
    why: String,
    extensions: Vec<String>,
    #[serde(default)]
    paths: Option<Vec<String>>,
    fix: Option<CustomRuleFix>,
    example: Option<CustomRuleExample>,
}

#[derive(Deserialize, Clone)]
struct CustomRuleFix {
    strategy: String,
    steps: Vec<String>,
}

#[derive(Deserialize, Clone)]
struct CustomRuleExample {
    bad: String,
    good: String,
}

// --- カスタムルール構造化出力 (additionalContext 用) ---

#[derive(Serialize)]
struct LintViolation {
    r#type: String,
    severity: String,
    location: ViolationLocation,
    message: String,
    why: String,
    fix: ViolationFix,
    example: ViolationExample,
}

#[derive(Serialize)]
struct ViolationLocation {
    file: String,
    line: usize,
    symbol: String,
}

#[derive(Serialize)]
struct ViolationFix {
    strategy: String,
    steps: Vec<String>,
}

#[derive(Serialize)]
struct ViolationExample {
    bad: String,
    good: String,
}

/// デフォルトパイプライン (設定ファイルが無い場合のフォールバック)
fn default_pipelines() -> Vec<PipelineConfig> {
    vec![
        PipelineConfig {
            extensions: vec!["ts".into(), "tsx".into(), "js".into(), "jsx".into()],
            steps: vec![
                StepConfig {
                    cmd: "npx".into(),
                    args: vec![
                        "--no-install".into(),
                        "biome".into(),
                        "format".into(),
                        "--write".into(),
                        "{file}".into(),
                    ],
                    fix: true,
                },
                StepConfig {
                    cmd: "npx".into(),
                    args: vec![
                        "--no-install".into(),
                        "oxlint".into(),
                        "--fix".into(),
                        "{file}".into(),
                    ],
                    fix: true,
                },
                StepConfig {
                    cmd: "npx".into(),
                    args: vec!["--no-install".into(), "oxlint".into(), "{file}".into()],
                    fix: false,
                },
            ],
        },
        PipelineConfig {
            extensions: vec!["py".into()],
            steps: vec![
                StepConfig {
                    cmd: "ruff".into(),
                    args: vec!["check".into(), "--fix".into(), "{file}".into()],
                    fix: true,
                },
                StepConfig {
                    cmd: "ruff".into(),
                    args: vec!["format".into(), "{file}".into()],
                    fix: true,
                },
                StepConfig {
                    cmd: "ruff".into(),
                    args: vec!["check".into(), "{file}".into()],
                    fix: false,
                },
            ],
        },
    ]
}

/// 設定ファイルのパス解決
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定ファイルを読み込む
fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!(
                "[post-tool-linter] Warning: Failed to parse {}: {}",
                path.display(),
                e
            );
            Config::default()
        }),
        Err(_) => Config::default(),
    }
}

/// ファイル拡張子に一致するパイプラインを検索
fn find_pipeline<'a>(file: &str, pipelines: &'a [PipelineConfig]) -> Option<&'a PipelineConfig> {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())?;

    pipelines
        .iter()
        .find(|p| p.extensions.iter().any(|e| e.to_lowercase() == ext))
}

/// コマンドを実行し、(stdout, stderr) を返す
/// シェル (cmd /c) を経由しないため、ファイルパスのメタ文字によるインジェクションを防止する
fn run_command(program: &str, args: &[String]) -> (String, String) {
    match Command::new(program).args(args).output() {
        Ok(o) => (
            String::from_utf8_lossy(&o.stdout).to_string(),
            String::from_utf8_lossy(&o.stderr).to_string(),
        ),
        Err(e) => (String::new(), format!("Failed to run {}: {}", program, e)),
    }
}

/// stdout と stderr を適切に結合する
fn combine_output(stdout: &str, stderr: &str) -> String {
    if stdout.is_empty() {
        stderr.to_string()
    } else if stderr.is_empty() {
        stdout.to_string()
    } else if stdout.ends_with('\n') {
        format!("{}{}", stdout, stderr)
    } else {
        format!("{}\n{}", stdout, stderr)
    }
}

/// フィードバック JSON を stdout に出力
fn emit_feedback(message: &str) {
    let output = HookOutput {
        hook_specific_output: HookSpecificOutput {
            hook_event_name: "PostToolUse".to_string(),
            additional_context: message.to_string(),
        },
    };
    if let Ok(json) = serde_json::to_string(&output) {
        println!("{}", json);
    }
}

/// args 内の {file} プレースホルダーをファイルパスに置換
fn resolve_args(args: &[String], file: &str) -> Vec<String> {
    args.iter().map(|a| a.replace("{file}", file)).collect()
}

/// パイプラインを実行
fn run_pipeline(file: &str, pipeline: &PipelineConfig) {
    let mut diagnostics = String::new();

    for step in &pipeline.steps {
        let resolved = resolve_args(&step.args, file);
        let (stdout, stderr) = run_command(&step.cmd, &resolved);

        if !step.fix {
            // 診断ステップ: 出力を収集
            let combined = combine_output(&stdout, &stderr);
            if !combined.trim().is_empty() {
                if !diagnostics.is_empty() {
                    diagnostics.push('\n');
                }
                diagnostics.push_str(&combined);
            }
        }
        // fix ステップ: 出力を捨てて続行
    }

    // 診断結果があればフィードバック (先頭20行に制限)
    let trimmed: String = diagnostics.lines().take(20).collect::<Vec<_>>().join("\n");
    if !trimmed.trim().is_empty() {
        emit_feedback(&trimmed);
    }
}

/// カスタムルール設定ファイルのパス解決
fn custom_rules_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("custom-lint-rules.toml")
}

/// コンパイル済み正規表現と paths glob set を持つルール。
///
/// `paths_glob` は `rule.paths` が `Some(non-empty)` の場合のみ compiled GlobSet を保持し、
/// `None` (path filter なし) では `None` を保持する。Empty Vec は **filter なし** として扱う
/// (= `None` と同等) ことで「[]` と `None` の semantic 差を排除し、`Option<Vec<String>>` の意味を
/// 「未指定 or 明示空 = 全 path 受容」に統一する。
struct CompiledRule {
    rule: CustomRule,
    regex: Regex,
    paths_glob: Option<GlobSet>,
}

/// `CustomRule::paths` を GlobSet に compile する。
///
/// - `None` または `Some(empty Vec)` → `Ok(None)` (filter なし)
/// - `Some(non-empty)` で全 glob valid → `Ok(Some(GlobSet))`
/// - 1 つでも glob が invalid → `Err(error message)` (rule 全体を破棄)
fn compile_paths_glob(paths: &Option<Vec<String>>) -> Result<Option<GlobSet>, String> {
    let Some(pattern_list) = paths else {
        return Ok(None);
    };
    if pattern_list.is_empty() {
        return Ok(None);
    }
    let mut builder = GlobSetBuilder::new();
    for pattern in pattern_list {
        let glob = Glob::new(pattern).map_err(|e| format!("invalid glob '{}': {}", pattern, e))?;
        builder.add(glob);
    }
    builder
        .build()
        .map(Some)
        .map_err(|e| format!("failed to build GlobSet: {}", e))
}

/// `CustomRule` 単体を compile し、`CompiledRule` を返す。失敗時は warn log + None。
fn compile_rule(rule: CustomRule) -> Option<CompiledRule> {
    let regex = match Regex::new(&rule.pattern) {
        Ok(r) => r,
        Err(e) => {
            eprintln!(
                "[post-tool-linter] Warning: Invalid regex in rule '{}': {}",
                rule.id, e
            );
            return None;
        }
    };
    let paths_glob = match compile_paths_glob(&rule.paths) {
        Ok(g) => g,
        Err(msg) => {
            eprintln!(
                "[post-tool-linter] Warning: rule '{}' paths filter compile failed, dropping rule: {}",
                rule.id, msg
            );
            return None;
        }
    };
    Some(CompiledRule {
        rule,
        regex,
        paths_glob,
    })
}

/// カスタムルール設定を読み込み、正規表現をプリコンパイルする
fn load_custom_rules() -> Vec<CompiledRule> {
    let path = custom_rules_path();
    let rules = match std::fs::read_to_string(&path) {
        Ok(content) => {
            let config: CustomRulesConfig = toml::from_str(&content).unwrap_or_else(|e| {
                eprintln!(
                    "[post-tool-linter] Warning: Failed to parse {}: {}",
                    path.display(),
                    e
                );
                CustomRulesConfig::default()
            });
            config.rules.unwrap_or_default()
        }
        Err(_) => return Vec::new(),
    };

    for missing_id in find_powershell_rules_missing_case_insensitive_flag(&rules) {
        eprintln!(
            "[post-tool-linter] Warning: rule '{}' targets ps1 but lacks (?i) flag (PowerShell is case-insensitive — see ~/.claude/rules/common/code-review.md)",
            missing_id
        );
    }

    rules.into_iter().filter_map(compile_rule).collect()
}

fn find_powershell_rules_missing_case_insensitive_flag(rules: &[CustomRule]) -> Vec<String> {
    rules
        .iter()
        .filter(|r| r.extensions.iter().any(|e| e.eq_ignore_ascii_case("ps1")))
        .filter(|r| !r.pattern.contains("(?i)"))
        .map(|r| r.id.clone())
        .collect()
}

/// カスタムルール違反の最大出力件数 (外部ツール診断の20行制限と同等)
const MAX_CUSTOM_VIOLATIONS: usize = 20;

/// ファイル拡張子がルールの対象かチェック
fn rule_matches_ext(rule: &CustomRule, file: &str) -> bool {
    let ext = Path::new(file)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext {
        Some(ext) => rule.extensions.iter().any(|e| e.to_lowercase() == ext),
        None => false,
    }
}

/// `compiled.paths_glob` が `None` (filter なし) または `Some(GlobSet)` で file path がマッチする場合 true。
///
/// 順位 102 (PR #140 T1-#2 採用、Phase D D-3): `extensions` filter と AND 結合で評価する path filter。
/// 比較対象は **path 全体** で、Unix-style separator (`/`) のみで matching する。Windows path 入力
/// (`\` 含む) は事前に normalize しておく必要があるが、本 hook の入力 (`tool_input.file_path` /
/// `tool_input.path`) は Claude Code が POSIX-style で渡すため通常は問題なし。
fn rule_matches_path(compiled: &CompiledRule, file: &str) -> bool {
    let Some(globset) = compiled.paths_glob.as_ref() else {
        return true;
    };
    let normalized = file.replace('\\', "/");
    globset.is_match(&normalized)
}

/// 1 件の regex match と rule 定義から `LintViolation` の JSON 文字列を構築する。
///
/// `m.start()` 以前の `\n` 数 + 1 を 1-indexed line number として算出 (`find_iter` の byte
/// offset を line 番号に変換するため line-by-line search では捕捉できない multiline pattern
/// = 例: PowerShell `} catch {\n}` にも対応)。
fn build_violation_json(
    file: &str,
    rule: &CustomRule,
    m: regex::Match,
    content: &str,
) -> Option<String> {
    let line_no = content[..m.start()].bytes().filter(|b| *b == b'\n').count() + 1;
    let violation = LintViolation {
        r#type: rule.id.to_uppercase().replace('-', "_"),
        severity: rule.severity.clone(),
        location: ViolationLocation {
            file: file.to_string(),
            line: line_no,
            symbol: m.as_str().to_string(),
        },
        message: rule.message.clone(),
        why: rule.why.clone(),
        fix: ViolationFix {
            strategy: rule
                .fix
                .as_ref()
                .map_or_else(String::new, |f| f.strategy.clone()),
            steps: rule.fix.as_ref().map_or_else(Vec::new, |f| f.steps.clone()),
        },
        example: ViolationExample {
            bad: rule
                .example
                .as_ref()
                .map_or_else(String::new, |e| e.bad.clone()),
            good: rule
                .example
                .as_ref()
                .map_or_else(String::new, |e| e.good.clone()),
        },
    };
    serde_json::to_string(&violation).ok()
}

fn collect_violations_for_rule(
    file: &str,
    content: &str,
    compiled: &CompiledRule,
    violations: &mut Vec<String>,
) {
    for m in compiled.regex.find_iter(content) {
        if violations.len() >= MAX_CUSTOM_VIOLATIONS {
            return;
        }
        if let Some(json) = build_violation_json(file, &compiled.rule, m, content) {
            violations.push(json);
        }
    }
}

fn run_custom_rules(file: &str, rules: &[CompiledRule]) -> Vec<String> {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut violations = Vec::new();

    for compiled in rules {
        if !rule_matches_ext(&compiled.rule, file) {
            continue;
        }
        if !rule_matches_path(compiled, file) {
            continue;
        }
        collect_violations_for_rule(file, &content, compiled, &mut violations);
        if violations.len() >= MAX_CUSTOM_VIOLATIONS {
            break;
        }
    }

    violations
}

/// UTF-8 整合性チェック: U+FFFD (置換文字) の検出
///
/// AI ツールの Edit/Write でマルチバイト文字が破壊されると、
/// U+FFFD が残るか、raw invalid bytes が生成される。
/// `std::fs::read` + `from_utf8_lossy` で両方のケースを捕捉する。
fn check_utf8_integrity(file: &str) -> Vec<String> {
    let bytes = match std::fs::read(file) {
        Ok(b) => b,
        Err(_) => return Vec::new(),
    };

    let content = String::from_utf8_lossy(&bytes);

    let mut violations = Vec::new();

    for (line_idx, line) in content.lines().enumerate() {
        if violations.len() >= MAX_CUSTOM_VIOLATIONS {
            break;
        }

        if line.contains('\u{FFFD}') {
            let violation = LintViolation {
                r#type: "UTF8_INTEGRITY".to_string(),
                severity: "error".to_string(),
                location: ViolationLocation {
                    file: file.to_string(),
                    line: line_idx + 1,
                    symbol: "\u{FFFD}".to_string(),
                },
                message: "U+FFFD (replacement character) detected — possible mojibake from AI edit"
                    .to_string(),
                why: "AI tool edits can corrupt multi-byte characters (e.g., Japanese text). Fix before commit."
                    .to_string(),
                fix: ViolationFix {
                    strategy: "Restore the original text from version control history".to_string(),
                    steps: vec![
                        "Check the original content with `jj diff` or `git diff`".to_string(),
                        "Restore the corrupted characters manually".to_string(),
                    ],
                },
                example: ViolationExample {
                    bad: "進みま\u{FFFD}\u{FFFD}。".to_string(),
                    good: "進みます。".to_string(),
                },
            };

            if let Ok(json) = serde_json::to_string(&violation) {
                violations.push(json);
            }
        }
    }

    violations
}

fn main() {
    let config = load_config();

    // stdin を消費（フックの仕様上必須）
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[post-tool-linter] Warning: Failed to read stdin: {}", e);
        return;
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(_) => return,
    };

    let file = hook_input
        .tool_input
        .and_then(|t| t.file_path.filter(|s| !s.is_empty()).or(t.path))
        .unwrap_or_default();

    if file.is_empty() {
        return;
    }

    // 第0層: UTF-8 整合性チェック (全ファイル対象, ~1ms)
    let utf8_violations = check_utf8_integrity(&file);
    if !utf8_violations.is_empty() {
        let feedback = format!(
            "[utf8-integrity] {} violation(s) found:\n{}",
            utf8_violations.len(),
            utf8_violations.join("\n")
        );
        emit_feedback(&feedback);
        return;
    }

    // 第1層: カスタムルール (正規表現ベース, ~1ms)
    let compiled_rules = load_custom_rules();
    let violations = run_custom_rules(&file, &compiled_rules);
    if !violations.is_empty() {
        let feedback = format!(
            "[custom-lint] {} violation(s) found:\n{}",
            violations.len(),
            violations.join("\n")
        );
        emit_feedback(&feedback);
    }

    // 第2層: 外部ツールパイプライン (biome, oxlint, ruff 等)
    let pipelines = config
        .post_tool_linter
        .and_then(|c| c.pipelines)
        .unwrap_or_else(default_pipelines);

    if let Some(pipeline) = find_pipeline(&file, &pipelines) {
        run_pipeline(&file, pipeline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- パイプライン検索テスト ---

    #[test]
    fn finds_ts_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("src/app.ts", &pipelines).is_some());
    }

    #[test]
    fn finds_tsx_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("components/App.tsx", &pipelines).is_some());
    }

    #[test]
    fn finds_js_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("index.js", &pipelines).is_some());
    }

    #[test]
    fn finds_jsx_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("Component.jsx", &pipelines).is_some());
    }

    #[test]
    fn finds_py_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("main.py", &pipelines).is_some());
    }

    #[test]
    fn finds_py_windows_path() {
        let pipelines = default_pipelines();
        assert!(find_pipeline(r"e:\work\project\src\app.py", &pipelines).is_some());
    }

    #[test]
    fn finds_py_case_insensitive() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("file.PY", &pipelines).is_some());
        assert!(find_pipeline("file.Py", &pipelines).is_some());
    }

    #[test]
    fn rs_has_no_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("main.rs", &pipelines).is_none());
    }

    #[test]
    fn json_has_no_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("package.json", &pipelines).is_none());
    }

    #[test]
    fn no_extension_has_no_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("Makefile", &pipelines).is_none());
    }

    #[test]
    fn empty_has_no_pipeline() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("", &pipelines).is_none());
    }

    #[test]
    fn windows_path_ts() {
        let pipelines = default_pipelines();
        assert!(find_pipeline(r"e:\work\project\src\app.ts", &pipelines).is_some());
    }

    #[test]
    fn case_insensitive_ts() {
        let pipelines = default_pipelines();
        assert!(find_pipeline("file.TS", &pipelines).is_some());
        assert!(find_pipeline("file.Tsx", &pipelines).is_some());
    }

    // --- 出力結合 ---

    #[test]
    fn combine_empty_stdout() {
        assert_eq!(combine_output("", "error"), "error");
    }

    #[test]
    fn combine_empty_stderr() {
        assert_eq!(combine_output("output", ""), "output");
    }

    #[test]
    fn combine_both_with_trailing_newline() {
        assert_eq!(combine_output("output\n", "error"), "output\nerror");
    }

    #[test]
    fn combine_both_without_trailing_newline() {
        assert_eq!(combine_output("output", "error"), "output\nerror");
    }

    // --- フィードバック JSON ---

    #[test]
    fn feedback_json_has_correct_structure() {
        let output = HookOutput {
            hook_specific_output: HookSpecificOutput {
                hook_event_name: "PostToolUse".to_string(),
                additional_context: "test diagnostic".to_string(),
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains(r#""hookEventName":"PostToolUse""#));
        assert!(json.contains(r#""additionalContext":"test diagnostic""#));
    }

    // --- args 置換 ---

    #[test]
    fn resolve_args_replaces_file() {
        let args = vec!["check".to_string(), "{file}".to_string()];
        let resolved = resolve_args(&args, "src/app.ts");
        assert_eq!(resolved, vec!["check", "src/app.ts"]);
    }

    #[test]
    fn resolve_args_no_placeholder() {
        let args = vec!["--fix".to_string()];
        let resolved = resolve_args(&args, "src/app.ts");
        assert_eq!(resolved, vec!["--fix"]);
    }

    // --- カスタムパイプライン ---

    #[test]
    fn custom_pipeline_matches() {
        let pipelines = vec![PipelineConfig {
            extensions: vec!["go".into()],
            steps: vec![StepConfig {
                cmd: "gofmt".into(),
                args: vec!["-w".into(), "{file}".into()],
                fix: true,
            }],
        }];
        assert!(find_pipeline("main.go", &pipelines).is_some());
        assert!(find_pipeline("main.rs", &pipelines).is_none());
    }

    // --- デフォルトパイプライン ---

    #[test]
    fn default_pipelines_has_ts_and_py() {
        let pipelines = default_pipelines();
        assert_eq!(pipelines.len(), 2);
        assert!(pipelines[0].extensions.contains(&"ts".to_string()));
        assert!(pipelines[1].extensions.contains(&"py".to_string()));
    }

    #[test]
    fn ts_pipeline_has_3_steps() {
        let pipelines = default_pipelines();
        assert_eq!(pipelines[0].steps.len(), 3);
    }

    #[test]
    fn py_pipeline_has_3_steps() {
        let pipelines = default_pipelines();
        assert_eq!(pipelines[1].steps.len(), 3);
    }

    #[test]
    fn fix_steps_come_before_check() {
        let pipelines = default_pipelines();
        for p in &pipelines {
            // fix ステップが先、check (fix=false) が最後
            let last = p.steps.last().unwrap();
            assert!(!last.fix, "Last step should be a check (fix=false)");
        }
    }

    // --- カスタムルール: ルール拡張子マッチ ---

    fn make_test_rule(id: &str, pattern: &str, extensions: &[&str]) -> CustomRule {
        CustomRule {
            id: id.into(),
            pattern: pattern.into(),
            severity: "error".into(),
            message: "test message".into(),
            why: "test reason".into(),
            extensions: extensions.iter().map(|e| e.to_string()).collect(),
            paths: None,
            fix: Some(CustomRuleFix {
                strategy: "test strategy".into(),
                steps: vec!["step1".into()],
            }),
            example: Some(CustomRuleExample {
                bad: "bad code".into(),
                good: "good code".into(),
            }),
        }
    }

    fn make_test_rule_with_paths(
        id: &str,
        pattern: &str,
        extensions: &[&str],
        paths: &[&str],
    ) -> CustomRule {
        let mut rule = make_test_rule(id, pattern, extensions);
        rule.paths = Some(paths.iter().map(|p| p.to_string()).collect());
        rule
    }

    #[test]
    fn rule_matches_ts_extension() {
        let rule = make_test_rule("test", "pattern", &["ts", "tsx"]);
        assert!(rule_matches_ext(&rule, "src/app.ts"));
        assert!(rule_matches_ext(&rule, "src/App.tsx"));
    }

    #[test]
    fn rule_does_not_match_other_extension() {
        let rule = make_test_rule("test", "pattern", &["ts"]);
        assert!(!rule_matches_ext(&rule, "main.rs"));
        assert!(!rule_matches_ext(&rule, "style.css"));
    }

    #[test]
    fn rule_matches_case_insensitive() {
        let rule = make_test_rule("test", "pattern", &["ts"]);
        assert!(rule_matches_ext(&rule, "file.TS"));
        assert!(rule_matches_ext(&rule, "file.Ts"));
    }

    #[test]
    fn rule_no_match_for_no_extension() {
        let rule = make_test_rule("test", "pattern", &["ts"]);
        assert!(!rule_matches_ext(&rule, "Makefile"));
        assert!(!rule_matches_ext(&rule, ""));
    }

    #[test]
    fn rule_matches_windows_path() {
        let rule = make_test_rule("test", "pattern", &["ts"]);
        assert!(rule_matches_ext(&rule, r"e:\work\project\src\app.ts"));
    }

    /// 順位 102 (PR #140 T1-#2 採用): paths filter 未指定 (None) → 全 path 受容 (filter なし扱い)
    #[test]
    fn paths_filter_none_accepts_any_path() {
        let rule = make_test_rule("test", "x", &["md"]);
        let compiled = compile_rule(rule).expect("rule must compile");
        assert!(rule_matches_path(&compiled, "any/file.md"));
        assert!(rule_matches_path(&compiled, "docs/adr/foo.md"));
        assert!(rule_matches_path(&compiled, "README.md"));
    }

    /// 順位 102: paths filter empty (Some(vec![])) → None と同等扱い (全 path 受容)
    #[test]
    fn paths_filter_empty_vec_accepts_any_path() {
        let rule = make_test_rule_with_paths("test", "x", &["md"], &[]);
        let compiled = compile_rule(rule).expect("rule must compile");
        assert!(rule_matches_path(&compiled, "any/file.md"));
    }

    /// 順位 102: paths filter `docs/**/*.md` で docs 配下のみ match (rule⑧ の migration target)
    #[test]
    fn paths_filter_recursive_glob_matches_docs_only() {
        let rule = make_test_rule_with_paths("test", "x", &["md"], &["docs/**/*.md"]);
        let compiled = compile_rule(rule).expect("rule must compile");
        assert!(rule_matches_path(&compiled, "docs/spec.md"));
        assert!(rule_matches_path(&compiled, "docs/adr/adr-001.md"));
        assert!(rule_matches_path(&compiled, "docs/a/b/c/deep.md"));
        assert!(!rule_matches_path(&compiled, "README.md"));
        assert!(!rule_matches_path(&compiled, "CLAUDE.md"));
    }

    /// 順位 102: Windows-style backslash path も normalize して match できる (Claude Code hook 実環境想定)
    #[test]
    fn paths_filter_normalizes_windows_separators() {
        let rule = make_test_rule_with_paths("test", "x", &["md"], &["docs/**/*.md"]);
        let compiled = compile_rule(rule).expect("rule must compile");
        assert!(rule_matches_path(&compiled, r"docs\adr\adr-001.md"));
    }

    /// 順位 102: paths filter は複数 glob を OR で評価 (= いずれか 1 つに match で受容)
    #[test]
    fn paths_filter_multiple_globs_or_semantics() {
        let rule =
            make_test_rule_with_paths("test", "x", &["md"], &["docs/**/*.md", "tests/**/*.md"]);
        let compiled = compile_rule(rule).expect("rule must compile");
        assert!(rule_matches_path(&compiled, "docs/foo.md"));
        assert!(rule_matches_path(&compiled, "tests/integration.md"));
        assert!(!rule_matches_path(&compiled, "src/main.md"));
    }

    /// 順位 102: invalid glob は compile_rule 段階で reject されて rule 自体が drop される
    #[test]
    fn paths_filter_invalid_glob_drops_rule() {
        let rule = make_test_rule_with_paths("test", "x", &["md"], &["docs/[unclosed"]);
        assert!(
            compile_rule(rule).is_none(),
            "invalid glob in paths should cause compile_rule to drop the rule"
        );
    }

    /// 順位 102: extensions × paths AND 結合 = 拡張子マッチ AND path マッチ 両方を要求
    #[test]
    fn run_custom_rules_extensions_and_paths_are_anded() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let docs_dir = dir.path().join("docs");
        std::fs::create_dir(&docs_dir).unwrap();
        let in_docs = docs_dir.join("foo.md");
        let mut f = std::fs::File::create(&in_docs).unwrap();
        f.write_all(b"FORBIDDEN\n").unwrap();

        let outside = dir.path().join("README.md");
        let mut f2 = std::fs::File::create(&outside).unwrap();
        f2.write_all(b"FORBIDDEN\n").unwrap();

        let rule = make_test_rule_with_paths("test", "FORBIDDEN", &["md"], &["**/docs/**/*.md"]);
        let compiled = compile_test_rules(vec![rule]);

        let in_docs_violations = run_custom_rules(in_docs.to_str().unwrap(), &compiled);
        let outside_violations = run_custom_rules(outside.to_str().unwrap(), &compiled);

        assert_eq!(
            in_docs_violations.len(),
            1,
            "docs 配下 + .md = 両方マッチで violation 検出"
        );
        assert!(
            outside_violations.is_empty(),
            "root-level README.md は paths filter で除外 (= AND の片方が false)"
        );
    }

    /// テスト用: CustomRule からコンパイル済みルールを生成するヘルパー
    fn compile_test_rules(rules: Vec<CustomRule>) -> Vec<CompiledRule> {
        rules.into_iter().filter_map(compile_rule).collect()
    }

    #[test]
    fn run_custom_rules_detects_console_log() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "const x = 1;").unwrap();
            writeln!(f, "console.log('debug');").unwrap();
            writeln!(f, "const y = 2;").unwrap();
        }

        let rules = compile_test_rules(vec![make_test_rule(
            "no-console-log",
            r"console\.log\(",
            &["ts"],
        )]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);

        assert_eq!(violations.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        assert_eq!(v["type"], "NO_CONSOLE_LOG");
        assert_eq!(v["severity"], "error");
        assert_eq!(v["location"]["line"], 2);
        assert_eq!(v["message"], "test message");
    }

    #[test]
    fn run_custom_rules_no_violation_on_clean_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clean.ts");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "const x = 1;").unwrap();
            writeln!(f, "logger.info('message');").unwrap();
        }

        let rules = compile_test_rules(vec![make_test_rule(
            "no-console-log",
            r"console\.log\(",
            &["ts"],
        )]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);

        assert!(violations.is_empty());
    }

    #[test]
    fn run_custom_rules_skips_non_matching_extension() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.rs");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "console.log('should be ignored');").unwrap();
        }

        let rules = compile_test_rules(vec![make_test_rule(
            "no-console-log",
            r"console\.log\(",
            &["ts"],
        )]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);

        assert!(violations.is_empty());
    }

    #[test]
    fn run_custom_rules_multiple_violations() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("multi.ts");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "console.log('first');").unwrap();
            writeln!(f, "const x = 1;").unwrap();
            writeln!(f, "console.log('second');").unwrap();
        }

        let rules = compile_test_rules(vec![make_test_rule(
            "no-console-log",
            r"console\.log\(",
            &["ts"],
        )]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);

        assert_eq!(violations.len(), 2);
        let v1: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&violations[1]).unwrap();
        assert_eq!(v1["location"]["line"], 1);
        assert_eq!(v2["location"]["line"], 3);
    }

    #[test]
    fn run_custom_rules_respects_max_violations() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("many.ts");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            for i in 0..30 {
                writeln!(f, "console.log('line {}');", i).unwrap();
            }
        }

        let rules = compile_test_rules(vec![make_test_rule(
            "no-console-log",
            r"console\.log\(",
            &["ts"],
        )]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);

        assert_eq!(violations.len(), MAX_CUSTOM_VIOLATIONS);
    }

    #[test]
    fn run_custom_rules_outer_break_skips_subsequent_rules() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("outer_break.ts");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            for i in 0..21 {
                writeln!(f, "console.log('cl {}');", i).unwrap();
            }
            for i in 0..5 {
                writeln!(f, "alert('al {}');", i).unwrap();
            }
        }

        let rules = compile_test_rules(vec![
            make_test_rule("rule-a", r"console\.log\(", &["ts"]),
            make_test_rule("rule-b", r"alert\(", &["ts"]),
        ]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);

        assert_eq!(violations.len(), MAX_CUSTOM_VIOLATIONS);
        for raw in &violations {
            let v: serde_json::Value = serde_json::from_str(raw).unwrap();
            assert_eq!(
                v["type"], "RULE_A",
                "rule-b must not run once rule-a exhausts the cap"
            );
        }
    }

    #[test]
    fn run_custom_rules_inner_cap_after_partial_first_rule() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("inner_cap.ts");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            for i in 0..19 {
                writeln!(f, "console.log('cl {}');", i).unwrap();
            }
            for i in 0..5 {
                writeln!(f, "alert('al {}');", i).unwrap();
            }
        }

        let rules = compile_test_rules(vec![
            make_test_rule("rule-a", r"console\.log\(", &["ts"]),
            make_test_rule("rule-b", r"alert\(", &["ts"]),
        ]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);

        assert_eq!(violations.len(), MAX_CUSTOM_VIOLATIONS);
        let mut rule_a_count = 0;
        let mut rule_b_count = 0;
        for raw in &violations {
            let v: serde_json::Value = serde_json::from_str(raw).unwrap();
            match v["type"].as_str() {
                Some("RULE_A") => rule_a_count += 1,
                Some("RULE_B") => rule_b_count += 1,
                other => panic!("unexpected violation type: {other:?}"),
            }
        }
        assert_eq!(rule_a_count, 19);
        assert_eq!(rule_b_count, 1);
    }

    #[test]
    fn compile_test_rules_filters_invalid_regex() {
        let rules = vec![
            make_test_rule("bad-rule", r"[invalid(", &["ts"]),
            make_test_rule("good-rule", r"console\.log\(", &["ts"]),
        ];
        let compiled = compile_test_rules(rules);

        // 不正な正規表現のルールはフィルタされ、有効なルールのみ残る
        assert_eq!(compiled.len(), 1);
        assert_eq!(compiled[0].rule.id, "good-rule");
    }

    #[test]
    fn run_custom_rules_nonexistent_file() {
        let rules = compile_test_rules(vec![make_test_rule("test", r"pattern", &["ts"])]);
        let violations = run_custom_rules("/nonexistent/file.ts", &rules);
        assert!(violations.is_empty());
    }

    // --- カスタムルール: 構造化 JSON 出力 ---

    #[test]
    fn violation_json_has_all_fields() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("test.ts");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "console.log('x');").unwrap();
        }

        let rules = compile_test_rules(vec![make_test_rule(
            "no-console-log",
            r"console\.log\(",
            &["ts"],
        )]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();

        // 記事のフォーマットに準拠した全フィールドの存在を確認
        assert!(v.get("type").is_some());
        assert!(v.get("severity").is_some());
        assert!(v.get("location").is_some());
        assert!(v["location"].get("file").is_some());
        assert!(v["location"].get("line").is_some());
        assert!(v["location"].get("symbol").is_some());
        assert!(v.get("message").is_some());
        assert!(v.get("why").is_some());
        assert!(v.get("fix").is_some());
        assert!(v["fix"].get("strategy").is_some());
        assert!(v["fix"].get("steps").is_some());
        assert!(v.get("example").is_some());
        assert!(v["example"].get("bad").is_some());
        assert!(v["example"].get("good").is_some());
    }

    // --- カスタムルール: TOML パース ---

    #[test]
    fn parse_custom_rules_toml() {
        let toml_str = r#"
[[rules]]
id = "no-console-log"
pattern = 'console\.log\('
severity = "error"
message = "console.log は禁止"
why = "デバッグコード残留防止"
extensions = ["ts", "tsx"]

[rules.fix]
strategy = "削除 or logger置換"
steps = ["console.log行を削除する"]

[rules.example]
bad = "console.log('x');"
good = "logger.debug('x');"
"#;

        let config: CustomRulesConfig = toml::from_str(toml_str).unwrap();
        let rules = config.rules.unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].id, "no-console-log");
        assert_eq!(rules[0].severity, "error");
        assert_eq!(rules[0].extensions, vec!["ts", "tsx"]);
        assert!(rules[0].fix.is_some());
        assert!(rules[0].example.is_some());
    }

    // --- UTF-8 整合性チェック ---

    #[test]
    fn utf8_integrity_detects_fffd() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mojibake.rs");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "let msg = \"進みま\u{FFFD}\u{FFFD}。\";").unwrap();
        }

        let violations = check_utf8_integrity(file.to_str().unwrap());
        assert_eq!(violations.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        assert_eq!(v["type"], "UTF8_INTEGRITY");
        assert_eq!(v["severity"], "error");
        assert_eq!(v["location"]["line"], 1);
        assert_eq!(v["location"]["symbol"], "\u{FFFD}");
    }

    #[test]
    fn utf8_integrity_clean_file() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("clean.rs");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "let msg = \"正常な日本語テキスト\";").unwrap();
        }

        let violations = check_utf8_integrity(file.to_str().unwrap());
        assert!(violations.is_empty());
    }

    #[test]
    fn utf8_integrity_invalid_raw_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("invalid.txt");
        // 0xFF 0xFE は有効な UTF-8 シーケンスではない
        std::fs::write(&file, b"hello \xFF\xFE world").unwrap();

        let violations = check_utf8_integrity(file.to_str().unwrap());
        assert_eq!(violations.len(), 1);
        let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        assert_eq!(v["type"], "UTF8_INTEGRITY");
    }

    #[test]
    fn utf8_integrity_multiple_lines() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("multi.rs");
        {
            let mut f = std::fs::File::create(&file).unwrap();
            writeln!(f, "let a = \"正常\";").unwrap();
            writeln!(f, "let b = \"壊れた\u{FFFD}文字\";").unwrap();
            writeln!(f, "let c = \"正常\";").unwrap();
            writeln!(f, "let d = \"また\u{FFFD}\u{FFFD}\";").unwrap();
        }

        let violations = check_utf8_integrity(file.to_str().unwrap());
        assert_eq!(violations.len(), 2);
        let v1: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&violations[1]).unwrap();
        assert_eq!(v1["location"]["line"], 2);
        assert_eq!(v2["location"]["line"], 4);
    }

    #[test]
    fn utf8_integrity_nonexistent_file() {
        let violations = check_utf8_integrity("/nonexistent/file.txt");
        assert!(violations.is_empty());
    }

    #[test]
    fn parse_custom_rules_toml_minimal() {
        let toml_str = r#"
[[rules]]
id = "no-todo"
pattern = "TODO"
severity = "warning"
message = "TODO残留"
extensions = ["ts", "js"]
"#;

        let config: CustomRulesConfig = toml::from_str(toml_str).unwrap();
        let rules = config.rules.unwrap();
        assert_eq!(rules.len(), 1);
        assert!(rules[0].fix.is_none());
        assert!(rules[0].example.is_none());
        assert_eq!(rules[0].why, "");
    }

    // --- 新規ルール: PowerShell 空 catch ブロック (no-empty-powershell-catch) ---

    fn ps_empty_catch_rule() -> CustomRule {
        make_test_rule(
            "no-empty-powershell-catch",
            r"(?i)catch\s*\{\s*\}",
            &["ps1"],
        )
    }

    fn write_file(dir: &std::path::Path, name: &str, content: &str) -> std::path::PathBuf {
        use std::io::Write;
        let file = dir.join(name);
        let mut f = std::fs::File::create(&file).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        file
    }

    #[test]
    fn ps_empty_catch_detects_violation() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "swallow.ps1", "try { Get-Item $p } catch {}\n");
        let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ps_empty_catch_detects_with_internal_whitespace() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "ws.ps1", "try { ... } catch {  }\n");
        let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ps_empty_catch_skips_non_empty_block() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "ok.ps1",
            "try { ... } catch { Write-Error $_ }\n",
        );
        let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn ps_empty_catch_only_targets_ps1() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "elsewhere.ts", "try { x() } catch {}\n");
        let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn ps_empty_catch_detects_capitalized_keyword() {
        // PowerShell は case-insensitive なので `Catch {}` も検出すべき
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "cap.ps1", "try { Get-Item $p } Catch {}\n");
        let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ps_empty_catch_detects_uppercase_keyword() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "upper.ps1", "try { Get-Item $p } CATCH {}\n");
        let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ps_empty_catch_detects_multiline_block() {
        // PowerShell の慣用形: `} catch {\n}` の複数行空ブロックも検出すべき
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "multi.ps1",
            "try {\n    Get-Item $p\n} catch {\n}\n",
        );
        let rules = compile_test_rules(vec![ps_empty_catch_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
        // catch keyword is on line 3 in the fixture
        let v: serde_json::Value = serde_json::from_str(&violations[0]).unwrap();
        assert_eq!(v["location"]["line"], 3);
    }

    // --- 新規ルール: -ErrorAction SilentlyContinue (no-silent-error-action) ---

    fn ps_silent_error_rule() -> CustomRule {
        make_test_rule(
            "no-silent-error-action",
            r"(?i)-ErrorAction\s+SilentlyContinue",
            &["ps1"],
        )
    }

    #[test]
    fn ps_silent_error_detects_basic_form() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "silent.ps1",
            "$d = ConvertFrom-Json $r -ErrorAction SilentlyContinue\n",
        );
        let rules = compile_test_rules(vec![ps_silent_error_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ps_silent_error_skips_stop_action() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "stop.ps1",
            "ConvertFrom-Json $r -ErrorAction Stop\n",
        );
        let rules = compile_test_rules(vec![ps_silent_error_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn ps_silent_error_skips_ignore_action() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "ignore.ps1",
            "Get-Item $p -ErrorAction Ignore\n",
        );
        let rules = compile_test_rules(vec![ps_silent_error_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn ps_silent_error_detects_lowercase_param() {
        // PowerShell parameter 名は case-insensitive なので `-erroraction silentlycontinue` も検出すべき
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "lc.ps1",
            "Get-Item $p -erroraction silentlycontinue\n",
        );
        let rules = compile_test_rules(vec![ps_silent_error_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn ps_silent_error_detects_mixed_case() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "mixed.ps1",
            "ConvertFrom-Json $r -ErrorAction SILENTLYCONTINUE\n",
        );
        let rules = compile_test_rules(vec![ps_silent_error_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    // --- 新規ルール: Markdown 非 ASCII GFM アンカー (no-mutable-anchor) ---

    fn md_mutable_anchor_rule() -> CustomRule {
        // path 部から `:` を除外することで http(s):// など外部 URL を除外
        make_test_rule("no-mutable-anchor", r"\]\([^)#:]*#[^\x00-\x7F)]+", &["md"])
    }

    #[test]
    fn md_mutable_anchor_detects_inline_fragment() {
        // `[link](#日本語)` パターン (path 部空、fragment が non-ASCII)
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "frag.md", "See [section](#推奨実行順序)\n");
        let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn md_mutable_anchor_detects_path_with_fragment() {
        // `[link](other.md#日本語)` パターン (path 部あり、fragment が non-ASCII)
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "cross.md",
            "See [other](other.md#日本語見出し)\n",
        );
        let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn md_mutable_anchor_skips_ascii_fragment() {
        // `[link](#stable-id)` パターン (ASCII fragment、許容)
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "ascii.md", "See [section](#stable-ascii-id)\n");
        let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn md_mutable_anchor_skips_link_without_fragment() {
        // `[link](https://example.com)` パターン (fragment なし、許容)
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "url.md",
            "Visit [example](https://example.com)\n",
        );
        let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn md_mutable_anchor_skips_path_only_link() {
        // `[link](other.md)` パターン (path だけ、許容)
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "path.md", "See [other](other.md)\n");
        let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn md_mutable_anchor_only_targets_md() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "other.txt", "See [section](#日本語)\n");
        let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn md_mutable_anchor_skips_external_url_with_fragment() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "external.md",
            "See [spec](https://example.com/#日本語)\n",
        );
        let rules = compile_test_rules(vec![md_mutable_anchor_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    fn rs_time_field_strict_greater_rule() -> CustomRule {
        make_test_rule(
            "no-time-field-strict-greater",
            r"\b(created_at|submitted_at|updated_at|comment_event_time|event_time|comment_created_at|published_at|posted_at|commented_at)\s*>\s*[a-zA-Z_]",
            &["rs"],
        )
    }

    fn build_rs_source_with_op(field_lhs: &str, op: &str, rhs: &str) -> String {
        format!("fn f() {{ items.iter().filter(|c| c.{field_lhs} {op} {rhs}); }}\n")
    }

    fn build_doc_comment_source(field_lhs: &str, op: &str, rhs: &str) -> String {
        format!("/// `{field_lhs} {op} {rhs}` (epoch 0 で実質全件)\nfn f() {{}}\n")
    }

    fn build_toml_with_field(field_lhs: &str, op: &str, rhs: &str) -> String {
        format!("comment = \"{field_lhs} {op} {rhs}\"\n")
    }

    #[test]
    fn rs_time_field_strict_greater_detects_created_at_gt_push_time() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "parse.rs",
            &build_rs_source_with_op("created_at", ">", "push_time"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn rs_time_field_strict_greater_detects_submitted_at_gt_since() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "parse.rs",
            &build_rs_source_with_op("submitted_at", ">", "since"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn rs_time_field_strict_greater_detects_updated_at_gt_threshold() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "parse.rs",
            &build_rs_source_with_op("updated_at", ">", "threshold"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn rs_time_field_strict_greater_detects_comment_event_time() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "parse.rs",
            &build_rs_source_with_op("comment_event_time", ">", "now"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn rs_time_field_strict_greater_skips_inclusive_comparison() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "parse.rs",
            &build_rs_source_with_op("created_at", ">=", "push_time"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn rs_time_field_strict_greater_skips_strict_less_than() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "stale.rs",
            &build_rs_source_with_op("created_at", "<", "threshold"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn rs_time_field_strict_greater_skips_le_inclusive() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "parse.rs",
            &build_rs_source_with_op("created_at", "<=", "cutoff"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn rs_time_field_strict_greater_skips_numeric_rhs() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "parse.rs",
            &build_rs_source_with_op("created_at", ">", "0"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn rs_time_field_strict_greater_skips_doc_comment_with_inclusive() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "doc.rs",
            &build_doc_comment_source("created_at", ">=", "push_time"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn rs_time_field_strict_greater_skips_unrelated_field() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "parse.rs",
            &build_rs_source_with_op("count", ">", "limit"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn rs_time_field_strict_greater_only_targets_rs() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "config.toml",
            &build_toml_with_field("created_at", ">", "push_time"),
        );
        let rules = compile_test_rules(vec![rs_time_field_strict_greater_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    fn md_no_docs_relative_back_to_docs_rule() -> CustomRule {
        make_test_rule(
            "no-docs-relative-back-to-docs",
            r"(?i)\]\(\.\./docs/",
            &["md"],
        )
    }

    #[test]
    fn md_no_docs_relative_detects_pr133_pattern() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "todo7.md",
            "See [ADR-036](../docs/adr/adr-036-bundle-z-three-layer-review.md) for details.\n",
        );
        let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn md_no_docs_relative_detects_uppercase_path() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "note.md",
            "Reference [Spec](../DOCS/feature.md).\n",
        );
        let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn md_no_docs_relative_skips_same_directory_link() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "todo7.md",
            "See [ADR-036](adr/adr-036-bundle-z-three-layer-review.md) for details.\n",
        );
        let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn md_no_docs_relative_skips_parent_to_other_dir() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "page.md",
            "See [README](../README.md) and [src](../src/main.rs).\n",
        );
        let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn md_no_docs_relative_only_targets_md() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "config.toml",
            "doc = \"](../docs/adr/foo.md)\"\n",
        );
        let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    /// 順位 101 (PR #140 T1-#1 採用): depth-1 root MD ファイル (例: `./CLAUDE.md`、`./README.md`) から
    /// `../docs/` を参照すると、リポジトリの親ディレクトリ (= リポジトリ外) を指してしまい必ず broken link になる。
    /// pattern `(?i)\]\(\.\./docs/` は path-aware ではないが、root-level MD では `../docs/` が
    /// 必然的に意味を持たない参照になるため **fire = true positive** として正しい挙動。
    #[test]
    fn md_no_docs_relative_detects_root_level_back_reference() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "CLAUDE.md",
            "See [TODO summary](../docs/todo-summary.md) for context.\n",
        );
        let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(
            violations.len(),
            1,
            "rule⑧ should fire on root-level MD `../docs/` reference (= reaches outside repo, broken link)"
        );
    }

    /// 順位 101 (PR #140 T1-#1 採用) 補強: README.md 等の root-level fixture でも同じ挙動が成立することを確認。
    /// 上の `_detects_root_level_back_reference` は CLAUDE.md fixture でカバー、本テストは別 fixture 名で
    /// 「root-level MD 全般で fire」が安定することを assert する (false negative 防止)。
    #[test]
    fn md_no_docs_relative_detects_root_readme_back_reference() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "README.md",
            "Project setup guide: [setup](../docs/setup.md)\n",
        );
        let rules = compile_test_rules(vec![md_no_docs_relative_back_to_docs_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    fn no_ephemeral_todo_reference_rule() -> CustomRule {
        let stem = "todo";
        let pattern = format!(r"(?i)docs/{stem}[0-9]*\.md");
        make_test_rule(
            "no-ephemeral-todo-reference",
            &pattern,
            &[
                "rs", "toml", "jsonc", "json", "yaml", "yml", "ts", "tsx", "js", "jsx", "py", "ps1",
            ],
        )
    }

    fn build_concrete_digit_fixture(digit: u32) -> String {
        let stem = "todo";
        format!("const MSG: &str = \"see docs/{stem}{digit}.md\";\n")
    }

    fn build_zero_digit_fixture() -> String {
        let stem = "todo";
        format!("pub const NOTE: &str = \"linked from docs/{stem}.md baseline\";\n")
    }

    fn build_letter_placeholder_fixture() -> String {
        let stem = "todo";
        let placeholder = "N";
        format!(
            "/// example: \"docs/{stem}{placeholder}.md\" ({placeholder} = digit) is the placeholder form\n"
        )
    }

    fn build_asterisk_literal_fixture() -> String {
        let stem = "todo";
        let glob = "*";
        format!("pub const GLOB: &str = \"docs/{stem}{glob}.md\";\n")
    }

    #[test]
    fn no_ephemeral_todo_detects_concrete_digit_reference() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "config.rs", &build_concrete_digit_fixture(3));
        let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn no_ephemeral_todo_detects_zero_digit_form() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "lib.rs", &build_zero_digit_fixture());
        let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(violations.len(), 1);
    }

    #[test]
    fn no_ephemeral_todo_skips_letter_placeholder() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(
            dir.path(),
            "explainer.rs",
            &build_letter_placeholder_fixture(),
        );
        let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn no_ephemeral_todo_skips_asterisk_literal() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "doc_glob.rs", &build_asterisk_literal_fixture());
        let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn no_ephemeral_todo_only_targets_listed_extensions_md_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let file = write_file(dir.path(), "note.md", &build_concrete_digit_fixture(3));
        let rules = compile_test_rules(vec![no_ephemeral_todo_reference_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(violations.is_empty());
    }

    #[test]
    fn no_ephemeral_todo_self_exclusion_invariant_holds_on_deployed_toml() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(".claude")
            .join("custom-lint-rules.toml");

        assert!(
            path.exists(),
            "deployed custom-lint-rules.toml not found at {:?} — \
             self-exclusion invariant test would silent-pass on missing file \
             (run_custom_rules returns empty Vec when path is missing). \
             check if `.claude/custom-lint-rules.toml` was moved / deleted. \
             (順位 106 PR #141 T2-#1 false-green guard 1)",
            path
        );

        let rule = no_ephemeral_todo_reference_rule();
        assert!(
            rule.extensions.iter().any(|e| e == "toml"),
            "rule⑥ extensions list does not contain \"toml\" — \
             self-exclusion invariant test would silent-pass on rule scope change \
             (run_custom_rules early-returns when extension is not listed). \
             extensions actual: {:?}. \
             (順位 106 PR #141 T2-#1 false-green guard 2)",
            rule.extensions
        );

        let rules = compile_test_rules(vec![rule]);
        let violations = run_custom_rules(path.to_str().unwrap(), &rules);
        assert!(
            violations.is_empty(),
            "self-exclusion invariant broken: rule⑥ self-triggered on deployed custom-lint-rules.toml"
        );
    }

    fn ps_rule_with_pattern(id: &str, pattern: &str) -> CustomRule {
        make_test_rule(id, pattern, &["ps1"])
    }

    #[test]
    fn powershell_validation_flags_rule_without_case_insensitive_flag() {
        let rules = vec![ps_rule_with_pattern("ps-bad", r"\bcatch\s*\{\s*\}")];
        let missing = find_powershell_rules_missing_case_insensitive_flag(&rules);
        assert_eq!(missing, vec!["ps-bad".to_string()]);
    }

    #[test]
    fn powershell_validation_passes_rule_with_case_insensitive_flag() {
        let rules = vec![ps_rule_with_pattern("ps-good", r"(?i)\bcatch\s*\{\s*\}")];
        let missing = find_powershell_rules_missing_case_insensitive_flag(&rules);
        assert!(missing.is_empty());
    }

    #[test]
    fn powershell_validation_ignores_non_ps1_rules() {
        let rule = make_test_rule("rs-rule", r"\bfn\s+main", &["rs"]);
        let missing = find_powershell_rules_missing_case_insensitive_flag(&[rule]);
        assert!(missing.is_empty());
    }

    #[test]
    fn powershell_validation_handles_mixed_extension_list() {
        let rule = make_test_rule("mixed-rule", r"\bcatch\s*\{\s*\}", &["js", "ps1", "ts"]);
        let missing = find_powershell_rules_missing_case_insensitive_flag(&[rule]);
        assert_eq!(missing, vec!["mixed-rule".to_string()]);
    }

    #[test]
    fn powershell_validation_treats_extension_case_insensitively() {
        let rule = make_test_rule("upper-ext", r"\bcatch\s*\{\s*\}", &["PS1"]);
        let missing = find_powershell_rules_missing_case_insensitive_flag(&[rule]);
        assert_eq!(missing, vec!["upper-ext".to_string()]);
    }

    #[test]
    fn powershell_validation_returns_multiple_violators() {
        let rules = vec![
            ps_rule_with_pattern("ps-a", r"\bcatch"),
            ps_rule_with_pattern("ps-b", r"\berroraction"),
            ps_rule_with_pattern("ps-c-ok", r"(?i)\bwrite-host"),
        ];
        let missing = find_powershell_rules_missing_case_insensitive_flag(&rules);
        assert_eq!(missing, vec!["ps-a".to_string(), "ps-b".to_string()]);
    }

    #[test]
    fn deployed_custom_rules_pass_powershell_case_insensitive_validation() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(".claude")
            .join("custom-lint-rules.toml");
        let content = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read deployed custom-lint-rules.toml: {e}"));
        let config: CustomRulesConfig = toml::from_str(&content).unwrap();
        let rules = config.rules.unwrap_or_default();
        let missing = find_powershell_rules_missing_case_insensitive_flag(&rules);
        assert!(
            missing.is_empty(),
            "PowerShell rules without (?i) flag detected: {:?}",
            missing
        );
    }

    fn takt_workflow_persona_without_model_rule() -> CustomRule {
        make_test_rule(
            "takt-workflow-persona-without-model",
            r"(?m)^[ \t]+persona:[ \t]+[\w-]+[ \t]*\r?\n[ \t]+(?:policy|instruction|edit|provider_options|knowledge|condition|rules|inputs|outputs|allowed_tools|disallowed_tools|name|type|cmd|when|description|tool|tools|output_contracts|pass_previous_response|required_permission_mode|parallel):",
            &["yaml"],
        )
    }

    /// judge / loop_monitor block で persona: → instruction: が違反として検出される。
    /// PR #98 post-merge-feedback で post-pr-review.yaml loop_monitor の persona: 後続
    /// に model: が不在で指摘された pattern を再現。
    #[test]
    fn takt_workflow_persona_detects_judge_block_violation() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = "loop_monitors:\n  - cycle:\n      - analyze\n      - fix\n    judge:\n      persona: supervisor\n      instruction: loop-monitor-reviewers-fix\n";
        let file = write_file(dir.path(), "post-pr-review.yaml", fixture);
        let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(
            violations.len(),
            1,
            "judge block persona: + instruction: は violation として 1 件検出されるべき"
        );
    }

    /// steps の supervise step で persona: → policy: が違反として検出される。
    /// PR #98 で実際に指摘された post-pr-review.yaml supervise step の構造を再現。
    #[test]
    fn takt_workflow_persona_detects_supervise_step_violation() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = "steps:\n  - name: supervise\n    edit: false\n    persona: supervisor\n    policy: review\n";
        let file = write_file(dir.path(), "post-pr-review.yaml", fixture);
        let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(
            violations.len(),
            1,
            "supervise step の persona: + policy: は violation として 1 件検出されるべき"
        );
    }

    /// persona: の直後に model: がある場合は clean (violation 0 件)。
    /// PR #98 fix 後の post-pr-review.yaml supervise step の構造を再現。
    #[test]
    fn takt_workflow_persona_skips_when_model_directly_follows() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = "steps:\n  - name: supervise\n    edit: false\n    persona: supervisor\n    model: sonnet\n    policy: review\n";
        let file = write_file(dir.path(), "post-pr-review.yaml", fixture);
        let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(
            violations.is_empty(),
            "persona: → model: 構造は clean、violation 0 件であるべき。実際: {:?}",
            violations
        );
    }

    /// 複数 violation が同 file 内にある場合、すべて検出される (judge block + supervise step)。
    #[test]
    fn takt_workflow_persona_detects_multiple_violations_in_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = "loop_monitors:\n  - cycle:\n      - analyze\n    judge:\n      persona: supervisor\n      instruction: monitor\nsteps:\n  - name: supervise\n    persona: supervisor\n    policy: review\n";
        let file = write_file(dir.path(), "post-pr-review.yaml", fixture);
        let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(
            violations.len(),
            2,
            "判定ブロック + supervise step の両方が violation として検出されるべき"
        );
    }

    /// PR #150 CR Major 採用: persona: 直後に `output_contracts` / `pass_previous_response` /
    /// `required_permission_mode` / `parallel` が来た場合も violation として検出される。
    /// 当初列挙から漏れていた 4 fields の regression test。
    #[test]
    fn takt_workflow_persona_detects_required_permission_mode_violation() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = "steps:\n  - name: fix\n    persona: coder\n    required_permission_mode: edit\n    pass_previous_response: false\n";
        let file = write_file(dir.path(), "pre-push-review.yaml", fixture);
        let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert_eq!(
            violations.len(),
            1,
            "persona: + required_permission_mode: は violation として 1 件検出されるべき (PR #150 CR Major fix)"
        );
    }

    /// extensions filter で yaml 以外 (md など) はスキップされる。
    /// rule の `extensions = ["yaml"]` 制約を検証 (paths filter は別途 PR #148 D-3 で検証済)。
    #[test]
    fn takt_workflow_persona_skips_non_yaml_extension() {
        let dir = tempfile::tempdir().unwrap();
        let fixture = "persona: supervisor\ninstruction: loop\n";
        let file = write_file(dir.path(), "fake.md", fixture);
        let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
        let violations = run_custom_rules(file.to_str().unwrap(), &rules);
        assert!(
            violations.is_empty(),
            "yaml 以外の extension では rule は fire しないべき"
        );
    }

    /// 配布済 `.takt/workflows/*.yaml` が clean baseline を維持していることを assert。
    /// PR #126 の `deployed_custom_rules_pass_powershell_case_insensitive_validation` と
    /// 同パターン: rule 追加と clean baseline 確保を同 commit で land した後、後続編集での
    /// regression を test 層で防ぐ。
    #[test]
    fn deployed_takt_workflows_have_clean_baseline_for_persona_model_rule() {
        let workflows_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join(".takt")
            .join("workflows");
        let rules = compile_test_rules(vec![takt_workflow_persona_without_model_rule()]);
        let mut total_violations: Vec<String> = Vec::new();
        for entry in std::fs::read_dir(&workflows_dir)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", workflows_dir.display()))
        {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("yaml") {
                let violations = run_custom_rules(path.to_str().unwrap(), &rules);
                for v in violations {
                    total_violations.push(format!("{}: {}", path.display(), v));
                }
            }
        }
        assert!(
            total_violations.is_empty(),
            ".takt/workflows/*.yaml で persona: → model: 不在 violation が検出されました。`model:` 行を追加してください。違反内容: {:?}",
            total_violations
        );
    }
}
