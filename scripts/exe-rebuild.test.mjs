/**
 * Stop 時の自動再ビルドと PostToolUse の cargo check の部品のテスト (ADR-082、順位 373)。
 *
 * cargo を起動する層 (ビルド・check の実行) は対象外で、判定と deploy の差し替えだけを
 * 一時ディレクトリで固定する。
 */

import { spawn } from "node:child_process";
import {
  copyFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  renameSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, beforeEach, describe, expect, it } from "vitest";

import { findPackageName, formatCheckReport, parsePackageName } from "./cargo-check-for-file.mjs";
import { skipReason } from "./check-exe-freshness.mjs";
import { deployArtifacts, replaceFile } from "./deploy-artifacts.mjs";
import { hashPackageDir, parseStamp } from "./exe-fingerprint.mjs";
import { planRebuild, summarizeBuildErrors } from "./rebuild-stale-exes.mjs";

const EXE_SUFFIX = process.platform === "win32" ? ".exe" : "";

/** 子プロセスの起動・終了を待つ上限 (無期限に待たない)。 */
const CHILD_START_TIMEOUT_MS = 10_000;

let root;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), "exe-rebuild-"));
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

describe("planRebuild", () => {
  const graph = {
    bins: new Map([
      ["cli-a", "cli-a"],
      ["cli-b", "pkg-b"],
      ["cli-b2", "pkg-b"],
    ]),
  };

  /** 同じ package の bin は 1 回の -p で足りる。orphan は再ビルドしない。 */
  it("再ビルドする bin と package を分け、orphan を除く", () => {
    const plan = planRebuild(
      [
        { bin: "cli-b", reason: "mismatch" },
        { bin: "cli-b2", reason: "missing-stamp" },
        { bin: "cli-gone", reason: "orphan" },
        { bin: "cli-a", reason: "mismatch" },
      ],
      graph,
    );
    expect(plan.bins).toEqual(["cli-b", "cli-b2", "cli-a"]);
    expect(plan.packages).toEqual(["cli-a", "pkg-b"]);
    expect(plan.orphans).toEqual([{ bin: "cli-gone", reason: "orphan" }]);
  });

  it("古い exe が無ければ何もしない", () => {
    expect(planRebuild([], graph)).toEqual({ bins: [], packages: [], orphans: [] });
  });
});

describe("summarizeBuildErrors", () => {
  it("error 行だけを取り出す", () => {
    const stderr = [
      "   Compiling cli-a v0.1.0",
      "src\\main.rs:3:5: error[E0425]: cannot find value `x` in this scope",
      "src\\main.rs:9:1: warning: unused import",
      "error: could not compile `cli-a` (bin \"cli-a\") due to 1 previous error",
    ].join("\r\n");
    expect(summarizeBuildErrors(stderr)).toBe(
      [
        "src\\main.rs:3:5: error[E0425]: cannot find value `x` in this scope",
        'error: could not compile `cli-a` (bin "cli-a") due to 1 previous error',
      ].join("\n"),
    );
  });

  /** error 行の形をしていない失敗 (リンカ・ロックなど) でも理由を空にしない。 */
  it("error 行が無ければ末尾の行を返す", () => {
    expect(summarizeBuildErrors("line1\nlinker failed\n\n")).toBe("line1\nlinker failed");
  });
});

