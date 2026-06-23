//! リンター/フォーマッター設定ファイル + 機密ファイルの編集保護。
//!
//! `PROTECTED_CONFIG_FILES` 定数リスト + `extra_protected_files` (config) を結合して
//! ファイルパスが保護対象に該当するか判定する。判定は case-insensitive で、
//! Windows / Unix 両方のパスセパレータを正規化する。

pub(crate) const PROTECTED_CONFIG_FILES: &[&str] = &[
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
    "lefthook.yml",
    "lefthook.yaml",
    ".pre-commit-config.yaml",
    ".husky",
    "pyproject.toml",
    ".flake8",
    ".pylintrc",
    "setup.cfg",
    "rustfmt.toml",
    ".rustfmt.toml",
    "clippy.toml",
    ".clippy.toml",
    ".golangci.yml",
    ".golangci.yaml",
    ".swiftlint.yml",
    ".swiftlint.yaml",
    ".env",
    ".env.local",
    ".env.development",
    ".env.production",
    ".env.staging",
    ".env.test",
];

/// ファイルパスが保護対象の設定ファイルに該当するか判定
pub(crate) fn is_protected_config(file_path: &str, extra_files: &[String]) -> bool {
    let normalized = file_path.replace('\\', "/");
    let normalized_lower = normalized.to_ascii_lowercase();

    let file_name = normalized_lower
        .rsplit('/')
        .next()
        .unwrap_or(&normalized_lower);

    let check_name = |protected: &str| -> bool {
        let protected_lower = protected.to_ascii_lowercase();
        if protected == ".husky" {
            let dir_prefix = format!("{}/", protected_lower);
            file_name == protected_lower
                || normalized_lower.contains(&format!("/{}", dir_prefix))
                || normalized_lower.starts_with(&dir_prefix)
        } else if protected_lower.contains('/') {
            normalized_lower == protected_lower
                || normalized_lower.ends_with(&format!("/{}", protected_lower))
        } else {
            file_name == protected_lower
        }
    };

    PROTECTED_CONFIG_FILES.iter().any(|&p| check_name(p))
        || extra_files.iter().any(|p| check_name(p))
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(is_protected_config(
            "/home/user/project/.eslintrc.json",
            &[]
        ));
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

    #[test]
    fn protects_husky_pre_commit() {
        assert!(is_protected_config(".husky/pre-commit", &[]));
    }

    #[test]
    fn protects_husky_with_absolute_path() {
        assert!(is_protected_config(
            "/home/user/project/.husky/pre-commit",
            &[]
        ));
    }

    #[test]
    fn protects_husky_with_windows_path() {
        assert!(is_protected_config(
            r"e:\work\project\.husky\pre-commit",
            &[]
        ));
    }

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

    #[test]
    fn extra_protected_files_blocks() {
        let extra = vec!["settings.local.json".to_string()];
        assert!(is_protected_config("settings.local.json", &extra));
        assert!(is_protected_config(
            r"e:\work\.claude\settings.local.json",
            &extra
        ));
    }

    #[test]
    fn extra_protected_files_does_not_affect_default() {
        let extra = vec!["settings.local.json".to_string()];
        assert!(is_protected_config("biome.json", &extra));
        assert!(!is_protected_config("src/app.ts", &extra));
    }

    #[test]
    fn extra_protected_path_matches_full_path() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        assert!(is_protected_config(
            r"e:\work\project\.claude\hooks-config.toml",
            &extra
        ));
        assert!(is_protected_config(
            "/home/user/project/.claude/hooks-config.toml",
            &extra
        ));
    }

    #[test]
    fn extra_protected_path_does_not_match_different_dir() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        assert!(!is_protected_config("other/hooks-config.toml", &extra));
    }

    #[test]
    fn extra_protected_path_does_not_match_bare_basename() {
        let extra = vec![".claude/hooks-config.toml".to_string()];
        assert!(!is_protected_config("hooks-config.toml", &extra));
    }

    #[test]
    fn extra_protected_basename_still_works() {
        let extra = vec!["hooks-config.toml".to_string()];
        assert!(is_protected_config("hooks-config.toml", &extra));
        assert!(is_protected_config(
            r"e:\work\.claude\hooks-config.toml",
            &extra
        ));
        assert!(is_protected_config("other/hooks-config.toml", &extra));
    }
}
