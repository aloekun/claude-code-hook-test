use std::time::{Duration, Instant};

use crate::config::{GroupConfig, QualityGateConfig, DEFAULT_STEP_TIMEOUT_SECS};
use crate::log::{log_stage, log_step};
use crate::runner::run_cmd;

pub(crate) fn run_group(group: &GroupConfig, timeout: u64) -> (String, bool, Duration) {
    let start = Instant::now();

    if let Some(pre) = &group.pre {
        log_step(&group.name, "PRE", pre);
        let (ok, output) = run_cmd(&group.name, pre, timeout);
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
        let (ok, output) = run_cmd(&group.name, cmd, timeout);
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

pub(crate) fn run_quality_gate(config: &QualityGateConfig) -> bool {
    let timeout = config.step_timeout.unwrap_or(DEFAULT_STEP_TIMEOUT_SECS);
    let parallel = config.parallel.unwrap_or(true);

    log_stage(
        "quality_gate",
        &format!(
            "開始 ({} グループ, {})",
            config.groups.len(),
            if parallel { "並列" } else { "直列" }
        ),
    );

    let results: Vec<(String, bool, Duration)> = if parallel {
        let handles: Vec<_> = config
            .groups
            .iter()
            .map(|group| {
                let group = group.clone();
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
        config
            .groups
            .iter()
            .map(|group| run_group(group, timeout))
            .collect()
    };

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
            !run_quality_gate(&config),
            "mixed pass/fail should return false overall"
        );
    }
}
