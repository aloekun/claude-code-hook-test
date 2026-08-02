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
/// ADR-052 原則 5 の契約は背圧の接続も自動実行可の前提条件とするが、背圧の指標は操作クラス
/// ごとに異なる。よって「背圧が接続済みか」は本 enum の性質として持たせる。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Operation {
    /// 既存 PR ブランチへの fix push。背圧は cli-pr-monitor の有界 retry (max_retries) が担う。
    FixPush,
    /// draft PR 作成。背圧の指標は「未マージ draft 数」で、WP-18 まで未接続。
    DraftPr,
}

impl Operation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Operation::FixPush => "fix-push",
            Operation::DraftPr => "draft-pr",
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "fix-push" => Some(Operation::FixPush),
            "draft-pr" => Some(Operation::DraftPr),
            _ => None,
        }
    }

    /// 本操作クラスの背圧が接続済みか (ADR-052 原則 5 の契約)。
    ///
    /// `DraftPr` が `false` 固定なのは未実装の placeholder ではなく、**現時点の正しい
    /// fail-closed 状態**である。未マージ draft 数の背圧 (WP-18) が入るまで draft PR の
    /// 自動作成を構造的に禁止し、「kill-switch だけ有効化して draft の山を積む」経路を塞ぐ。
    /// WP-18 で背圧を実装する PR がここを `true` へ反転させる。
    fn backpressure_connected(self) -> bool {
        match self {
            Operation::FixPush => true,
            Operation::DraftPr => false,
        }
    }
}

/// 判定入力。すべて読み取り済みの値で、`Option` の `None` が「接続されていない」を表す。
#[derive(Clone, Copy, Debug)]
pub(crate) struct GateInputs<'a> {
    /// `autonomy-config.toml` の `[autonomy] enabled`。
    /// `None` = ファイル欠落 / 読み取り不能 / parse 失敗 / section 欠落 / キー未指定。
    pub(crate) repo_config_enabled: Option<bool>,
    /// 外部フラグ (CI variable → env / ローカル env) の生値。`None` = 未設定。
    pub(crate) external_raw: Option<&'a str>,
    pub(crate) operation: Operation,
}

/// deny の理由。loud 出力と exit コードの根拠になる。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DenyReason {
    /// 外部フラグが未設定 (CI variable 未定義 / env 未設定)。
    ExternalUnset,
    /// 外部フラグが truthy でない。生値を診断へ出す (ADR-039 § 2: 診断は実受理値を反映する)。
    ExternalNotTruthy(String),
    /// repo config を読めない (欠落 / parse 失敗 / section 欠落 / キー未指定)。
    RepoConfigUnavailable,
    /// repo config が明示的に `enabled = false`。
    RepoConfigDisabled,
    /// 操作クラスの背圧が未接続。
    BackpressureUnavailable(Operation),
}

impl DenyReason {
    /// 人間向けの 1 行説明。run log に出て原因切り分けに使われる。
    pub(crate) fn describe(&self, env_name: &str, config_path: &str) -> String {
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
                "操作クラス {} の背圧が未接続です (ADR-052 原則 5 の契約により停止)",
                op.as_str()
            ),
        }
    }

    /// telemetry / grep 用の安定した短縮コード。生値は含めない。
    pub(crate) fn code(&self) -> &'static str {
        match self {
            DenyReason::ExternalUnset => "external-unset",
            DenyReason::ExternalNotTruthy(_) => "external-not-truthy",
            DenyReason::RepoConfigUnavailable => "repo-config-unavailable",
            DenyReason::RepoConfigDisabled => "repo-config-disabled",
            DenyReason::BackpressureUnavailable(_) => "backpressure-unavailable",
        }
    }
}

/// 判定結果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Decision {
    Allowed,
    Denied(DenyReason),
}

