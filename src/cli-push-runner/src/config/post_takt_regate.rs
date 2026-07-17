use serde::Deserialize;

/// T12 (fix 後の決定論再ゲート) — takt (reviewers → fix loop) がコードを書き換えた場合に
/// のみ quality_gate を決定論的に再実行する stage の config。
///
/// ## 背景
///
/// `main.rs` の run_pipeline は quality_gate → takt → push の順で、**takt の fix が
/// コードを書き換えた後に決定論検証が無かった**。fix の検証は `fix.md` が fix agent に
/// 義務付ける自己申告 (`cargo build/test`) のみに依存しており、虚偽ではないが検証不足の
/// `fully_resolved` (例: `cargo test` は通したが `#[ignore]` 統合テスト未実行) が回帰を
/// 素通しさせ得た。同型の穴は post-PR 経路で PR #224 の実害後に決定論 gate
/// (`cli-pr-monitor/src/stages/gate.rs`) で塞がれたが、pre-push 経路は未対応だった。
/// 本 stage が pre-push 経路にも同じ機械的 backstop を導入する (ADR-037 §Mitigations 追記、
/// ADR-043 整合)。
///
/// ## 変化検出 (ADR-021 の原則)
///
/// takt 起動前の diff snapshot (Stage 1.5 が書き出した `[diff] output_path`) を保持し、
/// takt 実行後に `[diff] command` を再取得して**前後比較**する。両者が一致 = 作業コピー
/// 不変 = fix はコードを書き換えていない → re-gate を skip する。差分あり = fix が変更した
/// → quality_gate を再実行する。snapshot の前後比較は metadata のみの変化 (auto-snapshot
/// timestamp 等、ADR-021 § commit_id 単独比較の限界) に不感で、「実質変更があったか」を
/// 直接判定する。判定は pure function (`decide_regate`)、jj 呼び出しは closure 注入
/// (ADR-021 原則 3)。
///
/// ## fail-closed (ADR-043)
///
/// 前後どちらかの snapshot が取得不能なときは「変化あり」に倒して re-gate を実行する。
/// judgment 不能を「skip 可能」に倒すことはしない。post-pr gate と同じ gate 系の
/// fail-closed 方向 (ADR-021 原則 4 の repush 系 fail-**safe** = 何もしない、とは逆向き
/// であることに注意: gate では「判定不能なら実行」が安全側)。
///
/// ## ADR-039 (Experimental feature 標準パターン) 3 点セット準拠
///
/// - **Config opt-in**: 試験運用のため default OFF。`[post_takt_regate]` section 不在 /
///   `enabled` 未設定 / `enabled = false` のいずれも re-gate を完全 skip
///   (= 従来どおり takt 後に再検証なし)。派生 repo の templates は本 section を置かず
///   default OFF を継承する。本リポジトリでは明示的に `enabled = true` で dogfood を開始。
/// - **Kill-switch**: `enabled = false` で完全停止。env `POST_TAKT_REGATE_DISABLE=1` で
///   個別 push の意図的バイパス (再ゲートを飛ばして push したいとき)。
/// - **Bounded lifetime**: fix 発生 push で re-gate が破壊的変更を検出して block する効果と
///   誤 block (fix が実は無害な変更なのに gate が落ちる) の有無を観測後、default-ON 昇格 or
///   却下を判定。判定は `docs/adr/adr-058-post-takt-regate.md` の bounded lifetime に引き継ぐ。
#[derive(Deserialize)]
pub(crate) struct PostTaktRegateConfig {
    pub(crate) enabled: Option<bool>,
}

impl PostTaktRegateConfig {
    /// `enabled = true` が明示されているときのみ re-gate を有効とみなす
    /// (ADR-039 opt-in: 未設定 / false は OFF)。
    pub(crate) fn is_enabled(&self) -> bool {
        self.enabled == Some(true)
    }
}

#[cfg(test)]
mod tests {
    use crate::config::Config;

    fn parse(toml_str: &str) -> Config {
        toml::from_str(toml_str).expect("config should parse")
    }

    const BASE: &str = r#"
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

    #[test]
    fn config_absent_section_yields_none() {
        let config = parse(BASE);
        assert!(
            config.post_takt_regate.is_none(),
            "absent [post_takt_regate] should yield None (default OFF lane)"
        );
    }

    #[test]
    fn enabled_true_is_enabled() {
        let toml_str = format!("{}\n[post_takt_regate]\nenabled = true\n", BASE);
        let s = parse(&toml_str).post_takt_regate.unwrap();
        assert!(s.is_enabled());
    }

    #[test]
    fn enabled_false_is_not_enabled() {
        let toml_str = format!("{}\n[post_takt_regate]\nenabled = false\n", BASE);
        let s = parse(&toml_str).post_takt_regate.unwrap();
        assert!(
            !s.is_enabled(),
            "enabled = false must be OFF (opt-in: only explicit true activates)"
        );
    }

    #[test]
    fn enabled_omitted_is_not_enabled() {
        let toml_str = format!("{}\n[post_takt_regate]\n", BASE);
        let s = parse(&toml_str).post_takt_regate.unwrap();
        assert!(
            !s.is_enabled(),
            "section present but enabled omitted must be OFF (opt-in)"
        );
    }
}
