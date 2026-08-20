use super::*;
use tempfile::TempDir;

#[test]
fn acquire_in_clean_dir_succeeds() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    match acquire_at(path.clone(), "test", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Acquired(_lock) => {
            assert!(path.exists(), "lock file should be created");
        }
        LockResult::Busy { .. } => panic!("expected Acquired in clean dir"),
        LockResult::Unavailable { reason } => {
            panic!(
                "expected Acquired in clean dir, got Unavailable: {}",
                reason
            )
        }
    }
}

#[test]
fn drop_removes_lock_file() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    {
        let _lock = match acquire_at(path.clone(), "test", DEFAULT_STALE_THRESHOLD_SECS) {
            LockResult::Acquired(l) => l,
            LockResult::Busy { .. } => panic!("expected Acquired"),
            LockResult::Unavailable { reason } => {
                panic!("expected Acquired, got Unavailable: {}", reason)
            }
        };
        assert!(path.exists());
    }
    assert!(!path.exists(), "Drop should remove the lock file");
}

/// 順位 292 (incident 再現): **stale takeover 後に旧 guard の Drop が
/// 新しい holder の lock を消してはならない。**
///
/// 消えると B は自分が lock を持っているつもりのまま走り続け、その間に C が
/// acquire できてしまう (= 同時監視 = レートリミット浪費)。無条件 `remove_file`
/// だとこれが起きる。`lib_jj_helpers::pipeline_lock` が PR #271 で塞いだのと同型。
#[test]
fn stale_takeover_then_old_guard_drop_keeps_new_lock() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");

    // A が取得。stale threshold 0 にして、次の acquire が必ず takeover 側へ回るようにする。
    let old = match acquire_at(path.clone(), "old", 0) {
        LockResult::Acquired(l) => l,
        other => panic!("expected Acquired for old: {}", describe(&other)),
    };
    // B が takeover (別 token で同じパスへ上書き)。
    let new_lock = match acquire_at(path.clone(), "new", 0) {
        LockResult::Acquired(l) => l,
        other => panic!("expected takeover for new: {}", describe(&other)),
    };
    let new_token = new_lock.token.clone();
    assert_ne!(old.token, new_token, "takeover は別 token を書くこと");

    // A の guard が落ちる。ここで B の lock を消してはならない。
    drop(old);

    assert!(
        path.exists(),
        "旧 guard の Drop が takeover 後の lock を削除した (順位 292 の退行)"
    );
    let content = std::fs::read_to_string(&path).unwrap();
    let parsed: LockFile = toml::from_str(&content).unwrap();
    assert_eq!(
        parsed.token, new_token,
        "残っている lock は takeover した側のものであること"
    );

    // B 自身の Drop は従来どおり消す (所有権確認が「誰も消せない」に振れていないこと)。
    drop(new_lock);
    assert!(!path.exists(), "所有者自身の Drop は lock を削除すること");
}

/// token を持たない **旧 format の lock** を fresh 扱いのまま読めること (順位 292)。
///
/// `token` を必須フィールドにすると旧 lock の parse が失敗し、`holder_still_writing`
/// の「内容あり = 破損 = stale」経路へ落ちて **fresh な旧 lock を踏み越えて takeover**
/// してしまう。`serde(default)` がその退行を防いでいることを固定する。
#[test]
fn legacy_lock_without_token_is_still_honored_while_fresh() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    let legacy = format!(
        "pid = 4242\nstart_time = \"{}\"\nmode = \"legacy\"\n",
        crate::util::utc_now_iso8601()
    );
    std::fs::write(&path, legacy).unwrap();

    match acquire_at(path.clone(), "new", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Busy { holder_pid, .. } => {
            assert_eq!(holder_pid, 4242, "旧 format でも holder として認識すること");
        }
        other => panic!("fresh な旧 format lock は Busy であるべき: {}", describe(&other)),
    }
}

/// 上のテストの対比: 旧 format でも stale なら従来どおり takeover できること。
#[test]
fn legacy_lock_without_token_is_taken_over_when_stale() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    std::fs::write(
        &path,
        "pid = 4242\nstart_time = \"1980-01-01T00:00:00Z\"\nmode = \"legacy\"\n",
    )
    .unwrap();

    match acquire_at(path.clone(), "new", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Acquired(_l) => {}
        other => panic!("stale な旧 format lock は takeover できるべき: {}", describe(&other)),
    }
}

