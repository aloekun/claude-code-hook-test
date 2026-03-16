// E2E test runner
// .env.e2e が存在しない場合は自動スキップ
import { existsSync } from "node:fs";
import { execFileSync } from "node:child_process";

if (!existsSync(".env.e2e")) {
  console.log("Skipped: .env.e2e not found");
  process.exit(0);
}

execFileSync("npx", ["vitest", "run", "--config", "vitest.e2e.config.ts"], {
  stdio: "inherit",
});
