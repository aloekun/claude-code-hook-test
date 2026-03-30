/**
 * Logger — プロジェクト共通のログ出力
 *
 * カスタムリンター (no-console-log) で console.log を禁止しているため、
 * 内部実装は console.info / console.warn / console.error を使用する (ADR-007)。
 */

type LogLevel = "debug" | "info" | "warn" | "error";

const LEVEL_PRIORITY: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

function jstTimestamp(): string {
  return new Date().toLocaleString("ja-JP", {
    timeZone: "Asia/Tokyo",
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
}

class Logger {
  private level: LogLevel;

  constructor(level: LogLevel = "debug") {
    this.level = level;
  }

  private shouldLog(target: LogLevel): boolean {
    return LEVEL_PRIORITY[target] >= LEVEL_PRIORITY[this.level];
  }

  debug(...args: unknown[]): void {
    if (this.shouldLog("debug")) {
      console.info(`${jstTimestamp()} [DEBUG]`, ...args);
    }
  }

  info(...args: unknown[]): void {
    if (this.shouldLog("info")) {
      console.info(`${jstTimestamp()} [INFO]`, ...args);
    }
  }

  warn(...args: unknown[]): void {
    if (this.shouldLog("warn")) {
      console.warn(`${jstTimestamp()} [WARN]`, ...args);
    }
  }

  error(...args: unknown[]): void {
    if (this.shouldLog("error")) {
      console.error(`${jstTimestamp()} [ERROR]`, ...args);
    }
  }
}

export const logger = new Logger();
export { Logger, LogLevel };
