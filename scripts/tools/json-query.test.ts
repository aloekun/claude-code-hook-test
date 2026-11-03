/**
 * `scripts/tools/json-query.ts` のテスト。実ファイルは読まず、文字列を渡して判定だけを固定する。
 */

import { describe, expect, it } from "vitest";

import { UsageError, getPath, parseArgs, parseRecords, query } from "./json-query.ts";

const run = (records: unknown[], args: string[]) => query(records, parseArgs(["x.json", ...args]));

const RECORDS = [
  { type: "user", tool: "Bash", input: { command: "ls" } },
  { type: "assistant", tool: "Bash", input: { command: "python -c 1" } },
  { type: "assistant", tool: "Edit", input: { file: "a.rs" } },
];

describe("getPath", () => {
  it("follows dotted keys and array indexes", () => {
    expect(getPath({ a: [{ b: 1 }] }, "a.0.b")).toBe(1);
  });

  it("returns undefined for a missing branch", () => {
    expect(getPath({ a: 1 }, "a.b.c")).toBeUndefined();
  });
});

describe("parseRecords", () => {
  it("reads one record per line from .jsonl, skipping blank lines", () => {
    expect(parseRecords('{"a":1}\r\n\n{"a":2}\n', "t.jsonl")).toEqual([{ a: 1 }, { a: 2 }]);
  });

  it("reports the line number of an unreadable .jsonl line instead of skipping it", () => {
    expect(() => parseRecords('{"a":1}\n{broken\n', "t.jsonl")).toThrow("t.jsonl:2");
  });

  it("uses --root to reach the array inside a JSON document", () => {
    expect(parseRecords('{"data":{"items":[1,2]}}', "t.json", "data.items")).toEqual([1, 2]);
  });

  it("fails when --root does not exist", () => {
    expect(() => parseRecords('{"data":{}}', "t.json", "data.items")).toThrow("--root data.items");
  });

  it("wraps a non-array document as a single record", () => {
    expect(parseRecords('{"a":1}', "t.json")).toEqual([{ a: 1 }]);
  });
});

describe("query", () => {
  it("combines --where and --match with AND", () => {
    const { lines } = run(RECORDS, ["--where", "tool=Bash", "--match", "input.command=^python"]);
    expect(lines).toEqual([JSON.stringify(RECORDS[1])]);
  });

  it("--select prints only the chosen keys, null for missing ones", () => {
    const { lines } = run(RECORDS, ["--select", "tool,input.file"]);
    expect(lines).toEqual([
      '{"tool":"Bash","input.file":null}',
      '{"tool":"Bash","input.file":null}',
      '{"tool":"Edit","input.file":"a.rs"}',
    ]);
  });

  it("--count prints the number of matching records", () => {
    expect(run(RECORDS, ["--count", "--where", "type=assistant"]).lines).toEqual(["2"]);
  });

  it("--group-by counts per value, most frequent first", () => {
    expect(run(RECORDS, ["--group-by", "tool"]).lines).toEqual(["2\tBash", "1\tEdit"]);
  });

  it("--limit truncates and reports how many were omitted", () => {
    expect(run(RECORDS, ["--limit", "1"])).toEqual({ lines: [JSON.stringify(RECORDS[0])], omitted: 2 });
  });
});

describe("parseArgs", () => {
  it.each([
    [[], "ファイルを指定"],
    [["a.json", "b.json"], "1 つだけ"],
    [["a.json", "--where", "novalue"], "<key>=<value>"],
    [["a.json", "--limit", "-1"], "0 以上の整数"],
    [["a.json", "--count", "--group-by", "k"], "同時に使えません"],
    [["a.json", "--unknown"], "未知のオプション"],
  ])("rejects %j", (args, message) => {
    expect(() => parseArgs(args)).toThrow(UsageError);
    expect(() => parseArgs(args)).toThrow(message);
  });
});
