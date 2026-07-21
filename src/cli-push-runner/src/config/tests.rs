//! config のテスト (production は ./mod.rs)。ファイル長 800 行ガイドライン
//! (順位 147) 遵守のため test mod を切り出した。

use super::*;

/// base branch 解決の 3 段 (section override → top-level → 既定値) と、
/// **3 stage が同じ範囲に解決される**ことを固定する。
///
/// 後者が本 PR の要点: 以前は stage ごとに独立解決で、`[diff]` だけが PR 範囲を
/// 見ていない非対称を許していた (todo 順位 288、4 回再発)。
mod base_branch_resolution {
    use super::*;

    const MINIMAL: &str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "t"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"

[diff]
command = "jj diff --git -r {{PR_RANGE}}"
output_path = ".takt/d.txt"
"#;

    /// TOML の table 構文上、top-level key は**全 section より前**に置く必要が
    /// あるため、prefix / suffix を分けて組み立てる。
    fn parse(top_level: &str, sections: &str) -> Config {
        toml::from_str(&format!("{}{}{}", top_level, MINIMAL, sections))
            .expect("config should parse")
    }

    #[test]
    fn all_stages_share_the_same_range_by_default() {
        let config = parse("", "");
        let expected = format!("{}..@", DEFAULT_BASE_BRANCH);
        assert_eq!(config.diff_pr_range(), expected);
        assert_eq!(config.docs_only_pr_range(), expected);
        assert_eq!(config.pr_size_pr_range(), expected);
    }

    #[test]
    fn top_level_default_branch_applies_to_all_stages() {
        let config = parse("default_branch = \"main\"\n", "");
        assert_eq!(config.diff_pr_range(), "main..@");
        assert_eq!(config.docs_only_pr_range(), "main..@");
        assert_eq!(config.pr_size_pr_range(), "main..@");
    }

    /// 後方互換: 既存の派生プロジェクト config が持つ section 側の
    /// `default_branch` は top-level より優先される。この divergence を
    /// 個々の resolve 関数レベルでは許すが、`load_config` の
    /// `validate_config` は stage 間の不一致を fail-closed で拒否する
    /// (`validate_config_rejects_disagreeing_section_override` 参照。
    /// SIM-NEW-config-mod-rs-L69)。
    #[test]
    fn section_override_wins_over_top_level() {
        let config = parse(
            "default_branch = \"main\"\n",
            "\n[pr_size_check]\nenabled = true\ndefault_branch = \"develop\"\n",
        );
        assert_eq!(config.pr_size_pr_range(), "develop..@");
        assert_eq!(
            config.diff_pr_range(),
            "main..@",
            "override は指定した section にのみ効く"
        );
    }

    #[test]
    fn blank_override_falls_back_instead_of_producing_empty_range() {
        let config = parse(
            "default_branch = \"main\"\n",
            "\n[pr_size_check]\nenabled = true\ndefault_branch = \"   \"\n",
        );
        assert_eq!(
            config.pr_size_pr_range(),
            "main..@",
            "空白のみの override は未設定として扱う (`..@` を作らない)"
        );
    }

    #[test]
    fn blank_top_level_falls_back_to_default() {
        let config = parse("default_branch = \"\"\n", "");
        assert_eq!(config.diff_pr_range(), format!("{}..@", DEFAULT_BASE_BRANCH));
    }

    /// SIM-NEW-config-mod-rs-L69: section override が top-level / 他 section と
    /// 食い違う config は、`section_override_wins_over_top_level` が示す通り
    /// 個々の resolve 関数では「解決できてしまう」が、`load_config` が呼ぶ
    /// `validate_config` はこれを fail-closed で拒否しなければならない。
    #[test]
    fn validate_config_rejects_disagreeing_section_override() {
        let config = parse(
            "default_branch = \"main\"\n",
            "\n[pr_size_check]\nenabled = true\ndefault_branch = \"develop\"\n",
        );
        let err = validate_config(&config).expect_err("disagreeing ranges must fail-closed");
        assert!(err.contains("PR 範囲が stage 間で一致しません"), "{err}");
    }

    /// 全 stage が同じ値を明示していれば (本リポジトリの
    /// `push-runner-config.toml` の実運用形) 検証を通す。
    #[test]
    fn validate_config_accepts_matching_section_overrides() {
        let config = parse(
            "default_branch = \"main\"\n",
            "\n[pr_size_check]\nenabled = true\ndefault_branch = \"main\"\n\
             \n[docs_only_routing]\nenabled = true\ndefault_branch = \"main\"\n",
        );
        assert!(validate_config(&config).is_ok());
    }

