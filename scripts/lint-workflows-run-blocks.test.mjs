/**
 * `scripts/lint-workflows-run-blocks.mjs` (契約検査 3、順位 515 / 撤1-①) の fixture テスト。
 *
 * 由来 incident (PR #428、順位 319) の行をそのまま bad fixture に置き、条件文脈 / `|| true` /
 * 継続行 の境界を negative で固定する。PR #491 の CodeRabbit 指摘 2 件 (パイプライン先頭の
 * `grep`、実効 shell の解決) に対応する回帰も同居させる。
 */

import { describe, expect, it } from "vitest";

import {
  checkRunBlock,
  classifyRunnerOs,
  collectRunSteps,
  joinContinuationLines,
  resolveShellKind,
  splitPipelineSegments,
  splitStatements,
} from "./lint-workflows-run-blocks.mjs";

describe("joinContinuationLines", () => {
  it("joins backslash-continued physical lines into one logical line keeping the first line number", () => {
    const text = "a \\\n  b \\\n  c\nd\n";
    expect(joinContinuationLines(text)).toEqual([
      { line: 1, text: "a  b  c" },
      { line: 4, text: "d" },
      { line: 5, text: "" },
    ]);
  });
});

describe("splitPipelineSegments", () => {
  it("splits on real pipe operators", () => {
    expect(splitPipelineSegments("a | b | c")).toEqual(["a ", " b ", " c"]);
  });

  it("keeps a pipe inside single quotes as literal regex alternation", () => {
    expect(splitPipelineSegments(`grep -E '^a|^b' file`)).toEqual([`grep -E '^a|^b' file`]);
  });

  it("keeps a pipe inside double quotes as literal", () => {
    expect(splitPipelineSegments(`echo "a|b"`)).toEqual([`echo "a|b"`]);
  });

  it("treats || as logical OR, not a pipe", () => {
    expect(splitPipelineSegments("a || b")).toEqual(["a || b"]);
  });

  it("respects backslash escapes outside quotes", () => {
    expect(splitPipelineSegments("a \\| b")).toEqual(["a \\| b"]);
  });
});

describe("splitStatements", () => {
  it("splits on top-level semicolons and &&", () => {
    expect(splitStatements("a; b && c")).toEqual(["a", " b ", " c"]);
  });

  it("keeps `|| true` attached to the command it guards", () => {
    expect(splitStatements("a || true; b")).toEqual(["a || true", " b"]);
  });

  it("does not split inside a command substitution", () => {
    expect(splitStatements(`X=$(a; b)`)).toEqual([`X=$(a; b)`]);
  });

  it("does not split on a semicolon inside quotes", () => {
    expect(splitStatements(`echo "a; b"`)).toEqual([`echo "a; b"`]);
  });
});

describe("checkRunBlock: set without -e", () => {
  it("flags `set -uo pipefail` (the PR #428 shape)", () => {
    const findings = checkRunBlock("set -uo pipefail\necho ok\n");
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(1);
    expect(findings[0].message).toContain("-e を外しません");
  });

  it("accepts `set -euo pipefail`", () => {
    expect(checkRunBlock("set -euo pipefail\necho ok\n")).toEqual([]);
  });

  it("accepts split flag groups as long as one carries e", () => {
    expect(checkRunBlock("set -u -e -o pipefail\n")).toEqual([]);
  });

  it("ignores `set +e` and `set -o pipefail` (no short flag group claiming to drop -e)", () => {
    expect(checkRunBlock("set +e\nset -o pipefail\n")).toEqual([]);
  });

  it("ignores commented-out set lines", () => {
    expect(checkRunBlock("# ここで `set -uo pipefail` と書いても -e は外れない\nset -euo pipefail\n")).toEqual([]);
  });
});