describe("replaceFile", () => {
  it("既存の dest を差し替え、退避した .old と一時の .new を残さない", () => {
    const src = join(root, "new.bin");
    const dest = join(root, "tool.bin");
    writeFileSync(src, "v2");
    writeFileSync(dest, "v1");
    replaceFile(src, dest);
    expect(readFileSync(dest, "utf8")).toBe("v2");
    expect(existsSync(`${dest}.old`)).toBe(false);
    expect(existsSync(`${dest}.new`)).toBe(false);
  });

  it("dest が無ければそのまま置く", () => {
    const src = join(root, "new.bin");
    writeFileSync(src, "v1");
    replaceFile(src, join(root, "tool.bin"));
    expect(readFileSync(join(root, "tool.bin"), "utf8")).toBe("v1");
  });

  /** CodeRabbit #516: 退避の後の改名に失敗しても、exe が無い状態を残さない。 */
  it("新しい中身の改名に失敗したら、旧い dest を戻して投げる", () => {
    const src = join(root, "new.bin");
    const dest = join(root, "tool.bin");
    writeFileSync(src, "v2");
    writeFileSync(dest, "v1");
    const failOnStaged = (from, to) => {
      if (from === `${dest}.new`) throw new Error("EBUSY: simulated");
      renameSync(from, to);
    };
    expect(() => replaceFile(src, dest, failOnStaged)).toThrow("EBUSY: simulated");
    expect(readFileSync(dest, "utf8")).toBe("v1");
    expect(existsSync(`${dest}.old`)).toBe(false);
  });

  /**
   * 本物の実行中プロセスの exe を差し替える (ADR-082 決定 4 の前提を実測で固定する)。
   * 両 OS に確実にある実行ファイルとして Node 自身をコピーして起動する。差し替えの後も
   * 実行中のプロセスは退避した旧いファイルで動き続け、dest は新しい中身になる。
   *
   * 負の対照 (直接の上書きが失敗すること) は Windows でだけ断言する。Windows では実行中の
   * exe への書き込みが確実に EBUSY になる。Linux の ETXTBSY はカーネルの版で扱いが
   * 変わった経緯があり、CI の runner で必ず失敗するとは言い切れない。
   */
  it("実行中の exe でも差し替えられ、実行中のプロセスは生き続ける", async () => {
    const dest = join(root, `running${EXE_SUFFIX}`);
    const src = join(root, "replacement.bin");
    copyFileSync(process.execPath, dest);
    writeFileSync(src, "new-content");
    const child = spawn(dest, ["-e", "process.stdout.write('ready'); setInterval(() => {}, 1000)"], {
      stdio: ["ignore", "pipe", "ignore"],
    });
    const exited = new Promise((done) => child.once("exit", done));
    try {
      await new Promise((ready, fail) => {
        const timer = setTimeout(() => fail(new Error("child did not start")), CHILD_START_TIMEOUT_MS);
        child.stdout.once("data", () => {
          clearTimeout(timer);
          ready();
        });
        child.once("error", fail);
      });
      if (process.platform === "win32") {
        expect(() => copyFileSync(src, dest)).toThrow();
      }
      replaceFile(src, dest);
      expect(readFileSync(dest, "utf8")).toBe("new-content");
      // シグナルで終了した場合も exitCode は null なので、signalCode も見る (CodeRabbit #517)。
      expect(child.exitCode).toBeNull();
      expect(child.signalCode).toBeNull();
    } finally {
      child.kill();
      await Promise.race([exited, new Promise((done) => setTimeout(done, CHILD_START_TIMEOUT_MS))]);
    }
  }, 30_000);

  it("前回の差し替えで残った .old を片付ける", () => {
    const src = join(root, "new.bin");
    const dest = join(root, "tool.bin");
    writeFileSync(src, "v3");
    writeFileSync(dest, "v2");
    writeFileSync(`${dest}.old`, "v1");
    replaceFile(src, dest);
    expect(readFileSync(dest, "utf8")).toBe("v3");
    expect(existsSync(`${dest}.old`)).toBe(false);
  });
});

describe("deployArtifacts", () => {
  const hex = "5".repeat(64);
  let releaseDir;
  let claudeDir;

  beforeEach(() => {
    releaseDir = join(root, "release");
    claudeDir = join(root, ".claude");
    mkdirSync(releaseDir);
    mkdirSync(claudeDir);
  });

  /** 渡された (= ビルド前に計算した) 値を記録する。deploy 時に再計算しない。 */
  it("exe を置き、渡されたフィンガープリントを記録する", () => {
    writeFileSync(join(releaseDir, `cli-a${EXE_SUFFIX}`), "bin");
    deployArtifacts(["cli-a"], new Map([["cli-a", { fingerprint: hex }]]), { releaseDir, claudeDir });
    expect(readFileSync(join(claudeDir, `cli-a${EXE_SUFFIX}`), "utf8")).toBe("bin");
    expect(parseStamp(readFileSync(join(claudeDir, "cli-a.fingerprint"), "utf8"))).toBe(hex);
  });

  it("ビルド成果物が無ければ投げ、記録を書かない", () => {
    const fingerprints = new Map([["cli-a", { fingerprint: hex }]]);
    expect(() => deployArtifacts(["cli-a"], fingerprints, { releaseDir, claudeDir })).toThrow(
      "build artifact not found",
    );
    expect(existsSync(join(claudeDir, "cli-a.fingerprint"))).toBe(false);
  });

  it("workspace の bin でなければ投げ、exe を置かない", () => {
    writeFileSync(join(releaseDir, `cli-x${EXE_SUFFIX}`), "bin");
    expect(() => deployArtifacts(["cli-x"], new Map(), { releaseDir, claudeDir })).toThrow(
      "not a bin target",
    );
    expect(existsSync(join(claudeDir, `cli-x${EXE_SUFFIX}`))).toBe(false);
  });
});

describe("skipReason", () => {
  it("BUILD_INFO があれば検査しない (prebuilt バイナリ)", () => {
    writeFileSync(join(root, "BUILD_INFO"), "");
    expect(skipReason(root, {})).toContain("BUILD_INFO");
  });

  it("override が truthy なら検査しない", () => {
    expect(skipReason(root, { EXE_FRESHNESS_CHECK_OVERRIDE: "TRUE" })).toContain("OVERRIDE");
    expect(skipReason(root, { EXE_FRESHNESS_CHECK_OVERRIDE: "0" })).toBeNull();
  });

  it("どちらも無ければ検査する", () => {
    expect(skipReason(root, {})).toBeNull();
  });
});

