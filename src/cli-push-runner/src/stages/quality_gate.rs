use std::time::{Duration, Instant};

use lib_subprocess::run_cmd_shell_unlimited;

use crate::config::{GroupConfig, QualityGateConfig, DEFAULT_STEP_TIMEOUT_SECS};
use crate::log::{log_stage, log_step};

/// quality_gate の 1 step を実行し `(success, 全量出力)` を返す。
///
/// 出力を **truncate せず全量**取得するのは、失敗 step の診断情報 (cargo test の
/// 失敗テスト一覧など) を落とさないため。旧実装は `run_cmd_shell_capped`
/// (`MAX_LINES` = 40 行の silent truncate) の出力を失敗時にそのまま表示していたため、
/// 41 行目以降に出る失敗内容が消えて診断できなかった (R1 = T5「失敗経路は診断を
/// 落とさない」原則の残り半分。§4 T5 / §6 backlog 1)。
///
/// success/failure の判定は exit status に基づき出力量に依存しないが、失敗時の
/// **表示**には全量が要る。成功時は出力を表示しない (従来どおり quiet) ため、
/// 全量保持のコスト (メモリ線形成長) は失敗 step の診断のためだけに払う。
fn run_step(name: &str, cmd: &str, timeout: u64) -> (bool, String) {
    run_cmd_shell_unlimited(name, cmd, timeout)
}

pub(crate) fn run_group(group: &GroupConfig, timeout: u64) -> (String, bool, Duration) {
    let start = Instant::now();

    if let Some(pre) = &group.pre {
        log_step(&group.name, "PRE", pre);
        let (ok, output) = run_step(&group.name, pre, timeout);
        if !ok {
            log_step(&group.name, "FAIL", "pre コマンド失敗");
            if !output.is_empty() {
                eprintln!("{}", output);
            }
            return (group.name.clone(), false, start.elapsed());
        }
    }

    for cmd in &group.commands {
        log_step(&group.name, "RUN", cmd);
        let (ok, output) = run_step(&group.name, cmd, timeout);
        if ok {
            log_step(&group.name, "PASS", "");
        } else {
            log_step(&group.name, "FAIL", "");
            if !output.is_empty() {
                eprintln!("{}", output);
            }
            return (group.name.clone(), false, start.elapsed());
        }
    }

    (group.name.clone(), true, start.elapsed())
}

/// docs-only routing (T11) が返した skip 対象を除いた実行対象 group を返す。
/// skip 対象の group 名を 1 件でも実 group にマッチさせられなかった場合は
/// warning を出す (skip_groups の typo が silent no-op になるのを防ぐ)。
fn effective_groups<'a>(
    groups: &'a [GroupConfig],
    skip_groups: &[String],
) -> Vec<&'a GroupConfig> {
    if skip_groups.is_empty() {
        return groups.iter().collect();
    }
    for name in skip_groups {
        if !groups.iter().any(|g| &g.name == name) {
            log_stage(
                "quality_gate",
                &format!(
                    "警告: skip 指定の group '{}' が存在しません (docs_only_routing の設定を確認)",
                    name
                ),
            );
        }
    }
    let retained: Vec<&GroupConfig> = groups
        .iter()
        .filter(|g| !skip_groups.contains(&g.name))
        .collect();
    if retained.is_empty() {
        log_stage(
            "quality_gate",
            "警告: skip 指定が全 group を除外するため skip を無視して全 group を実行 (fail-closed)",
        );
        return groups.iter().collect();
    }
    let skipped: Vec<&str> = groups
        .iter()
        .filter(|g| skip_groups.contains(&g.name))
        .map(|g| g.name.as_str())
        .collect();
    if !skipped.is_empty() {
        log_stage(
            "quality_gate",
            &format!("docs-only のため skip: {}", skipped.join(", ")),
        );
    }
    retained
}

fn run_groups(groups: &[&GroupConfig], timeout: u64, parallel: bool) -> Vec<(String, bool, Duration)> {
    if parallel {
        let handles: Vec<_> = groups
            .iter()
            .map(|group| {
                let group = (*group).clone();
                std::thread::spawn(move || run_group(&group, timeout))
            })
            .collect();

        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or(("unknown".into(), false, Duration::ZERO))
            })
            .collect()
    } else {
        groups
            .iter()
            .map(|group| run_group(group, timeout))
            .collect()
    }
}

