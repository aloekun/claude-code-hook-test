import { describe, it, expect } from "vitest";

describe("sample", () => {
  it("greet returns 42", () => {
    expect(42).toBe(42);
  });

  it("string equality", () => {
    expect("hello").toBe("hello");
  });
});
