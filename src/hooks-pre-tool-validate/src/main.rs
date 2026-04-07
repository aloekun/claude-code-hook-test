//! コマンド検証フック (設定駆動型)
//!
//! Bashコマンド実行前に危険なコマンドをブロックします。
//! .claude/hooks-config.toml からプリセット選択・追加保護ファイルを読み込みます。
//!
//! 終了コード:
//!   0 - コマンドを許可
//!   2 - コマンドをブロック（stderrのメッセージがClaudeに表示される）
//!
//! MIT License - based on xiaobei930/claude-code-best-practices

use regex::Regex;
use serde::Deserialize;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

// --- 入力 ---

#[derive(Deserialize)]
struct HookInput {
    tool_name: Option<String>,
    tool_input: Option<ToolInput>,
}

#[derive(Deserialize)]
struct ToolInput {
    command: Option<String>,
    file_path: Option<String>,
    path: Option<String>,
}

// --- 設定 ---

#[derive(Deserialize, Default)]
struct Config {
    pre_tool_validate: Option<PreToolValidateConfig>,
}

#[derive(Deserialize, Default)]
struct PreToolValidateConfig {
    blocked_patterns: Option<Vec<String>>,
    extra_protected_files: Option<Vec<String>>,
}

// --- ブロックパターン ---

struct BlockedPattern {
    pattern: Regex,
    message: &'static str,
}

/// プリセット: default (rm -rf, cd /d)
fn preset_default() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*r[a-zA-Z]*f|-[a-zA-Z]*f[a-zA-Z]*r)\s").unwrap(),
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*r[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*f[a-zA-Z]*(\s|$)").unwrap(),
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)rm\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*f[a-zA-Z]*\s+(-[a-zA-Z]*\s+)*-[a-zA-Z]*r[a-zA-Z]*(\s|$)").unwrap(),
            message: r#"**rm -rf コマンドがブロックされました**

このコマンドは再帰的に強制削除を行うため、重要なファイルを失う可能性があります。

**安全な代替方法:**
- 削除前にファイル一覧を確認: `ls -la <path>`
- 単一ファイルの削除: `rm <file>`
- 確認付き削除: `rm -ri <directory>`
- ゴミ箱への移動を検討"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?im)(^|&&|;|\|\||\||&)\s*cd\s+/d\s").unwrap(),
            message: r#"**cd /d コマンドがブロックされました**

`cd /d` は Windows のコマンドプロンプト固有の構文で、Claude Code の bash 環境では動作しません。

**代替方法:**
- 単純にディレクトリを変更: `cd <path>`
- または絶対パスでコマンドを実行してください

**例:**
```
# NG: cd /d e:\work\project && npm run lint
# OK: cd /e/work/project && npm run lint
# OK: npm run lint --prefix /e/work/project
```"#,
        },
    ]
}

/// プリセット: git (直接 + シェルラッパー経由)
fn preset_git() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?i)\b(bash|sh)\s+-[a-zA-Z]*c[a-zA-Z]*\s+["'][^"']*\bgit\s+"#).unwrap(),
            message: r#"**git コマンドがブロックされました（シェルラッパー経由）**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
`bash -c 'git ...'` 等のラッパー経由でも git コマンドは使用できません。

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*git(?:\s+|$)"#).unwrap(),
            message: r#"**git コマンドがブロックされました**

このプロジェクトでは Jujutsu (jj) をバージョン管理に使用しています。
git コマンドを直接使用すると、バージョン履歴に不整合が生じる可能性があります。

**jj コマンドの代替:**
| git コマンド | jj コマンド |
|-------------|------------|
| git status | jj status |
| git log | jj log |
| git diff | jj diff |
| git add + commit | jj describe -m "message" && jj new |
| git push | jj git push |
| git fetch | jj git fetch |

詳細は CLAUDE.md の "Version Control" セクションを参照してください。"#,
        },
    ]
}

/// プリセット: jj-immutable
fn preset_jj_immutable() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?is)\bjj\b.*--ignore-immutable").unwrap(),
            message: r#"**jj --ignore-immutable がブロックされました**

immutable commits（main 等）の書き換え保護を無効化するオプションのため、使用が禁止されています。

immutable commits を変更する必要がある場合は、ユーザーに確認を取ってください。"#,
        },
    ]
}

