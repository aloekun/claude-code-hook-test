/**
 * `renameWithRetry` の再試行の条件・回数・終了条件と、Windows の実際のファイル lock に対する
 * `replaceFile` の挙動のテスト。
 *
 * 前者は `rename` を差し替えて失敗を起こす (OS に依らず決定的に回る)。後者は別プロセスに
 * 削除の共有を許さないハンドルを持たせ、OS レベルの lock を本当に作る (Windows でだけ回る)。
 */

import { spawn } from "node:child_process";
import { existsSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { replaceFile } from "./deploy-artifacts.mjs";
import { isTransientLockError, RETRY_DELAYS_MS, renameWithRetry } from "./rename-with-retry.mjs";

/** 補助プロセスの起動・終了を待つ上限 (無期限に待たない)。 */
const CHILD_TIMEOUT_MS = 15_000;

/** `code` 付きの、fs が投げるのと同じ形のエラー。 */
function fsError(code) {
  return Object.assign(new Error(`${code}: simulated`), { code });
}

/** 先頭の `failures` 回だけ `code` で失敗し、その後は何もせず成功する rename と呼び出し記録。 */
function failingRename(failures, code) {
  const calls = [];
  const rename = (from, to) => {
    calls.push([from, to]);
    if (calls.length <= failures) throw fsError(code);
  };
  return { rename, calls };
}

/** 待ち時間とログを記録する差し替え口 (実際には待たない)。 */
function recorder() {
  const slept = [];
  const logged = [];
  return { slept, logged, sleep: (ms) => slept.push(ms), log: (line) => logged.push(line) };
}

describe("isTransientLockError", () => {
  it("Windows の EBUSY / EPERM だけを一時的な lock とみなす", () => {
    expect(isTransientLockError(fsError("EBUSY"), "win32")).toBe(true);
    expect(isTransientLockError(fsError("EPERM"), "win32")).toBe(true);
    expect(isTransientLockError(fsError("EACCES"), "win32")).toBe(false);
    expect(isTransientLockError(fsError("ENOENT"), "win32")).toBe(false);
    expect(isTransientLockError(new Error("no code"), "win32")).toBe(false);
  });

  /** Linux / macOS の改名は開いているファイルでも通るので、これらは恒久的な権限の問題。 */
  it("Windows 以外では再試行しない", () => {
    expect(isTransientLockError(fsError("EBUSY"), "linux")).toBe(false);
    expect(isTransientLockError(fsError("EPERM"), "darwin")).toBe(false);
  });
});

describe("renameWithRetry", () => {
  it("初回で成功すれば待たず、記録も出さない", () => {
    const { rename, calls } = failingRename(0, "EBUSY");
    const r = recorder();
    renameWithRetry("a", "b", { rename, platform: "win32", ...r });
    expect(calls).toEqual([["a", "b"]]);
    expect(r.slept).toEqual([]);
    expect(r.logged).toEqual([]);
  });

  it("一時的な lock が外れれば成功し、何回目で成功したかを 1 行記録する", () => {
    const { rename, calls } = failingRename(2, "EBUSY");
    const r = recorder();
    renameWithRetry("a", "b", { rename, platform: "win32", label: "退避", ...r });
    expect(calls).toHaveLength(3);
    expect(r.slept).toEqual([100, 200]);
    expect(r.logged).toEqual(["[rename-retry] 退避: EBUSY,EBUSY で 2 回失敗した後、3 回目で成功 (a -> b)"]);
  });

  it("lock が外れなければ 6 回で諦め、コードを保って試行回数を足したエラーを投げる", () => {
    const { rename, calls } = failingRename(Infinity, "EPERM");
    const r = recorder();
    let thrown;
    try {
      renameWithRetry("a", "b", { rename, platform: "win32", ...r });
    } catch (err) {
      thrown = err;
    }
    expect(thrown?.code).toBe("EPERM");
    expect(thrown?.message).toBe("EPERM: simulated (6 回試行)");
    expect(calls).toHaveLength(RETRY_DELAYS_MS.length + 1);
    expect(r.slept).toEqual([100, 200, 400, 800, 500]);
    expect(r.slept.reduce((a, b) => a + b, 0)).toBe(2000);
    expect(r.logged).toHaveLength(1);
    expect(r.logged[0]).toContain("6 回試して諦めた");
  });

  /** CodeRabbit #624: 成功後に投げると、呼び出し側が済んだ改名を失敗と見なして巻き戻す。 */
  it("記録に失敗しても、改名の結果 (成功・元のエラー) を変えない", () => {
    const log = () => {
      throw new Error("log failed");
    };
    const sleep = () => {};
    const retried = failingRename(1, "EBUSY");
    expect(() => renameWithRetry("a", "b", { rename: retried.rename, platform: "win32", sleep, log })).not.toThrow();
    expect(retried.calls).toHaveLength(2);

    const exhausted = failingRename(Infinity, "EBUSY");
    expect(() => renameWithRetry("a", "b", { rename: exhausted.rename, platform: "win32", sleep, log })).toThrow(
      "EBUSY: simulated (6 回試行)",
    );
  });

  it("一時的でないエラーは再試行せず、そのまま投げる", () => {
    for (const code of ["EACCES", "ENOENT", "EXDEV"]) {
      const { rename, calls } = failingRename(Infinity, code);
      const r = recorder();
      expect(() => renameWithRetry("a", "b", { rename, platform: "win32", ...r })).toThrow(`${code}: simulated`);
      expect(calls).toHaveLength(1);
      expect(r.slept).toEqual([]);
      expect(r.logged).toEqual([]);
    }
  });

  it("Windows 以外では EBUSY でも再試行しない", () => {
    const { rename, calls } = failingRename(Infinity, "EBUSY");
    const r = recorder();
    expect(() => renameWithRetry("a", "b", { rename, platform: "linux", ...r })).toThrow("EBUSY: simulated");
    expect(calls).toHaveLength(1);
    expect(r.slept).toEqual([]);
  });
});

/** 同期の待ち (`replaceFile` の差し替え口に渡す。実装側の `sleepSync` と同じ。ADR-084 で複製)。 */
function sleepSync(ms) {
  Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, ms);
}

