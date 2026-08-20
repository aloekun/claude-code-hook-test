// .github/workflows/*.yml の構文と最小構造を検査する。
//
// 動機: workflow の変更は実走でしか意味的な検証ができない (docs/dev-conventions.md) が、
// **構文エラーだけは実走を待つ必要がない**。GitHub は push されるまで parse しないため、
// 壊れた YAML は「次の schedule 実行が黙って起きない」形で現れる。ローカルで parse だけでも
// 通しておけば、その失敗モードを 1 つ減らせる。
//
// ADR-072 § 検証記録 が「js-yaml で 17 step 構成を確認した」と記録している検証を、
// 手打ちの node -e ではなく再現可能な script として固定したもの。
//
// parse に加えて、**同じ文字列を 2 か所以上で持つ契約**を検査する (順位 319 / 431)。
// どちらも「片方だけ直しても動いているように見えるが、実際には黙って機能しなくなる」
// 形の結合であり、実走観測でしか気づけない失敗モードを決定論層で潰す (ADR-042)。
// docs/dev-conventions.md「同一事実が複数箇所に分散する場合の変更手順」4 (機械検証
// できるものは lint へ寄せる) の実装にあたる。
//   - pr-monitor.yml の冪等マーカー: 投稿時に「書く側」と起動時に「探す側」が別 step。
//   - CodeRabbit の marker: review-request.yml / pr-monitor.yml / markers.rs の 3 か所。
//     CodeRabbit 側の format 変更は外部要因で、追随漏れは silent success を招く
//     (ADR-034 § CR rate-limit format evolution、ADR-051 のクロスシステム結合)。

import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import { createRequire } from 'node:module';

// js-yaml 5.x は CommonJS のみで default export を持たない。ESM の `import yaml from` は
// SyntaxError になるため require で取る。
const yaml = createRequire(import.meta.url)('js-yaml');

const WORKFLOW_DIR = '.github/workflows';
const MARKERS_RS = 'src/check-ci-coderabbit/src/markers.rs';

let failures = 0;
const fail = (message) => {
  console.error(`[lint-workflows] ${message}`);
  failures += 1;
};

/** parse 済み workflow を name で引けるようにしておく (後段の契約検査で使う)。 */
const documents = new Map();

for (const name of readdirSync(WORKFLOW_DIR).filter((f) => /\.ya?ml$/.test(f)).sort()) {
  const path = join(WORKFLOW_DIR, name);
  let document;
  try {
    document = yaml.load(readFileSync(path, 'utf8'));
  } catch (error) {
    fail(`${name}: YAML の parse に失敗しました\n  ${error.message}`);
    continue;
  }
  const jobs = document?.jobs;
  if (!jobs || typeof jobs !== 'object' || Object.keys(jobs).length === 0) {
    fail(`${name}: jobs がありません`);
    continue;
  }
  documents.set(name, document);
  const stepCounts = Object.entries(jobs).map(([jobName, job]) => {
    const steps = Array.isArray(job?.steps) ? job.steps.length : 0;
    return `${jobName}=${steps}`;
  });
  console.log(`[lint-workflows] ${name}: OK (${stepCounts.join(' ')})`);
}

// --- 契約検査 1: pr-monitor の冪等マーカー (順位 319) -------------------------------
//
// 「同一 head SHA へ投稿済みなら起動しない」ガードは、投稿本文に付けるマーカー
// (extract step) と、それを探すクエリ (dedup step) が同じ文字列であって初めて成立する。
// 片方を書き換えると **重複投稿が黙って復活する** (ガードは常に「未投稿」と答える)。
const BACKSTOP_MARKER_PREFIX = '<!-- pr-monitor-backstop: sha=';