/// プリセット: jj-main-guard (jj new main / jj edit main)
fn preset_jj_main_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?i)(jj\s+new|pnpm\s+jj-new)\s+(?:"main"|'main'|main)(?:\s|$)"#).unwrap(),
            message: r#"**jj new main がブロックされました**

ローカルの main ブックマークをベースに change を作成することは禁止されています。
ローカル main はリモートより古い可能性があり、先祖返りの原因になります。

**正しい作業開始コマンド:**
```
pnpm jj-start-change
```

これにより origin/main を fetch してから新しい change を作成します。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?i)(jj\s+edit|pnpm\s+jj-edit)\s+(?:"main"|'main'|main)(?:\s|$)"#).unwrap(),
            message: r#"**jj edit main がブロックされました**

main ブックマークが指す commit を直接編集することは禁止されています。
編集すると main の内容が変わり、履歴の破損や先祖返りの原因になります。

**正しい作業開始コマンド:**
```
pnpm jj-start-change
```

これにより origin/main をベースに新しい change を作成します。"#,
        },
    ]
}

/// プリセット: electron (Electron GUI 実行ブロック)
fn preset_electron() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r"(?i)(^|\s)(npm\s+(run\s+)?start|electron\b|npx\s+electron|yarn\s+start|npm\s+run\s+test:e2e:electron|pnpm\s+(run\s+)?start|pnpm\s+(run\s+)?test:e2e:electron)(\s|$)").unwrap(),
            message: r#"**Electron GUI 実行がブロックされました**

Claude Code から Electron アプリを直接実行することはできません。
GUI アプリケーションは Claude Code のヘッドレス環境では動作しません。

**代替方法:**
| 目的 | コマンド |
|------|---------|
| E2E テストの実行 | npm run jenkins:e2e (Jenkins 経由) |
| Jenkins ログの確認 | npm run jenkins:sync-log |
| ビルド確認 | npm run build |
| 開発サーバー (Renderer) | npm run dev |

**Note:** npm run start や npm run test:e2e:electron はユーザー環境でのみ実行可能です。

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r"(?i)\b(npx|pnpm\s+exec)\s+playwright\s+test\b.*\belectron\b").unwrap(),
            message: r#"**Electron GUI 実行がブロックされました**

Claude Code から Electron アプリを直接実行することはできません。
GUI アプリケーションは Claude Code のヘッドレス環境では動作しません。

**代替方法:**
| 目的 | コマンド |
|------|---------|
| E2E テストの実行 | npm run jenkins:e2e (Jenkins 経由) |
| Jenkins ログの確認 | npm run jenkins:sync-log |

詳細は CLAUDE.md の "Electron E2E Testing" セクションを参照してください。"#,
        },
    ]
}

/// プリセット: jj-push-guard (jj git push / jj push を禁止し pnpm push に誘導)
fn preset_jj_push_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*jj\s+git\s+push(\s|$)"#).unwrap(),
            message: r#"**jj git push がブロックされました**

直接の push は禁止されています。push 前パイプライン（テスト・レビュー）を通す必要があります。

**代わりに以下を実行してください:**
```
pnpm push
```

これにより、テスト実行 → レビュー → push が一括で行われます。"#,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*jj\s+push(\s|$)"#).unwrap(),
            message: r#"**jj push がブロックされました**

`jj push` は非推奨です。代わりに `jj git push` を使用しますが、
直接の push は禁止されています。push 前パイプラインを通す必要があります。

**代わりに以下を実行してください:**
```
pnpm push
```

これにより、テスト実行 → レビュー → push が一括で行われます。"#,
        },
    ]
}

/// プリセット: gh-pr-create-guard (gh pr create を禁止し pnpm pr-create に誘導)
fn preset_gh_pr_create_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*gh\s+(?:.*\s+)?pr\s+create(\s|$)"#).unwrap(),
            message: r#"**gh pr create がブロックされました**

PR 作成は pnpm pr-create 経由で行ってください。
pnpm pr-create は PR 作成後に CI・CodeRabbit の自動監視も開始します。

**代わりに以下を実行してください:**
```
pnpm pr-create -- --title "タイトル" --body "本文"
```

-- 以降の引数はそのまま gh pr create に転送されます。"#,
        },
    ]
}

