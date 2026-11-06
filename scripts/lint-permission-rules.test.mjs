/**
 * `scripts/lint-permission-rules.mjs` (順位 389) の fixture テスト。
 *
 * 純粋層 (`checkSettingsText` / `checkWorkflowText`) と、実ディレクトリを読む層
 * (`lintPermissionRules`) を tempdir fixture で固定する。リポジトリの実ファイルは push 時と
 * CI の `pnpm lint:permissions` が見る。
 */

import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import { afterEach, describe, expect, it } from "vitest";

import { checkSettingsText, checkWorkflowText, lintPermissionRules } from "./lint-permission-rules.mjs";

const settings = (permissions) => JSON.stringify({ permissions }, null, 2);

describe("checkSettingsText", () => {
  it("flags Write(path) in deny, the silent fail-open case", () => {
    const findings = checkSettingsText(settings({ deny: ["Write(master-ref/**)"] }));
    expect(findings).toHaveLength(1);
    expect(findings[0]).toContain("permissions.deny");
    expect(findings[0]).toContain("Write(master-ref/**)");
  });

  it.each(["allow", "ask"])("flags Write(path) in %s too", (list) => {
    expect(checkSettingsText(settings({ [list]: ["Write(docs/**)"] }))).toHaveLength(1);
  });

  it("reports every offending rule, not only the first", () => {
    const findings = checkSettingsText(settings({ allow: ["Write(a/**)"], deny: ["Write(b/**)", "Write(c/**)"] }));
    expect(findings).toHaveLength(3);
  });

  it("accepts Edit(path), Read(path), bare Write and other tools", () => {
    const text = settings({
      allow: ["Edit(work/**)", "Read(docs/**)", "Write", "Bash(pnpm test)", "NotebookWrite(x)"],
      deny: ["Edit(master-ref/**)"],
    });
    expect(checkSettingsText(text)).toEqual([]);
  });

  it("does not look at Write( outside permission lists", () => {
    const text = JSON.stringify({ permissions: { allow: [] }, hooks: { note: "Write(x)" } });
    expect(checkSettingsText(text)).toEqual([]);
  });

  it("accepts settings without a permissions block", () => {
    expect(checkSettingsText(JSON.stringify({ env: {} }))).toEqual([]);
  });

  it("fails closed on unparsable JSON", () => {
    const findings = checkSettingsText('{ "permissions": { "deny": ["Write(x)"] ');
    expect(findings).toHaveLength(1);
    expect(findings[0]).toContain("JSON として読めません");
  });

  it("fails closed when a permission list is not an array", () => {
    const findings = checkSettingsText(JSON.stringify({ permissions: { deny: "Write(x)" } }));
    expect(findings).toHaveLength(1);
    expect(findings[0]).toContain("配列ではありません");
  });
});

describe("checkWorkflowText", () => {
  it("flags Write(path) in claude_args --allowedTools (the pr-monitor.yml shape)", () => {
    const text = [
      "          claude_args: |",
      "            --model claude-sonnet-5",
      '            --allowedTools "Read(pr/docs/**),Edit(pr/docs/**),Write(pr/docs/**)"',
      '            --disallowedTools "Bash,WebFetch"',
    ].join("\n");
    expect(checkWorkflowText(text)).toEqual([{ line: 3, message: expect.stringContaining("--allowedTools") }]);
  });

  it("flags Write(path) in --disallowedTools", () => {
    const findings = checkWorkflowText('  --disallowedTools "Bash,Write(master-ref/**)"\n');
    expect(findings).toHaveLength(1);
    expect(findings[0].message).toContain("--disallowedTools");
  });

  it.each([
    ["kebab-case flag", 'claude -p x --allowed-tools "Write(a/**)"'],
    ["= separator", "claude --allowedTools=Write(a/**)"],
    ["variadic arguments on one line", '--allowedTools "Read" "Write(a/**)"'],
    ["single quotes", "--allowedTools 'Edit(a/**),Write(a/**)'"],
  ])("flags the %s form", (_label, line) => {
    expect(checkWorkflowText(line)).toHaveLength(1);
  });

  it("follows an argument whose quote closes on a later line", () => {
    const text = ['--allowedTools "Read(a/**),', '  Write(a/**)"', "--model x"].join("\n");
    expect(checkWorkflowText(text)).toEqual([{ line: 1, message: expect.any(String) }]);
  });

  it("follows a shell line continuation in run: blocks", () => {
    const text = ["claude -p x --allowedTools \\", '  "Edit(a/**),Write(a/**)" \\', "  --model y"].join("\n");
    expect(checkWorkflowText(text)).toEqual([{ line: 1, message: expect.any(String) }]);
  });

  it("does not follow a continuation past the argument when it is clean", () => {
    const text = ['claude --allowedTools "Edit(a/**)" \\', "  --model y", "echo Write(x)"].join("\n");
    expect(checkWorkflowText(text)).toEqual([]);
  });

  it("accepts Edit/Read scopes and bare Write", () => {
    const text = '--allowedTools "Read(pr/docs/**),Edit(pr/docs/**),Write"\n--disallowedTools "Bash"';
    expect(checkWorkflowText(text)).toEqual([]);
  });

  it("ignores Write( in comment lines (nightly-todo.yml explains the rule this way)", () => {
    const text = [
      "          # `Write(path)` 指定子はファイル権限チェックにマッチせず no-op で",
      '          # --allowedTools "Write(work/**)" のように並べても効かない',
    ].join("\n");
    expect(checkWorkflowText(text)).toEqual([]);
  });

  it("ignores Write( in a trailing shell comment (PR #622 CodeRabbit)", () => {
    expect(checkWorkflowText('claude --allowedTools "Read" # Write(x) is not used\n')).toEqual([]);
  });

  it("ignores a tools flag that only appears inside a trailing comment", () => {
    expect(checkWorkflowText('echo hi # --allowedTools "Write(x)"\n')).toEqual([]);
  });

  it("treats # inside quotes as part of the argument, not a comment", () => {
    expect(checkWorkflowText('--allowedTools "Read(#a),Write(a/**)"\n')).toHaveLength(1);
    expect(checkWorkflowText("--allowedTools Read(a)#Write(a/**)\n")).toHaveLength(1);
  });

  it("does not read an apostrophe in a trailing comment as an open quote", () => {
    const text = ['--allowedTools "Read" # don\'t add Edit here', '--disallowedTools "Write(x)"'].join("\n");
    expect(checkWorkflowText(text)).toEqual([{ line: 2, message: expect.stringContaining("--disallowedTools") }]);
  });

  it("keeps escaped quotes inside double quotes from closing the argument", () => {
    const text = ['--allowedTools "Read(a\\"b),', '  Write(a/**)"'].join("\n");
    expect(checkWorkflowText(text)).toEqual([{ line: 1, message: expect.any(String) }]);
  });

  it("does not continue a line whose backslash is inside a comment", () => {
    const text = ['--allowedTools "Read" # see \\', "Write(x)"].join("\n");
    expect(checkWorkflowText(text)).toEqual([]);
  });

  it("ignores Write( on lines without a tools flag", () => {
    expect(checkWorkflowText("prompt: Write(path) は使わないこと\n")).toEqual([]);
  });

  it("handles CRLF line endings", () => {
    expect(checkWorkflowText('a\r\n--allowedTools "Write(a/**)"\r\n')).toEqual([
      { line: 2, message: expect.any(String) },
    ]);
  });
});

