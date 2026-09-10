// .takt/facets/instructions/*.md の出力言語指定を検査する (順位 515 / 撤1-②)。
//
// 動機: 2026-08-15 の weekly-review で `review-todo-whole` facet の出力がほぼ全文ハングルに
// なった。原因は退行ではなく **言語指定の不在** で、`.takt/config.yaml` が無いため takt builtin
// の `en` ロケールへフォールバックしており、instruction にも output contract にも言語指定が
// 1 箇所も無かった。`~/.claude/settings.json` の `"language"` は Claude Code 本体の設定で、
// takt が spawn する provider には伝播しない。
//
// 対処は「各 instruction に 1 行ずつ直書き」しかない。takt が facet へ渡すのは当該 instruction
// の本文だけで、共通ファイルを参照させても中身は届かないからである。直書きの代償は分散で、
// 新しい facet を足すときに 1 行を落としても誰も気づかない。この検査はその穴を塞ぐ
// (旧 dev-conventions.md の同名 convention を機構化したもの。規約の中身は本ファイルが正で、
// dev-conventions.md 側は § 機構への索引 の 1 行だけを持つ、ADR-042)。
//
// 検査する契約は **言語指定と免除リストの対**: workflow の `rules.condition` は
// `analysis complete` / `approved` / `needs_fix` 等を英語リテラルで照合するので、「日本語で書く」
// だけを指示するとモデルがこれらまで訳して gate が通らなくなる (PR #410 で実測)。
// よって「日本語で書く」と「訳さない」が **同じ行** にあることを要求する
// (既存 20 facet 全てがこの形。別行に分かれると言語指定と免除の対応が読めなくなる)。
//
// 免除リストの **中身** が workflow の condition と一致するかは本検査の範囲外
// (順位 465 B-1)。ここは「行が存在すること」だけを固定する。
//
// なお instruction の言語指定は best-effort であり、最終成果物の言語契約は aggregate facet の
// 出力 1 枚に置く (ADR-031)。本検査はその契約の上流を守る層で、run の成否は変えない。

import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

export const INSTRUCTIONS_DIR = '.takt/facets/instructions';

const LANGUAGE_DIRECTIVE = '日本語で書く';
const EXEMPTION_DIRECTIVE = '訳さない';

/**
 * 1 つの instruction 本文を検査し、違反メッセージの配列を返す (空配列 = OK)。
 * @param {string} text
 * @returns {string[]}
 */
export function checkFacetInstruction(text) {
  const lines = text.split(/\r?\n/);
  const hasPair = lines.some((line) => line.includes(LANGUAGE_DIRECTIVE) && line.includes(EXEMPTION_DIRECTIVE));
  if (hasPair) {
    return [];
  }
  const hasLanguage = lines.some((line) => line.includes(LANGUAGE_DIRECTIVE));
  const hasExemption = lines.some((line) => line.includes(EXEMPTION_DIRECTIVE));
  if (!hasLanguage) {
    return [
      `出力言語の指定 (「${LANGUAGE_DIRECTIVE}」を含む行) がありません。takt は instruction 本文しか facet へ渡さないため、` +
        '共通ファイルへの参照では届きません。`## 出力言語` 節を 1 行で直書きしてください',
    ];
  }
  if (!hasExemption) {
    return [
      `言語指定はありますが免除リスト (「${EXEMPTION_DIRECTIVE}」) が同じ行にありません。workflow の rules.condition は ` +
        '英語リテラルで照合するため、訳してはならない固定トークンを言語指定と対で書いてください (PR #410)',
    ];
  }
  return [
    `「${LANGUAGE_DIRECTIVE}」と「${EXEMPTION_DIRECTIVE}」が別の行に分かれています。言語指定と免除リストは対応が読めるよう同じ行に書いてください`,
  ];
}

/**
 * ディレクトリ内の全 instruction を検査する。
 * @param {string} dir
 * @returns {{ file: string, message: string }[]}
 */
export function lintInstructionsDir(dir) {
  const names = readdirSync(dir).filter((f) => f.endsWith('.md')).sort();
  if (names.length === 0) {
    return [{ file: dir, message: 'instruction ファイルが 1 つもありません (ディレクトリの指定を確認)' }];
  }
  const findings = [];
  for (const name of names) {
    const path = join(dir, name);
    for (const message of checkFacetInstruction(readFileSync(path, 'utf8'))) {
      findings.push({ file: path, message });
    }
  }
  return findings;
}

function main() {
  const findings = lintInstructionsDir(INSTRUCTIONS_DIR);
  for (const { file, message } of findings) {
    console.error(`[lint-takt-facets] ${file}: ${message}`);
  }
  if (findings.length > 0) {
    console.error(`[lint-takt-facets] ${findings.length} 件の検査に失敗しました`);
    process.exit(1);
  }
  const count = readdirSync(INSTRUCTIONS_DIR).filter((f) => f.endsWith('.md')).length;
  console.log(`[lint-takt-facets] OK (${count} instruction に出力言語の指定と免除リストの対を確認)`);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  main();
}
