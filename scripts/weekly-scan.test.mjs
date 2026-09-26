import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import {
  countLines,
  findScratchFiles,
  findUnexpectedRootFiles,
  formatSize,
  normalizeFileList,
  selectLargeTodos,
  selectLongRustFiles,
  workspaceHygieneReport,
} from './weekly-scan.mjs';

const SCRIPT = fileURLToPath(new URL('./weekly-scan.mjs', import.meta.url));

describe('file-length の判定', () => {
  it('wc -l と同じく改行の数を数える', () => {
    expect(countLines('a\nb\n')).toBe(2);
    expect(countLines('a\nb')).toBe(1);
    expect(countLines('')).toBe(0);
  });

  it('800 行ちょうどは含めず、801 行から行数の降順で返す', () => {
    const picked = selectLongRustFiles([
      { path: 'src/a.rs', lines: 800 },
      { path: 'src/b.rs', lines: 801 },
      { path: 'src/c.rs', lines: 950 },
    ]);
    expect(picked).toEqual([
      { path: 'src/c.rs', lines: 950 },
      { path: 'src/b.rs', lines: 801 },
    ]);
  });

  it('todo は 49152 バイトちょうどから含める', () => {
    const picked = selectLargeTodos([
      { path: 'docs/todo1.md', bytes: 49151 },
      { path: 'docs/todo2.md', bytes: 49152 },
    ]);
    expect(picked).toEqual([{ path: 'docs/todo2.md', bytes: 49152 }]);
  });
});

describe('workspace-hygiene の判定', () => {
  it('Windows の jj が出す \\ 区切りを / にそろえる', () => {
    expect(normalizeFileList('src\\a.rs\r\nCLAUDE.md\r\n\r\n')).toEqual(['src/a.rs', 'CLAUDE.md']);
  });

  it('root 直下で allowlist に無いものだけを返す (サブディレクトリは対象外)', () => {
    const unexpected = findUnexpectedRootFiles(['CLAUDE.md', 'stray.py', 'src/stray.py']);
    expect(unexpected).toEqual(['stray.py']);
  });

  it('basename が __* / _tmp_* のものを whole-tree で拾う', () => {
    const scratch = findScratchFiles(['__notes.md', 'src/_tmp_x.rs', 'src/a__b.rs', 'docs/__/ok.md']);
    expect(scratch).toEqual(['__notes.md', 'src/_tmp_x.rs']);
  });

  it('du -sh に近い単位で表す', () => {
    expect(formatSize(512)).toBe('512B');
    expect(formatSize(1536)).toBe('1.5K');
    expect(formatSize(300 * 1024 * 1024)).toBe('300M');
  });

  it('一覧の取得に失敗したら 0 件ではなく未実施と書く', () => {
    const report = workspaceHygieneReport({
      listFiles: () => ({ ok: false, reason: 'exit 1' }),
      exists: () => false,
    });
    expect(report).toContain('jj_file_list: FAILED');
    expect(report).not.toContain('(0 件)');
    expect(report.match(/\(未実施: jj file list 失敗 — exit 1\)/g)).toHaveLength(2);
  });

  it('検出 0 件と、不在のディレクトリを区別して書く', () => {
    const report = workspaceHygieneReport({
      listFiles: () => ({ ok: true, paths: ['CLAUDE.md', 'src/a.rs'] }),
      exists: (dir) => dir === 'target',
      sizeOf: () => 2048,
    });
    expect(report).toBe(
      [
        '### scan-status',
        'jj_file_list: OK',
        '### root-unexpected (allowlist 突合)',
        '(0 件)',
        '### scratch-pattern (__* / _tmp_*)',
        '(0 件)',
        '### ignored-size (報告のみ)',
        '(不在: .takt/runs — cloud 実行では正常)',
        '2.0K\ttarget',
      ].join('\n'),
    );
  });

  it('サイズの取得に失敗したら握り潰さず取得失敗と書く', () => {
    const report = workspaceHygieneReport({
      listFiles: () => ({ ok: true, paths: [] }),
      exists: () => true,
      sizeOf: () => {
        throw new Error('EACCES');
      },
    });
    expect(report).toContain('(取得失敗: target — EACCES)');
  });
});