/// プリセット: gh-pr-merge-guard (gh pr merge を禁止し pnpm merge に誘導)
fn preset_gh_pr_merge_guard() -> Vec<BlockedPattern> {
    let msg = r#"**gh pr merge がブロックされました**

PR マージは pnpm merge 経由で行ってください。
pnpm merge は PR のマージに加え、ローカル環境の同期も自動で行います。

**代わりに以下を実行してください:**
```
pnpm merge
```

現在のブックマークから PR を自動検出してマージします。"#;
    vec![
        // 直接実行パターン
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*gh\s+(?:.*\s+)?pr\s+merge(\s|$)"#).unwrap(),
            message: msg,
        },
        // シェルラッパー経由パターン (bash -c 'gh pr merge ...')
        BlockedPattern {
            pattern: Regex::new(r#"(?i)\b(bash|sh)\s+-[a-zA-Z]*c[a-zA-Z]*\s+["'][^"']*\bgh\s+(?:.*\s+)?pr\s+merge"#).unwrap(),
            message: msg,
        },
    ]
}

/// 設定ファイルに基づいてブロックパターンを構築
fn build_blocked_patterns(config: &Config) -> Vec<BlockedPattern> {
    let preset_names: Vec<String> = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.blocked_patterns.as_ref())
        .cloned()
        .unwrap_or_else(|| {
            // 設定が無い場合: 全プリセット有効 (後方互換)
            vec![
                "default".to_string(),
                "git".to_string(),
                "jj-immutable".to_string(),
                "jj-main-guard".to_string(),
                "jj-push-guard".to_string(),
                "electron".to_string(),
            ]
        });

    let mut patterns = Vec::new();
    for name in &preset_names {
        match name.as_str() {
            "default" => patterns.extend(preset_default()),
            "git" => patterns.extend(preset_git()),
            "jj-immutable" => patterns.extend(preset_jj_immutable()),
            "jj-main-guard" => patterns.extend(preset_jj_main_guard()),
            "jj-push-guard" => patterns.extend(preset_jj_push_guard()),
            "gh-pr-create-guard" => patterns.extend(preset_gh_pr_create_guard()),
            "gh-pr-merge-guard" => patterns.extend(preset_gh_pr_merge_guard()),
            "electron" => patterns.extend(preset_electron()),
            custom => {
                // プリセット名以外はカスタム正規表現として扱う
                if let Ok(re) = Regex::new(custom) {
                    patterns.push(BlockedPattern {
                        pattern: re,
                        message: "**カスタムパターンによりブロックされました**\n\nこのコマンドは hooks-config.toml のカスタムルールによりブロックされています。",
                    });
                } else {
                    eprintln!("[validate-command] Warning: Invalid regex in blocked_patterns: {}", custom);
                }
            }
        }
    }
    patterns
}

fn validate_command(command: &str, patterns: &[BlockedPattern]) -> Option<&'static str> {
    for pattern in patterns {
        if pattern.pattern.is_match(command) {
            return Some(pattern.message);
        }
    }
    None
}

/// リンター/フォーマッター設定ファイルとして保護する対象 (デフォルトリスト)
const PROTECTED_CONFIG_FILES: &[&str] = &[
    // JavaScript / TypeScript
    ".eslintrc",
    ".eslintrc.js",
    ".eslintrc.cjs",
    ".eslintrc.json",
    ".eslintrc.yml",
    ".eslintrc.yaml",
    "eslint.config.js",
    "eslint.config.mjs",
    "eslint.config.cjs",
    "eslint.config.ts",
    "eslint.config.mts",
    "eslint.config.cts",
    ".prettierrc",
    ".prettierrc.js",
    ".prettierrc.cjs",
    ".prettierrc.json",
    ".prettierrc.yml",
    ".prettierrc.yaml",
    "prettier.config.js",
    "prettier.config.cjs",
    "biome.json",
    "biome.jsonc",
    "tsconfig.json",
    "tsconfig.build.json",
    // Git hooks / pre-commit
    "lefthook.yml",
    "lefthook.yaml",
    ".pre-commit-config.yaml",
    ".husky",
    // Python
    "pyproject.toml",
    ".flake8",
    ".pylintrc",
    "setup.cfg",
    // Rust
    "rustfmt.toml",
    ".rustfmt.toml",
    "clippy.toml",
    ".clippy.toml",
    // Go
    ".golangci.yml",
    ".golangci.yaml",
    // Swift
    ".swiftlint.yml",
    ".swiftlint.yaml",
    // Secrets / Environment
    ".env",
    ".env.local",
    ".env.development",
    ".env.production",
    ".env.staging",
    ".env.test",
];

