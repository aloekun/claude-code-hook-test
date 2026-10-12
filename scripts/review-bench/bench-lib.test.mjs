import { readFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { describe, expect, it } from 'vitest';
import { candidatesFor, categoryOf, classifyFinding, severityOf, summarize, summaryOf, undecidedEntries } from './bench-lib.mjs';
import { buildWorksheet } from './score.mjs';

describe('classifyFinding', () => {
  it('CodeRabbit の Addressed 印があれば返信が無くても採用', () => {
    expect(classifyFinding('…\n✅ Addressed in commit df91a35', [])).toBe('accepted');
  });

  it('返信が無く印も無ければ未確定 (行の書き換えだけでは採用にしない)', () => {
    expect(classifyFinding('指摘本文', [])).toBe('unconfirmed');
  });

  it('採用の返信は採用', () => {
    expect(classifyFinding('指摘本文', ['指摘のとおりでした。対応済みです (abc123)。'])).toBe('accepted');
  });

  it('却下の語だけの返信は却下', () => {
    expect(classifyFinding('指摘本文', ['この指摘は成立しないため、修正は行いません。'])).toBe('rejected');
  });

  it('却下の語と採用の語が両方あれば採用 (「意図的」は採用の返信にも現れる)', () => {
    expect(classifyFinding('指摘本文', ['意図的な挙動でしたが、指摘のとおり直しました。'])).toBe('accepted');
  });

  it('「採用しません」「採用しない」は却下 (採用の語の接頭辞に一致させない)', () => {
    expect(classifyFinding('指摘本文', ['この提案は採用しません。'])).toBe('rejected');
    expect(classifyFinding('指摘本文', ['今回は採用しないことにしました。'])).toBe('rejected');
    expect(classifyFinding('指摘本文', ['採用しました (abc123)。'])).toBe('accepted');
  });

  it('明示的な却下は、同じ返信に採用の語 (作業の報告) があっても却下', () => {
    expect(classifyFinding('指摘本文', ['この指摘は採用しません。別のチェックを追加しました。'])).toBe('rejected');
    expect(classifyFinding('指摘本文', ['修正は行いません。理由をコメントに追加しました。'])).toBe('rejected');
  });

  it('どちらとも読めない返信は未確定', () => {
    expect(classifyFinding('指摘本文', ['確認します。'])).toBe('unconfirmed');
  });
});

describe('指摘本文の読み取り', () => {
  const body = '_🎯 Functional Correctness_ | _🟠 Major_ | _⚡ Quick win_\n\n**現 HEAD に紐づく証拠だけを成功条件にしてください。**\n\n説明';

  it('重大度とカテゴリを取り出す', () => {
    expect(severityOf(body)).toBe('Major');
    expect(categoryOf(body)).toBe('Functional Correctness');
  });

  it('要約は見出し行 (| を含む太字) を飛ばした最初の太字', () => {
    const bold = '**🎯 Functional Correctness** | **🟡 Minor** | **⚡ Quick win**\n\n**PR の base branch を検証してください。**';
    expect(summaryOf(bold)).toBe('PR の base branch を検証してください。');
    expect(summaryOf(body)).toBe('現 HEAD に紐づく証拠だけを成功条件にしてください。');
  });

  it('例示コードの中の太字は要約にしない (コードブロックとインラインコードを除く)', () => {
    const withCode = [
      '_🎯 Functional Correctness_ | _🟡 Minor_',
      '',
      "`rg -n '**/*.rs' --glob '!dist/**'`",
      '',
      '```sh',
      "echo '**not a summary here**'",
      '```',
      '',
      '**本文の要約はこちらです。**',
    ].join('\n');
    expect(summaryOf(withCode)).toBe('本文の要約はこちらです。');
  });

  it('要約の太字の中にあるインラインコードは欠けさせない', () => {
    const inner = "_🎯 Functional Correctness_ | _🟡 Minor_\n\n**`--glob` の要件を正しい挙動に修正してください。**";
    expect(summaryOf(inner)).toBe('`--glob` の要件を正しい挙動に修正してください。');
  });

  it('読み取れないときは ? を返す', () => {
    expect(severityOf('本文だけ')).toBe('?');
    expect(categoryOf('本文だけ')).toBe('?');
  });
});

describe('候補の絞り込みと集計', () => {
  const truth = { id: 't1', pr: 1, path: 'a.rs', line: 100, severity: 'Major' };

  it('同じ PR・同じファイルで行が幅以内の候補だけを返す (境界を含む)', () => {
    const findings = [
      { pr: 1, path: 'a.rs', line: 125 },
      { pr: 1, path: 'a.rs', line: 126 },
      { pr: 1, path: 'b.rs', line: 100 },
      { pr: 2, path: 'a.rs', line: 100 },
    ];
    expect(candidatesFor(truth, findings, 25)).toEqual([{ pr: 1, path: 'a.rs', line: 125 }]);
  });

  it('行が不明な候補・正解はファイルの一致だけで候補に残す', () => {
    expect(candidatesFor(truth, [{ pr: 1, path: 'a.rs', line: null }], 25)).toHaveLength(1);
    expect(candidatesFor({ ...truth, line: null }, [{ pr: 1, path: 'a.rs', line: 999 }], 25)).toHaveLength(1);
  });

  it('どの正解にも近くない候補は正解外として判定シートに出る。候補には通し番号が付く', () => {
    const sheet = buildWorksheet([truth], [{ pr: 1, path: 'a.rs', line: 100 }, { pr: 1, path: 'z.rs', line: 1 }]);
    expect(sheet.truths[0].candidates).toEqual([{ fid: 0, pr: 1, path: 'a.rs', line: 100, same: null, valid: null }]);
    expect(sheet.extras).toEqual([{ fid: 1, pr: 1, path: 'z.rs', line: 1, valid: null }]);
  });

  it('判定値は true / false だけを受け付け、same: false の候補には valid も求める', () => {
    const sheet = {
      truths: [{ id: 't1', candidates: [{ fid: 0, same: 'false', valid: null }, { fid: 1, same: false, valid: null }, { fid: 2, same: true, valid: null }] }],
      extras: [{ fid: 3, valid: 'true' }, { fid: 4, valid: false }],
    };
    expect(undecidedEntries(sheet)).toEqual(['t1 の候補 0', 't1 の候補 1', '正解外の候補 3']);
  });

  it('再現率は Major 以上を別に数え、近いだけで別の問題の候補も正解外として数える', () => {
    const s = summarize({
      truths: [
        { severity: 'Major', candidates: [{ fid: 0, same: true }] },
        { severity: 'Minor', candidates: [{ fid: 1, same: true }, { fid: 2, same: false, valid: true }] },
        { severity: 'Minor', candidates: [{ fid: 3, same: false, valid: false }] },
        { severity: 'Critical', candidates: [] },
      ],
      extras: [{ fid: 4, valid: true }, { fid: 5, valid: false }],
    });
    expect(s).toEqual({
      truthTotal: 4,
      matched: 2,
      recall: 0.5,
      majorTotal: 2,
      majorMatched: 1,
      majorRecall: 0.5,
      extraValid: 2,
      noise: 2,
    });
  });

  it('複数の正解の近くにある候補は、どれか 1 つで同じ問題なら正解外に数えない', () => {
    const s = summarize({
      truths: [
        { severity: 'Minor', candidates: [{ fid: 0, same: true }] },
        { severity: 'Minor', candidates: [{ fid: 0, same: false, valid: true }] },
      ],
      extras: [],
    });
    expect(s.matched).toBe(1);
    expect(s.extraValid + s.noise).toBe(0);
  });
});

describe('同梱の正解データ', () => {
  const dir = new URL('.', import.meta.url);
  const gt = JSON.parse(readFileSync(new URL('ground-truth.json', dir), 'utf8'));
  const diffs = JSON.parse(gunzipSync(readFileSync(new URL('diffs.json.gz', dir))).toString('utf8'));

  it('2026-10-12 の計測と同じ 45 件 / 25 PR / Major 21 件', () => {
    expect(gt.truths).toHaveLength(45);
    expect(new Set(gt.truths.map((t) => t.pr)).size).toBe(25);
    expect(gt.truths.filter((t) => t.severity === 'Major')).toHaveLength(21);
  });

  it('すべての正解に、指摘が付いた時点の差分がある', () => {
    for (const t of gt.truths) {
      const diff = diffs[`${t.pr}@${t.commit}`];
      expect(diff, t.id).toBeTypeOf('string');
      expect(diff.includes(t.path), `${t.id} の差分に ${t.path} が無い`).toBe(true);
    }
  });
});
