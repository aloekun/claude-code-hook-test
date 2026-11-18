/**
 * `scripts/wsl-run/wsl-run.ts` のテスト。
 *
 * 引数の検査 (確認なしで走るための制限)、パスの変換、bash への埋め込みと、組み立てたスクリプトを固定する。
 * WSL を実際に起動する経路 (`runInWsl`) は CI に WSL が無いので、ここでは走らせない。
 */

import { spawnSync } from "node:child_process";
import { describe, expect, it } from "vitest";

import { ALLOWED_ENV, UsageError, buildScript, cargoCommand, parseArgs, shQuote, toWslPath } from "./wsl-run.ts";

describe("parseArgs", () => {
  it("reads sync and the cargo subcommands", () => {
    expect(parseArgs(["sync"])).toEqual({ kind: "sync" });
    for (const sub of ["test", "build", "check", "clippy"]) {
      expect(parseArgs(["cargo", sub, "--workspace"])).toEqual({ kind: "cargo", env: [], args: [sub, "--workspace"] });
    }
    for (const sub of ["run", "list"]) {
      expect(parseArgs(["cargo", "nextest", sub, "--workspace"])).toEqual({
        kind: "cargo",
        env: [],
        args: ["nextest", sub, "--workspace"],
      });
    }
  });

  it("rejects nextest subcommands other than run and list, and options before the subcommand", () => {
    for (const args of [
      ["cargo", "nextest"],
      ["cargo", "nextest", "self", "update", "-y"],
      ["cargo", "nextest", "archive", "--archive-file", "x.tar.zst"],
      ["cargo", "nextest", "show-config", "test-groups"],
      ["cargo", "nextest", "--color", "always", "run"],
    ]) {
      expect(() => parseArgs(args), JSON.stringify(args)).toThrow(/run と list だけ/);
    }
  });

  it("reads allowed environment variables before cargo", () => {
    expect(
      parseArgs(["--env", "CARGO_INCREMENTAL=0", "--env", "CARGO_PROFILE_TEST_DEBUG=line-tables-only", "cargo", "test"]),
    ).toEqual({
      kind: "cargo",
      env: [
        ["CARGO_INCREMENTAL", "0"],
        ["CARGO_PROFILE_TEST_DEBUG", "line-tables-only"],
      ],
      args: ["test"],
    });
  });

  it("rejects anything other than sync and the listed cargo subcommands", () => {
    for (const args of [[], ["sync", "x"], ["bash", "-c", "x"], ["cargo"], ["cargo", "install", "x"], ["cargo", "run"]]) {
      expect(() => parseArgs(args), JSON.stringify(args)).toThrow(UsageError);
    }
  });

  it("rejects cargo arguments that can start an arbitrary program", () => {
    for (const bad of [
      "--config",
      "--config=target.x.runner='sh'",
      "-Zunstable-options",
      "--manifest-path",
      "--manifest-path=/tmp/x/Cargo.toml",
      "+nightly",
      "--config-file",
      "--tool-config-file=x:y.toml",
    ]) {
      expect(() => parseArgs(["cargo", "test", bad]), bad).toThrow(/受け付けません/);
    }
    expect(parseArgs(["cargo", "test", "--", "--configure"]).kind).toBe("cargo");
  });

  it("rejects environment variables that can name a program, and malformed pairs", () => {
    for (const name of ["RUSTFLAGS", "RUSTC_WRAPPER", "CARGO_BUILD_RUSTC_WRAPPER", "CARGO_TARGET_X86_64_UNKNOWN_LINUX_GNU_RUNNER", "CARGO_PROFILE_TEST_RUSTFLAGS", "PATH", "LD_PRELOAD"]) {
      expect(() => parseArgs(["--env", `${name}=x`, "cargo", "test"]), name).toThrow(/受け付けません/);
    }
    for (const pair of [undefined, "NOEQ", "=x"]) {
      const args = pair === undefined ? ["--env"] : ["--env", pair, "cargo", "test"];
      expect(() => parseArgs(args), String(pair)).toThrow(/NAME=VALUE/);
    }
  });

  it("allows the profile keys that cannot name a program", () => {
    for (const name of ["CARGO_PROFILE_DEV_DEBUG", "CARGO_PROFILE_TEST_OPT_LEVEL", "CARGO_PROFILE_CI_CODEGEN_UNITS", "CARGO_BUILD_JOBS", "RUST_TEST_THREADS", "NEXTEST_TEST_THREADS", "NEXTEST_RETRIES", "NEXTEST_PROFILE"]) {
      expect(ALLOWED_ENV.test(name), name).toBe(true);
    }
  });
});

