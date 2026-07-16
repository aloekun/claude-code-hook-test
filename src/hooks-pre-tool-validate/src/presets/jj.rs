//! jj 関連プリセット: jj-immutable, jj-main-guard, jj-push-guard, jj-message-required。

use crate::blocked_patterns::BlockedPattern;
use regex::Regex;

const JJ_IMMUTABLE_MSG: &str = r#"**jj --ignore-immutable がブロックされました**

immutable commits（main 等）の書き換え保護を無効化するオプションのため、使用が禁止されています。

immutable commits を変更する必要がある場合は、ユーザーに確認を取ってください。"#;

const JJ_NEW_MAIN_MSG: &str = r#"**jj new main がブロックされました**

ローカルの main ブックマークをベースに change を作成することは禁止されています。
ローカル main はリモートより古い可能性があり、先祖返りの原因になります。

**正しい作業開始コマンド:**
```
pnpm jj-start-change
```

これにより origin/main を fetch してから新しい change を作成します。"#;

const JJ_EDIT_MAIN_MSG: &str = r#"**jj edit main がブロックされました**

main ブックマークが指す commit を直接編集することは禁止されています。
編集すると main の内容が変わり、履歴の破損や先祖返りの原因になります。

**正しい作業開始コマンド:**
```
pnpm jj-start-change
```

これにより origin/main をベースに新しい change を作成します。"#;

const JJ_GIT_PUSH_MSG: &str = r#"**jj git push がブロックされました**

直接の push は禁止されています。push 前パイプライン（テスト・レビュー）を通す必要があります。

**代わりに以下を実行してください:**
```
pnpm push
```

これにより、テスト実行 → レビュー → push が一括で行われます。"#;

const JJ_PUSH_MSG: &str = r#"**jj push がブロックされました**

`jj push` は非推奨です。代わりに `jj git push` を使用しますが、
直接の push は禁止されています。push 前パイプラインを通す必要があります。

**代わりに以下を実行してください:**
```
pnpm push
```

これにより、テスト実行 → レビュー → push が一括で行われます。"#;

const JJ_MSG_REQUIRED_MSG: &str = r#"**jj new / jj split に -m 引数なしがブロックされました**

理由:
- `jj new` (引数なし or revision 指定) で message を省略すると description 未設定の commit が作成され、
  後続の編集が意図しない commit に混入する事故が起こる (PR #171 で実観測)
- `jj split` を `-m` なしで実行すると interactive editor が起動し、Claude セッションが hang する

**正しい使い方:**
```
jj new -m "WIP: <description>"               # 新 commit 開始
jj new master -m "WIP: <description>"        # revision 指定
jj split -m "<message>" <files>              # commit 分離
```

設計判断 (順位 144、PR #171 T3-#8): `pnpm jj-new` 等の wrapper は scope 外。"#;

/// プリセット: jj-immutable
pub(crate) fn preset_jj_immutable() -> Vec<BlockedPattern> {
    vec![BlockedPattern {
        pattern: Regex::new(r"(?is)\bjj\b.*--ignore-immutable").unwrap(),
        exception: None,
        message: JJ_IMMUTABLE_MSG,
    }]
}

/// プリセット: jj-main-guard (jj new main / jj edit main)
pub(crate) fn preset_jj_main_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?i)(jj\s+new|pnpm\s+jj-new)\s+(?:"main"|'main'|main)(?:\s|$)"#)
                .unwrap(),
            exception: None,
            message: JJ_NEW_MAIN_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(
                r#"(?i)(jj\s+edit|pnpm\s+jj-edit)\s+(?:"main"|'main'|main)(?:\s|$)"#,
            )
            .unwrap(),
            exception: None,
            message: JJ_EDIT_MAIN_MSG,
        },
    ]
}

/// プリセット: jj-push-guard (jj git push / jj push を禁止し pnpm push に誘導)
pub(crate) fn preset_jj_push_guard() -> Vec<BlockedPattern> {
    vec![
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*jj\s+git\s+push(\s|$)"#).unwrap(),
            exception: None,
            message: JJ_GIT_PUSH_MSG,
        },
        BlockedPattern {
            pattern: Regex::new(r#"(?im)(^|&&|;|\|\||\||&)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*jj\s+push(\s|$)"#).unwrap(),
            exception: None,
            message: JJ_PUSH_MSG,
        },
    ]
}

