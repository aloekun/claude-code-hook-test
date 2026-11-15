/**
 * `scripts/mutate/mutate.ts` のテスト。cargo は起動しない。書き戻しは tempdir の実ファイルで確かめる。
 */

import { spawnSync } from "node:child_process";
import { existsSync, mkdtempSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import {
  UsageError,
  acquireLock,
  applyMutation,
  assertWritable,
  checkCommand,
  classify,
  isAlive,
  isInterrupted,
  lockPathFor,
  parseArgs,
  parseSpec,
  runCommand,
  summaryLines,
  withMutation,
} from "./mutate.ts";

describe("parseArgs", () => {
  it("reads the spec path before -- and the test command after it", () => {
    expect(parseArgs(["s.json", "--", "cargo", "test", "-p", "a", "--", "--nocapture"])).toEqual({
      specFile: "s.json",
      command: ["cargo", "test", "-p", "a", "--", "--nocapture"],
    });
  });

  /** 置換の文字列を引数で渡す形は受け付けない (Windows で cmd.exe に壊されるため)。 */
  it("rejects a missing -- and replacement strings passed as arguments", () => {
    expect(() => parseArgs(["s.json", "cargo", "test"])).toThrow(UsageError);
    expect(() => parseArgs(["--", "cargo", "test"])).toThrow(UsageError);
    expect(() => parseArgs(["src/a.rs", "x > 0", "x >= 0", "--", "cargo", "test"])).toThrow(UsageError);
  });
});

describe("checkCommand", () => {
  it("accepts cargo test with ordinary arguments", () => {
    expect(() => checkCommand(["cargo", "test", "-p", "x", "--bins", "--", "name"])).not.toThrow();
  });

  it("rejects anything other than cargo test", () => {
    for (const command of [["cargo", "run"], ["sh", "-c", "cargo test"], ["pnpm", "test"], []]) {
      expect(() => checkCommand(command), command.join(" ")).toThrow(UsageError);
    }
  });

  it("rejects cargo arguments that can start an arbitrary program", () => {
    for (const arg of ["--config", "--config=target.x.runner='sh'", "-Zunstable-options", "--manifest-path=/tmp/C.toml", "+nightly", "--no-run"]) {
      expect(() => checkCommand(["cargo", "test", arg]), arg).toThrow(UsageError);
    }
  });
});

describe("parseSpec", () => {
  it("reads mutants and names unnamed ones by position", () => {
    const spec = JSON.stringify({
      mutants: [
        { name: "keep-wai", file: "a.rs", from: "wai|", to: "" },
        { file: "b.rs", from: "?;", to: ";" },
      ],
    });
    expect(parseSpec(spec)).toEqual([
      { name: "keep-wai", file: "a.rs", from: "wai|", to: "" },
      { name: "M2", file: "b.rs", from: "?;", to: ";" },
    ]);
  });

  it("rejects malformed specs", () => {
    for (const text of ["{", "{}", '{"mutants":[]}', '{"mutants":[{"file":"a","from":"x"}]}', '{"mutants":[{"file":"a","from":"x","to":"x"}]}']) {
      expect(() => parseSpec(text), text).toThrow(UsageError);
    }
  });
});

describe("applyMutation", () => {
  it("replaces the single occurrence", () => {
    expect(applyMutation("a > b;\nc;", "a > b", "a >= b")).toBe("a >= b;\nc;");
  });

  it("rejects a missing, empty or repeated source string", () => {
    expect(() => applyMutation("abc", "x", "y")).toThrow(UsageError);
    expect(() => applyMutation("abc", "", "y")).toThrow(UsageError);
    expect(() => applyMutation("x;x;", "x", "y")).toThrow(UsageError);
  });
});

describe("classify", () => {
  it("tells killed, survived and non-compiling mutants apart", () => {
    expect(classify({ status: 0, output: "test result: ok. 3 passed" })).toBe("SURVIVED");
    expect(classify({ status: 101, output: "test a ... FAILED\ntest result: FAILED. 1 failed" })).toBe("KILLED");
    expect(classify({ status: 101, output: "error[E0308]: mismatched types\n" })).toBe("NO_COMPILE");
    expect(classify({ status: 101, output: "error: could not compile `x`" })).toBe("NO_COMPILE");
  });

  /** 実走で取り違えた形: テストが落ちると cargo は `error: test failed` も出す。 */
  it("counts a failed test as killed even though cargo prints error: test failed", () => {
    const output = "test a ... FAILED\ntest result: FAILED. 1 failed\nerror: test failed, to rerun pass `-p x --bin x`";
    expect(classify({ status: 101, output })).toBe("KILLED");
  });

  /** 集計を出す前にテストのバイナリが落ちた場合も、テストが変異を捕まえたと数える。 */
  it("counts a test binary that crashed before its summary as killed", () => {
    const output = "thread 'a' has overflowed its stack\nerror: test failed, to rerun pass `-p x --lib`\n\nCaused by:\n  process didn't exit successfully (exit code: 0xc00000fd, STATUS_STACK_OVERFLOW)";
    expect(classify({ status: 101, output })).toBe("KILLED");
  });

  /** 終了コード 0 でもテストが走っていなければ (`-- --list` / `--no-run`)、SURVIVED と数えない。 */
  it("reports a successful exit without a test result as NO_RUN", () => {
    expect(classify({ status: 0, output: "a::b: test\n\n1 test, 0 benchmarks" })).toBe("NO_RUN");
    expect(classify({ status: 0, output: "    Finished `test` profile\n  Executable unittests src/main.rs" })).toBe("NO_RUN");
  });

  /** 引数の誤りなどでテストが走らなかった非 0 終了を、KILLED と数えない。 */
  it("reports a failure without any test run as NO_RUN", () => {
    const output = "error: unexpected argument '--definitely-invalid' found\n\nUsage: cargo test [OPTIONS]";
    expect(classify({ status: 1, output })).toBe("NO_RUN");
    expect(classify({ status: 101, output: "error: package ID specification `nope` did not match any packages" })).toBe("NO_RUN");
  });
});

describe("acquireLock", () => {
  let dir: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "mutate-lock-"));
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it("refuses a second holder until the first releases", () => {
    const lock = join(dir, "x.lock");
    const release = acquireLock(lock);
    expect(() => acquireLock(lock)).toThrow(UsageError);
    release();
    expect(existsSync(lock)).toBe(false);
    acquireLock(lock)();
  });

  /** ロックは PID を書いた状態で現れる。作業用の一時ファイルは残さない。 */
  it("creates the lock with the pid already written and leaves no staging file", () => {
    const lock = join(dir, "x.lock");
    const release = acquireLock(lock);
    expect(readFileSync(lock, "utf8")).toBe(String(process.pid));
    expect(readdirSync(dir)).toEqual(["x.lock"]);
    release();
    expect(readdirSync(dir)).toEqual([]);
  });

  /** 生きているプロセスのロックは、取り直しも解放もしない。 */
  it("never takes over or removes a lock held by a live process", () => {
    const lock = join(dir, "x.lock");
    writeFileSync(lock, String(process.ppid));
    expect(() => acquireLock(lock)).toThrow(UsageError);
    expect(readFileSync(lock, "utf8")).toBe(String(process.ppid));
    expect(readdirSync(dir)).toEqual(["x.lock"]);
  });

  /** 持ち主のプロセスが終わっているロック (前回の異常終了の残り) は取り直す。 */
  it("takes over a lock whose owner has exited", () => {
    const lock = join(dir, "x.lock");
    const exited = spawnSync(process.execPath, ["-e", "0"]).pid;
    writeFileSync(lock, String(exited));
    const release = acquireLock(lock);
    expect(readFileSync(lock, "utf8")).toBe(String(process.pid));
    release();
  });

  /** 取り直された後の解放で、他人のロックを消さない。 */
  it("does not remove a lock that another process now owns", () => {
    const lock = join(dir, "x.lock");
    const release = acquireLock(lock);
    writeFileSync(lock, String(process.ppid));
    release();
    expect(readFileSync(lock, "utf8")).toBe(String(process.ppid));
  });

  it("uses one lock per repository, outside the repository", () => {
    const other = mkdtempSync(join(tmpdir(), "mutate-lock-other-"));
    try {
      expect(lockPathFor(dir)).toBe(lockPathFor(dir));
      expect(lockPathFor(dir)).not.toBe(lockPathFor(other));
      expect(lockPathFor(dir).startsWith(dir)).toBe(false);
    } finally {
      rmSync(other, { recursive: true, force: true });
    }
  });
});

