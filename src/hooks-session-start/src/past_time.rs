//! 過去性が型レベルで保証された timestamp。
//!
//! `saturating_sub` 系の silent semantic mismatch を排除するための型 (Bundle W = PR #199
//! 順位 197 移植)。`from_parts` の construction 時に `then <= now` を検証することで、
//! `age_secs()` が常に非負である invariant を構造的に保証する。
//!
//! 過去の bug class:
//!   - orphan reaper の age 計算で `now.saturating_sub(start)` を使うと、clock rewind や
//!     破損 future-dated `startTime` で `age=0` が返り、reaper が「young」判定で skip。
//!     結果 `.failed` marker 未生成 → ADR-030 §L2 out-of-process recovery の silent 沈黙。
//!
//! PastTime は construction 時に future timestamp を `None` で reject するため、
//! 同型の silent skip bug を型層で再発不能化する。caller は `None` で明示 skip し、
//! `saturating_sub` のような silent age=0 fallback には到達しない。

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PastTime {
    epoch_secs: i64,
    captured_now: i64,
}

impl PastTime {
    /// テスト注入 / proptest 用: `now` を引数で受ける variant。
    /// `then > now` (future) の場合 `None`。それ以外は invariant を満たす PastTime を返す。
    pub(crate) fn from_parts(then_epoch_secs: i64, now_epoch_secs: i64) -> Option<Self> {
        if then_epoch_secs > now_epoch_secs {
            return None;
        }
        Some(Self {
            epoch_secs: then_epoch_secs,
            captured_now: now_epoch_secs,
        })
    }

    /// 経過秒数 (construction 時点の `captured_now - epoch_secs`)。invariant により常に非負。
    pub(crate) fn age_secs(&self) -> i64 {
        debug_assert!(self.captured_now >= self.epoch_secs);
        self.captured_now - self.epoch_secs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn past_time_from_parts_accepts_past() {
        let pt = PastTime::from_parts(100, 200).expect("then < now should succeed");
        assert_eq!(pt.age_secs(), 100);
    }

    #[test]
    fn past_time_from_parts_accepts_equal() {
        let pt = PastTime::from_parts(100, 100).expect("then == now should succeed");
        assert_eq!(pt.age_secs(), 0);
    }

    #[test]
    fn past_time_from_parts_rejects_future() {
        assert_eq!(
            PastTime::from_parts(200, 100),
            None,
            "then > now must be rejected (silent fresh bug 防止)"
        );
    }

    #[test]
    fn past_time_from_parts_rejects_future_by_one_second() {
        assert_eq!(
            PastTime::from_parts(101, 100),
            None,
            "off-by-one future must be rejected"
        );
    }
}

#[cfg(test)]
mod proptests {
    //! 順位 197 (PR #199 Bundle W 移植): proptest properties for `PastTime::from_parts` と
    //! `parse_iso8601_to_unix`。本 module は spec 層で AI が flaky 実装を書ける窓を塞ぐ
    //! regression net。
    //!
    //! 主要 property:
    //!   - P1: from_parts(then, now) で then <= now → age_secs == now - then
    //!   - P2: from_parts(then, now) で then > now → None (silent fresh 防止)
    //!   - P3: parse_iso8601_to_unix は任意 string で panic しない
    //!   - P4: parse_iso8601_to_unix は pre-epoch year を必ず reject
    //!   - P5: parse_iso8601_to_unix は有効範囲内の date を必ず accept
    //!
    //! proptest case 数は default 256。実行時間は数百 ms 程度。

    use super::*;
    use crate::parse_iso8601_to_unix;
    use proptest::prelude::*;

    proptest! {
        /// P1: from_parts(then, now) で then <= now のとき age_secs == now - then が成立。
        /// `saturating_sub` 系の silent semantic mismatch が混入したらこのプロパティが落ちる
        /// regression net (PR #96 Finding D / Bundle W 起源)。
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
        /// future timestamp が fresh 値を生むことは構造的に不可能 (ADR-030 §L2 silent
        /// recovery 沈黙 を encode)。
        #[test]
        fn past_time_rejects_future(
            now in -1_000_000_000_i64..=1_000_000_000_i64,
            future_offset in 1_i64..=1_000_000_i64,
        ) {
            let then = now + future_offset;
            prop_assert_eq!(PastTime::from_parts(then, now), None);
        }

        /// P3: parse_iso8601_to_unix は任意 string で panic しない (corrupt input は None)。
        /// days_from_epoch 系の index out-of-bounds panic を regression net で再検出可能化。
        #[test]
        fn parse_iso8601_to_unix_never_panics(s in ".*") {
            let _ = parse_iso8601_to_unix(&s);
        }

        /// P4: pre-epoch year (< 1970) は必ず reject。
        #[test]
        fn parse_iso8601_to_unix_rejects_pre_epoch_year(
            year in 0_u32..1970,
            month in 1_u32..=12,
            day in 1_u32..=28,
        ) {
            let s = format!("{:04}-{:02}-{:02}T00:00:00Z", year, month, day);
            prop_assert_eq!(parse_iso8601_to_unix(&s), None);
        }

        /// P5: 有効範囲内の正規 ISO 8601 は必ず accept (round-trip 基本性質)。
        /// day を 1..=28 に絞ることで全月で有効な日付に限定 (うるう年判定を回避)。
        #[test]
        fn parse_iso8601_to_unix_accepts_well_formed(
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
            prop_assert!(parse_iso8601_to_unix(&s).is_some(), "should accept: {}", s);
        }
    }
}
