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

    /// remote tracking ref が**存在しない**環境を注入して PR 範囲を組み立てる。
    ///
    /// 本 mod のテストが検証したいのは「top-level / section override / 既定値の
    /// 優先順位」であって remote 優先解決ではない。実 jj を見に行くと、この repo では
    /// `master@origin` が解決できてしまい期待値が環境依存になる (順位 254 の実装後に
    /// 3 テストが落ちて判明)。remote 優先そのものは
    /// [`super::preferred_base_ref`] の専用テストで固定する。
    fn no_remote(_revset: &str) -> bool {
        false
    }

    fn diff_range_no_remote(config: &Config) -> String {
        config.pr_range_revset_with(
            config.diff.as_ref().and_then(|c| c.default_branch.as_deref()),
            no_remote,
        )
    }

    fn docs_only_range_no_remote(config: &Config) -> String {
        config.pr_range_revset_with(
            config
                .docs_only_routing
                .as_ref()
                .and_then(|c| c.default_branch.as_deref()),
            no_remote,
        )
    }

    fn pr_size_range_no_remote(config: &Config) -> String {
        config.pr_range_revset_with(
            config
                .pr_size_check
                .as_ref()
                .and_then(|c| c.default_branch.as_deref()),
            no_remote,
        )
    }

    /// 順位 254: remote tracking ref が解決できるなら base はそちらを優先する。
    #[test]
    fn base_prefers_remote_tracking_ref_when_it_resolves() {
        let config = parse("", "");
        assert_eq!(
            config.pr_range_revset_with(None, |r| r == "master@origin"),
            "master@origin..@",
            "ローカル master の遅延で他 PR のマージ分を合算しないよう remote を優先する"
        );
    }

    /// 対比: 解決できない環境ではローカル名へ戻る (派生プロジェクト / remote 無しでも壊れない)。
    #[test]
    fn base_falls_back_to_local_name_when_remote_ref_missing() {
        let config = parse("", "");
        assert_eq!(
            config.pr_range_revset_with(None, no_remote),
            "master..@",
            "remote が無い環境では従来どおりローカル bookmark 名で解決する"
        );
    }

    /// 既に remote 修飾された値は二重修飾しない。
    #[test]
    fn base_does_not_double_qualify_remote_ref() {
        let config = parse("default_branch = \"master@origin\"\n", "");
        assert_eq!(
            config.pr_range_revset_with(None, |_| true),
            "master@origin..@"
        );
    }

    #[test]
    fn all_stages_share_the_same_range_by_default() {
        let config = parse("", "");
        let expected = format!("{}..@", DEFAULT_BASE_BRANCH);
        assert_eq!(diff_range_no_remote(&config), expected);
        assert_eq!(docs_only_range_no_remote(&config), expected);
        assert_eq!(pr_size_range_no_remote(&config), expected);
    }

    #[test]
    fn top_level_default_branch_applies_to_all_stages() {
        let config = parse("default_branch = \"main\"\n", "");
        assert_eq!(diff_range_no_remote(&config), "main..@");
        assert_eq!(docs_only_range_no_remote(&config), "main..@");
        assert_eq!(pr_size_range_no_remote(&config), "main..@");
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
        assert_eq!(pr_size_range_no_remote(&config), "develop..@");
        assert_eq!(
            diff_range_no_remote(&config),
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
            pr_size_range_no_remote(&config),
            "main..@",
            "空白のみの override は未設定として扱う (`..@` を作らない)"
        );
    }

    #[test]
    fn blank_top_level_falls_back_to_default() {
        let config = parse("default_branch = \"\"\n", "");
        assert_eq!(
            diff_range_no_remote(&config),
            format!("{}..@", DEFAULT_BASE_BRANCH)
        );
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
            diff_range_no_remote(&config),
            "main..@",
            "override 未設定の [diff] も全一致値を共有する"
        );
        assert_eq!(docs_only_range_no_remote(&config), "main..@");
        assert_eq!(pr_size_range_no_remote(&config), "main..@");
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
        ledger_completion: None,
        pr_size_check: None,
        pre_push_review: None,
        docs_only_routing: None,
        post_takt_regate: None,
        testability_gate: None,
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
        ledger_completion: None,
        pr_size_check: None,
        pre_push_review: None,
        docs_only_routing: None,
        post_takt_regate: None,
        testability_gate: None,
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

