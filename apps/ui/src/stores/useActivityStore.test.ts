import { describe, expect, it } from "vitest";
import { useActivityStore } from "./useActivityStore";

describe("useActivityStore", () => {
  it("push prepends newest first and stamps id + ts", () => {
    useActivityStore.getState().push({ level: "info", message: "first" });
    useActivityStore.getState().push({ level: "warn", message: "second" });
    const entries = useActivityStore.getState().entries;
    expect(entries).toHaveLength(2);
    expect(entries[0].message).toBe("second");
    expect(entries[1].message).toBe("first");
    expect(entries[0].id).not.toBe(entries[1].id);
    expect(entries[0].ts).toBeGreaterThan(0);
  });

  it("push honors an explicit ts", () => {
    useActivityStore.getState().push({ level: "error", message: "boom", ts: 1234 });
    expect(useActivityStore.getState().entries[0].ts).toBe(1234);
  });

  it("caps the feed at 200 entries, dropping the oldest", () => {
    for (let i = 0; i < 205; i++) {
      useActivityStore.getState().push({ level: "info", message: `m${i}` });
    }
    const entries = useActivityStore.getState().entries;
    expect(entries).toHaveLength(200);
    expect(entries[0].message).toBe("m204");
    expect(entries[199].message).toBe("m5");
  });

  it("clear empties", () => {
    useActivityStore.getState().push({ level: "success", message: "x" });
    useActivityStore.getState().clear();
    expect(useActivityStore.getState().entries).toEqual([]);
  });
});
