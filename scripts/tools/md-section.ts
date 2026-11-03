/**
 * Markdown から見出しの節を取り出して表示する調査用ツール。
 *
 * 台帳 (`docs/todo*.md`) や ADR の「### 順位 552: ...」のような節を、行番号付きで丸ごと読むために使う。
 * 節を消すときは、ここで表示した内容を Edit で消す。実行コマンドは `.claude/settings.json` の
 * 許可リストにあり、確認なしで走る。そのため、**ファイルを読んで標準出力に書くことしかしない** —
 * 書き込み・子プロセス・ネットワークは使わない。読める場所も `read-scope.ts` の 3 か所
 * (リポジトリ / scratchpad / transcript) に限る。
 *
 * 使い方:
 *   pnpm -s tool:md-section <file> <heading>   見出しが <heading> で始まる節を表示する
 *   pnpm -s tool:md-section <file> --list      見出しの一覧を `行番号<TAB>見出し行` で表示する
 *
 * 節は見出し行から、同じかそれより上の階層の次の見出しの手前まで。<heading> は `#` を除いた見出しの
 * 先頭との一致で、直後が文字・数字なら一致としない (`順位 55` は `順位 552` に一致しない)。
 * コードブロック (``` / ~~~) の中の `#` 行は見出しとして扱わない。該当が無ければ終了コード 1。
 */

import { readFileSync, realpathSync } from "node:fs";
import { pathToFileURL } from "node:url";

import { assertReadable } from "./read-scope.ts";

export type Heading = { line: number; level: number; text: string };

export type Section = { start: number; end: number; lines: string[] };

export type Args = { file: string; query: string };

export class UsageError extends Error {}

/** `<file> <heading>` か `<file> --list` を受け取る。それ以外は UsageError。 */
export function parseArgs(args: string[]): Args {
  const [file, query, ...rest] = args;
  if (file === undefined || query === undefined || rest.length > 0) {
    throw new UsageError("使い方: pnpm -s tool:md-section <file> <heading> | --list");
  }
  return { file, query };
}

/** 見出しの一覧 (行番号は 1 始まり)。コードブロックの中は除く。 */
export function headings(lines: string[]): Heading[] {
  const found: Heading[] = [];
  let fence: string | null = null;
  lines.forEach((line, index) => {
    if (fence === null) {
      const open = /^ {0,3}(`{3,}|~{3,})/.exec(line)?.[1];
      if (open !== undefined) {
        fence = open;
        return;
      }
    } else {
      // 閉じフェンスは開始と同じ文字で、開始以上の長さ、後ろに情報文字列を持たない行 (CommonMark)
      const close = /^ {0,3}(`{3,}|~{3,})\s*$/.exec(line)?.[1];
      if (close !== undefined && close[0] === fence[0] && close.length >= fence.length) fence = null;
      return;
    }
    const m = /^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$/.exec(line);
    if (m) found.push({ line: index + 1, level: m[1].length, text: m[2] });
  });
  return found;
}

/** 見出しの本文が `query` で始まり、直後が文字・数字でないか。 */
export function headingMatches(text: string, query: string): boolean {
  if (!text.startsWith(query)) return false;
  const next = text.charAt(query.length);
  return next === "" || !/[\p{L}\p{N}_]/u.test(next);
}

/** `query` に一致する節をすべて返す。 */
export function findSections(lines: string[], query: string): Section[] {
  const all = headings(lines);
  return all
    .filter((h) => headingMatches(h.text, query))
    .map((h) => {
      const next = all.find((o) => o.line > h.line && o.level <= h.level);
      const end = next === undefined ? lines.length : next.line - 1;
      return { start: h.line, end, lines: lines.slice(h.line - 1, end) };
    });
}

function main(): void {
  try {
    const { file, query } = parseArgs(process.argv.slice(2));
    const realFile = assertReadable(file);
    const lines = readFileSync(realFile, "utf8").split(/\r?\n/);
    if (query === "--list") {
      for (const h of headings(lines)) process.stdout.write(`${h.line}\t${"#".repeat(h.level)} ${h.text}\n`);
      return;
    }
    const sections = findSections(lines, query);
    if (sections.length === 0) {
      process.stderr.write(`md-section: 「${query}」で始まる見出しが ${file} にありません\n`);
      process.exitCode = 1;
      return;
    }
    for (const s of sections) {
      process.stdout.write(`== ${file}:${s.start}-${s.end}\n`);
      s.lines.forEach((line, i) => process.stdout.write(`${s.start + i}\t${line}\n`));
    }
  } catch (e) {
    process.stderr.write(`md-section: ${(e as Error).message}\n`);
    process.exitCode = e instanceof UsageError ? 2 : 1;
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(realpathSync(process.argv[1])).href) {
  main();
}
