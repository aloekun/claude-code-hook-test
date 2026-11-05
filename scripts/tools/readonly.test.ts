/**
 * `scripts/tools/` のツールが読み取り専用であることを固定する。
 *
 * これらのツールの実行コマンドは `.claude/settings.json` の許可リストにあり、確認なしで走る。
 * 書き込み・子プロセス・ネットワークを使えるようになると、許可リストが確認を素通りする経路になる。
 * そこで、ツールが import してよいものを「ファイルの読み取り」と「パスの変換」だけに限る。
 * 同じディレクトリのモジュール (`./read-scope.ts` など) は import してよい。それ自身もこの検査を受ける。
 *
 * import を経ずに届く経路も拒否する (順位 596)。
 * - 組み込みモジュールの取得: `process.getBuiltinModule("node:child_process")`
 * - 内部 binding とネイティブモジュール: `process.binding` / `process._linkedBinding` / `process.dlopen`
 * - import の要らないネットワーク API: `fetch` / `WebSocket` / `XMLHttpRequest` / `EventSource`
 * - 文字列からのコードの組み立て: `eval` / `new Function` (上の名前を文字列にして検査をすり抜けられる)
 *
 * 検査はソースの字面で名前を探す。自分たちで書くツールに誤って入るのを止めるためのもので、
 * `globalThis["fe" + "tch"]` のような意図的な難読化までは見ない。
 */

import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

import { describe, expect, it } from "vitest";

const TOOLS_DIR = fileURLToPath(new URL(".", import.meta.url));

/** module ごとに import してよい名前。 */
const ALLOWED_IMPORTS: Record<string, string[]> = {
  "node:fs": ["readFileSync", "readdirSync", "realpathSync"],
  "node:os": ["homedir", "tmpdir"],
  "node:path": ["join", "relative"],
  "node:url": ["fileURLToPath", "pathToFileURL"],
};

/** 同じディレクトリの .ts (サブディレクトリやテストは含めない)。 */
const SIBLING_MODULE = /^\.\/[\w-]+\.ts$/;

const toolFiles = () =>
  readdirSync(TOOLS_DIR).filter((f) => f.endsWith(".ts") && !f.endsWith(".test.ts"));

/** `import { a, b } from "m";` を [m, [a, b]] の並びにする。それ以外の import の形は null。 */
function importsOf(source: string): Array<[string, string[] | null]> {
  const found: Array<[string, string[] | null]> = [];
  for (const m of source.matchAll(/^\s*import\s+([\s\S]*?)\s+from\s+["']([^"']+)["']/gm)) {
    const names = /^\{([^}]*)\}$/.exec(m[1].trim());
    found.push([m[2], names ? names[1].split(",").map((n) => n.trim()).filter((n) => n !== "") : null]);
  }
  return found;
}

function violations(source: string): string[] {
  const found: string[] = [];
  for (const [module, names] of importsOf(source)) {
    if (SIBLING_MODULE.test(module)) continue;
    const allowed = ALLOWED_IMPORTS[module];
    if (allowed === undefined) found.push(`${module} は import できない`);
    else if (names === null) found.push(`${module} は名前を指定して import する`);
    else for (const n of names) if (!allowed.includes(n)) found.push(`${module} の ${n} は使えない`);
  }
  if (/\bimport\s*\(|\brequire\s*\(/.test(source)) found.push("動的 import / require は使えない");
  for (const [pattern, message] of FORBIDDEN_GLOBALS) if (pattern.test(source)) found.push(message);
  return found;
}

/** import を経ずに子プロセス・ネイティブコード・ネットワーク・任意のコードへ届く経路。 */
const FORBIDDEN_GLOBALS: Array<[RegExp, string]> = [
  [/\bgetBuiltinModule\b/, "getBuiltinModule は使えない"],
  [/\bprocess\s*\.\s*(?:binding|_linkedBinding|dlopen)\b/, "process の binding / dlopen は使えない"],
  [/\b(?:fetch|WebSocket|XMLHttpRequest|EventSource)\b/, "ネットワーク API は使えない"],
  [/\beval\s*\(|\bFunction\s*\(/, "eval / Function は使えない"],
];

describe("scripts/tools", () => {
  it("has tools to check (false-green guard)", () => {
    expect(toolFiles().length).toBeGreaterThan(0);
  });

  it.each(toolFiles())("%s imports only read-only APIs", (file) => {
    expect(violations(readFileSync(join(TOOLS_DIR, file), "utf8"))).toEqual([]);
  });

  it.each([
    ['import { writeFileSync } from "node:fs";', "node:fs の writeFileSync は使えない"],
    ['import { spawnSync } from "node:child_process";', "node:child_process は import できない"],
    ['import * as fs from "node:fs";', "node:fs は名前を指定して import する"],
    ['const cp = await import("node:child_process");', "動的 import / require は使えない"],
    ['const cp = process.getBuiltinModule("node:child_process");', "getBuiltinModule は使えない"],
    ['const { getBuiltinModule } = process; getBuiltinModule("node:net");', "getBuiltinModule は使えない"],
    ['const w = process.binding("spawn_wrap");', "process の binding / dlopen は使えない"],
    ['process._linkedBinding("x");', "process の binding / dlopen は使えない"],
    ['process.dlopen(module, "x.node");', "process の binding / dlopen は使えない"],
    ['await fetch("https://example.invalid/");', "ネットワーク API は使えない"],
    ['new WebSocket("wss://example.invalid/");', "ネットワーク API は使えない"],
    ["const x = new XMLHttpRequest();", "ネットワーク API は使えない"],
    ['new EventSource("https://example.invalid/");', "ネットワーク API は使えない"],
    ['eval("1");', "eval / Function は使えない"],
    ['const f = new Function("return 1");', "eval / Function は使えない"],
  ])("rejects %s", (source, message) => {
    expect(violations(source)).toEqual([message]);
  });
});
