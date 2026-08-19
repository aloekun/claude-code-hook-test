//! Bundle W (順位 34): proptest properties for `parse_iso8601` / `PastTime::from_parts`.
//!
//! 本 module は spec 層で AI が flaky 実装を書ける窓を塞ぐ regression net。
//! 主要 property:
//!   - P1: from_parts(then, now) で then <= now → age_secs == now - then
//!   - P2: from_parts(then, now) で then > now → None (silent fresh 防止 / Finding D)
//!   - P3: parse_iso8601 は任意 string 入力で panic しない
//!   - P4: parse_iso8601 は pre-epoch year を必ず reject
//!   - P5: parse_iso8601 は有効範囲内の date を必ず accept
//!
//! proptest case 数は default 256。実行時間は数百 ms 程度 (pre-push pipeline
//! 完了基準 +1 秒以内に収まる)。

use super::*;
use proptest::prelude::*;

proptest! {
    /// P1: from_parts(then, now) で then <= now のとき age_secs == now - then が成立。
    /// `saturating_sub` 系の silent semantic mismatch (CR finding D) が混入したら
    /// このプロパティが落ちる regression net。
    #[test]
    fn past_time_age_is_correct_when_in_past(
        then in -1_000_000_000_000_i64..=1_000_000_000_000_i64,
        offset in 0_i64..=1_000_000_000_i64,
    ) {
        let now = then + offset;
        let pt = PastTime::from_parts(then, now).expect("then <= now");
        prop_assert_eq!(pt.age_secs(), offset);
    }

    /// P2: from_parts(then, now) で then > now のとき必ず None。
    /// Finding D を直接 encode: future timestamp が fresh 値を生むことは構造的に不可能。
    #[test]
    fn past_time_rejects_future(
        now in -1_000_000_000_i64..=1_000_000_000_i64,
        future_offset in 1_i64..=1_000_000_i64,
    ) {
        let then = now + future_offset;
        prop_assert_eq!(PastTime::from_parts(then, now), None);
    }

    /// P3: parse_iso8601 は任意 string で panic しない (corrupt input は None)。
    /// 過去に `days_from_epoch` の index out-of-bounds panic が発生した
    /// regression: range check が抜けると proptest がこれを再検出する。
    #[test]
    fn parse_iso8601_never_panics(s in ".*") {
        let _ = parse_iso8601(&s);
    }

    /// P4: pre-epoch year (< 1970) は必ず reject。
    #[test]
    fn parse_iso8601_rejects_pre_epoch_year(
        year in 0_u32..1970,
        month in 1_u32..=12,
        day in 1_u32..=28,
    ) {
        let s = format!("{:04}-{:02}-{:02}T00:00:00Z", year, month, day);
        prop_assert_eq!(parse_iso8601(&s), None);
    }

    /// P5: 有効範囲内の正規 ISO 8601 は必ず accept (round-trip 基本性質)。
    /// day を 1..=28 に絞ることで全月で有効な日付に限定 (うるう年判定を回避)。
    #[test]
    fn parse_iso8601_accepts_well_formed(
        year in 1970_u32..=9999,
        month in 1_u32..=12,
        day in 1_u32..=28,
        hour in 0_u32..=23,
        minute in 0_u32..=59,
        second in 0_u32..=59,
    ) {
        let s = format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            year, month, day, hour, minute, second
        );
        prop_assert!(parse_iso8601(&s).is_some(), "should accept: {}", s);
    }
}
