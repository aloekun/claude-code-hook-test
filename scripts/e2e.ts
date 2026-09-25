/**
 * E2E test runner
 * .env.e2e が存在しない場合は自動スキップ
 */

import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import { logger } from "../src/logger";

if (!existsSync(".env.e2e")) {
  logger.info("Skipped: .env.e2e not found");
  process.exit(0);
}

// vitest は devDependencies に固定してある (順位 16)。--no-install で、未導入のときに
// npx がネットワークから最新版を取りに行く経路を塞ぐ (package.json の test と揃える)。
execFileSync("npx", ["--no-install", "vitest", "run", "--config", "vitest.e2e.config.ts"], {
  stdio: "inherit",
});
