// .github/workflows/*.yml の構文と最小構造を検査する。
//
// 動機: workflow の変更は実走でしか意味的な検証ができない (docs/dev-conventions.md) が、
// **構文エラーだけは実走を待つ必要がない**。GitHub は push されるまで parse しないため、
// 壊れた YAML は「次の schedule 実行が黙って起きない」形で現れる。ローカルで parse だけでも
// 通しておけば、その失敗モードを 1 つ減らせる。
//
// ADR-072 § 検証記録 が「js-yaml で 17 step 構成を確認した」と記録している検証を、
// 手打ちの node -e ではなく再現可能な script として固定したもの。

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { createRequire } from 'node:module';

// js-yaml 5.x は CommonJS のみで default export を持たない。ESM の `import yaml from` は
// SyntaxError になるため require で取る。
const yaml = createRequire(import.meta.url)('js-yaml');

const WORKFLOW_DIR = '.github/workflows';

let failures = 0;

for (const name of readdirSync(WORKFLOW_DIR).filter((f) => /\.ya?ml$/.test(f)).sort()) {
  const path = join(WORKFLOW_DIR, name);
  let document;
  try {
    document = yaml.load(readFileSync(path, 'utf8'));
  } catch (error) {
    console.error(`[lint-workflows] ${name}: YAML の parse に失敗しました\n  ${error.message}`);
    failures += 1;
    continue;
  }
  const jobs = document?.jobs;
  if (!jobs || typeof jobs !== 'object' || Object.keys(jobs).length === 0) {
    console.error(`[lint-workflows] ${name}: jobs がありません`);
    failures += 1;
    continue;
  }
  const stepCounts = Object.entries(jobs).map(([jobName, job]) => {
    const steps = Array.isArray(job?.steps) ? job.steps.length : 0;
    return `${jobName}=${steps}`;
  });
  console.log(`[lint-workflows] ${name}: OK (${stepCounts.join(' ')})`);
}

if (failures > 0) {
  console.error(`[lint-workflows] ${failures} 件の workflow が検査に失敗しました`);
  process.exit(1);
}
console.log('[lint-workflows] OK (全 workflow の parse に成功)');
