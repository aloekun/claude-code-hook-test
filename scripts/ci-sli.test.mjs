import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  MIN_SAMPLES,
  STATUS_PATH,
  THRESHOLD_SECS,
  classify,
  evaluate,
  formatReport,
  main,
  median,
  resolveMainRoot,
  runWallSeconds,
  writeStatus,
} from './ci-sli.mjs';

const runs = (walls) =>
  walls.map((wall, i) => ({
    id: 1000 + i,
    created: `2026-11-${String(10 + (i % 9)).padStart(2, '0')}T00:00:00Z`,
    event: 'pull_request',
    branch: 'feat/x',
    wall,
  }));

describe('runWallSeconds', () => {
  it('最初の job の作成から最後の job の完了までを数える', () => {
    const jobs = [
      { created_at: '2026-11-19T00:00:10Z', completed_at: '2026-11-19T00:01:00Z' },
      { created_at: '2026-11-19T00:00:00Z', completed_at: '2026-11-19T00:04:00Z' },
      { created_at: '2026-11-19T00:00:05Z', completed_at: '2026-11-19T00:00:30Z' },
    ];
    expect(runWallSeconds(jobs)).toBe(240);
  });

  it('終わっていない job は数えない', () => {
    const jobs = [
      { created_at: '2026-11-19T00:00:00Z', completed_at: '2026-11-19T00:02:00Z' },
      { created_at: '2026-11-18T00:00:00Z', completed_at: null },
    ];
    expect(runWallSeconds(jobs)).toBe(120);
  });

  it('数えられる job が無ければ null', () => {
    expect(runWallSeconds([])).toBeNull();
    expect(runWallSeconds([{ created_at: '2026-11-19T00:00:00Z', completed_at: null }])).toBeNull();
  });
});

describe('median', () => {
  it('奇数個は真ん中、偶数個は中央 2 つの平均', () => {
    expect(median([300, 100, 200])).toBe(200);
    expect(median([400, 100, 200, 300])).toBe(250);
    expect(median([1, 2])).toBe(2); // 1.5 を四捨五入
  });

  it('空なら null', () => {
    expect(median([])).toBeNull();
  });
});

describe('classify', () => {
  it('しきい値以上は breach、80% 以上は warn、それ未満は ok (境界は上の level)', () => {
    expect(classify(330, 330, 0.8)).toBe('breach');
    expect(classify(329, 330, 0.8)).toBe('warn');
    expect(classify(264, 330, 0.8)).toBe('warn');
    expect(classify(263, 330, 0.8)).toBe('ok');
  });

  it('既定のしきい値は 330 秒', () => {
    expect(THRESHOLD_SECS).toBe(330);
    expect(classify(330)).toBe('breach');
    expect(classify(264)).toBe('warn');
    expect(classify(263)).toBe('ok');
  });
});

describe('evaluate', () => {
  it('中央値と level と対象期間を出す', () => {
    const result = evaluate(runs([200, 240, 250, 260, 270, 280, 300, 310, 320, 330]), new Date('2026-11-19T01:02:03.456Z'));
    expect(result.ok).toBe(true);
    expect(result.status).toEqual({
      schema_version: 1,
      measured_at: '2026-11-19T01:02:03Z',
      level: 'warn',
      median_secs: 275,
      threshold_secs: 330,
      warn_ratio: 0.8,
      samples: 10,
      oldest_run_at: '2026-11-10T00:00:00Z',
      newest_run_at: '2026-11-18T00:00:00Z',
    });
  });

  it('待ち時間が取れなかった run は除き、残りが足りなければ判定しない', () => {
    const walls = [...Array(MIN_SAMPLES - 1).fill(400), null, null];
    const result = evaluate(runs(walls));
    expect(result.ok).toBe(false);
    expect(result.reason).toContain(`${MIN_SAMPLES - 1} 本`);
  });
});