const prMonitor = documents.get('pr-monitor.yml');
if (prMonitor) {
  const steps = prMonitor.jobs?.analyze?.steps;
  if (!Array.isArray(steps)) {
    fail('pr-monitor.yml: jobs.analyze.steps が配列ではありません');
  } else {
    // 「探す側」= dedup step、「書く側」= extract step。id で引く (step 名の和訳ゆれに
    // 依存しないため)。
    for (const id of ['dedup', 'extract']) {
      const step = steps.find((s) => s?.id === id);
      if (!step) {
        fail(`pr-monitor.yml: analyze job に id=${id} の step がありません (順位 319 の冪等ガード)`);
      } else if (!String(step.run ?? '').includes(BACKSTOP_MARKER_PREFIX)) {
        fail(
          `pr-monitor.yml: id=${id} の step が冪等マーカー "${BACKSTOP_MARKER_PREFIX}" を含みません。` +
            '「書く側」と「探す側」が食い違うと重複投稿ガードが黙って無効になります',
        );
      }
    }
    // **step 単位の存在検査だけでは足りない。** extract step はマーカーを 2 回書く
    // (注入除去の sed と付与の printf)。片方だけ改名しても「step に 1 つ以上ある」は
    // 成立してしまうので、ファイル中の**似た形のマーカーがすべて同一綴り**であることも
    // 見る。実際、本検査を書いた直後の破壊テストでこの穴を踏んだ。
    const variants = new Set(readFileSync(join(WORKFLOW_DIR, 'pr-monitor.yml'), 'utf8')
      .match(/<!-- pr-monitor-[A-Za-z-]*: sha=/g) ?? []);
    const strays = [...variants].filter((v) => v !== BACKSTOP_MARKER_PREFIX);
    if (strays.length > 0) {
      fail(
        `pr-monitor.yml: 冪等マーカーの綴りが揺れています (${strays.join(' / ')})。` +
          `正は "${BACKSTOP_MARKER_PREFIX}" の 1 種類だけです`,
      );
    } else {
      console.log('[lint-workflows] pr-monitor.yml: 冪等マーカーの書く側/探す側が一致 (順位 319)');
    }
  }
}

// --- 契約検査 2: CodeRabbit marker の多重管理 (順位 431) ----------------------------
//
// CodeRabbit の応答 format は外部要因で変わる。同じ文字列を持つ層が複数あるため、
// **1 か所だけ追随すると残りが黙って壊れる**。ここで「全員が同じ文字列を持っている」
// ことだけを固定する (どの層がどう使うかは各ファイルのコメントが持つ)。
//
// `Review rate limited.` は当初 review-request.yml 専用だったが、同じ穴が
// markers.rs 側にもあることが実データで判明したため (PR #412 / #387) 共有 marker へ
// 格上げした。ack は placeholder と別 comment class だが、**どちらの層も同じ
// 「レート制限で拒否された」事実を判定している**ので、片方だけ追随すると再び
// 非対称に戻る。
const SHARED_CR_MARKERS = [
  {
    marker: 'rate limited by coderabbit.ai',
    files: [join(WORKFLOW_DIR, 'review-request.yml'), join(WORKFLOW_DIR, 'pr-monitor.yml'), MARKERS_RS],
  },
  {
    marker: 'Rate limit exceeded',
    files: [join(WORKFLOW_DIR, 'review-request.yml'), MARKERS_RS],
  },
  {
    marker: 'Review rate limited.',
    files: [join(WORKFLOW_DIR, 'review-request.yml'), MARKERS_RS],
  },
  {
    marker: '<!-- This is an auto-generated comment: summarize by coderabbit.ai -->',
    files: [join(WORKFLOW_DIR, 'review-request.yml'), join(WORKFLOW_DIR, 'pr-monitor.yml'), MARKERS_RS],
  },
];

for (const { marker, files } of SHARED_CR_MARKERS) {
  const missing = files.filter((file) => {
    try {
      return !readFileSync(file, 'utf8').includes(marker);
    } catch (error) {
      fail(`${file}: 読み取れません (CodeRabbit marker の同期検査)\n  ${error.message}`);
      return false;
    }
  });
  if (missing.length > 0) {
    fail(
      `CodeRabbit marker "${marker}" が ${missing.join(' / ')} にありません。` +
        'marker は複数層で同じ値を持つ契約です。1 か所だけ変えると、変えなかった層が ' +
        '「反応はあった」で success を返し続けます (silent success)',
    );
  }
}
if (failures === 0) {
  console.log(`[lint-workflows] CodeRabbit marker の同期 OK (${SHARED_CR_MARKERS.length} 件、順位 431)`);
}

if (failures > 0) {
  console.error(`[lint-workflows] ${failures} 件の検査に失敗しました`);
  process.exit(1);
}
console.log('[lint-workflows] OK (全 workflow の parse に成功)');
