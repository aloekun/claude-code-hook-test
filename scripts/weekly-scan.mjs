#!/usr/bin/env node
// weekly-review の機械 scan (file-length-watchlist / workspace-hygiene-scan step) の本体。
//
// 以前は facet に書いた固定の shell script を agent が Bash で実行していた。その script は
// `find -exec` や `if FILES="$(...)"` を含み、コマンド単位の許可 (`Bash(<prefix>:*)`) では
// 「静的に解析できない」として拒否されるため、step に素の Bash を残すしかなかった
// (ADR-083 § 既知の限界)。本 script に移すことで、step は `pnpm weekly-scan:<mode>` だけを
// 許可すればよくなる。
//
// 出力は旧 shell script と同じ section 見出し・行形式を保つ (facet の Phase 2 整形が依存する)。
//
// Usage: node scripts/weekly-scan.mjs <file-length|workspace-hygiene>

import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, readFileSync, statSync, lstatSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

export const RS_LINE_THRESHOLD = 800;
export const TODO_BYTE_WARN = 49152;

// root 直下に置いてよいファイル。root に正当なファイルを足す PR では、同じ PR でここにも足す。
// 突合は完全一致 (パターン解釈による誤除外は起きない)。
export const ROOT_ALLOWLIST = [
  '.coderabbit.yaml',
  '.gitignore',
  '.markdownlint-cli2.jsonc',
  'CLAUDE.md',
  'Cargo.lock',
  'Cargo.toml',
  'README.md',
  'autonomy-config.toml',
  'package.json',
  'pnpm-lock.yaml',
  'pr-monitor-config.toml',
  'push-runner-config.toml',
  'tsconfig.json',
];

export const IGNORED_DIRS = ['.takt/runs', 'target'];

const JJ_TIMEOUT_MS = 60_000;

/** `wc -l` と同じく改行の数を数える。 */
export function countLines(text) {
  let count = 0;
  for (let i = 0; i < text.length; i++) if (text.charCodeAt(i) === 10) count++;
  return count;
}

/** 閾値を超えた `{ path, lines }` を行数の降順で返す。 */
export function selectLongRustFiles(entries, threshold = RS_LINE_THRESHOLD) {
  return entries.filter((e) => e.lines > threshold).sort((a, b) => b.lines - a.lines || a.path.localeCompare(b.path));
}

/** 警告閾値以上の `{ path, bytes }` をバイト数の降順で返す。 */
export function selectLargeTodos(entries, threshold = TODO_BYTE_WARN) {
  return entries.filter((e) => e.bytes >= threshold).sort((a, b) => b.bytes - a.bytes || a.path.localeCompare(b.path));
}

/** `jj file list` の出力を `/` 区切りのパス配列にする (Windows は `\` 区切りで出す)。 */
export function normalizeFileList(output) {
  return output
    .split(/\r?\n/)
    .map((line) => line.trim().replaceAll('\\', '/'))
    .filter((line) => line.length > 0);
}

/** root 直下のファイルのうち allowlist に無いもの。 */
export function findUnexpectedRootFiles(paths, allowlist = ROOT_ALLOWLIST) {
  const allowed = new Set(allowlist);
  return paths.filter((p) => !p.includes('/') && !allowed.has(p));
}

/** basename が `__*` / `_tmp_*` のもの (push-runner の scratch 検査と同じ pattern)。 */
export function findScratchFiles(paths) {
  return paths.filter((p) => /(^|\/)(__|_tmp_)[^/]*$/.test(p));
}

/** `du -sh` に近い読み方の人間向けサイズ (1024 単位)。 */
export function formatSize(bytes) {
  const units = ['B', 'K', 'M', 'G', 'T'];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit++;
  }
  return unit === 0 ? `${value}B` : `${value < 10 ? value.toFixed(1) : Math.round(value)}${units[unit]}`;
}

/**
 * ディレクトリ配下のファイルサイズの合計。symlink は辿らない。
 *
 * ファイルの見かけのサイズの合計であり、`du` (ディスク上のブロック使用量、NTFS 圧縮も反映) とは
 * 1〜2 割ずれる (2026-09-26 実測: `.takt/runs` 339M 対 du 375M / `target` 19G 対 du 17G)。
 * 報告専用の値なので差は許容し、`du` の有無に依存しないことを優先する。
 */
export function directorySize(dir) {
  let total = 0;
  const stack = [dir];
  while (stack.length > 0) {
    const current = stack.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const path = join(current, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory()) stack.push(path);
      else if (entry.isFile()) total += lstatSync(path).size;
    }
  }
  return total;
}

/**
 * `dir` 配下を再帰的に走査し `predicate` に合うファイルパスを返す。
 *
 * 個別ディレクトリの `readdirSync` が失敗しても (権限エラー、走査中の削除等)、旧 shell 実装
 * (`2>/dev/null`) と同じくそのディレクトリだけを諦めて走査を続ける。失敗は `onError` に渡す。
 */
function walkFiles(dir, predicate, skipDir, onError) {
  const found = [];
  if (!existsSync(dir)) return found;
  const stack = [dir];
  while (stack.length > 0) {
    const current = stack.pop();
    let entries;
    try {
      entries = readdirSync(current, { withFileTypes: true });
    } catch (e) {
      onError(current.replaceAll('\\', '/'), e);
      continue;
    }
    for (const entry of entries) {
      const path = join(current, entry.name);
      if (entry.isDirectory()) {
        if (!skipDir(entry.name)) stack.push(path);
      } else if (entry.isFile() && predicate(entry.name)) {
        found.push(path.replaceAll('\\', '/'));
      }
    }
  }
  return found;
}