/// 純粋判定コア。
///
/// 判定順は決定論的に「外部フラグ → repo config → 背圧」。外部フラグを先に見るのは、
/// 緊急停止で最初に操作される面 (CI variable) だからで、drill 時の deny 理由が操作クラスに
/// 依らず一定になる。全ソースの状態は [`describe_sources`] が別途 loud 出力するため、
/// 先頭理由だけを返しても診断情報は失われない。
pub(crate) fn evaluate(inputs: GateInputs<'_>) -> Decision {
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
    if !inputs.operation.backpressure_connected() {
        return Decision::Denied(DenyReason::BackpressureUnavailable(inputs.operation));
    }
    Decision::Allowed
}

/// 全ソースの状態を 1 行へ要約する (loud 出力用)。
///
/// deny 理由を 1 つに絞る [`evaluate`] と違い、こちらは 3 ソースすべてを出す。
/// 「フラグを 1 つ直したのにまだ止まる」という切り分けを 1 run の log だけで完結させる。
pub(crate) fn describe_sources(inputs: GateInputs<'_>, env_name: &str) -> String {
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
    let backpressure = if inputs.operation.backpressure_connected() {
        "connected"
    } else {
        "unavailable"
    };
    format!(
        "{env_name}={external} repo_config={repo_config} backpressure({})={backpressure}",
        inputs.operation.as_str()
    )
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

    fn inputs<'a>(
        repo_config_enabled: Option<bool>,
        external_raw: Option<&'a str>,
        operation: Operation,
    ) -> GateInputs<'a> {
        GateInputs {
            repo_config_enabled,
            external_raw,
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
        for op in [Operation::FixPush, Operation::DraftPr] {
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

    /// 背圧未接続の操作クラスは、kill-switch が両面とも有効でも通さない。
    #[test]
    fn denies_draft_pr_until_backpressure_lands() {
        assert_eq!(
            evaluate(inputs(Some(true), Some("true"), Operation::DraftPr)),
            Decision::Denied(DenyReason::BackpressureUnavailable(Operation::DraftPr))
        );
    }

    /// 全組み合わせを走査し、「許可されるのは 3 条件が揃った場合だけ」を網羅的に固定する。
    #[test]
    fn allow_is_exhaustively_limited_to_the_single_all_connected_combination() {
        let externals: Vec<Option<&str>> = std::iter::once(None)
            .chain(TRUTHY.iter().chain(NOT_TRUTHY.iter()).map(|s| Some(*s)))
            .collect();
        let mut allowed_count = 0;
        for op in [Operation::FixPush, Operation::DraftPr] {
            for repo in [None, Some(true), Some(false)] {
                for external in &externals {
                    let allowed = evaluate(inputs(repo, *external, op)) == Decision::Allowed;
                    let expected = repo == Some(true)
                        && external.is_some_and(lib_telemetry::is_truthy)
                        && op == Operation::FixPush;
                    assert_eq!(allowed, expected, "op={op:?} repo={repo:?} ext={external:?}");
                    allowed_count += usize::from(allowed);
                }
            }
        }
        assert_eq!(allowed_count, TRUTHY.len(), "許可される組み合わせ数が想定外");
    }

    #[test]
    fn describe_sources_reports_every_source_state() {
        let line = describe_sources(inputs(None, Some("0"), Operation::DraftPr), "AUTONOMY_ENABLED");
        assert!(line.contains("AUTONOMY_ENABLED=not-truthy"), "{line}");
        assert!(line.contains("repo_config=unavailable"), "{line}");
        assert!(line.contains("backpressure(draft-pr)=unavailable"), "{line}");
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
        assert_eq!(Operation::parse("draft-pr"), Some(Operation::DraftPr));
        assert_eq!(Operation::parse("merge"), None);
        assert_eq!(Operation::parse("FIX-PUSH"), None);
        assert_eq!(Operation::parse(""), None);
    }

    #[test]
    fn deny_reason_codes_are_stable_and_free_of_raw_values() {
        let reason = DenyReason::ExternalNotTruthy("secret-ish".to_string());
        assert_eq!(reason.code(), "external-not-truthy");
        assert!(!reason.code().contains("secret"));
    }
}