/// プリセット: jj-message-required (jj new / jj split を `-m` / `--message` なしで block)
///
/// 順位 144 (PR #171 T3-#8 採用): PR #171 セッションで `jj new` 忘れによる混合 commit 事故を
/// 実観測したのを契機に、message 必須化を機械強制する mechanical enforcement 層を導入。
/// memory rule `feedback_pipeline_over_rules.md` 適用 = パイプライン側機械的修正で
/// Claude 判断介入を排除。
///
/// 設計判断 (2026-05-24 ユーザー承認済):
/// - A: `jj new` 引数なしも block (= `-m` を強制)
/// - B: `jj new <revision>` (例: `jj new master`) で `-m` なしも block
/// - C: `jj split` interactive (= `-m` なし) は editor hang issue があるため strong block
/// - D: scope は `jj` 直接呼び出しのみ (`pnpm jj-new` 等のラッパーは scope 外)
///
/// `BlockedPattern.exception` を活用し「pattern match + exception 不一致」の 2 段判定で
/// `-m`/`--message` 存在時の allow を実現する (Rust 標準 regex crate は negative lookahead 非対応)。
pub(crate) fn preset_jj_message_required() -> Vec<BlockedPattern> {
    let exception = Regex::new(r"\s(-m|--message)\b").unwrap();
    vec![BlockedPattern {
        pattern: Regex::new(r"(?im)(^|&&|;|\|\||\||&|\n)\s*(?:[A-Za-z_][A-Za-z0-9_]*=\S+\s+|command\s+|env\s+)*jj\s+(new|split)\b").unwrap(),
        exception: Some(exception),
        message: JJ_MSG_REQUIRED_MSG,
    }]
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

    fn is_blocked(command: &str) -> bool {
        let patterns = build_blocked_patterns(&Config::default());
        validate_command(command, &patterns).is_some()
    }

    const JJ_MSG_REQ: &[&str] = &["jj-message-required"];

    #[test]
    fn jj_presets_independent() {
        assert!(is_blocked_with(
            "jj --ignore-immutable rebase",
            &["jj-immutable"]
        ));
        assert!(!is_blocked_with("jj new main", &["jj-immutable"]));

        assert!(is_blocked_with("jj new main", &["jj-main-guard"]));
        assert!(!is_blocked_with(
            "jj --ignore-immutable rebase",
            &["jj-main-guard"]
        ));
    }

    #[test]
    fn jj_message_required_blocks_bare_jj_new() {
        assert!(is_blocked_with("jj new", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_blocks_jj_new_with_revision() {
        assert!(is_blocked_with("jj new master", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_blocks_jj_split_without_message() {
        assert!(is_blocked_with("jj split file.rs", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_blocks_jj_new_after_double_ampersand() {
        assert!(is_blocked_with("cd /tmp && jj new", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_blocks_jj_split_after_newline() {
        assert!(is_blocked_with("echo ok\njj split src/main.rs", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_allows_jj_new_with_m_flag() {
        assert!(!is_blocked_with("jj new -m \"WIP: foo\"", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_allows_jj_new_with_revision_and_m_flag() {
        assert!(!is_blocked_with(
            "jj new master -m \"WIP: foo\"",
            JJ_MSG_REQ
        ));
    }

    #[test]
    fn jj_message_required_allows_jj_new_with_long_message_flag() {
        assert!(!is_blocked_with(
            "jj new --message \"WIP: foo\"",
            JJ_MSG_REQ
        ));
    }

    #[test]
    fn jj_message_required_allows_jj_split_with_m_flag() {
        assert!(!is_blocked_with(
            "jj split -m \"split message\" file.rs",
            JJ_MSG_REQ
        ));
    }

    #[test]
    fn jj_message_required_allows_jj_split_with_long_message_flag() {
        assert!(!is_blocked_with(
            "jj split --message \"split message\" file.rs",
            JJ_MSG_REQ
        ));
    }

    #[test]
    fn jj_message_required_with_main_guard_still_blocks_jj_new_main_even_with_m() {
        assert!(is_blocked_with(
            "jj new main -m \"WIP\"",
            &["jj-main-guard", "jj-message-required"]
        ));
    }

    #[test]
    fn jj_message_required_does_not_affect_other_jj_subcommands() {
        assert!(!is_blocked_with("jj status", JJ_MSG_REQ));
        assert!(!is_blocked_with("jj log", JJ_MSG_REQ));
        assert!(!is_blocked_with("jj describe -m \"x\"", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_scope_excludes_pnpm_wrappers() {
        assert!(!is_blocked_with("pnpm jj-new", JJ_MSG_REQ));
        assert!(!is_blocked_with("pnpm jj-start-change", JJ_MSG_REQ));
    }

    #[test]
    fn jj_message_required_not_in_default_fallback_is_opt_in() {
        let patterns = build_blocked_patterns(&Config::default());
        assert!(
            validate_command("jj new", &patterns).is_none(),
            "default fallback should NOT include jj-message-required (opt-in via hooks-config.toml)"
        );
    }

    #[test]
    fn blocks_jj_git_push() {
        assert!(is_blocked("jj git push"));
    }

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

    #[test]
    fn blocks_jj_ignore_immutable_multiline() {
        assert!(is_blocked("jj rebase\n--ignore-immutable -r abc -d main"));
    }

    #[test]
    fn allows_jj_status() {
        assert!(!is_blocked("jj status"));
    }
}