/**
 * `.rs` の行数と `docs/todo*.md` のバイト数の watchlist。
 *
 * 読めなかったディレクトリ・ファイルは飛ばして走査を続け、`### scan-errors` section に
 * `(取得失敗: <root からの相対パス> — <理由>)` として出す。旧 shell 実装の `2>/dev/null` は
 * 失敗を黙って捨てていたが、ここでは捨てずに報告する (workspace-hygiene と同じく、失敗を
 * 0 件に見せない)。section は失敗があるときだけ出る。
 */
export function fileLengthReport(root = '.') {
  const errors = [];
  const onError = (path, e) =>
    errors.push(`(取得失敗: ${relative(root, path.replaceAll('\\', '/'))} — ${e.message})`);

  const rustFiles = [];
  for (const path of walkFiles(join(root, 'src'), (n) => n.endsWith('.rs'), (n) => n === 'target', onError)) {
    try {
      rustFiles.push({ path: relative(root, path), lines: countLines(readFileSync(path, 'utf8')) });
    } catch (e) {
      onError(path, e);
    }
  }

  const docsDir = join(root, 'docs');
  let docEntries = [];
  if (existsSync(docsDir)) {
    try {
      docEntries = readdirSync(docsDir, { withFileTypes: true });
    } catch (e) {
      onError(docsDir, e);
    }
  }
  const todos = [];
  for (const e of docEntries.filter((entry) => entry.isFile() && /^todo.*\.md$/.test(entry.name))) {
    const path = join(docsDir, e.name);
    try {
      todos.push({ path: relative(root, path.replaceAll('\\', '/')), bytes: statSync(path).size });
    } catch (err) {
      onError(path, err);
    }
  }

  const lines = ['### rs-lines (>800)'];
  for (const f of selectLongRustFiles(rustFiles)) lines.push(`${f.lines} ${f.path}`);
  lines.push('### todo-bytes (>=49152 = 48KB, 閾値 50KB=51200)');
  for (const f of selectLargeTodos(todos)) lines.push(`${f.bytes} ${f.path}`);
  if (errors.length > 0) {
    lines.push('### scan-errors');
    lines.push(...errors);
  }
  return lines.join('\n');
}

function relative(root, path) {
  const prefix = root.replaceAll('\\', '/').replace(/\/$/, '');
  return prefix === '.' ? path.replace(/^\.\//, '') : path.slice(prefix.length + 1);
}

/** `jj file list -r @` を実行する。失敗は `{ ok: false, reason }`。 */
export function listTrackedFiles(cwd = '.') {
  const result = spawnSync('jj', ['file', 'list', '-r', '@'], {
    cwd,
    encoding: 'utf8',
    timeout: JJ_TIMEOUT_MS,
    maxBuffer: 64 * 1024 * 1024,
  });
  if (result.error) return { ok: false, reason: result.error.message };
  if (result.status !== 0) return { ok: false, reason: `exit ${result.status}` };
  return { ok: true, paths: normalizeFileList(result.stdout) };
}

/**
 * workspace hygiene の report 本文。`listFiles` / `sizeOf` / `exists` は差し替え可能にしてある
 * (jj の無いテスト環境でも判定と失敗時の出力を検査するため)。
 *
 * **scan 失敗を 0 件として扱わない**: 一覧の取得に失敗したら、一覧を使う 2 検査 (root-unexpected /
 * scratch-pattern) を `(未実施: ...)` とする。ignored-size は一覧を使わないので続けて出す。
 */
export function workspaceHygieneReport({
  listFiles = () => listTrackedFiles('.'),
  sizeOf = directorySize,
  exists = existsSync,
} = {}) {
  const listed = listFiles();
  const lines = ['### scan-status', `jj_file_list: ${listed.ok ? 'OK' : 'FAILED'}`];
  const notRun = `(未実施: jj file list 失敗${listed.ok ? '' : ` — ${listed.reason}`})`;

  lines.push('### root-unexpected (allowlist 突合)');
  if (listed.ok) {
    const unexpected = findUnexpectedRootFiles(listed.paths);
    lines.push(...(unexpected.length > 0 ? unexpected : ['(0 件)']));
  } else {
    lines.push(notRun);
  }

  lines.push('### scratch-pattern (__* / _tmp_*)');
  if (listed.ok) {
    const scratch = findScratchFiles(listed.paths);
    lines.push(...(scratch.length > 0 ? scratch : ['(0 件)']));
  } else {
    lines.push(notRun);
  }

  lines.push('### ignored-size (報告のみ)');
  for (const dir of IGNORED_DIRS) {
    if (!exists(dir)) {
      lines.push(`(不在: ${dir} — cloud 実行では正常)`);
      continue;
    }
    try {
      lines.push(`${formatSize(sizeOf(dir))}\t${dir}`);
    } catch (e) {
      lines.push(`(取得失敗: ${dir} — ${e.message})`);
    }
  }
  return lines.join('\n');
}

function main(argv) {
  const mode = argv[2];
  try {
    if (mode === 'file-length') {
      process.stdout.write(`${fileLengthReport('.')}\n`);
      return 0;
    }
    if (mode === 'workspace-hygiene') {
      process.stdout.write(`${workspaceHygieneReport()}\n`);
      return 0;
    }
  } catch (e) {
    process.stderr.write(`weekly-scan failed: ${e.message}\n`);
    return 1;
  }
  process.stderr.write('usage: node scripts/weekly-scan.mjs <file-length|workspace-hygiene>\n');
  return 2;
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  process.exitCode = main(process.argv);
}
