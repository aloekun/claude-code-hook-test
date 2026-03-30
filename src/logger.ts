/**
 * 最小限の Logger 実装
 *
 * カスタムリンター (no-console-log) で console.log を禁止しているため、
 * 内部実装は console.info / console.warn / console.error を使用する。
 *
 * フル実装（ログレベル制御・フォーマット・出力先切替）は別PRで対応予定。
 */

type LogLevel = "debug" | "info" | "warn" | "error";

const LEVEL_PRIORITY: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
};

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
      // console.info を使用: console.log はカスタムリンターで禁止 (ADR-007)
      console.info("[DEBUG]", ...args);
    }
  }

  info(...args: unknown[]): void {
    if (this.shouldLog("info")) {
      console.info("[INFO]", ...args);
    }
  }

  warn(...args: unknown[]): void {
    if (this.shouldLog("warn")) {
      console.warn("[WARN]", ...args);
    }
  }

  error(...args: unknown[]): void {
    if (this.shouldLog("error")) {
      console.error("[ERROR]", ...args);
    }
  }
}

export const logger = new Logger();
export { Logger, LogLevel };