describe("hashPackageDir のシンボリックリンク", () => {
  /** 読み飛ばすとリンク先の付け替えを検出できない (PR #515 の takt レビュー指摘)。 */
  it("リンク先を付け替えるとハッシュが変わる", (ctx) => {
    const pkg = join(root, "pkg");
    mkdirSync(pkg);
    writeFileSync(join(pkg, "a.txt"), "a");
    writeFileSync(join(pkg, "b.txt"), "b");
    try {
      symlinkSync("a.txt", join(pkg, "link"));
    } catch {
      // Windows で開発者モードが無いとシンボリックリンクを作れない。CI の Linux で検証する。
      ctx.skip();
    }
    const before = hashPackageDir(pkg);
    rmSync(join(pkg, "link"));
    symlinkSync("b.txt", join(pkg, "link"));
    expect(hashPackageDir(pkg)).not.toBe(before);
  });
});

describe("parsePackageName", () => {
  it("[package] の name を読む", () => {
    expect(parsePackageName('[package]\nname = "cli-a"\nversion = "0.1.0"\n')).toBe("cli-a");
  });

  /** 別の節の name (例: [[bin]]) を package 名と取り違えない。 */
  it("[package] 以外の節の name は読まない", () => {
    const manifest = '[package]\nversion = "0.1.0"\n\n[[bin]]\nname = "other"\n';
    expect(parsePackageName(manifest)).toBeNull();
  });

  it("workspace だけの Cargo.toml は null", () => {
    expect(parsePackageName('[workspace]\nmembers = ["src/*"]\n')).toBeNull();
  });
});

describe("findPackageName", () => {
  beforeEach(() => {
    writeFileSync(join(root, "Cargo.toml"), '[workspace]\nmembers = ["src/*"]\n');
    mkdirSync(join(root, "src", "cli-a", "src", "stages"), { recursive: true });
    writeFileSync(join(root, "src", "cli-a", "Cargo.toml"), '[package]\nname = "cli-a"\n');
  });

  it("入れ子のディレクトリからでも最寄りの package を見つける", () => {
    expect(findPackageName(join(root, "src", "cli-a", "src", "stages", "gate.rs"), root)).toBe("cli-a");
  });

  it("どの package にも属さないファイルは null", () => {
    expect(findPackageName(join(root, "scripts", "x.rs"), root)).toBeNull();
  });

  /** 別の workspace や別ドライブのファイルで、無関係な package を check しない。 */
  it("root の外のファイルは null", () => {
    expect(findPackageName(join(tmpdir(), "elsewhere", "main.rs"), root)).toBeNull();
  });
});

describe("formatCheckReport", () => {
  it("error が無ければ空 (= PostToolUse に何も出さない)", () => {
    expect(formatCheckReport("src\\lib.rs:1:1: warning: unused\n", "cli-a")).toBe("");
  });

  /**
   * pre-push simplicity review (本 PR): 場所の無い行頭の error を拾えず、cargo check が
   * 失敗しているのに無出力になっていた。集計行は個々の error ではないので件数に数えない。
   */
  it("行頭の error も数え、集計行は数えない", () => {
    const stderr = [
      "error[E0601]: `main` function not found in crate `cli_a`",
      "src\\lib.rs:3:5: error[E0425]: cannot find value `x` in this scope",
      "error: could not compile `cli-a` (bin \"cli-a\") due to 2 previous errors",
    ].join("\n");
    const lines = formatCheckReport(stderr, "cli-a").split("\n");
    expect(lines[0]).toBe("cargo check -p cli-a: compile error 2 件");
    expect(lines.slice(2)).toEqual([
      "error[E0601]: `main` function not found in crate `cli_a`",
      "src\\lib.rs:3:5: error[E0425]: cannot find value `x` in this scope",
    ]);
  });

  /** build script の失敗などで、集計行しか error の形をしていないときも無出力にしない。 */
  it("集計行しか無ければ、それを出す", () => {
    const report = formatCheckReport("error: could not compile `cli-a` (lib)\n", "cli-a");
    expect(report.split("\n")[0]).toBe("cargo check -p cli-a: compile error あり");
    expect(report).toContain("could not compile");
  });

  it("件数と、編集途中なら続けてよい旨と、先頭の error を出す", () => {
    const stderr = Array.from({ length: 7 }, (_, i) => `src\\lib.rs:${i}:1: error[E0425]: e${i}`).join("\n");
    const lines = formatCheckReport(stderr, "cli-a").split("\n");
    expect(lines[0]).toBe("cargo check -p cli-a: compile error 7 件");
    expect(lines[1]).toContain("編集を続けてよい");
    expect(lines.slice(2)).toHaveLength(5);
  });
});
