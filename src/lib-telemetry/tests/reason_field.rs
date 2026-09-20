//! `Firing::reason` の JSONL 契約。
//!
//! `src/lib.rs` は 800 行ゲートの上限際 (ADR-080) のため、本ファイルを統合テストとして
//! 分けている。`record_to` / `Firing` はいずれも `pub` なので外から組める。
//!
//! 固定したいのは 2 点:
//!
//! - `reason` を渡した行にだけフィールドが出る (集計側の後方互換。`cli-telemetry-report`
//!   の `FiringRecord` は未知フィールドを無視するため、既存行の形が変わらないことが前提)
//! - 渡さない行には**現れない** (全 hook の既存行が 1 バイトも変わらない)

use lib_telemetry::{record_to, Decision, Firing, FiringKind};

/// 2026-04-01T12:00:00Z。partition 名を決定論にするため固定する。
const T: u64 = 1_775_044_800;

fn written_line(dir: &std::path::Path) -> String {
    let entries = std::fs::read_dir(dir.join("telemetry")).expect("telemetry ディレクトリ");
    let path = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .find(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("firings-"))
        })
        .expect("firings ファイル");
    std::fs::read_to_string(path).expect("読み取り")
}

fn firing(reason: Option<&'static str>) -> Firing<'static> {
    Firing {
        hook: "cli-push-runner",
        kind: FiringKind::Hook,
        id: "testability_gate:scan-incomplete",
        decision: Decision::Warn,
        session_id: None,
        reason,
    }
}

#[test]
fn reason_is_serialized_when_present() {
    let dir = tempfile::tempdir().unwrap();
    record_to(dir.path(), &firing(Some("unparsable-status")), T).unwrap();
    let line = written_line(dir.path());
    assert!(
        line.contains(r#""reason":"unparsable-status""#),
        "reason が行に出ていません: {line}"
    );
}

#[test]
fn reason_is_absent_when_none() {
    let dir = tempfile::tempdir().unwrap();
    record_to(dir.path(), &firing(None), T).unwrap();
    let line = written_line(dir.path());
    assert!(
        !line.contains("reason"),
        "reason を渡していないのに行へ出ています (既存行の形が変わります): {line}"
    );
}