describe('workspace-hygiene を実際に起動する', () => {
  let dir;
  afterEach(() => {
    if (dir) rmSync(dir, { recursive: true, force: true });
  });

  /** jj リポジトリでない場所では、jj があってもなくても一覧の取得に失敗する。 */
  it('jj の一覧が取れない場所でも落ちず、各検査を未実施と書く', () => {
    dir = mkdtempSync(join(tmpdir(), 'weekly-scan-hygiene-'));

    const result = spawnSync(process.execPath, [SCRIPT, 'workspace-hygiene'], {
      cwd: dir,
      encoding: 'utf8',
      timeout: 60_000,
    });

    expect(result.status).toBe(0);
    const lines = result.stdout.trimEnd().split('\n');
    expect(lines.slice(0, 2)).toEqual(['### scan-status', 'jj_file_list: FAILED']);
    expect(lines.filter((l) => l.startsWith('(未実施: jj file list 失敗'))).toHaveLength(2);
    expect(lines).not.toContain('(0 件)');
    expect(lines.slice(-2)).toEqual([
      '(不在: .takt/runs — cloud 実行では正常)',
      '(不在: target — cloud 実行では正常)',
    ]);
  });
});

describe('file-length を実際に起動する', () => {
  let dir;
  afterEach(() => {
    if (dir) rmSync(dir, { recursive: true, force: true });
  });

  it('閾値を超えたファイルだけを見出しの下に出し、target/ 配下は数えない', () => {
    dir = mkdtempSync(join(tmpdir(), 'weekly-scan-'));
    mkdirSync(join(dir, 'src', 'pkg'), { recursive: true });
    mkdirSync(join(dir, 'src', 'target'), { recursive: true });
    mkdirSync(join(dir, 'docs'), { recursive: true });
    writeFileSync(join(dir, 'src', 'pkg', 'long.rs'), 'x\n'.repeat(801));
    writeFileSync(join(dir, 'src', 'pkg', 'short.rs'), 'x\n'.repeat(800));
    writeFileSync(join(dir, 'src', 'target', 'generated.rs'), 'x\n'.repeat(900));
    writeFileSync(join(dir, 'docs', 'todo9.md'), 'a'.repeat(50000));
    writeFileSync(join(dir, 'docs', 'todo8.md'), 'a'.repeat(100));

    const result = spawnSync(process.execPath, [SCRIPT, 'file-length'], {
      cwd: dir,
      encoding: 'utf8',
      timeout: 30_000,
    });

    expect(result.status).toBe(0);
    expect(result.stdout).toBe(
      [
        '### rs-lines (>800)',
        '801 src/pkg/long.rs',
        '### todo-bytes (>=49152 = 48KB, 閾値 50KB=51200)',
        '50000 docs/todo9.md',
        '',
      ].join('\n'),
    );
  });

  it('読めないディレクトリがあっても落ちず、scan-errors に相対パスで報告する', () => {
    dir = mkdtempSync(join(tmpdir(), 'weekly-scan-'));
    writeFileSync(join(dir, 'src'), 'src がディレクトリでなくファイルなので readdir が失敗する');

    const result = spawnSync(process.execPath, [SCRIPT, 'file-length'], {
      cwd: dir,
      encoding: 'utf8',
      timeout: 30_000,
    });

    expect(result.status).toBe(0);
    const lines = result.stdout.trimEnd().split('\n');
    expect(lines.slice(0, 3)).toEqual([
      '### rs-lines (>800)',
      '### todo-bytes (>=49152 = 48KB, 閾値 50KB=51200)',
      '### scan-errors',
    ]);
    expect(lines[3]).toMatch(/^\(取得失敗: src — ENOTDIR/);
    expect(lines).toHaveLength(4);
  });

  it('不明なモードは使い方を出して終了コード 2', () => {
    const result = spawnSync(process.execPath, [SCRIPT, 'nope'], { encoding: 'utf8', timeout: 30_000 });
    expect(result.status).toBe(2);
    expect(result.stderr).toContain('usage:');
  });
});
