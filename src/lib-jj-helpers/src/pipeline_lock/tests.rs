use super::*;

fn temp_lock_path(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "pipeline-lock-{}-{}-{}",
        prefix,
        std::process::id(),
        nanos
    ))
}

#[test]
fn acquire_creates_lock_and_drop_removes_it() {
    let path = temp_lock_path("acquire");
    let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000);
    assert!(matches!(result, PipelineLockResult::Acquired(_)));
    assert!(path.exists());
    drop(result);
    assert!(!path.exists(), "RAII drop で lock が削除される");
}

#[test]
fn second_acquire_is_busy_while_fresh() {
    let path = temp_lock_path("busy");
    let _guard = acquire_pipeline_lock_at(path.clone(), "merge", 1800, 1_000_000);
    let second = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_100);
    match second {
        PipelineLockResult::Busy {
            holder_pid,
            holder_age_secs,
        } => {
            assert_eq!(holder_pid, std::process::id());
            assert_eq!(holder_age_secs, 100);
        }
        _ => panic!("fresh lock 保持中は Busy になるべき"),
    }
}

#[test]
fn stale_lock_is_taken_over() {
    let path = temp_lock_path("stale");
    std::fs::write(&path, "pid=99999\nstart_unix=1000000\nlabel=crashed\n").unwrap();
    let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000 + 1800);
    assert!(
        matches!(result, PipelineLockResult::Acquired(_)),
        "threshold 到達で takeover"
    );
}

#[test]
fn future_dated_lock_is_treated_as_stale() {
    let path = temp_lock_path("future");
    std::fs::write(&path, "pid=99999\nstart_unix=2000000\nlabel=corrupt\n").unwrap();
    let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000);
    assert!(
        matches!(result, PipelineLockResult::Acquired(_)),
        "future timestamp は stale 扱い (永続 fresh 化 bug class の防止)"
    );
}

#[test]
fn corrupt_lock_is_taken_over() {
    let path = temp_lock_path("corrupt");
    std::fs::write(&path, "not a lock file").unwrap();
    let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000);
    assert!(matches!(result, PipelineLockResult::Acquired(_)));
}

#[test]
fn read_fresh_lock_parses_fields_and_age() {
    let path = temp_lock_path("read");
    std::fs::write(&path, "pid=4321\nstart_unix=1000000\nlabel=merge\n").unwrap();
    let held = read_fresh_lock(&path, 1800, 1_000_500);
    assert_eq!(held, Some((4321, 500)));
    let _ = std::fs::remove_file(&path);
}

#[test]
fn missing_lock_reads_as_not_held() {
    let path = temp_lock_path("missing");
    assert_eq!(read_fresh_lock(&path, 1800, 1_000_000), None);
}