/// panic メッセージ用の LockResult 記述子 (テストヘルパ)。
fn describe(result: &LockResult) -> String {
    match result {
        LockResult::Acquired(_) => "Acquired".to_string(),
        LockResult::Busy {
            holder_pid,
            holder_age_secs,
        } => format!("Busy(pid={holder_pid}, age={holder_age_secs}s)"),
        LockResult::Unavailable { reason } => format!("Unavailable({reason})"),
    }
}

#[test]
fn fresh_lock_blocks_second_acquire() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    let _first = match acquire_at(path.clone(), "first", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Acquired(l) => l,
        LockResult::Busy { .. } => panic!("expected Acquired for first"),
        LockResult::Unavailable { reason } => {
            panic!("expected Acquired for first, got Unavailable: {}", reason)
        }
    };

    match acquire_at(path.clone(), "second", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Busy { holder_pid, .. } => {
            assert_eq!(holder_pid, std::process::id());
        }
        LockResult::Acquired(_) => panic!("second should be Busy while first holds"),
        LockResult::Unavailable { reason } => {
            panic!("second should be Busy, got Unavailable: {}", reason)
        }
    }
}

#[test]
fn stale_lock_is_taken_over() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    // 古い start_time を持つ lock を仕込む (1980-01-01 = epoch+10年、確実に stale)
    let stale = LockFile {
        token: "stale-token".into(),
        pid: 999_999,
        start_time: "1980-01-01T00:00:00Z".into(),
        mode: "stale-test".into(),
    };
    std::fs::write(&path, toml::to_string(&stale).unwrap()).unwrap();

    // threshold=1800s でも 1980 は stale 判定 → takeover 成功
    match acquire_at(path.clone(), "takeover", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Acquired(_lock) => {
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(content.contains(&format!("pid = {}", std::process::id())));
        }
        LockResult::Busy { .. } => panic!("stale lock should allow takeover"),
        LockResult::Unavailable { reason } => {
            panic!(
                "stale lock should allow takeover, got Unavailable: {}",
                reason
            )
        }
    }
}

#[test]
fn concurrent_acquire_only_one_wins() {
    // 真の concurrency test: 8 thread が同一 path に同時 acquire を試み、
    // 1 つだけが Acquired (lock 保持) で残りは Busy になることを確認。
    // create_new による atomic create が機能していない場合、複数が
    // Acquired になり test 失敗する。
    //
    // 2 barrier 構成の意図: `start` で全 thread 同時に acquire_at に突入させ、
    // `finish` で全 thread が判定を終えるまで Acquired guard を保持する。
    // 1 barrier だと先行 thread の guard が判定後に即 drop され、後続 thread が
    // 逐次 Acquired する flaky window が生じる (CR finding E)。
    use std::sync::{Arc, Barrier};
    use std::thread;

    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    let start = Arc::new(Barrier::new(8));
    let finish = Arc::new(Barrier::new(8));
    let mut handles = vec![];
    for _ in 0..8 {
        let p = path.clone();
        let start_b = start.clone();
        let finish_b = finish.clone();
        handles.push(thread::spawn(move || {
            start_b.wait();
            let result = acquire_at(p, "concurrent", DEFAULT_STALE_THRESHOLD_SECS);
            let acquired = matches!(result, LockResult::Acquired(_));
            // 全 thread の判定が終わるまで result (Acquired なら guard) を保持
            finish_b.wait();
            acquired
        }));
    }
    let acquired_count = handles
        .into_iter()
        .map(|h| h.join().unwrap())
        .filter(|&v| v)
        .count();
    // race-safe な実装なら 1 thread のみが Acquired。
    // (Drop は thread 終了で走るため、その後の状態は不定 — 取得回数だけを検証)
    assert_eq!(
        acquired_count, 1,
        "exactly one thread should acquire the lock under concurrency"
    );
}

/// クロックスキュー対策 (CodeRabbit PR #307): mtime が**未来**でも齢 0 とみなすこと。
///
/// 旧実装は `duration_since` の `Err` を `None` に潰しており、呼び出し側が
/// 「齢不明 = stale」に倒れて空 lock を takeover していた。つまり WP-15 で
/// 塞いだ同時取得レースが、クロックスキューという別経路から再発しうる。
#[test]
fn future_mtime_is_treated_as_just_created() {
    let now = std::time::SystemTime::now();
    let future = now + std::time::Duration::from_secs(3600);
    assert_eq!(
        age_secs_between(future, now),
        0,
        "未来 mtime は「たった今作られた」と解釈すること (stale 誤判定を防ぐ)",
    );
}