describe("lintPermissionRules", () => {
  const roots = [];
  afterEach(() => {
    for (const root of roots.splice(0)) {
      rmSync(root, { recursive: true, force: true });
    }
  });

  function makeRepo({ settingsFiles = {}, workflowFiles = {} }) {
    const root = mkdtempSync(join(tmpdir(), "lint-permission-rules-"));
    roots.push(root);
    const settingsDir = join(root, ".claude");
    const workflowDir = join(root, "workflows");
    mkdirSync(settingsDir);
    mkdirSync(workflowDir);
    for (const [name, content] of Object.entries(settingsFiles)) {
      writeFileSync(join(settingsDir, name), content, "utf8");
    }
    for (const [name, content] of Object.entries(workflowFiles)) {
      writeFileSync(join(workflowDir, name), content, "utf8");
    }
    return { settingsDir, workflowDir };
  }

  const cleanSettings = settings({ allow: ["Edit(a/**)"] });

  it("passes a clean repository and counts the files it checked", () => {
    const dirs = makeRepo({
      settingsFiles: { "settings.json": cleanSettings, "settings.local.json": cleanSettings },
      workflowFiles: { "ci.yml": "jobs: {}\n" },
    });
    expect(lintPermissionRules(dirs)).toEqual({ findings: [], checked: 3 });
  });

  it("reports settings.local.json and workflow findings with their locations", () => {
    const dirs = makeRepo({
      settingsFiles: { "settings.json": cleanSettings, "settings.local.json": settings({ deny: ["Write(x)"] }) },
      workflowFiles: { "a.yml": 'x\n--allowedTools "Write(x)"\n', "b.yaml": "y\n", "notes.txt": "--allowedTools Write(x)" },
    });
    const files = lintPermissionRules(dirs).findings.map((f) => f.file);
    expect(files).toEqual([join(dirs.settingsDir, "settings.local.json"), `${join(dirs.workflowDir, "a.yml")}:2`]);
  });

  it("ignores JSON files that are not settings*.json", () => {
    const dirs = makeRepo({
      settingsFiles: { "settings.json": cleanSettings, "other.json": settings({ deny: ["Write(x)"] }) },
      workflowFiles: { "ci.yml": "" },
    });
    expect(lintPermissionRules(dirs).findings).toEqual([]);
  });

  it("fails closed when settings.json is missing", () => {
    const dirs = makeRepo({ workflowFiles: { "ci.yml": "" } });
    const { findings } = lintPermissionRules(dirs);
    expect(findings).toHaveLength(1);
    expect(findings[0].message).toContain("settings.json がありません");
  });

  it("fails closed when there are no workflow files", () => {
    const dirs = makeRepo({ settingsFiles: { "settings.json": cleanSettings } });
    const { findings } = lintPermissionRules(dirs);
    expect(findings).toHaveLength(1);
    expect(findings[0].message).toContain("workflow ファイルが 1 つもありません");
  });
});