#[test]
fn acquire_writes_a_token() {
    let path = temp_lock_path("token");
    let _guard = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000);
    let content = std::fs::read_to_string(&path).unwrap();
    let token = parse_field(&content, "token").expect("token が書かれる");
    assert_eq!(token.len(), 32, "128bit hex");
    assert!(token.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn generate_token_is_unique_per_call() {
    assert_ne!(generate_token(), generate_token(), "取得ごとに異なる token");
}

/// CodeRabbit Major #271 の regression guard: stale takeover 後に旧プロセスの Drop が
/// **新プロセスの lock を消さない**。A の guard を保持したまま同じパスを B が takeover
/// (別 token を上書き) し、A を drop しても B の lock ファイルが残ることを確認する。
#[test]
fn drop_does_not_remove_lock_after_takeover() {
    let path = temp_lock_path("takeover-guard");
    let a_guard = acquire_pipeline_lock_at(path.clone(), "A", 1800, 1_000_000);
    assert!(matches!(a_guard, PipelineLockResult::Acquired(_)));

    let b_takeover_content =
        build_lock_content("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb", 55555, 1_000_100, "B");
    std::fs::write(&path, &b_takeover_content).unwrap();

    drop(a_guard);

    assert!(path.exists(), "A の Drop が B の lock を消してはならない");
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(
        after.contains("token=bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        "B の lock がそのまま残る"
    );
    let _ = std::fs::remove_file(&path);
}

/// 通常ケース: 自分の token が残っていれば Drop で削除される。
#[test]
fn drop_removes_lock_when_token_matches() {
    let path = temp_lock_path("self-remove");
    let guard = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000);
    assert!(path.exists());
    drop(guard);
    assert!(!path.exists(), "自分の token の lock は削除される");
}

/// CodeRabbit re-review Major の regression guard: 同じ stale lock に対する
/// takeover を 2 スレッドが同時に行っても、`Acquired` になるのは 1 つだけ。
#[test]
fn concurrent_stale_takeover_only_one_wins() {
    let path = temp_lock_path("concurrent-stale");
    std::fs::write(&path, "pid=99999\nstart_unix=1000000\nlabel=crashed\n").unwrap();

    let path_a = path.clone();
    let path_b = path.clone();
    let a = std::thread::spawn(move || {
        acquire_pipeline_lock_at(path_a, "A", 1800, 1_000_000 + 1800)
    });
    let b = std::thread::spawn(move || {
        acquire_pipeline_lock_at(path_b, "B", 1800, 1_000_000 + 1800)
    });
    let result_a = a.join().unwrap();
    let result_b = b.join().unwrap();

    let acquired_count = [&result_a, &result_b]
        .into_iter()
        .filter(|r| matches!(r, PipelineLockResult::Acquired(_)))
        .count();
    assert_eq!(
        acquired_count, 1,
        "stale takeover のレースで Acquired になるのは 1 プロセスのみのはず"
    );

    let _ = std::fs::remove_file(&path);
}

/// 高競合ストレス: N スレッドが同一 stale lock を同時 takeover しても、**同時に**
/// `Acquired` になるのはちょうど 1 つ。2 スレッド版 (`concurrent_stale_takeover_only_one_wins`)
/// は旧実装で ~10% しか再現しなかったが、スレッド数を増やすとほぼ確実に踏む。
///
/// **全ガードを Vec に保持してから数える**のが要点。遅延イテレータ
/// (`map(join).filter().count()`) で数えると、まだ acquire 中の他スレッドの傍らで
/// 先行結果が drop され、その `PipelineLock::drop` が lock を削除 → 走行中スレッドが
/// 正当に取得し「2 Acquired」に見える (= 同時保持ではなく解放後の再取得。テスト
/// アーティファクトであって lock のバグではない)。collect で全ガードを保持し、
/// 「同時点で 2 つ保持され得るか」だけを検証する。
#[test]
fn concurrent_stale_takeover_many_threads_single_winner() {
    for round in 0..40 {
        let path = temp_lock_path(&format!("stress-{round}"));
        std::fs::write(&path, "pid=99999\nstart_unix=1000000\nlabel=crashed\n").unwrap();

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let p = path.clone();
                std::thread::spawn(move || {
                    acquire_pipeline_lock_at(p, &format!("T{i}"), 1800, 1_000_000 + 1800)
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let acquired = results
            .iter()
            .filter(|r| matches!(r, PipelineLockResult::Acquired(_)))
            .count();

        assert_eq!(
            acquired, 1,
            "round {round}: stale takeover の高競合で同時 Acquired は 1 つのみのはず (得た数: {acquired})"
        );
        drop(results);
        let _ = std::fs::remove_file(&path);
    }
}

/// SIM-NEW-pipeline_lock-L146 の regression guard: takeover 中に対象が既に fresh lock に
/// 変わっていた場合 (別 takeover が直前に完了)、奪わずに `Busy` へ倒し、その fresh lock を
/// 破壊しない (2 プロセスとも `Acquired` の再発防止)。
#[test]
fn takeover_preserves_fresh_lock_that_appeared_during_takeover() {
    let path = temp_lock_path("snapshot-mismatch");

    // NOTE: takeover 中に対象が別 takeover 由来の fresh lock へ変わった状況を模す (age=100 < 1800=fresh)。
    let fresh_lock_from_concurrent_takeover =
        build_lock_content("cccccccccccccccccccccccccccccccc", 12345, 1_000_100, "other");
    std::fs::write(&path, &fresh_lock_from_concurrent_takeover).unwrap();

    let result = takeover_stale_lock(
        path.clone(),
        "dddddddddddddddddddddddddddddddd".to_string(),
        build_lock_content(
            "dddddddddddddddddddddddddddddddd",
            std::process::id(),
            1_000_200,
            "push",
        ),
        1800,
        1_000_200,
    );

    assert!(
        matches!(result, PipelineLockResult::Busy { .. }),
        "対象が fresh 化していたら奪わず Busy に倒すべき"
    );
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after, fresh_lock_from_concurrent_takeover,
        "他プロセスの fresh lock は破壊されず残る"
    );

    assert!(
        !takeover_sentinel_path(&path).exists(),
        "takeover sentinel は完了後に残らない"
    );

    let _ = std::fs::remove_file(&path);
}

/// SIM-NEW-pipeline_lock-L157 の regression guard: sentinel 保持者が
/// `perform_takeover` 実行中にクラッシュして `<lock>.takeover` が孤立しても、
/// `SENTINEL_STALE_SECS` 経過後は自己修復し、本物の stale な pipeline lock を
/// 取得できる。従来は sentinel に age 判定が皆無で、この状態になると以降の
/// 取得試行が永久に `Busy` へ倒れていた。
#[test]
fn orphaned_stale_sentinel_self_heals_and_lock_is_acquired() {
    let path = temp_lock_path("orphaned-sentinel");
    // NOTE: 本物の pipeline lock も stale (クラッシュした pipeline の残骸)。
    std::fs::write(&path, "pid=99999\nstart_unix=1000000\nlabel=crashed\n").unwrap();
    // NOTE: sentinel 保持者が takeover 途中でクラッシュし孤立した状態を模す。
    let sentinel = takeover_sentinel_path(&path);
    std::fs::write(&sentinel, "pid=88888\nstart_unix=1000000\n").unwrap();

    let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, 1_000_000 + 1800);

    assert!(
        matches!(result, PipelineLockResult::Acquired(_)),
        "孤立した stale sentinel は自己修復され、本物の stale lock を取得できるべき"
    );
    assert!(!sentinel.exists(), "取得完了後は sentinel が残らない");

    drop(result);
    let _ = std::fs::remove_file(&sentinel);
}

/// sentinel が fresh (直近作成 = 別スレッドが takeover 実行中) な間は、本物の lock が
/// stale であっても sentinel を奪わず `Busy` に倒し、fresh sentinel を破壊しない。
#[test]
fn fresh_sentinel_blocks_takeover_without_being_stolen() {
    let path = temp_lock_path("fresh-sentinel");
    let now = 1_000_000 + 1800;
    std::fs::write(&path, "pid=99999\nstart_unix=1000000\nlabel=crashed\n").unwrap();
    let sentinel = takeover_sentinel_path(&path);
    // NOTE: age = 5s < SENTINEL_STALE_SECS(30s) = fresh (取得直後を模す)。
    let fresh_sentinel_content = format!("pid={}\nstart_unix={}\n", std::process::id(), now - 5);
    std::fs::write(&sentinel, &fresh_sentinel_content).unwrap();

    let result = acquire_pipeline_lock_at(path.clone(), "push", 1800, now);

    assert!(
        matches!(result, PipelineLockResult::Busy { .. }),
        "fresh sentinel が既にある間は奪わず Busy に倒すべき"
    );
    let after = std::fs::read_to_string(&sentinel).unwrap();
    assert_eq!(
        after, fresh_sentinel_content,
        "fresh sentinel は破壊されず残る"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&sentinel);
}

/// SIM-NEW-pipeline_lock-L157 の regression guard (高競合版): 孤立した stale
/// sentinel が存在する状態で N スレッドが同時に取得を試みても、自己修復後も
/// **同時に** `Acquired` になるのはちょうど 1 つ (自己修復が単一 winner 性を
/// 壊していないことの確認)。
#[test]
fn concurrent_takeover_with_orphaned_sentinel_single_winner() {
    for round in 0..20 {
        let path = temp_lock_path(&format!("orphaned-sentinel-stress-{round}"));
        std::fs::write(&path, "pid=99999\nstart_unix=1000000\nlabel=crashed\n").unwrap();
        let sentinel = takeover_sentinel_path(&path);
        std::fs::write(&sentinel, "pid=88888\nstart_unix=1000000\n").unwrap();

        let handles: Vec<_> = (0..8)
            .map(|i| {
                let p = path.clone();
                std::thread::spawn(move || {
                    acquire_pipeline_lock_at(p, &format!("T{i}"), 1800, 1_000_000 + 1800)
                })
            })
            .collect();
        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let acquired = results
            .iter()
            .filter(|r| matches!(r, PipelineLockResult::Acquired(_)))
            .count();

        assert_eq!(
            acquired, 1,
            "round {round}: 孤立 sentinel の自己修復後も同時 Acquired は 1 つのみのはず (得た数: {acquired})"
        );
        drop(results);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&sentinel);
    }
}