/// 通常経路 (good): 過去の mtime は経過秒をそのまま返すこと。
#[test]
fn past_mtime_yields_elapsed_seconds() {
    let now = std::time::SystemTime::now();
    let past = now - std::time::Duration::from_secs(42);
    assert_eq!(age_secs_between(past, now), 42);
}

/// WP-15 incident 再現 (bad): `create_new` 直後の**空** lock を stale と誤判定
/// しないこと。
///
/// 由来: 2026-07-20 の Linux 実測 (WSL Ubuntu 24.04)。`create_new` は atomic
/// だが直後のファイルは空で、内容書き込みまでの窓に別スレッドが読むと TOML
/// parse に失敗する。これを「壊れている = stale」と扱っていたため全員が
/// takeover し、8 スレッド中 6 つが同時に Acquired になった。Windows では
/// スケジューリングの差で顕在化していなかっただけで、設計上の欠陥は同じ。
///
/// 修正の核心は「parse 不能でも十分新しければ書き込み中 = busy とみなす」。
#[test]
fn empty_lock_file_is_treated_as_busy_not_stale() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    std::fs::write(&path, "").unwrap();

    let result = acquire_at(path, "probe", DEFAULT_STALE_THRESHOLD_SECS);

    assert!(
        matches!(result, LockResult::Busy { .. }),
        "空 lock は「保持者が書き込み中」= Busy とすること。Acquired だと\
         create_new の排他が無意味になり同時取得が起きる (WP-15 の不具合)",
    );
}

#[test]
fn lock_format_matches_util_iso8601() {
    // util::utc_now_iso8601() の出力 format と本 module の parse_iso8601 が
    // round-trip することを確認 (advisor 指摘の "format alignment" check)。
    let now = crate::util::utc_now_iso8601();
    let parsed = parse_iso8601(&now);
    assert!(
        parsed.is_some(),
        "util's iso8601 format must parse: {}",
        now
    );
}

#[test]
fn future_timestamp_lock_is_taken_over() {
    // 時計巻き戻し / 破損 future timestamp の lock が永続 fresh で塩漬けに
    // ならず、stale 扱いで takeover されることを確認 (CR finding D)。
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    // 9999 年は確実に未来 (parse_iso8601 上限内)
    let future = LockFile {
        token: "future-token".into(),
        pid: 999_999,
        start_time: "9999-01-01T00:00:00Z".into(),
        mode: "future-test".into(),
    };
    std::fs::write(&path, toml::to_string(&future).unwrap()).unwrap();

    match acquire_at(path.clone(), "takeover", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Acquired(_lock) => {
            let content = std::fs::read_to_string(&path).unwrap();
            assert!(
                content.contains(&format!("pid = {}", std::process::id())),
                "lock should be overwritten with current PID"
            );
        }
        LockResult::Busy {
            holder_age_secs, ..
        } => panic!(
            "future timestamp should be treated as stale, got Busy with age={}s",
            holder_age_secs
        ),
        LockResult::Unavailable { reason } => {
            panic!(
                "future-stale takeover should succeed, got Unavailable: {}",
                reason
            )
        }
    }
}

#[test]
fn corrupt_lock_is_taken_over() {
    let tmp = TempDir::new().unwrap();
    let path = tmp.path().join("pr-monitor.lock");
    // parse 不能な内容を仕込む
    std::fs::write(&path, "this is not valid toml :::").unwrap();

    match acquire_at(path.clone(), "takeover", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Acquired(_lock) => {}
        LockResult::Busy { .. } => panic!("corrupt lock should be treated as stale"),
        LockResult::Unavailable { reason } => {
            panic!(
                "corrupt lock should allow takeover, got Unavailable: {}",
                reason
            )
        }
    }
}

#[test]
fn parse_iso8601_round_trip() {
    // 2026-04-30T00:00:00Z = 56 yr from epoch with leap year handling
    // 単純にパースが動くことを確認
    let ts = parse_iso8601("2026-04-30T00:00:00Z").unwrap();
    // 2026-04-30 should be > 2025-01-01 (1735689600 sec) and < 2027-01-01
    assert!(ts > 1_735_689_600);
    assert!(ts < 1_798_761_600);
}

#[test]
fn is_leap_correctness() {
    assert!(is_leap(2024));
    assert!(!is_leap(2025));
    assert!(!is_leap(1900)); // century non-leap
    assert!(is_leap(2000)); // 400-year leap
}