/// ファイルパスが保護対象の設定ファイルに該当するか判定
fn is_protected_config(file_path: &str, extra_files: &[String]) -> bool {
    let normalized = file_path.replace('\\', "/");
    let normalized_lower = normalized.to_ascii_lowercase();

    let file_name = normalized_lower
        .rsplit('/')
        .next()
        .unwrap_or(&normalized_lower);

    // デフォルトリスト + 追加ファイルを結合してチェック
    let check_name = |protected: &str| -> bool {
        let protected_lower = protected.to_ascii_lowercase();
        if protected == ".husky" {
            let dir_prefix = format!("{}/", protected_lower);
            file_name == protected_lower
                || normalized_lower.contains(&format!("/{}", dir_prefix))
                || normalized_lower.starts_with(&dir_prefix)
        } else if protected_lower.contains('/') {
            // パス付き指定: 完全一致 or スラッシュ境界での末尾一致
            normalized_lower == protected_lower
                || normalized_lower.ends_with(&format!("/{}", protected_lower))
        } else {
            file_name == protected_lower
        }
    };

    PROTECTED_CONFIG_FILES.iter().any(|&p| check_name(p))
        || extra_files.iter().any(|p| check_name(p))
}

/// 設定ファイルのパス解決: exe のあるディレクトリ / hooks-config.toml
fn config_path() -> PathBuf {
    std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or(Path::new("."))
        .join("hooks-config.toml")
}

/// 設定ファイルを読み込む (存在しない場合はデフォルト)
fn load_config() -> Config {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(content) => toml::from_str(&content).unwrap_or_else(|e| {
            eprintln!("[validate-command] Warning: Failed to parse {}: {}", path.display(), e);
            Config::default()
        }),
        Err(_) => Config::default(), // ファイル無し → デフォルト
    }
}