describe("checkRunBlock: grep in a pipeline", () => {
  it("flags the PR #428 command substitution (grep in the middle)", () => {
    const line = `EXISTING=$(printf '%s\\n' "$EXISTING" | grep -E '^[0-9]+$' | sort -n | tail -1)`;
    const findings = checkRunBlock(`set -euo pipefail\n${line}\n`);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(2);
    expect(findings[0].message).toContain("grep");
  });

  it("flags grep at the head of a pipeline (PR #491 CodeRabbit finding)", () => {
    const findings = checkRunBlock(`grep -E '^rank=' "$F" | sort -u\n`);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(1);
  });

  it("flags grep at the tail of a pipeline", () => {
    expect(checkRunBlock(`cat "$F" | grep rank\n`)).toHaveLength(1);
  });

  it("accepts grep inside an if condition", () => {
    expect(checkRunBlock(`if printf '%s\\n' "$CLASSES" | grep -qx 'RATE_LIMITED'; then\n  echo x\nfi\n`)).toEqual([]);
  });

  it("accepts a negated multi-line if condition joined by continuation lines", () => {
    const text =
      `if ! grep -qE '^\\[NIGHTLY_TASK\\]' "$F" \\\n` +
      `  || ! grep -qE '^summary=' "$F"; then\n` +
      `  exit 1\nfi\n`;
    expect(checkRunBlock(text)).toEqual([]);
  });

  it("accepts grep guarded by `|| true`", () => {
    expect(checkRunBlock(`COUNT=$(echo "$X" | grep -c foo || true)\n`)).toEqual([]);
  });

  it("flags a pipeline that only continues onto the next line", () => {
    const text = `RESULT=$(printf '%s' "$X" \\\n  | grep foo)\n`;
    const findings = checkRunBlock(text);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(1);
  });

  it("accepts a bare grep with no pipeline at all (redirect only)", () => {
    expect(checkRunBlock(`grep -E '^rank=' "$F" >> "$GITHUB_OUTPUT"\n`)).toEqual([]);
  });

  it("accepts alternation inside the grep pattern, which is not a pipeline", () => {
    // nightly-todo.yml の実在行。`|` は正規表現の一部で、パイプ演算子ではない。
    // クォートを見ずに分割すると、この 2 行が即座に誤検知になる。
    const line = `grep -E '^\\[NIGHTLY_TASK\\]|^summary_display=|^pr_title_display=' "$F"\n`;
    expect(checkRunBlock(line)).toEqual([]);
  });

  it("flags an unprotected grep in the then-branch of a single-line if", () => {
    // 免除を論理行全体に当てると、条件部の `if` に引きずられて then 節が素通りする。
    const findings = checkRunBlock(`if [ -f "$F" ]; then cat "$F" | grep rank; fi\n`);
    expect(findings).toHaveLength(1);
    expect(findings[0].line).toBe(1);
  });

  it("flags a later statement even when an earlier one carries `|| true`", () => {
    const findings = checkRunBlock(`rm -f x || true; cat "$F" | grep rank\n`);
    expect(findings).toHaveLength(1);
  });

  it("still accepts a grep that is itself the single-line if condition", () => {
    expect(checkRunBlock(`if cat "$F" | grep -q rank; then echo y; fi\n`)).toEqual([]);
  });

  it("accepts a grep pipeline that is tolerated within its own statement", () => {
    expect(checkRunBlock(`echo start; cat "$F" | grep rank || true\n`)).toEqual([]);
  });

  it("accepts the awk replacement", () => {
    expect(checkRunBlock(`EXISTING=$(printf '%s\\n' "$EXISTING" | awk '/^[0-9]+$/' | sort -n | tail -1)\n`)).toEqual([]);
  });
});