#[test]
fn parse_iso8601_rejects_out_of_range_month() {
    // month=99 would cause index-out-of-bounds in days_from_epoch without bounds check
    assert_eq!(parse_iso8601("2026-99-30T00:00:00Z"), None);
}

#[test]
fn parse_iso8601_rejects_out_of_range_fields() {
    assert_eq!(parse_iso8601("1969-01-01T00:00:00Z"), None); // year < 1970
    assert_eq!(parse_iso8601("2026-00-01T00:00:00Z"), None); // month = 0
    assert_eq!(parse_iso8601("2026-13-01T00:00:00Z"), None); // month > 12
    assert_eq!(parse_iso8601("2026-01-00T00:00:00Z"), None); // day = 0
    assert_eq!(parse_iso8601("2026-01-32T00:00:00Z"), None); // day > 31
    assert_eq!(parse_iso8601("2026-01-01T24:00:00Z"), None); // hour = 24
    assert_eq!(parse_iso8601("2026-01-01T00:60:00Z"), None); // minute = 60
    assert_eq!(parse_iso8601("2026-01-01T00:00:60Z"), None); // second = 60
    assert_eq!(parse_iso8601("2026-02-29T00:00:00Z"), None); // day 29 in non-leap year
}

/// 親に「ディレクトリではなく通常ファイル」を仕込むと、`create_dir_all` は
/// silent fail、後続の `open()` が AlreadyExists 以外の I/O error を返し
/// `Unavailable` 経路に入る、というシナリオを構築する。
#[test]
fn io_error_returns_unavailable() {
    let tmp = tempfile::TempDir::new().unwrap();
    let file_as_dir = tmp.path().join("notadir");
    std::fs::write(&file_as_dir, "content").unwrap();
    let path = file_as_dir.join("pr-monitor.lock");
    match acquire_at(path, "test", DEFAULT_STALE_THRESHOLD_SECS) {
        LockResult::Unavailable { .. } => {}
        LockResult::Acquired(_) => panic!("expected Unavailable on I/O error, got Acquired"),
        LockResult::Busy { .. } => panic!("expected Unavailable on I/O error, got Busy"),
    }
}

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
fn past_time_from_iso8601_now_rejects_far_future_year_9999() {
    assert_eq!(PastTime::from_iso8601_now("9999-01-01T00:00:00Z"), None);
}

#[test]
fn past_time_from_iso8601_now_accepts_unix_epoch_origin() {
    let pt = PastTime::from_iso8601_now("1970-01-01T00:00:00Z").expect("epoch is past");
    assert!(pt.age_secs() >= 0);
}
/// CodeRabbit #430 Major の再現確認: **stale lock への takeover が排他か**。
///
/// 現行実装は「stale 判定 → `std::fs::write` で上書き」で、複数スレッドが同じ
/// stale lock を stale と読んだ場合に全員が `Acquired` を返しうる。lock の目的は
/// 「1 リポジトリ 1 アクティブ監視」なので、同時 Acquired はレートリミット浪費に直結する。
///
/// **全ガードを Vec に保持してから数える**のが要点。遅延イテレータで数えると、先行結果が
/// 判定前に drop されて後続が正当に取得し「2 Acquired」に見える (解放後の再取得であって
/// lock のバグではない)。同時保持され得るかだけを見る。
#[test]
fn concurrent_stale_takeover_only_one_wins() {
    use std::sync::{Arc, Barrier};
    use std::thread;

    for round in 0..40 {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("pr-monitor.lock");
        let stale = LockFile {
            token: "stale-token".into(),
            pid: 999_999,
            start_time: "1980-01-01T00:00:00Z".into(),
            mode: "crashed".into(),
        };
        std::fs::write(&path, toml::to_string(&stale).unwrap()).unwrap();

        let start = Arc::new(Barrier::new(8));
        let finish = Arc::new(Barrier::new(8));
        let mut handles = vec![];
        for _ in 0..8 {
            let p = path.clone();
            let start_b = start.clone();
            let finish_b = finish.clone();
            handles.push(thread::spawn(move || {
                start_b.wait();
                let result = acquire_at(p, "takeover", DEFAULT_STALE_THRESHOLD_SECS);
                let acquired = matches!(result, LockResult::Acquired(_));
                finish_b.wait();
                acquired
            }));
        }
        let acquired = handles
            .into_iter()
            .filter(|_| true)
            .map(|h| h.join().unwrap())
            .filter(|a| *a)
            .count();
        assert_eq!(
            acquired, 1,
            "round {round}: stale takeover で同時 Acquired は 1 つのみのはず (得た数: {acquired})"
        );
    }
}