describe("toWslPath", () => {
  it("converts drive paths with either separator", () => {
    expect(toWslPath("C:\\Users\\owner\\repo")).toBe("/mnt/c/Users/owner/repo");
    expect(toWslPath("D:/work/a b/")).toBe("/mnt/d/work/a b");
    expect(toWslPath("C:\\")).toBe("/mnt/c");
  });

  it("rejects a path that is not a Windows absolute path", () => {
    for (const p of ["/mnt/c/x", "repo/x", "\\\\server\\share"]) {
      expect(() => toWslPath(p), p).toThrow(UsageError);
    }
  });
});

describe("shQuote and cargoCommand", () => {
  const bash = spawnSync("bash", ["-c", "true"]).status === 0;

  it.runIf(bash)("passes every argument to bash unchanged", () => {
    const args = ["test", "it's", "a b", "$HOME", "`x`", "\"q\"", "a;b|c&d", "*"];
    const out = spawnSync("bash", ["-c", `printf '%s\\n' ${args.map(shQuote).join(" ")}`], { encoding: "utf8" });
    expect(out.stdout.split("\n").slice(0, -1)).toEqual(args);
  });

  it("puts the environment in front of cargo with env", () => {
    expect(cargoCommand([], ["test", "--workspace"])).toBe("cargo 'test' '--workspace'");
    expect(cargoCommand([["CARGO_INCREMENTAL", "0"]], ["test"])).toBe("env 'CARGO_INCREMENTAL=0' cargo 'test'");
  });
});

describe("buildScript", () => {
  const script = buildScript("/mnt/c/Users/owner/work/repo", "cargo 'test'");

  it("mirrors the repository to ext4, keeping the mirror's target", () => {
    expect(script).toContain('mirror="$HOME/mirror/repo"');
    expect(script).toMatch(/rsync -a --delete .*--exclude=target\/ .*'\/mnt\/c\/Users\/owner\/work\/repo\/' "\$mirror\/"/);
    expect(script).not.toMatch(/--delete-excluded/);
  });

  it("replaces PATH instead of extending the Windows one", () => {
    expect(script).toMatch(/^export PATH="\$HOME\/\.cargo\/bin:\$HOME\/\.local\/bin:[^$"]*"$/m);
  });

  it("runs the command in the mirror under a timeout and exits with its status", () => {
    const lines = script.trimEnd().split("\n");
    expect(lines.indexOf('cd "$mirror"')).toBeGreaterThan(lines.findIndex((l) => l.startsWith("rsync ")));
    expect(script).toMatch(/^timeout --kill-after=10s \d+s cargo 'test'$/m);
    expect(lines.at(-1)).toBe('exit "$status"');
  });

  it("only mirrors when there is no command", () => {
    const sync = buildScript("/mnt/c/repo", undefined);
    expect(sync).toContain("rsync ");
    expect(sync).not.toContain("cd ");
    expect(sync).not.toContain("timeout ");
  });

  it.runIf(spawnSync("bash", ["-n", "-c", "true"]).status === 0)("is valid bash", () => {
    const check = spawnSync("bash", ["-n"], { input: script, encoding: "utf8" });
    expect(check.stderr).toBe("");
    expect(check.status).toBe(0);
  });
});
