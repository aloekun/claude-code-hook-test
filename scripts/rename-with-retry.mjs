/**
 * Windows の一時的なファイル lock に耐える `rename` (deploy-artifacts の `replaceFile` が使う)。
 *
 * Windows では、別のプロセスが削除の共有を許さずにファイルを開いている間、そのファイルの
 * 改名が `EBUSY` / `EPERM` で失敗する。開いているのが誰かはこちらから制御できない
 * (2026-11-06 の CI で、作ったばかりの exe の改名が 1 回だけ `EBUSY` で落ちた。発生源は
 * 未特定)。ネットワークや DB と同じく外部資源への操作なので、一時的なエラーに限って
 * 回数と時間に上限を付けて再試行する。
 *
 * 本モジュールは「改名が成功したか失敗したか」だけを扱う。失敗したときに旧いファイルを
 * どう戻すかは呼び出し側 (`replaceFile`) の状態管理が決める。
 */

import { renameSync } from "node:fs";

/**
 * 再試行の前に待つ時間 (ms)。初回を含めて最大 `RETRY_DELAYS_MS.length + 1` 回 (= 6 回) 試し、
 * 待ちは合計 2.0 秒。発生は 200 回に 1 回程度で、lock は短時間で外れると見込んだ初期値。
 * 100ms から倍々に伸ばし、最後だけ 500ms に抑えて合計を 2.0 秒に収める (倍々のままだと
 * 1600ms で合計 3.1 秒になり、恒久的な失敗で配布を待たせる時間が伸びる)。
 * 下の記録で「何回目で成功したか」を観測して見直す。
 */
export const RETRY_DELAYS_MS = Object.freeze([100, 200, 400, 800, 500]);

/**
 * 再試行するエラーコード。Windows の共有違反は `EBUSY`、削除待ちのファイルなどへの
 * アクセス拒否は `EPERM` になる。`EACCES` は権限の問題でも出るので含めない
 * (待っても直らない失敗に 2 秒を使わない)。
 */
const TRANSIENT_CODES = new Set(["EBUSY", "EPERM"]);

/**
 * `err` が待てば解消しうる lock かどうか。Windows 以外では `false` — Linux / macOS の改名は
 * 開いているファイルでも成功し、これらのコードは恒久的な権限の問題を表す。
 *
 * @param {unknown} err
 * @param {string} platform `process.platform` (テストで差し替える)
 */
export function isTransientLockError(err, platform = process.platform) {
  return platform === "win32" && TRANSIENT_CODES.has(err?.code);
}

/** 同期の待ち。`replaceFile` は同期 API なので、イベントループに戻らずに待つ。 */
function sleepSync(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/**
 * `from` を `to` へ改名する。一時的な lock なら上限まで再試行する。
 *
 * 再試行が起きたときだけ、操作・エラーコード・試行回数・結果を `log` へ 1 行出す
 * (CI のログでも手元でも見え、発生源と頻度を後から調べられる)。諦めたときは最後の
 * エラーを、コードを保ったまま試行回数をメッセージに足して投げる。
 *
 * @param {string} from
 * @param {string} to
 * @param {object} [options] テストで差し替える口
 * @param {(from: string, to: string) => void} [options.rename]
 * @param {(ms: number) => void} [options.sleep]
 * @param {string} [options.platform]
 * @param {(line: string) => void} [options.log]
 * @param {string} [options.label] ログに出す操作名
 */
export function renameWithRetry(from, to, options = {}) {
  const {
    rename = renameSync,
    sleep = sleepSync,
    platform = process.platform,
    log = (line) => console.error(line),
    label = "rename",
  } = options;
  const codes = [];
  for (let attempt = 1; ; attempt++) {
    try {
      rename(from, to);
      if (codes.length > 0) {
        log(`[rename-retry] ${label}: ${codes.join(",")} で ${codes.length} 回失敗した後、${attempt} 回目で成功 (${from} -> ${to})`);
      }
      return;
    } catch (err) {
      const delay = RETRY_DELAYS_MS[attempt - 1];
      if (!isTransientLockError(err, platform) || delay === undefined) {
        if (codes.length > 0) {
          log(`[rename-retry] ${label}: ${[...codes, err.code].join(",")} で ${attempt} 回試して諦めた (${from} -> ${to})`);
          err.message = `${err.message} (${attempt} 回試行)`;
        }
        throw err;
      }
      codes.push(err.code);
      sleep(delay);
    }
  }
}
