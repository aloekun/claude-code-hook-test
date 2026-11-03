/**
 * JSON / JSONL を読み、条件で絞って表示・集計する調査用ツール。
 *
 * python や jq の代わりに使う。このリポジトリは PreToolUse で python などのランタイムを止めており、
 * transcript や telemetry の JSONL、調査で書き出した JSON を読む手段をここに用意する。
 * 実行コマンドは `.claude/settings.json` の許可リストにあり、確認なしで走る。そのため、
 * **ファイルを読んで標準出力に書くことしかしない** — 書き込み・子プロセス・ネットワークは使わない。
 * 読める場所も `read-scope.ts` の 3 か所 (リポジトリ / scratchpad / transcript) に限る。
 *
 * 使い方:
 *   pnpm -s tool:json-query <file> [options]
 *
 *   --root <path>        JSON の中で、レコードの配列がある位置 (例: `data.items`)
 *   --where <key>=<val>  key の値 (文字列にしたもの) が val と一致するものだけ残す。複数指定は AND
 *   --match <key>=<re>   key の値 (文字列にしたもの) が正規表現 re に一致するものだけ残す。複数指定は AND
 *   --select <k1,k2>     各レコードから指定した key だけを 1 行 1 JSON で出す
 *   --count              件数だけを出す
 *   --group-by <key>     key の値ごとの件数を、多い順に `件数<TAB>値` で出す
 *   --limit <n>          出す件数の上限。切った場合は stderr に残りの件数を出す
 *
 * key はドット区切りのパス (`message.content.0.type`)。配列の要素は番号で指す。
 * 拡張子が `.jsonl` / `.ndjson` なら 1 行 1 レコード、それ以外は JSON として読む。
 * 読めない行は黙って飛ばさず、行番号を付けてエラーにする。
 */

import { readFileSync, realpathSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { assertReadable } from "./read-scope.ts";

export type Filter = { key: string; kind: "where" | "match"; value: string; re?: RegExp };

export type Options = {
  file: string;
  root?: string;
  filters: Filter[];
  select?: string[];
  count: boolean;
  groupBy?: string;
  limit?: number;
};

export class UsageError extends Error {}

/** ドット区切りのパスで値を取り出す。途中で無ければ `undefined`。 */
export function getPath(value: unknown, path: string): unknown {
  let current: unknown = value;
  for (const part of path.split(".")) {
    if (current === null || typeof current !== "object") return undefined;
    current = (current as Record<string, unknown>)[part];
  }
  return current;
}

/** 比較・表示用の文字列。オブジェクトと配列は JSON にする。 */
export function asText(value: unknown): string {
  if (value === undefined) return "";
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}

function splitPair(arg: string, flag: string): [string, string] {
  const at = arg.indexOf("=");
  if (at <= 0) throw new UsageError(`${flag} は <key>=<value> の形で指定してください: ${arg}`);
  return [arg.slice(0, at), arg.slice(at + 1)];
}

function requireValue(args: string[], i: number, flag: string): string {
  const value = args[i + 1];
  if (value === undefined) throw new UsageError(`${flag} に値がありません`);
  return value;
}

export function parseArgs(args: string[]): Options {
  const options: Options = { file: "", filters: [], count: false };
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    switch (arg) {
      case "--root":
        options.root = requireValue(args, i++, arg);
        break;
      case "--where": {
        const [key, value] = splitPair(requireValue(args, i++, arg), arg);
        options.filters.push({ key, kind: "where", value });
        break;
      }
      case "--match": {
        const [key, value] = splitPair(requireValue(args, i++, arg), arg);
        options.filters.push({ key, kind: "match", value, re: new RegExp(value) });
        break;
      }
      case "--select":
        options.select = requireValue(args, i++, arg).split(",").filter((k) => k !== "");
        break;
      case "--count":
        options.count = true;
        break;
      case "--group-by":
        options.groupBy = requireValue(args, i++, arg);
        break;
      case "--limit": {
        const n = Number(requireValue(args, i++, arg));
        if (!Number.isInteger(n) || n < 0) throw new UsageError(`--limit は 0 以上の整数です: ${args[i]}`);
        options.limit = n;
        break;
      }
      default:
        if (arg.startsWith("--")) throw new UsageError(`未知のオプションです: ${arg}`);
        if (options.file !== "") throw new UsageError(`ファイルは 1 つだけ指定してください: ${arg}`);
        options.file = arg;
    }
  }
  if (options.file === "") throw new UsageError("ファイルを指定してください");
  if (options.count && options.groupBy !== undefined) {
    throw new UsageError("--count と --group-by は同時に使えません");
  }
  return options;
}

/** ファイルの中身をレコードの配列にする。 */
export function parseRecords(text: string, file: string, root?: string): unknown[] {
  if (/\.(jsonl|ndjson)$/i.test(file)) {
    const records: unknown[] = [];
    text.split(/\r?\n/).forEach((line, index) => {
      if (line.trim() === "") return;
      try {
        records.push(JSON.parse(line));
      } catch (e) {
        throw new Error(`${file}:${index + 1} を JSON として読めません: ${(e as Error).message}`);
      }
    });
    return records;
  }
  const parsed: unknown = JSON.parse(text);
  const target = root === undefined ? parsed : getPath(parsed, root);
  if (root !== undefined && target === undefined) {
    throw new Error(`--root ${root} が ${file} にありません`);
  }
  return Array.isArray(target) ? target : [target];
}

export function matches(record: unknown, filters: Filter[]): boolean {
  return filters.every((f) => {
    const text = asText(getPath(record, f.key));
    return f.kind === "where" ? text === f.value : (f.re as RegExp).test(text);
  });
}

/** 出力する行と、`--limit` で切った件数を返す。 */
export function query(records: unknown[], options: Options): { lines: string[]; omitted: number } {
  const hits = records.filter((r) => matches(r, options.filters));
  if (options.count) return { lines: [String(hits.length)], omitted: 0 };

  let lines: string[];
  if (options.groupBy !== undefined) {
    const counts = new Map<string, number>();
    for (const r of hits) {
      const key = asText(getPath(r, options.groupBy));
      counts.set(key, (counts.get(key) ?? 0) + 1);
    }
    lines = [...counts.entries()]
      .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
      .map(([key, n]) => `${n}\t${key}`);
  } else {
    const select = options.select;
    lines = hits.map((r) =>
      JSON.stringify(
        select === undefined ? r : Object.fromEntries(select.map((k) => [k, getPath(r, k) ?? null])),
      ),
    );
  }
  if (options.limit === undefined || lines.length <= options.limit) return { lines, omitted: 0 };
  return { lines: lines.slice(0, options.limit), omitted: lines.length - options.limit };
}

function main(): void {
  try {
    const options = parseArgs(process.argv.slice(2));
    const realFile = assertReadable(options.file);
    const records = parseRecords(readFileSync(realFile, "utf8"), options.file, options.root);
    const { lines, omitted } = query(records, options);
    for (const line of lines) process.stdout.write(`${line}\n`);
    if (omitted > 0) process.stderr.write(`(--limit で ${omitted} 件を省略しました)\n`);
  } catch (e) {
    process.stderr.write(`json-query: ${(e as Error).message}\n`);
    process.exitCode = e instanceof UsageError ? 2 : 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  main();
}