/// 順位 254 の I/O 層: remote tracking ref 解決の timeout / 失敗経路。
mod remote_ref_resolution {
    /// simplicity review (PR #432 SIM-NEW-config-mod-L94) の regression guard:
    /// remote tracking ref の解決が**ハングしても push-runner を止めない**。
    ///
    /// この呼び出しは `diff_pr_range` / `pr_size_pr_range` 経由の pre-check 段階で走り、
    /// `DEFAULT_STEP_TIMEOUT_SECS` 等のバックストップ対象外。timeout が無いと
    /// push-runner 全体が無期限にハングする。
    ///
    /// 応答しない子プロセスを注入して、**timeout 到達で false (= ローカル名へ
    /// フォールバック = 従来挙動) に倒れ、かつ**子の終了を待ち切らない**ことを固定する。
    ///
    /// **直接の子プロセスを spawn する** (シェルを挟まない) のが要点。シェル経由にすると
    /// 孫プロセスがパイプ handle を握り続け、`kill` 後も drain thread の `join` が孫の
    /// 終了までブロックする — 実測で 1 秒 timeout に対し 19 秒かかった。これは本 timeout
    /// の穴ではなく **順位 323 (timeout の孫プロセス穴) の領域**なので、本テストは
    /// `resolve_with_timeout` が保証する範囲 (直接の子) を固定する。
    #[test]
    fn resolve_with_timeout_returns_false_when_child_hangs() {
        use std::process::Stdio;

        let started = std::time::Instant::now();
        let hung = crate::config::resolve_with_timeout(1, || {
            let mut cmd = if cfg!(windows) {
                let mut c = std::process::Command::new("ping");
                c.args(["-n", "20", "127.0.0.1"]);
                c
            } else {
                let mut c = std::process::Command::new("sleep");
                c.arg("20");
                c
            };
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        });

        assert!(
            !hung,
            "timeout した解決は false (ローカル bookmark 名へフォールバック) に倒すこと"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(15),
            "timeout が効かず子プロセスの終了まで待っている (実測 {:?})",
            started.elapsed()
        );
    }

    /// spawn 自体が失敗しても false に倒れること (jj が PATH に無い環境)。
    #[test]
    fn resolve_with_timeout_returns_false_when_spawn_fails() {
        let result = crate::config::resolve_with_timeout(5, || {
            std::process::Command::new("definitely-not-an-executable-xyz").spawn()
        });
        assert!(!result, "spawn 失敗も安全側 (false) に倒すこと");
    }

    /// 対比: 正常終了する子プロセスは true (効きすぎ防止)。
    #[test]
    fn resolve_with_timeout_returns_true_on_success() {
        use std::process::Stdio;

        let result = crate::config::resolve_with_timeout(30, || {
            let mut cmd = if cfg!(windows) {
                let mut c = std::process::Command::new("cmd");
                c.args(["/C", "exit 0"]);
                c
            } else {
                std::process::Command::new("true")
            };
            cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()
        });
        assert!(result, "正常終了は true (remote ref が解決できた扱い)");
    }
    /// simplicity review (SIM-NEW-config-mod-L228) の regression guard:
    /// **同一 revset の解決は 1 回だけ spawn する**。
    ///
    /// `pr_range_revset` は 1 回の push で最低 9 回呼ばれる (main.rs の pre-check /
    /// diff 準備、post_takt_regate、validate)。メモ化が外れると同じ `jj log` を
    /// 毎回 spawn することになる。
    ///
    /// 注入版 (`pr_range_revset_with`) はキャッシュを通さない設計なので、ここでは
    /// メモ化本体 (`remote_ref_exists` のキャッシュ) を直接検証する。
    #[test]
    fn remote_ref_exists_memoizes_per_revset() {
        let calls = std::cell::Cell::new(0u32);
        let resolved_first = crate::config::remote_ref_exists_cached("test@origin", || {
            calls.set(calls.get() + 1);
            true
        });
        let resolved_second = crate::config::remote_ref_exists_cached("test@origin", || {
            calls.set(calls.get() + 1);
            true
        });

        assert!(resolved_first && resolved_second, "解決結果は一貫すること");
        assert_eq!(
            calls.get(),
            1,
            "同一 revset の 2 回目は spawn せずキャッシュを返すこと"
        );
    }

    /// 別 revset は別エントリとして解決される (キャッシュが取り違えないこと)。
    #[test]
    fn remote_ref_exists_cache_is_keyed_by_revset() {
        let hit = crate::config::remote_ref_exists_cached("resolvable@origin", || true);
        let miss = crate::config::remote_ref_exists_cached("missing@origin", || false);

        assert!(hit, "解決できる revset は true");
        assert!(!miss, "解決できない revset は false (ローカル名へフォールバック)");
    }
}