describe("classifyRunnerOs", () => {
  it.each([
    ["ubuntu-latest", "posix"],
    ["ubuntu-22.04", "posix"],
    ["macos-14", "posix"],
    ["windows-latest", "windows"],
    ["${{ matrix.os }}", "dynamic"],
  ])("classifies %s as %s", (runsOn, expected) => {
    expect(classifyRunnerOs(runsOn)).toBe(expected);
  });

  it("treats an array containing a windows label as windows", () => {
    expect(classifyRunnerOs(["self-hosted", "windows-2022"])).toBe("windows");
  });

  it("treats unknown self-hosted labels as unknown rather than posix", () => {
    expect(classifyRunnerOs(["self-hosted", "gpu"])).toBe("unknown");
  });

  it("reads the labels field of the runs-on object form", () => {
    expect(classifyRunnerOs({ group: "g", labels: ["ubuntu-latest"] })).toBe("posix");
  });

  it("treats a missing runs-on as unknown", () => {
    expect(classifyRunnerOs(undefined)).toBe("unknown");
  });
});

describe("resolveShellKind", () => {
  it("treats an explicit bash declaration as bash on any runner", () => {
    expect(resolveShellKind("bash", "windows")).toBe("bash");
    expect(resolveShellKind("bash --noprofile --norc -eo pipefail {0}", "dynamic")).toBe("bash");
  });

  it("treats known non-bash shells as out of scope", () => {
    for (const shell of ["pwsh", "powershell", "cmd", "python", "sh", "node"]) {
      expect(resolveShellKind(shell, "posix")).toBe("other");
    }
  });

  it("defaults to bash only when the runner is statically non-Windows", () => {
    expect(resolveShellKind(undefined, "posix")).toBe("bash");
  });

  it("reports ambiguity when the shell is unset and the OS is not statically known", () => {
    expect(resolveShellKind(undefined, "windows")).toBe("ambiguous");
    expect(resolveShellKind(undefined, "dynamic")).toBe("ambiguous");
    expect(resolveShellKind(undefined, "unknown")).toBe("ambiguous");
  });
});

describe("collectRunSteps", () => {
  it("keeps bash steps, drops pwsh and uses-only steps, resolving shell through defaults", () => {
    const document = {
      defaults: { run: { shell: "bash" } },
      jobs: {
        build: {
          "runs-on": "ubuntu-latest",
          steps: [
            { name: "bash step", run: "echo a" },
            { id: "ps", shell: "pwsh", run: "Write-Host a" },
            { uses: "actions/checkout@v4" },
          ],
        },
        win: {
          "runs-on": "windows-latest",
          defaults: { run: { shell: "pwsh" } },
          steps: [
            { name: "inherits pwsh", run: "Write-Host b" },
            { name: "explicit bash", shell: "bash", run: "echo b" },
          ],
        },
      },
    };
    const { bash, ambiguous } = collectRunSteps(document);
    expect(bash).toEqual([
      { job: "build", step: "bash step", run: "echo a" },
      { job: "win", step: "explicit bash", run: "echo b" },
    ]);
    expect(ambiguous).toEqual([]);
  });

  it("treats an unspecified shell on a POSIX runner as bash", () => {
    const document = { jobs: { j: { "runs-on": "ubuntu-latest", steps: [{ run: "echo" }] } } };
    const { bash, ambiguous } = collectRunSteps(document);
    expect(bash).toEqual([{ job: "j", step: "#1", run: "echo" }]);
    expect(ambiguous).toEqual([]);
  });

  it("reports an unspecified shell on a matrix runner instead of assuming bash", () => {
    const document = {
      jobs: { j: { "runs-on": "${{ matrix.os }}", steps: [{ name: "s", run: "set -uo pipefail" }] } },
    };
    const { bash, ambiguous } = collectRunSteps(document);
    expect(bash).toEqual([]);
    expect(ambiguous).toEqual([{ job: "j", step: "s", runnerOs: "dynamic" }]);
  });

  it("reports an unspecified shell on a Windows runner, where the default is pwsh", () => {
    const document = { jobs: { j: { "runs-on": "windows-latest", steps: [{ run: "echo" }] } } };
    expect(collectRunSteps(document).ambiguous).toEqual([{ job: "j", step: "#1", runnerOs: "windows" }]);
  });
});