    /// CodeRabbit #313: top-level 不在でも section override が全一致していれば、
    /// override 未設定の stage (例 [diff]) もその値を共有する。旧実装は [diff] が
    /// `DEFAULT_BASE_BRANCH` に落ち、他 section が "main" のとき「一致しない」と誤って
    /// reject していた (valid legacy config の誤 reject)。
    #[test]
    fn absent_top_level_inherits_agreed_section_overrides() {
        let config = parse(
            "",
            "\n[pr_size_check]\nenabled = true\ndefault_branch = \"main\"\n\
             \n[docs_only_routing]\nenabled = true\ndefault_branch = \"main\"\n",
        );
        assert_eq!(
            config.diff_pr_range(),
            "main..@",
            "override 未設定の [diff] も全一致値を共有する"
        );
        assert_eq!(config.docs_only_pr_range(), "main..@");
        assert_eq!(config.pr_size_pr_range(), "main..@");
        assert!(
            validate_config(&config).is_ok(),
            "一致する section override のみ (top-level 不在) の config は通す"
        );
    }

    /// top-level 不在で section override が食い違う場合は従来どおり fail-closed
    /// (genuine な不一致は緩めない)。
    #[test]
    fn absent_top_level_with_disagreeing_section_overrides_still_fails_closed() {
        let config = parse(
            "",
            "\n[pr_size_check]\nenabled = true\ndefault_branch = \"main\"\n\
             \n[docs_only_routing]\nenabled = true\ndefault_branch = \"develop\"\n",
        );
        assert!(
            validate_config(&config).is_err(),
            "食い違う section override は genuine な不一致として reject"
        );
    }
}

/// CodeRabbit #313: `[diff] command` が `{{PR_RANGE}}` を欠く config は config-load 時に
/// fail-closed で拒否する (legacy の `-r @` 直書きを設定時点で弾く)。
#[test]
fn validate_config_rejects_diff_command_without_pr_range_placeholder() {
    let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "t"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/d.txt"

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let err = validate_config(&config)
        .expect_err("{{PR_RANGE}} を欠く [diff] command は fail-closed で拒否");
    assert!(
        err.contains(DIFF_PR_RANGE_PLACEHOLDER),
        "診断に placeholder 名を含めること: {err}"
    );
}

/// `{{PR_RANGE}}` を含む modern な command は通す (本リポジトリの実運用形)。
#[test]
fn validate_config_accepts_diff_command_with_pr_range_placeholder() {
    let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "t"
commands = ["echo ok"]

[diff]
command = "jj diff --git -r {{PR_RANGE}}"
output_path = ".takt/d.txt"

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(validate_config(&config).is_ok());
}

#[test]
fn config_parses_full_without_diff() {
    let toml_str = r#"
[quality_gate]
parallel = true
step_timeout = 60

[[quality_gate.groups]]
name = "lint"
commands = ["pnpm lint"]

[[quality_gate.groups]]
name = "test"
pre = "pnpm install"
commands = ["pnpm test", "pnpm test:e2e"]

[takt]
workflow = "pre-push-review"
task = "pre-push review"
extra_args = ["--pipeline", "--skip-git"]

[push]
command = "jj git push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();

    assert_eq!(config.quality_gate.parallel, Some(true));
    assert_eq!(config.quality_gate.step_timeout, Some(60));
    assert_eq!(config.quality_gate.groups.len(), 2);
    assert!(config.diff.is_none());

    assert_eq!(config.takt.workflow, "pre-push-review");
    assert_eq!(config.takt.task, "pre-push review");
    assert_eq!(config.takt.extra_args.as_ref().unwrap().len(), 2);

    assert_eq!(config.push.command, "jj git push");
    assert!(config.push.timeout.is_none());
}

#[test]
fn config_push_timeout_explicit() {
    let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "jj git push"
timeout = 600
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.push.timeout, Some(600));
    assert_eq!(
        config.push.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS),
        600,
    );
}

#[test]
fn config_push_timeout_defaults() {
    let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.push.timeout.is_none());
    assert_eq!(
        config.push.timeout.unwrap_or(DEFAULT_PUSH_TIMEOUT_SECS),
        DEFAULT_PUSH_TIMEOUT_SECS,
    );
}

#[test]
fn config_parses_with_diff() {
    let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "pre-push-review"
task = "pre-push review"

[push]
command = "jj git push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();

    let diff = config.diff.unwrap();
    assert_eq!(diff.command, "jj diff -r @");
    assert_eq!(diff.output_path, ".takt/review-diff.txt");
    assert!(diff.timeout.is_none());
}