describe("isInterrupted", () => {
  /** Ctrl+C で止めた実行を KILLED と数えず、変異ループごと止めるための判定。 */
  it("treats a signal death and the Windows Ctrl+C exit code as interruptions", () => {
    expect(isInterrupted(null)).toBe(true);
    expect(isInterrupted(0xc000013a)).toBe(true);
    for (const status of [0, 1, 101]) {
      expect(isInterrupted(status), String(status)).toBe(false);
    }
  });
});

describe("runCommand", () => {
  let dir: string;

  beforeEach(() => {
    dir = mkdtempSync(join(tmpdir(), "mutate-run-"));
  });

  afterEach(() => {
    rmSync(dir, { recursive: true, force: true });
  });

  it("returns the exit status and both output streams", async () => {
    const script = join(dir, "ok.cjs");
    writeFileSync(script, "process.stdout.write('test result: ok. 1 passed\\n'); process.stderr.write('warn\\n'); process.exitCode = 3;");
    const r = await runCommand([process.execPath, script], 10_000);
    expect(r.status).toBe(3);
    expect(r.output).toContain("test result: ok. 1 passed");
    expect(r.output).toContain("warn");
  });

  /**
   * 指摘の再現: 子 (cargo 役) が孫 (テストのバイナリ役) に出力のパイプを継がせて止まらない。
   * spawnSync の timeout では戻らなかった形。タイムアウトで例外になり、孫まで止まることを確かめる。
   */
  it("times out and stops the whole tree even when a grandchild holds the output pipes", async () => {
    const grandchild = join(dir, "grandchild.cjs");
    const parent = join(dir, "parent.cjs");
    const pidFile = join(dir, "grandchild.pid");
    writeFileSync(grandchild, "setInterval(() => {}, 1000);");
    writeFileSync(
      parent,
      [
        "const { spawn } = require('node:child_process');",
        "const fs = require('node:fs');",
        "const g = spawn(process.execPath, [process.argv[2]], { stdio: ['ignore', 'inherit', 'inherit'] });",
        "fs.writeFileSync(process.argv[3], String(g.pid));",
        "setInterval(() => {}, 1000);",
      ].join("\n"),
    );
    const started = Date.now();
    await expect(runCommand([process.execPath, parent, grandchild, pidFile], 2_000)).rejects.toThrow(/終わらなかった/);
    expect(Date.now() - started).toBeLessThan(10_000);
    const pid = Number(readFileSync(pidFile, "utf8"));
    for (let i = 0; i < 50 && isAlive(pid); i++) await new Promise((r) => setTimeout(r, 100));
    expect(isAlive(pid)).toBe(false);
  }, 20_000);
});