pub(crate) fn run_quality_gate(config: &QualityGateConfig, skip_groups: &[String]) -> bool {
    let timeout = config.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);
    let parallel = config.parallel.unwrap_or(true);

    let groups = effective_groups(&config.groups, skip_groups);

    log_stage(
        "quality_gate",
        &format!(
            "開始 ({} グループ, {})",
            groups.len(),
            if parallel { "並列" } else { "直列" }
        ),
    );

    let results = run_groups(&groups, timeout, parallel);

    log_stage("quality_gate", "結果:");
    for (name, ok, elapsed) in &results {
        let status = if *ok { "PASS" } else { "FAIL" };
        log_step(name, status, &format!("{:.1}s", elapsed.as_secs_f64()));
    }

    let all_passed = results.iter().all(|(_, ok, _)| *ok);
    if all_passed {
        log_stage("quality_gate", "全グループ成功");
    } else {
        let failed: Vec<_> = results
            .iter()
            .filter(|(_, ok, _)| !ok)
            .map(|(name, _, _)| name.as_str())
            .collect();
        log_stage("quality_gate", &format!("失敗: {}", failed.join(", ")));
    }

    all_passed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{GroupConfig, QualityGateConfig};

    fn make_group(name: &str, commands: Vec<&str>) -> GroupConfig {
        GroupConfig {
            name: name.to_string(),
            pre: None,
            commands: commands.into_iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn run_group_returns_true_when_command_succeeds() {
        let g = make_group("ok-group", vec!["echo ok"]);
        let (name, ok, _) = run_group(&g, 10);
        assert_eq!(name, "ok-group");
        assert!(ok, "echo ok should succeed");
    }

    #[test]
    fn run_group_returns_false_when_command_fails() {
        let g = make_group("fail-group", vec!["exit 1"]);
        let (name, ok, _) = run_group(&g, 10);
        assert_eq!(name, "fail-group");
        assert!(!ok, "exit 1 should fail");
    }

    #[test]
    fn run_quality_gate_sequential_mixed_returns_false() {
        let config = QualityGateConfig {
            parallel: Some(false),
            step_timeout: Some(10),
            groups: vec![
                make_group("pass", vec!["echo ok"]),
                make_group("fail", vec!["exit 1"]),
            ],
        };
        assert!(
            !run_quality_gate(&config, &[]),
            "mixed pass/fail should return false overall"
        );
    }

    /// T11: docs-only routing が失敗 group を skip 指定すると gate が PASS になる。
    /// 逆に言えば skip リストが効いていることの証跡 (skip なしなら false)。
    #[test]
    fn run_quality_gate_skips_named_group() {
        let config = QualityGateConfig {
            parallel: Some(false),
            step_timeout: Some(10),
            groups: vec![
                make_group("keep", vec!["echo ok"]),
                make_group("rust-lint-test", vec!["exit 1"]),
            ],
        };
        assert!(
            !run_quality_gate(&config, &[]),
            "skip なしなら失敗 group が gate を落とす (対照)"
        );
        assert!(
            run_quality_gate(&config, &["rust-lint-test".to_string()]),
            "失敗 group を skip すれば残りが PASS"
        );
    }

    /// T11: skip 対象が実 group に 1 件も無い場合も、残り group は普通に評価される
    /// (typo 保護は warning のみで、gate 自体は骨抜きにしない)。
    #[test]
    fn effective_groups_retains_all_when_skip_unmatched() {
        let groups = vec![make_group("a", vec!["echo"]), make_group("b", vec!["echo"])];
        let skip = vec!["nonexistent".to_string()];
        let retained = effective_groups(&groups, &skip);
        assert_eq!(retained.len(), 2, "存在しない skip 名は誰も除外しない");
    }

    #[test]
    fn effective_groups_empty_skip_returns_all() {
        let groups = vec![make_group("a", vec!["echo"])];
        let retained = effective_groups(&groups, &[]);
        assert_eq!(retained.len(), 1);
    }

    /// fail-closed (ADR-043): skip 指定が全 group を覆う誤設定でも、gate を素通り
    /// (0 group 実行で `.all()` が空 vacuous pass) させず全 group を実行する。
    /// docs-only routing が JS 系まで skip 名に含める設定ミスに対する gate 骨抜き防止。
    #[test]
    fn effective_groups_skip_all_falls_back_to_full_run() {
        let groups = vec![make_group("a", vec!["echo"]), make_group("b", vec!["echo"])];
        let skip = vec!["a".to_string(), "b".to_string()];
        let retained = effective_groups(&groups, &skip);
        assert_eq!(
            retained.len(),
            2,
            "全 group を skip する指定は無視して全実行 (0 group で vacuous pass させない)"
        );
    }

    #[test]
    fn run_quality_gate_skip_all_does_not_vacuously_pass() {
        let config = QualityGateConfig {
            parallel: Some(false),
            step_timeout: Some(10),
            groups: vec![
                make_group("keep", vec!["echo ok"]),
                make_group("rust-lint-test", vec!["exit 1"]),
            ],
        };
        assert!(
            !run_quality_gate(
                &config,
                &["keep".to_string(), "rust-lint-test".to_string()]
            ),
            "全 group skip 指定でも失敗 group は実行され gate は落ちる (fail-closed)"
        );
    }

    /// R1 回帰テスト群: quality_gate の step 失敗時、出力が 40 行 cap で silent truncate
    /// され cargo test の失敗一覧が消えて診断できなかった不具合 (T5「失敗経路は診断を
    /// 落とさない」原則の残り半分。ADR-049 の流儀: 1 test = 1 failure mode + good/bad)。
    ///
    /// 由来: 2026-07-16 の push パイプライン調査で backlog 化 (§6 項目 1)。コード監査で
    /// 特定 (in the wild の発火記録は無く、cap 済み出力を失敗表示に使う構造的な診断喪失)。
    ///
    /// 事故の形: 旧 `run_group` は `run_cmd_shell_capped(MAX_LINES=40)` の出力を失敗時に
    /// `eprintln!` していたため、41 行目以降に出る失敗テスト名が silent truncate で落ち、
    /// 「どのテストが落ちたか」がログから判らなかった。
    ///
    /// 修正の核心は「失敗 step の出力を全量取得 (`run_step` = `run_cmd_shell_unlimited`)」。
    /// 成功時は従来どおり出力を表示しない (退行なし)。`run_step` を capped 版に戻すと
    /// `failing_step_output_is_not_truncated` が fail する (回帰テストが素通りしない証跡)。
    mod r1_failure_output_not_truncated {
        use super::*;
        use crate::runner::MAX_LINES;

        /// 40 行を超える出力を出してから失敗する step の再現。`cmd /c "A & exit 1"` は
        /// 最後のコマンド (`exit 1`) の exit code を返すため失敗として報告される。
        const FAIL_BEYOND_CAP: &str =
            "(for /L %i in (1,1,60) do @echo failing test line %i) & exit 1";

        /// incident 再現 (bad): 失敗 step の出力が cap (40 行) を超えても全量取得でき、
        /// cap の外にある診断行 (60 行目) が残ること。
        #[test]
        fn failing_step_output_is_not_truncated() {
            let (ok, output) = run_step("rust-lint-test", FAIL_BEYOND_CAP, 30);
            assert!(!ok, "exit 1 は失敗として報告される: {:?}", output);
            assert!(
                output.lines().count() > MAX_LINES,
                "run_step が {} 行に切り詰めている = R1 の不具合。失敗診断は truncate してはならない",
                output.lines().count(),
            );
            assert!(
                output.contains("failing test line 60"),
                "cap ({} 行) の外にある診断行が残ること: {} 行取得",
                MAX_LINES,
                output.lines().count(),
            );
        }

        /// good: 成功 step は従来どおり成功を報告する (退行なし)。成功経路の表示は
        /// `run_group` 側で変えていない (quiet のまま) ことと合わせ、変更が失敗経路に
        /// 閉じていることを固定する。
        #[test]
        fn passing_step_still_succeeds() {
            let (ok, output) = run_step("ok-group", "echo ok", 10);
            assert!(ok, "echo ok は成功: {:?}", output);
        }
    }
}
