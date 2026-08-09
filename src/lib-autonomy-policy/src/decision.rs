//! 自律実行ゲートの純粋判定コア (ADR-066)。
//!
//! I/O を一切行わない。config ファイル / env の読み取りは [`crate::sources`] が担い、本
//! module は読み取り済みの値だけを受け取って許可 / 拒否を決める。
//!
//! # fail-closed を 1 関数へ集約する理由
//!
//! 「入力の欠損・読み取り不能・解釈不能はすべて deny」という ADR-052 原則 3 / ADR-043 の
//! 既定を、呼び手ごとに書かせない。呼び手が env / config を直読みして独自に真偽を組み立てると、
//! 片方の呼び手だけが `unwrap_or(true)` を書いた瞬間に無音で fail-open へ反転する。
//! lib-docs-policy が ADR-035 の path 基準を単一実装に集約しているのと同じ drift 防止。

/// 診断メッセージへ載せる生値の最大長。異常に長い env 値で run log を埋めないための上限。
const RAW_VALUE_LOG_CAP: usize = 32;

/// 自律 actor が実行しようとしている操作クラス (ADR-052 原則 2 の自動実行可クラス内訳)。
///
/// ADR-052 原則 5 の契約は背圧の接続も自動実行可の前提条件とするが、背圧の**指標**は操作
/// クラスごとに異なる。よって「どの指標を要求するか」だけを本 enum が持ち、指標の実測値は
/// [`GateInputs`] から受け取る。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// 既存 PR ブランチへの fix push。背圧は cli-pr-monitor の有界 retry (max_retries) が担う。
    FixPush,
    /// 自律 actor による PR 作成。背圧の指標は「未マージの `claude/` PR 数」(ADR-071)。
    ///
    /// 指標は当初「未マージ **draft** 数」だったが、夜間ループの停止点を draft PR から
    /// 通常 PR へ移した時点で draft 属性を条件から外した (ADR-072 決定 15)。draft を
    /// 数え続けると**常に 0 件 = 背圧が無効化**され、ADR-052 原則 5 が禁じる状態になる。
    AutonomousPr,
}

impl Operation {
    pub fn as_str(self) -> &'static str {
        match self {
            Operation::FixPush => "fix-push",
            Operation::AutonomousPr => "autonomous-pr",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "fix-push" => Some(Operation::FixPush),
            "autonomous-pr" => Some(Operation::AutonomousPr),
            _ => None,
        }
    }

    /// 本操作クラスが「未マージの自律 PR 数」の背圧を要求するか (ADR-052 原則 5 / ADR-071)。
    ///
    /// `FixPush` が `false` なのは背圧が不要だからではない。fix push の背圧は cli-pr-monitor の
    /// 有界 retry (`max_retries`) が担っており、**呼び出しごとに gate へ渡す状態を持たない**
    /// ため、判定入力としては現れない。PR 数を入力として要求するのは `AutonomousPr` だけ。
    ///
    /// 背圧の「状態」(実測 PR 数・閾値) はここには持たない。持たせると [`GateInputs`] の
    /// 入力と二重管理になり、判定経路が分岐する (ADR-071 § 決定 2)。
    fn requires_autonomous_pr_backpressure(self) -> bool {
        matches!(self, Operation::AutonomousPr)
    }
}