describe("summaryLines", () => {
  it("keeps failed tests, result lines, panics and compile errors only", () => {
    const output = [
      "   Compiling x",
      "test a::ok ... ok",
      "test a::bad ... FAILED",
      "thread 'a::bad' panicked at src/a.rs:3:5:",
      "test result: FAILED. 1 passed; 1 failed",
    ].join("\n");
    expect(summaryLines(output)).toEqual([
      "test a::bad ... FAILED",
      "thread 'a::bad' panicked at src/a.rs:3:5:",
      "test result: FAILED. 1 passed; 1 failed",
    ]);
  });
});

describe("files on disk", () => {
  let root: string;

  beforeEach(() => {
    root = mkdtempSync(join(tmpdir(), "mutate-test-"));
  });

  afterEach(() => {
    rmSync(root, { recursive: true, force: true });
  });

  it("applies the mutation while running and restores the original bytes", async () => {
    const file = join(root, "a.rs");
    const original = "fn f() -> bool {\r\n    1 > 0\r\n}\r\n";
    writeFileSync(file, original);
    const seen = await withMutation(file, "1 > 0", "1 >= 0", () => readFileSync(file, "utf8"));
    expect(seen).toBe("fn f() -> bool {\r\n    1 >= 0\r\n}\r\n");
    expect(readFileSync(file, "utf8")).toBe(original);
  });

  it("restores the file when the test run throws or rejects", async () => {
    const file = join(root, "a.rs");
    writeFileSync(file, "keep me");
    await expect(
      withMutation(file, "keep", "lose", () => {
        throw new Error("boom");
      }),
    ).rejects.toThrow("boom");
    expect(readFileSync(file, "utf8")).toBe("keep me");
    await expect(withMutation(file, "keep", "lose", () => Promise.reject(new Error("late")))).rejects.toThrow("late");
    expect(readFileSync(file, "utf8")).toBe("keep me");
  });

  it("leaves the file untouched when the source string is not unique", async () => {
    const file = join(root, "a.rs");
    writeFileSync(file, "x;x;");
    await expect(withMutation(file, "x", "y", () => "ran")).rejects.toThrow(UsageError);
    expect(readFileSync(file, "utf8")).toBe("x;x;");
  });

  it("writes only inside the repository and outside protected directories", () => {
    for (const dir of ["src", ".claude", ".jj", "node_modules", join("src", "x", "target")]) {
      mkdirSync(join(root, dir), { recursive: true });
      writeFileSync(join(root, dir, "f.txt"), "");
    }
    expect(() => assertWritable(join(root, "src", "f.txt"), root)).not.toThrow();
    for (const dir of [".claude", ".jj", "node_modules", join("src", "x", "target")]) {
      expect(() => assertWritable(join(root, dir, "f.txt"), root), dir).toThrow(UsageError);
    }
    const outside = mkdtempSync(join(tmpdir(), "mutate-outside-"));
    try {
      writeFileSync(join(outside, "f.txt"), "");
      expect(() => assertWritable(join(outside, "f.txt"), root)).toThrow();
    } finally {
      rmSync(outside, { recursive: true, force: true });
    }
  });
});