/**
 * 別プロセスの PowerShell が `path` を削除の共有なし (`FileShare.Read`) で開き、`releaseFile` が
 * 作られるか `maxHoldMs` が過ぎるまで持ち続ける。開いたことを stdout の `locked` で知らせる。
 * Node 自身の open は削除の共有を許すので、この lock は Node では作れない。値は環境変数で渡す
 * (コマンド文字列に埋め込まない)。
 *
 * lock を外す時機を時間でなく合図のファイルで決めるのは、負荷の高い runner で `replaceFile` の
 * 初回の改名より先に lock が外れ、再試行を通らずに成功してしまうのを防ぐため (CodeRabbit #624)。
 * 起動を待てなかったときも、補助プロセスを終わらせてから投げる (後片付けの削除を lock させない)。
 */
async function holdLock(path, maxHoldMs) {
  const releaseFile = `${path}.release`;
  const script =
    "$f = [System.IO.File]::Open($env:LOCK_PATH, 'Open', 'Read', 'Read'); " +
    "[Console]::Out.WriteLine('locked'); [Console]::Out.Flush(); " +
    "$deadline = [DateTime]::Now.AddMilliseconds([int]$env:HOLD_MS); " +
    "while (-not (Test-Path -LiteralPath $env:RELEASE_PATH) -and [DateTime]::Now -lt $deadline) " +
    "{ Start-Sleep -Milliseconds 20 }; $f.Close()";
  const child = spawn("powershell.exe", ["-NoProfile", "-NonInteractive", "-Command", script], {
    env: { ...process.env, LOCK_PATH: path, RELEASE_PATH: releaseFile, HOLD_MS: String(maxHoldMs) },
    stdio: ["ignore", "pipe", "ignore"],
  });
  const exited = new Promise((done) => child.once("exit", done));
  const stop = async () => {
    child.kill();
    await Promise.race([exited, new Promise((done) => setTimeout(done, CHILD_TIMEOUT_MS))]);
  };
  try {
    await new Promise((ready, fail) => {
      const timer = setTimeout(() => fail(new Error("lock helper did not start")), CHILD_TIMEOUT_MS);
      child.stdout.on("data", (chunk) => {
        if (String(chunk).includes("locked")) {
          clearTimeout(timer);
          ready();
        }
      });
      child.once("error", fail);
      child.once("exit", (code) => fail(new Error(`lock helper exited early (code ${code})`)));
    });
  } catch (err) {
    await stop();
    throw err;
  }
  return { releaseFile, release: stop };
}

describe.skipIf(process.platform !== "win32")("replaceFile と Windows の実際の lock", () => {
  let root;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), "rename-retry-"));
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true, maxRetries: 10, retryDelay: 200 });
  });

  it("削除の共有を許さないハンドルがあると、改名は EBUSY で失敗する (前提の確認)", async () => {
    const path = join(root, "tool.bin");
    writeFileSync(path, "v1");
    const lock = await holdLock(path, 10_000);
    try {
      let code;
      try {
        renameSync(path, `${path}.old`);
      } catch (err) {
        code = err.code;
      }
      expect(code).toBe("EBUSY");
    } finally {
      await lock.release();
    }
  }, 30_000);

  it("lock が予算内に外れれば、再試行で差し替えに成功する", async () => {
    const src = join(root, "new.bin");
    const dest = join(root, "tool.bin");
    writeFileSync(src, "v2");
    writeFileSync(dest, "v1");
    const logged = [];
    const lock = await holdLock(dest, 10_000);
    // 最初の待ち (= 初回の退避が lock で失敗した後) で lock を外させる。
    let released = false;
    const sleep = (ms) => {
      if (!released) {
        writeFileSync(lock.releaseFile, "");
        released = true;
      }
      sleepSync(ms);
    };
    try {
      replaceFile(src, dest, { sleep, log: (line) => logged.push(line) });
      expect(released).toBe(true);
      expect(readFileSync(dest, "utf8")).toBe("v2");
      expect(logged).toHaveLength(1);
      expect(logged[0]).toMatch(/^\[rename-retry\] 退避: EBUSY/);
      expect(logged[0]).toContain("回目で成功");
    } finally {
      await lock.release();
    }
  }, 30_000);

  it("lock が予算を超えて続けば諦め、旧い中身を dest に残す", async () => {
    const src = join(root, "new.bin");
    const dest = join(root, "tool.bin");
    writeFileSync(src, "v2");
    writeFileSync(dest, "v1");
    const lock = await holdLock(dest, 10_000);
    try {
      let thrown;
      try {
        replaceFile(src, dest, { log: () => {} });
      } catch (err) {
        thrown = err;
      }
      expect(thrown?.code).toBe("EBUSY");
      expect(thrown?.message).toContain("(6 回試行)");
      expect(readFileSync(dest, "utf8")).toBe("v1");
      expect(existsSync(`${dest}.old`)).toBe(false);
      expect(existsSync(`${dest}.new`)).toBe(false);
    } finally {
      await lock.release();
    }
  }, 30_000);
});