/// 判定入力。すべて読み取り済みの値で、`Option` の `None` が「接続されていない」を表す。
#[derive(Clone, Copy, Debug)]
pub struct GateInputs<'a> {
    /// `autonomy-config.toml` の `[autonomy] enabled`。
    /// `None` = ファイル欠落 / 読み取り不能 / parse 失敗 / section 欠落 / キー未指定。
    pub repo_config_enabled: Option<bool>,
    /// 外部フラグ (CI variable → env / ローカル env) の生値。`None` = 未設定。
    pub external_raw: Option<&'a str>,
    /// 未マージの自律 PR (`claude/` prefix) の実測件数。`None` = 未取得 / 取得失敗 (= 停止)。
    ///
    /// 取得は呼び手 (workflow step の `gh api`) の責務。`0` と `None` は意味が異なる —
    /// `0` は「数えた結果 0 件」、`None` は「数えられなかった」で、後者は deny に倒れる。
    ///
    /// **draft 属性で絞り込まない。** 絞り込むと停止点が通常 PR になった時点で常に 0 件へ
    /// 潰れ、背圧が無音で無効化される (ADR-072 決定 15)。
    pub open_autonomous_prs: Option<u32>,
    /// `autonomy-config.toml` の `[autonomy] max_open_autonomous_prs`。`None` = 読めない (= 停止)。
    pub max_open_autonomous_prs: Option<u32>,
    pub operation: Operation,
}

/// deny の理由。loud 出力と exit コードの根拠になる。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DenyReason {
    /// 外部フラグが未設定 (CI variable 未定義 / env 未設定)。
    ExternalUnset,
    /// 外部フラグが truthy でない。生値を診断へ出す (ADR-039 § 2: 診断は実受理値を反映する)。
    ExternalNotTruthy(String),
    /// repo config を読めない (欠落 / parse 失敗 / section 欠落 / キー未指定)。
    RepoConfigUnavailable,
    /// repo config が明示的に `enabled = false`。
    RepoConfigDisabled,
    /// 操作クラスが要求する背圧の指標を読めない (実測値 / 閾値のいずれかが欠落)。
    BackpressureUnavailable(Operation),
    /// 背圧が飽和した = 未マージの自律 PR が閾値に達している (自主減速)。
    BackpressureSaturated { open: u32, limit: u32 },
}

impl DenyReason {
    /// 人間向けの 1 行説明。run log に出て原因切り分けに使われる。
    pub fn describe(&self, env_name: &str, config_path: &str) -> String {
        match self {
            DenyReason::ExternalUnset => {
                format!("外部フラグ {env_name} が未設定です (未接続 = 停止)")
            }
            DenyReason::ExternalNotTruthy(raw) => {
                format!("外部フラグ {env_name} が有効値ではありません (受理値: 1|true|yes|on、実値: {raw:?})")
            }
            DenyReason::RepoConfigUnavailable => format!(
                "{config_path} の [autonomy] enabled を読めません (欠落 / parse 失敗 / 未指定 = 停止)"
            ),
            DenyReason::RepoConfigDisabled => {
                format!("{config_path} で [autonomy] enabled = false が指定されています")
            }
            DenyReason::BackpressureUnavailable(op) => format!(
                "操作クラス {} の背圧を読めません (未マージの自律 PR 数の実測値または {config_path} の \
[autonomy] max_open_autonomous_prs が欠落。ADR-052 原則 5 の契約により停止)",
                op.as_str()
            ),
            DenyReason::BackpressureSaturated { open, limit } => format!(
                "未マージの自律 PR が {open} 件で閾値 {limit} 件に達しています (自主減速。\
該当 PR をマージ / クローズするか {config_path} の max_open_autonomous_prs を見直してください)"
            ),
        }
    }

    /// telemetry / grep 用の安定した短縮コード。生値は含めない。
    pub fn code(&self) -> &'static str {
        match self {
            DenyReason::ExternalUnset => "external-unset",
            DenyReason::ExternalNotTruthy(_) => "external-not-truthy",
            DenyReason::RepoConfigUnavailable => "repo-config-unavailable",
            DenyReason::RepoConfigDisabled => "repo-config-disabled",
            DenyReason::BackpressureUnavailable(_) => "backpressure-unavailable",
            DenyReason::BackpressureSaturated { .. } => "backpressure-saturated",
        }
    }
}

/// 判定結果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Denied(DenyReason),
}