/// T6: `[diff] timeout` 未指定時は既定値に落ちる (本リポジトリの config は未指定)。
#[test]
fn config_diff_timeout_defaults() {
    let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    let diff = config.diff.unwrap();
    assert!(diff.timeout.is_none());
    assert_eq!(
        diff.timeout.unwrap_or(DEFAULT_DIFF_TIMEOUT_SECS),
        DEFAULT_DIFF_TIMEOUT_SECS,
    );
}

/// T6: 大 diff / 低速環境向けの escape hatch (既定 60s では足りない場合)。
#[test]
fn config_diff_timeout_explicit() {
    let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[diff]
command = "jj diff -r @"
output_path = ".takt/review-diff.txt"
timeout = 180

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert_eq!(config.diff.unwrap().timeout, Some(180));
}

#[test]
fn config_quality_gate_defaults() {
    let toml_str = r#"
[quality_gate]

[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.quality_gate.parallel.unwrap_or(true));
    assert_eq!(
        config
            .quality_gate
            .step_timeout
            .unwrap_or(DEFAULT_STEP_TIMEOUT_SECS),
        DEFAULT_STEP_TIMEOUT_SECS,
    );
    assert!(config.takt.extra_args.is_none());
}

#[test]
fn config_pre_field_optional() {
    let toml_str = r#"
[quality_gate]
[[quality_gate.groups]]
name = "no-pre"
commands = ["echo test"]

[[quality_gate.groups]]
name = "with-pre"
pre = "echo install"
commands = ["echo test"]

[takt]
workflow = "w"
task = "t"

[push]
command = "echo push"
"#;
    let config: Config = toml::from_str(toml_str).unwrap();
    assert!(config.quality_gate.groups[0].pre.is_none());
    assert!(config.quality_gate.groups[1].pre.is_some());
}

#[test]
fn validate_rejects_empty_groups() {
    let config = Config {
        default_branch: None,
        quality_gate: QualityGateConfig {
            parallel: None,
            step_timeout: None,
            groups: vec![],
        },
        diff: None,
        lint_screen: None,
        scratch_file_warning: None,
        pr_size_check: None,
        pre_push_review: None,
        docs_only_routing: None,
        post_takt_regate: None,
        takt: TaktConfig {
            workflow: "w".into(),
            task: "t".into(),
            extra_args: None,
        },
        push: PushConfig {
            command: "echo".into(),
            timeout: None,
        },
    };
    let result = validate_config(&config);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("groups が空"));
}

#[test]
fn validate_rejects_empty_commands() {
    let config = Config {
        default_branch: None,
        quality_gate: QualityGateConfig {
            parallel: None,
            step_timeout: None,
            groups: vec![GroupConfig {
                name: "empty".into(),
                pre: None,
                commands: vec![],
            }],
        },
        diff: None,
        lint_screen: None,
        scratch_file_warning: None,
        pr_size_check: None,
        pre_push_review: None,
        docs_only_routing: None,
        post_takt_regate: None,
        takt: TaktConfig {
            workflow: "w".into(),
            task: "t".into(),
            extra_args: None,
        },
        push: PushConfig {
            command: "echo".into(),
            timeout: None,
        },
    };
    let result = validate_config(&config);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("'empty'"));
}

/// resolve_takt_workflow テスト用に base config + 任意の [pre_push_review]
/// section を組み立てる。base workflow は "pre-push-review"。
fn config_with_optional_pre_push(pre_push_section: &str) -> Config {
    let toml_str = format!(
        r#"
[quality_gate]
[[quality_gate.groups]]
name = "test"
commands = ["echo ok"]

[takt]
workflow = "pre-push-review"
task = "pre-push review"

[push]
command = "echo push"
{pre_push_section}
"#
    );
    toml::from_str(&toml_str).unwrap()
}

#[test]
fn resolve_workflow_base_when_section_absent() {
    let config = config_with_optional_pre_push("");
    assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
}

#[test]
fn resolve_workflow_base_when_refute_disabled() {
    let config = config_with_optional_pre_push(
        "[pre_push_review]\nrefute_enabled = false\nrefute_workflow = \"pre-push-review-refute\"",
    );
    assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
}

#[test]
fn resolve_workflow_refute_when_enabled() {
    let config = config_with_optional_pre_push(
        "[pre_push_review]\nrefute_enabled = true\nrefute_workflow = \"pre-push-review-refute\"",
    );
    assert_eq!(resolve_takt_workflow(&config), "pre-push-review-refute");
}

#[test]
fn resolve_workflow_base_when_enabled_but_no_refute_workflow() {
    let config = config_with_optional_pre_push("[pre_push_review]\nrefute_enabled = true");
    assert_eq!(resolve_takt_workflow(&config), "pre-push-review");
}