describe('formatReport', () => {
  it('1 行目は機械が読む [CI_TIME_SLI] 行', () => {
    const result = evaluate(runs(Array(12).fill(340)));
    const lines = formatReport(result).split('\n');
    expect(lines[0]).toBe('[CI_TIME_SLI] level=breach median_secs=340 threshold_secs=330 ratio=103%');
    expect(lines).toContain('- 判定: 超過 (改善作業に着手必須)');
    expect(lines.filter((l) => /^\| \d+ \|/.test(l))).toHaveLength(12);
  });
});

describe('resolveMainRoot', () => {
  const dirs = [];
  afterEach(() => {
    for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true });
  });
  const tmp = () => {
    const d = mkdtempSync(join(tmpdir(), 'ci-sli-'));
    dirs.push(d);
    return d;
  };

  it('.jj/repo がディレクトリならそこがメイン', () => {
    const root = tmp();
    mkdirSync(join(root, '.jj', 'repo'), { recursive: true });
    expect(resolveMainRoot(root)).toBe(root);
  });

  it('.jj/repo がファイルなら store の 2 つ上がメイン (相対 path は .jj 基準)', () => {
    const main = tmp();
    const ws = join(main, 'ws2');
    mkdirSync(join(ws, '.jj'), { recursive: true });
    writeFileSync(join(ws, '.jj', 'repo'), '../../.jj/repo\n');
    expect(resolveMainRoot(ws)).toBe(main);
  });

  it('.jj/repo がファイルで絶対 path ならそのまま辿る', () => {
    const main = tmp();
    const ws = tmp();
    mkdirSync(join(ws, '.jj'), { recursive: true });
    writeFileSync(join(ws, '.jj', 'repo'), join(main, '.jj', 'repo'));
    expect(resolveMainRoot(ws)).toBe(main);
  });

  it('.jj が無ければ渡された root を返す', () => {
    const root = tmp();
    expect(resolveMainRoot(root)).toBe(root);
  });

  it('writeStatus は .claude/ci-sli-status.json に書く', () => {
    const root = tmp();
    const path = writeStatus(root, { level: 'ok' });
    expect(path).toBe(join(root, STATUS_PATH));
    expect(JSON.parse(readFileSync(path, 'utf8'))).toEqual({ level: 'ok' });
  });
});

describe('main', () => {
  const capture = () => {
    const out = [];
    const err = [];
    return { out, err, io: { out: (s) => out.push(s), err: (s) => err.push(s) } };
  };

  it('測れたら breach でも 0 で終え、記録を書く', async () => {
    const { out, err, io } = capture();
    const written = [];
    const code = await main({
      fetch: async () => runs(Array(20).fill(400)),
      write: (s) => {
        written.push(s);
        return '/x/.claude/ci-sli-status.json';
      },
      ...io,
    });
    expect(code).toBe(0);
    expect(err).toEqual([]);
    expect(written).toHaveLength(1);
    expect(written[0].level).toBe('breach');
    expect(out.join('\n')).toContain('- 記録: /x/.claude/ci-sli-status.json');
  });

  it('gh が失敗したら 2 で終え、記録を書かない', async () => {
    const { out, err, io } = capture();
    let wrote = false;
    const code = await main({
      fetch: async () => {
        throw new Error('HTTP 502');
      },
      write: () => {
        wrote = true;
      },
      ...io,
    });
    expect(code).toBe(2);
    expect(wrote).toBe(false);
    expect(out).toEqual([]);
    expect(err.join('\n')).toContain('HTTP 502');
  });

  it('サンプル不足なら 2 で終え、記録を書かない', async () => {
    const { err, io } = capture();
    let wrote = false;
    const code = await main({
      fetch: async () => runs([300, 300]),
      write: () => {
        wrote = true;
      },
      ...io,
    });
    expect(code).toBe(2);
    expect(wrote).toBe(false);
    expect(err.join('\n')).toContain('サンプル不足');
  });

  it('記録の書込に失敗したら 2 で終える (指示に反映されないことを隠さない)', async () => {
    const { err, io } = capture();
    const code = await main({
      fetch: async () => runs(Array(20).fill(200)),
      write: () => {
        throw new Error('EACCES');
      },
      ...io,
    });
    expect(code).toBe(2);
    expect(err.join('\n')).toContain('EACCES');
  });
});