/// 純粋判定コア。
///
/// 判定順は決定論的に「外部フラグ → repo config → 背圧」。外部フラグを先に見るのは、
/// 緊急停止で最初に操作される面 (CI variable) だからで、drill 時の deny 理由が操作クラスに
/// 依らず一定になる。全ソースの状態は [`describe_sources`] が別途 loud 出力するため、
/// 先頭理由だけを返しても診断情報は失われない。
///
/// 背圧の飽和判定が `>=` であって `>` ではないのは、閾値が「これ以上は積まない」上限だから。
/// `limit = 0` は「自律 PR を 1 件も作らない」= 実質停止を意味する (ADR-071 § 決定 3)。
pub fn evaluate(inputs: GateInputs<'_>) -> Decision {
    match inputs.external_raw {
        None => return Decision::Denied(DenyReason::ExternalUnset),
        Some(raw) if !lib_telemetry::is_truthy(raw) => {
            return Decision::Denied(DenyReason::ExternalNotTruthy(truncate_for_log(raw)));
        }
        Some(_) => {}
    }
    match inputs.repo_config_enabled {
        None => return Decision::Denied(DenyReason::RepoConfigUnavailable),
        Some(false) => return Decision::Denied(DenyReason::RepoConfigDisabled),
        Some(true) => {}
    }
    if inputs.operation.requires_autonomous_pr_backpressure() {
        let (Some(open), Some(limit)) = (inputs.open_autonomous_prs, inputs.max_open_autonomous_prs) else {
            return Decision::Denied(DenyReason::BackpressureUnavailable(inputs.operation));
        };
        if open >= limit {
            return Decision::Denied(DenyReason::BackpressureSaturated { open, limit });
        }
    }
    Decision::Allowed
}

/// 全ソースの状態を 1 行へ要約する (loud 出力用)。
///
/// deny 理由を 1 つに絞る [`evaluate`] と違い、こちらは 3 ソースすべてを出す。
/// 「フラグを 1 つ直したのにまだ止まる」という切り分けを 1 run の log だけで完結させる。
pub fn describe_sources(inputs: GateInputs<'_>, env_name: &str) -> String {
    let external = match inputs.external_raw {
        None => "unset".to_string(),
        Some(raw) if lib_telemetry::is_truthy(raw) => "enabled".to_string(),
        Some(raw) => format!("not-truthy({:?})", truncate_for_log(raw)),
    };
    let repo_config = match inputs.repo_config_enabled {
        None => "unavailable",
        Some(true) => "enabled",
        Some(false) => "disabled",
    };
    format!(
        "{env_name}={external} repo_config={repo_config} backpressure({})={}",
        inputs.operation.as_str(),
        describe_backpressure(inputs)
    )
}

/// 背圧 1 ソース分の状態表記。
///
/// `structural` は「この操作クラスは自律 PR 数を入力として要求しない」を意味する
/// (fix push は cli-pr-monitor の有界 retry が背圧を担う)。自律 PR 数を要求するクラスでは
/// 実測値と閾値を必ず併記し、「止まったのは数え損ねか、それとも積み過ぎか」を run log
/// 1 行で切り分けられるようにする。
fn describe_backpressure(inputs: GateInputs<'_>) -> String {
    if !inputs.operation.requires_autonomous_pr_backpressure() {
        return "structural".to_string();
    }
    match (inputs.open_autonomous_prs, inputs.max_open_autonomous_prs) {
        (Some(open), Some(limit)) if open >= limit => format!("saturated({open}/{limit})"),
        (Some(open), Some(limit)) => format!("ok({open}/{limit})"),
        _ => "unavailable".to_string(),
    }
}