fn main() -> ExitCode {
    let config = load_config();

    // stdinからJSONを読み込む
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        eprintln!("[validate-command] Error: Failed to read stdin: {}", e);
        return ExitCode::FAILURE;
    }

    let hook_input: HookInput = match serde_json::from_str(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("[validate-command] Error: Failed to parse JSON: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let tool_name = hook_input.tool_name.unwrap_or_default();
    let tool_input = hook_input.tool_input.unwrap_or(ToolInput {
        command: None,
        file_path: None,
        path: None,
    });

    let extra_protected = config
        .pre_tool_validate
        .as_ref()
        .and_then(|c| c.extra_protected_files.as_ref())
        .cloned()
        .unwrap_or_default();

    match tool_name.as_str() {
        "Bash" => {
            let command = tool_input.command.unwrap_or_default();
            if command.trim().is_empty() {
                return ExitCode::SUCCESS;
            }

            let patterns = build_blocked_patterns(&config);
            if let Some(message) = validate_command(&command, &patterns) {
                let _ = io::stderr().write_all(message.as_bytes());
                return ExitCode::from(2);
            }
        }
        "Write" | "Edit" | "Replace" => {
            let file_path = tool_input
                .file_path
                .filter(|s| !s.is_empty())
                .or(tool_input.path)
                .unwrap_or_default();
            if !file_path.is_empty() && is_protected_config(&file_path, &extra_protected) {
                let msg = format!(
                    "**保護されたファイルの編集がブロックされました**\n\n\
                     `{}` は保護対象ファイル（設定ファイル/機密ファイル）のため、編集が禁止されています。\n\n\
                     リンター設定の場合: 設定を変更するのではなく **コード側を修正** してください。\n\
                     機密ファイルの場合: 秘密情報の漏洩を防ぐため、編集できません。\n\n\
                     変更が本当に必要な場合は、ユーザーに確認を取ってください。",
                    file_path
                        .rsplit(|c| c == '/' || c == '\\')
                        .next()
                        .unwrap_or(&file_path)
                );
                let _ = io::stderr().write_all(msg.as_bytes());
                return ExitCode::from(2);
            }
        }
        _ => {}
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- プリセットベースのパターン構築テスト ---

    fn all_patterns() -> Vec<BlockedPattern> {
        build_blocked_patterns(&Config::default())
    }

    fn patterns_with_presets(presets: &[&str]) -> Vec<BlockedPattern> {
        let config = Config {
            pre_tool_validate: Some(PreToolValidateConfig {
                blocked_patterns: Some(presets.iter().map(|s| s.to_string()).collect()),
                extra_protected_files: None,
            }),
        };
        build_blocked_patterns(&config)
    }

    fn is_blocked(command: &str) -> bool {
        validate_command(command, &all_patterns()).is_some()
    }

    fn is_blocked_with(command: &str, presets: &[&str]) -> bool {
        let patterns = patterns_with_presets(presets);
        validate_command(command, &patterns).is_some()
    }

    // --- 設定ベースのプリセット選択テスト ---

    #[test]
    fn default_config_enables_all_presets() {
        // Config::default() (設定ファイル無し) → 全プリセット有効
        assert!(is_blocked("git push"));
        assert!(is_blocked("rm -rf /tmp"));
        assert!(is_blocked("jj --ignore-immutable rebase"));
        assert!(is_blocked("jj new main"));
        assert!(is_blocked("electron ."));
    }

    #[test]
    fn only_default_preset_allows_git() {
        // "default" のみ → git は許可される
        assert!(!is_blocked_with("git push", &["default"]));
        // rm -rf は引き続きブロック
        assert!(is_blocked_with("rm -rf /tmp", &["default"]));
    }

    #[test]
    fn git_preset_blocks_git() {
        assert!(is_blocked_with("git push", &["git"]));
        assert!(is_blocked_with("bash -c 'git push'", &["git"]));
    }

    #[test]
    fn jj_presets_independent() {
        // jj-immutable のみ有効
        assert!(is_blocked_with("jj --ignore-immutable rebase", &["jj-immutable"]));
        assert!(!is_blocked_with("jj new main", &["jj-immutable"]));

        // jj-main-guard のみ有効
        assert!(is_blocked_with("jj new main", &["jj-main-guard"]));
        assert!(!is_blocked_with("jj --ignore-immutable rebase", &["jj-main-guard"]));
    }

    #[test]
    fn empty_presets_blocks_nothing() {
        let patterns = patterns_with_presets(&[]);
        assert!(validate_command("git push", &patterns).is_none());
        assert!(validate_command("rm -rf /tmp", &patterns).is_none());
    }

    #[test]
    fn custom_regex_pattern() {
        // カスタム正規表現パターン
        assert!(is_blocked_with("docker rm -f container", &[r"docker\s+rm"]));
        assert!(!is_blocked_with("docker ps", &[r"docker\s+rm"]));
    }

    // --- git: direct commands (should block) ---

    #[test]
    fn blocks_git_at_start() {
        assert!(is_blocked("git push"));
    }

    #[test]
    fn blocks_git_status() {
        assert!(is_blocked("git status"));
    }

    // --- git: chained after shell operators (should block) ---

    #[test]
    fn blocks_git_after_ampersand_ampersand() {
        assert!(is_blocked("cd /e/work && git push"));
    }

    #[test]
    fn blocks_git_after_semicolon() {
        assert!(is_blocked("true; git status"));
    }

    #[test]
    fn blocks_git_after_or() {
        assert!(is_blocked("false || git log"));
    }

    #[test]
    fn blocks_git_after_pipe() {
        assert!(is_blocked("echo data | git apply"));
    }

    #[test]
    fn blocks_git_in_triple_chain() {
        assert!(is_blocked("cd /path && echo ok && git commit -m 'test'"));
    }

    #[test]
    fn blocks_git_after_single_ampersand() {
        assert!(is_blocked("echo ok & git status"));
    }

    // --- git: multiline and bare git (should block) ---

    #[test]
    fn blocks_git_after_newline() {
        assert!(is_blocked("echo ok\ngit push"));
    }

    #[test]
    fn blocks_bare_git() {
        assert!(is_blocked("git"));
    }

    #[test]
    fn blocks_cd_d_after_newline() {
        assert!(is_blocked("echo ok\ncd /d e:\\work"));
    }

    // --- git: env/command prefix bypass (should block) ---

    #[test]
    fn blocks_git_with_env_prefix() {
        assert!(is_blocked("GIT_TRACE=1 git status"));
    }

    #[test]
    fn blocks_git_with_command_builtin() {
        assert!(is_blocked("command git push"));
    }

    #[test]
    fn blocks_git_with_env_builtin() {
        assert!(is_blocked("env VAR=value git log"));
    }

    #[test]
    fn blocks_git_env_prefix_after_chain() {
        assert!(is_blocked("echo x; GIT_TRACE=1 git diff"));
    }

    // --- git: allowed commands (should NOT block) ---

    #[test]
    fn blocks_jj_git_push() {
        // jj-push-guard プリセットにより、直接の jj git push はブロックされる
        // pnpm push 経由でのみ push を許可する設計
        assert!(is_blocked("jj git push"));
    }

    #[test]
    fn allows_jj_git_fetch() {
        assert!(!is_blocked("jj git fetch"));
    }

    #[test]
    fn allows_gh_pr_create() {
        assert!(!is_blocked("gh pr create --title 'test'"));
    }

    // --- gh-pr-create-guard ---

    #[test]
    fn gh_pr_create_guard_blocks_gh_pr_create() {
        assert!(is_blocked_with("gh pr create --title 'test'", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_blocks_gh_pr_create_in_chain() {
        assert!(is_blocked_with("cd /tmp && gh pr create --title 'test'", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_blocks_gh_with_repo_pr_create() {
        assert!(is_blocked_with("gh -R owner/repo pr create", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_view() {
        assert!(!is_blocked_with("gh pr view", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_list() {
        assert!(!is_blocked_with("gh pr list", &["gh-pr-create-guard"]));
    }

    #[test]
    fn gh_pr_create_guard_allows_gh_pr_merge() {
        assert!(!is_blocked_with("gh pr merge 42", &["gh-pr-create-guard"]));
    }

    // --- gh-pr-merge-guard ---

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge() {
        assert!(is_blocked_with("gh pr merge 42", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge_squash() {
        assert!(is_blocked_with("gh pr merge 42 --squash", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_pr_merge_in_chain() {
        assert!(is_blocked_with("cd /tmp && gh pr merge 42", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_gh_with_repo_pr_merge() {
        assert!(is_blocked_with("gh -R owner/repo pr merge 42", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_view() {
        assert!(!is_blocked_with("gh pr view 42", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_list() {
        assert!(!is_blocked_with("gh pr list", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_allows_gh_pr_create() {
        assert!(!is_blocked_with("gh pr create --title 'test'", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_bash_c_gh_pr_merge() {
        assert!(is_blocked_with("bash -c 'gh pr merge 42'", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn gh_pr_merge_guard_blocks_sh_lc_gh_pr_merge() {
        assert!(is_blocked_with("sh -lc 'gh pr merge 42 --squash'", &["gh-pr-merge-guard"]));
    }

    #[test]
    fn allows_pnpm_lint() {
        assert!(!is_blocked("pnpm lint"));
    }

    #[test]
    fn allows_jj_status() {
        assert!(!is_blocked("jj status"));
    }

    // --- cd /d: direct and chained (should block) ---

    #[test]
    fn blocks_cd_d_at_start() {
        assert!(is_blocked(r"cd /d e:\work"));
    }

    #[test]
    fn blocks_cd_d_after_ampersand_ampersand() {
        assert!(is_blocked(r"echo ok && cd /d e:\work"));
    }

    // --- rm -rf (should block regardless of position) ---

    #[test]
    fn blocks_rm_rf_at_start() {
        assert!(is_blocked("rm -rf /tmp/test"));
    }

    #[test]
    fn blocks_rm_rf_after_chain() {
        assert!(is_blocked("cd /path && rm -rf /tmp"));
    }

    #[test]
    fn blocks_rm_split_r_then_f() {
        assert!(is_blocked("rm -r -f /tmp/test"));
    }

    #[test]
    fn blocks_rm_split_f_then_r() {
        assert!(is_blocked("rm -f -r /tmp/test"));
    }

    // --- git in shell wrapper (should block) ---

    #[test]
    fn blocks_bash_c_git() {
        assert!(is_blocked("bash -c 'git push'"));
    }

    #[test]
    fn blocks_bash_lc_git() {
        assert!(is_blocked("bash -lc 'git status'"));
    }

    #[test]
    fn blocks_sh_c_git() {
        assert!(is_blocked(r#"sh -c "git log""#));
    }

    // --- Electron E2E (should block) ---

    #[test]
    fn blocks_npm_run_test_e2e_electron() {
        assert!(is_blocked("npm run test:e2e:electron"));
    }

    #[test]
    fn blocks_npx_playwright_electron() {
        assert!(is_blocked("npx playwright test --config=playwright-electron.config.ts"));
    }

    #[test]
    fn blocks_electron_with_path_arg() {
        assert!(is_blocked("electron ./dist/main.js"));
    }

    #[test]
    fn blocks_pnpm_exec_electron() {
        assert!(is_blocked("pnpm exec electron ./dist/main.js"));
    }

    #[test]
    fn blocks_pnpm_start() {
        assert!(is_blocked("pnpm start"));
    }

    #[test]
    fn blocks_pnpm_run_start() {
        assert!(is_blocked("pnpm run start"));
    }

    #[test]
    fn blocks_pnpm_run_test_e2e_electron() {
        assert!(is_blocked("pnpm run test:e2e:electron"));
    }

    #[test]
    fn blocks_pnpm_exec_playwright_electron() {
        assert!(is_blocked("pnpm exec playwright test --config=playwright-electron.config.ts"));
    }

    // --- jj new main: 第2層 (should block) ---

    #[test]
    fn blocks_jj_new_main() {
        assert!(is_blocked("jj new main"));
    }

    #[test]
    fn blocks_pnpm_jj_new_main() {
        assert!(is_blocked("pnpm jj-new main"));
    }

    #[test]
    fn blocks_jj_new_main_with_flag() {
        assert!(is_blocked("jj new main --no-edit"));
    }

    #[test]
    fn allows_jj_new_origin_main() {
        assert!(!is_blocked("jj new origin/main"));
    }

    #[test]
    fn allows_jj_new_main_at_origin() {
        assert!(!is_blocked("jj new main@origin"));
    }

    #[test]
    fn blocks_jj_new_main_single_quoted() {
        assert!(is_blocked("jj new 'main'"));
    }

    #[test]
    fn blocks_pnpm_jj_new_main_double_quoted() {
        assert!(is_blocked("pnpm jj-new \"main\""));
    }

    #[test]
    fn allows_jj_new_feature_branch() {
        assert!(!is_blocked("jj new feature/foo"));
    }

    #[test]
    fn allows_jj_new_mainline() {
        assert!(!is_blocked("jj new mainline"));
    }

    // --- jj edit main: 第3層 (should block) ---

    #[test]
    fn blocks_jj_edit_main() {
        assert!(is_blocked("jj edit main"));
    }

    #[test]
    fn blocks_pnpm_jj_edit_main() {
        assert!(is_blocked("pnpm jj-edit main"));
    }

    #[test]
    fn allows_jj_edit_feature_branch() {
        assert!(!is_blocked("jj edit feature/foo"));
    }

    // --- jj --ignore-immutable (should block) ---

    #[test]
    fn blocks_jj_ignore_immutable() {
        assert!(is_blocked("jj --ignore-immutable rebase -r abc -d main"));
    }

    #[test]
    fn blocks_jj_ignore_immutable_after_subcommand() {
        assert!(is_blocked("jj rebase --ignore-immutable -r abc -d main"));
    }

    #[test]
    fn allows_jj_rebase_without_ignore_immutable() {
        assert!(!is_blocked("jj rebase -r abc -d main"));
    }

    // --- safe commands (should NOT block) ---

    #[test]
    fn allows_empty_command() {
        assert!(!is_blocked(""));
    }

    #[test]
    fn allows_ls() {
        assert!(!is_blocked("ls -la"));
    }

    #[test]
    fn allows_cd_normal() {
        assert!(!is_blocked("cd /e/work/project"));
    }

    // --- protected config files (should block Write/Edit) ---

    #[test]
    fn protects_eslint_config() {
        assert!(is_protected_config("eslint.config.js", &[]));
    }

    #[test]
    fn protects_eslintrc_json() {
        assert!(is_protected_config(".eslintrc.json", &[]));
    }

    #[test]
    fn protects_biome_json() {
        assert!(is_protected_config("biome.json", &[]));
    }

    #[test]
    fn protects_prettierrc() {
        assert!(is_protected_config(".prettierrc", &[]));
    }

    #[test]
    fn protects_tsconfig() {
        assert!(is_protected_config("tsconfig.json", &[]));
    }

    #[test]
    fn protects_pyproject_toml() {
        assert!(is_protected_config("pyproject.toml", &[]));
    }

    #[test]
    fn protects_rustfmt_toml() {
        assert!(is_protected_config("rustfmt.toml", &[]));
    }

    #[test]
    fn protects_golangci_yml() {
        assert!(is_protected_config(".golangci.yml", &[]));
    }

    #[test]
    fn protects_lefthook_yml() {
        assert!(is_protected_config("lefthook.yml", &[]));
    }

    #[test]
    fn protects_pre_commit_config() {
        assert!(is_protected_config(".pre-commit-config.yaml", &[]));
    }

    #[test]
    fn protects_with_windows_path() {
        assert!(is_protected_config(r"e:\work\project\biome.json", &[]));
    }

    #[test]
    fn protects_with_unix_path() {
        assert!(is_protected_config("/home/user/project/.eslintrc.json", &[]));
    }

    #[test]
    fn allows_regular_ts_file() {
        assert!(!is_protected_config("src/app.ts", &[]));
    }

    #[test]
    fn allows_regular_json_file() {
        assert!(!is_protected_config("src/data.json", &[]));
    }

    #[test]
    fn allows_package_json() {
        assert!(!is_protected_config("package.json", &[]));
    }

    #[test]
    fn protects_env() {
        assert!(is_protected_config(".env", &[]));
    }

    #[test]
    fn protects_env_local() {
        assert!(is_protected_config(".env.local", &[]));
    }

    #[test]
    fn protects_env_production() {
        assert!(is_protected_config(".env.production", &[]));
    }

    #[test]
    fn protects_env_with_path() {
        assert!(is_protected_config(r"e:\work\project\.env", &[]));
    }

    // --- .husky ディレクトリ内ファイルの保護 ---

    #[test]
    fn protects_husky_pre_commit() {
        assert!(is_protected_config(".husky/pre-commit", &[]));
    }

    #[test]
    fn protects_husky_with_absolute_path() {
        assert!(is_protected_config("/home/user/project/.husky/pre-commit", &[]));
    }

    #[test]
    fn protects_husky_with_windows_path() {
        assert!(is_protected_config(r"e:\work\project\.husky\pre-commit", &[]));
    }

    // --- case-insensitive 保護 ---

    #[test]
    fn protects_uppercase_husky() {
        assert!(is_protected_config(".HUSKY/pre-commit", &[]));
    }

    #[test]
    fn protects_uppercase_eslintrc() {
        assert!(is_protected_config(".ESLINTRC.JSON", &[]));
    }

    #[test]
    fn protects_mixed_case_biome() {
        assert!(is_protected_config("Biome.Json", &[]));
    }

    // --- jj --ignore-immutable dotall ---

    #[test]
    fn blocks_jj_ignore_immutable_multiline() {
        assert!(is_blocked("jj rebase\n--ignore-immutable -r abc -d main"));
    }

    // --- extra_protected_files テスト ---

    #[test]
    fn extra_protected_files_blocks() {
        let extra = vec!["settings.local.json".to_string()];
        assert!(is_protected_config("settings.local.json", &extra));
        assert!(is_protected_config(r"e:\work\.claude\settings.local.json", &extra));
    }

    #[test]
    fn extra_protected_files_does_not_affect_default() {
        let extra = vec!["settings.local.json".to_string()];
        // デフォルトリストは引き続き有効
        assert!(is_protected_config("biome.json", &extra));
        // 追加リストにないファイルは許可
        assert!(!is_protected_config("src/app.ts", &extra));
    }

    // --- extra_protected_files パス付き指定テスト ---

    #[test]
    fn extra_protected_path_matches_full_path() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        assert!(is_protected_config(r"e:\work\project\.claude\hooks-config.toml", &extra));
        assert!(is_protected_config("/home/user/project/.claude/hooks-config.toml", &extra));
    }

    #[test]
    fn extra_protected_path_does_not_match_different_dir() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        // 別ディレクトリの同名ファイルはマッチしない
        assert!(!is_protected_config("other/hooks-config.toml", &extra));
    }

    #[test]
    fn extra_protected_path_does_not_match_bare_basename() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        // パス付き指定はベースネームだけではマッチしない
        assert!(!is_protected_config("hooks-config.toml", &extra));
    }

    #[test]
    fn extra_protected_basename_still_works() {
        // ベースネーム指定は従来通りどこでもマッチ
        let extra = vec!["hooks-config.toml".to_string()];
        assert!(is_protected_config("hooks-config.toml", &extra));
        assert!(is_protected_config(r"e:\work\.claude\hooks-config.toml", &extra));
        assert!(is_protected_config("other/hooks-config.toml", &extra));
    }
}