/// 生値を診断用に切り詰める。切り詰めた場合は省略記号を付けて全量でないことを明示する。
fn truncate_for_log(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(RAW_VALUE_LOG_CAP)
        .collect();
    if raw.chars().count() > RAW_VALUE_LOG_CAP {
        format!("{cleaned}…")
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// truthy として受理される表記 (lib-telemetry の受理集合 + 前後空白 / 大小差)。
    const TRUTHY: &[&str] = &["1", "true", "TRUE", "yes", "on", " on ", "  True  "];
    /// truthy でない表記。空文字・0・false に加え、解釈不能なゴミ値を含む。
    const NOT_TRUTHY: &[&str] = &["", "0", "false", "off", "no", "enabled", "2", "１"];

    /// 背圧入力を接続しない既定形。`AutonomousPr` はこの形では常に deny になる。
    fn inputs<'a>(
        repo_config_enabled: Option<bool>,
        external_raw: Option<&'a str>,
        operation: Operation,
    ) -> GateInputs<'a> {
        GateInputs {
            repo_config_enabled,
            external_raw,
            open_autonomous_prs: None,
            max_open_autonomous_prs: None,
            operation,
        }
    }

    /// kill-switch 2 面を有効にしたうえで背圧入力だけを差し替える。
    fn with_backpressure<'a>(
        operation: Operation,
        open_autonomous_prs: Option<u32>,
        max_open_autonomous_prs: Option<u32>,
    ) -> GateInputs<'a> {
        GateInputs {
            repo_config_enabled: Some(true),
            external_raw: Some("true"),
            open_autonomous_prs,
            max_open_autonomous_prs,
            operation,
        }
    }

    #[test]
    fn allows_only_when_every_source_is_connected() {
        for raw in TRUTHY {
            let decision = evaluate(inputs(Some(true), Some(raw), Operation::FixPush));
            assert_eq!(decision, Decision::Allowed, "truthy 表記 {raw:?} が許可されない");
        }
    }

    #[test]
    fn denies_when_external_is_unset() {
        for op in [Operation::FixPush, Operation::AutonomousPr] {
            for repo in [None, Some(true), Some(false)] {
                assert_eq!(
                    evaluate(inputs(repo, None, op)),
                    Decision::Denied(DenyReason::ExternalUnset),
                    "external 未設定は repo={repo:?} op={op:?} でも deny でなければならない"
                );
            }
        }
    }

    #[test]
    fn denies_when_external_is_not_truthy() {
        for raw in NOT_TRUTHY {
            let decision = evaluate(inputs(Some(true), Some(raw), Operation::FixPush));
            assert!(
                matches!(decision, Decision::Denied(DenyReason::ExternalNotTruthy(_))),
                "非 truthy 表記 {raw:?} が deny されない"
            );
        }
    }

    #[test]
    fn denies_when_repo_config_is_unavailable_or_disabled() {
        assert_eq!(
            evaluate(inputs(None, Some("1"), Operation::FixPush)),
            Decision::Denied(DenyReason::RepoConfigUnavailable)
        );
        assert_eq!(
            evaluate(inputs(Some(false), Some("1"), Operation::FixPush)),
            Decision::Denied(DenyReason::RepoConfigDisabled)
        );
    }

    /// 背圧入力が 1 つでも欠ければ、kill-switch が両面とも有効でも自律 PR は通さない。
    /// 実測値と閾値のどちらが欠けても同じ deny に倒れることを固定する (ADR-052 原則 5)。
    #[test]
    fn denies_autonomous_pr_when_any_backpressure_input_is_missing() {
        for (open, limit) in [(None, None), (Some(0), None), (None, Some(3))] {
            assert_eq!(
                evaluate(with_backpressure(Operation::AutonomousPr, open, limit)),
                Decision::Denied(DenyReason::BackpressureUnavailable(Operation::AutonomousPr)),
                "open={open:?} limit={limit:?} は背圧未接続として deny でなければならない"
            );
        }
    }

    /// 閾値ちょうどで止まる (`>` ではなく `>=`)。境界の off-by-one を pin する。
    #[test]
    fn autonomous_pr_stops_at_the_limit_not_after_it() {
        assert_eq!(
            evaluate(with_backpressure(Operation::AutonomousPr, Some(2), Some(3))),
            Decision::Allowed
        );
        assert_eq!(
            evaluate(with_backpressure(Operation::AutonomousPr, Some(3), Some(3))),
            Decision::Denied(DenyReason::BackpressureSaturated { open: 3, limit: 3 })
        );
        assert_eq!(
            evaluate(with_backpressure(Operation::AutonomousPr, Some(9), Some(3))),
            Decision::Denied(DenyReason::BackpressureSaturated { open: 9, limit: 3 })
        );
    }

    /// `limit = 0` は「自律 PR を 1 件も作らない」= 実質停止。0 件でも通さない。
    #[test]
    fn zero_limit_denies_every_autonomous_pr() {
        assert_eq!(
            evaluate(with_backpressure(Operation::AutonomousPr, Some(0), Some(0))),
            Decision::Denied(DenyReason::BackpressureSaturated { open: 0, limit: 0 })
        );
    }

    /// fix push の背圧は cli-pr-monitor の有界 retry が担うため、自律 PR 数の飽和では止めない。
    /// 「自律 PR が溜まったら fix push も止まる」という意図しない結合が入っていないことを固定する。
    #[test]
    fn fix_push_is_unaffected_by_autonomous_pr_backpressure_inputs() {
        for (open, limit) in [(None, None), (Some(99), Some(1)), (Some(0), Some(3))] {
            assert_eq!(
                evaluate(with_backpressure(Operation::FixPush, open, limit)),
                Decision::Allowed,
                "open={open:?} limit={limit:?} で fix-push が止まってはならない"
            );
        }
    }

    /// 背圧が通っても kill-switch 2 面は依然として先に効く (判定順の固定)。
    #[test]
    fn backpressure_never_overrides_the_kill_switch() {
        let saturated_but_switched_off = GateInputs {
            repo_config_enabled: Some(false),
            external_raw: Some("true"),
            open_autonomous_prs: Some(0),
            max_open_autonomous_prs: Some(3),
            operation: Operation::AutonomousPr,
        };
        assert_eq!(
            evaluate(saturated_but_switched_off),
            Decision::Denied(DenyReason::RepoConfigDisabled)
        );
    }

    /// 全組み合わせを走査し、「許可されるのは 3 条件が揃った場合だけ」を網羅的に固定する。
    #[test]
    fn allow_is_exhaustively_limited_to_the_fully_connected_combinations() {
        let externals: Vec<Option<&str>> = std::iter::once(None)
            .chain(TRUTHY.iter().chain(NOT_TRUTHY.iter()).map(|s| Some(*s)))
            .collect();
        let backpressures: [(Option<u32>, Option<u32>); 4] =
            [(None, None), (Some(0), None), (Some(0), Some(3)), (Some(3), Some(3))];
        let mut allowed_count = 0;
        for op in [Operation::FixPush, Operation::AutonomousPr] {
            for repo in [None, Some(true), Some(false)] {
                for external in &externals {
                    for (open, limit) in backpressures {
                        let allowed = evaluate(GateInputs {
                            repo_config_enabled: repo,
                            external_raw: *external,
                            open_autonomous_prs: open,
                            max_open_autonomous_prs: limit,
                            operation: op,
                        }) == Decision::Allowed;
                        let backpressure_ok = op == Operation::FixPush
                            || matches!((open, limit), (Some(o), Some(l)) if o < l);
                        let expected = repo == Some(true)
                            && external.is_some_and(lib_telemetry::is_truthy)
                            && backpressure_ok;
                        assert_eq!(
                            allowed, expected,
                            "op={op:?} repo={repo:?} ext={external:?} open={open:?} limit={limit:?}"
                        );
                        allowed_count += usize::from(allowed);
                    }
                }
            }
        }
        let fix_push_allows_every_backpressure = backpressures.len();
        let autonomous_pr_allows_only_the_unsaturated_one = 1;
        assert_eq!(
            allowed_count,
            TRUTHY.len()
                * (fix_push_allows_every_backpressure + autonomous_pr_allows_only_the_unsaturated_one),
            "許可される組み合わせ数が想定外"
        );
    }

    #[test]
    fn describe_sources_reports_every_source_state() {
        let line = describe_sources(inputs(None, Some("0"), Operation::AutonomousPr), "AUTONOMY_ENABLED");
        assert!(line.contains("AUTONOMY_ENABLED=not-truthy"), "{line}");
        assert!(line.contains("repo_config=unavailable"), "{line}");
        assert!(line.contains("backpressure(autonomous-pr)=unavailable"), "{line}");
    }

    /// 背圧の 3 状態が実測値付きで出ること。deny 行だけで「数え損ね」と「積み過ぎ」を
    /// 切り分けられるかは、この表記が実数を含むかに依存する。
    #[test]
    fn describe_sources_distinguishes_backpressure_states() {
        let describe = |open, limit, op| {
            describe_sources(with_backpressure(op, open, limit), "AUTONOMY_ENABLED")
        };
        assert!(describe(Some(1), Some(3), Operation::AutonomousPr).contains("backpressure(autonomous-pr)=ok(1/3)"));
        assert!(describe(Some(3), Some(3), Operation::AutonomousPr)
            .contains("backpressure(autonomous-pr)=saturated(3/3)"));
        assert!(describe(None, Some(3), Operation::AutonomousPr)
            .contains("backpressure(autonomous-pr)=unavailable"));
        assert!(describe(Some(9), Some(1), Operation::FixPush)
            .contains("backpressure(fix-push)=structural"));
    }

    #[test]
    fn long_and_control_raw_values_are_truncated_and_sanitized() {
        let raw = format!("{}\nTAIL", "x".repeat(RAW_VALUE_LOG_CAP));
        let truncated = truncate_for_log(&raw);
        assert!(truncated.ends_with('…'), "{truncated}");
        assert!(!truncated.contains('\n'), "改行が残ると log 行が壊れる: {truncated}");
        assert_eq!(truncate_for_log("a\tb"), "a b");
    }

    #[test]
    fn operation_parse_rejects_unknown_values() {
        assert_eq!(Operation::parse("fix-push"), Some(Operation::FixPush));
        assert_eq!(Operation::parse("autonomous-pr"), Some(Operation::AutonomousPr));
        assert_eq!(Operation::parse("merge"), None);
        assert_eq!(Operation::parse("FIX-PUSH"), None);
        assert_eq!(Operation::parse(""), None);
    }

    /// 旧名 `draft-pr` は受理しない (ADR-072 決定 15 の改名)。
    ///
    /// 呼び手を更新し忘れた場合に**別名として通ってしまう**のが最悪で、その形だと背圧付きの
    /// クラスが背圧なしで動く。`None` を返せば呼び手は引数不正 (exit 2) で止まるため、
    /// 取りこぼしは fail-closed 側に倒れる。
    #[test]
    fn the_pre_rename_operation_name_is_not_accepted_as_an_alias() {
        assert_eq!(Operation::parse("draft-pr"), None);
    }

    #[test]
    fn deny_reason_codes_are_stable_and_free_of_raw_values() {
        let reason = DenyReason::ExternalNotTruthy("secret-ish".to_string());
        assert_eq!(reason.code(), "external-not-truthy");
        assert!(!reason.code().contains("secret"));
        assert_eq!(
            DenyReason::BackpressureSaturated { open: 3, limit: 3 }.code(),
            "backpressure-saturated"
        );
    }

    /// 飽和の説明文は実数と復旧手段を含む。run log だけで次の操作が決まるようにする。
    #[test]
    fn saturated_description_names_the_numbers_and_the_remedy() {
        let text = DenyReason::BackpressureSaturated { open: 4, limit: 3 }
            .describe("AUTONOMY_ENABLED", "autonomy-config.toml");
        assert!(text.contains('4') && text.contains('3'), "{text}");
        assert!(text.contains("max_open_autonomous_prs"), "{text}");
    }
}
