import { describe, expect, it } from "vitest";
import type { LogEntryDto } from "../api/client";
import { useLogStore } from "./useLogStore";

function log(message: string): LogEntryDto {
  return { ts_ms: 1, level: "info", target: "core", message };
}

describe("useLogStore", () => {
  it("push appends and stamps a unique monotonically increasing seq", () => {
    useLogStore.getState().push(log("a"));
    useLogStore.getState().push(log("a"));
    const entries = useLogStore.getState().entries;
    expect(entries).toHaveLength(2);
    expect(entries[1].seq).toBeGreaterThan(entries[0].seq);
  });

  it("pushBatch appends in order", () => {
    useLogStore.getState().pushBatch([log("a"), log("b"), log("c")]);
    expect(useLogStore.getState().entries.map((e) => e.message)).toEqual(["a", "b", "c"]);
  });

  it("push and pushBatch are no-ops while paused", () => {
    useLogStore.getState().setPaused(true);
    useLogStore.getState().push(log("dropped"));
    useLogStore.getState().pushBatch([log("dropped too")]);
    expect(useLogStore.getState().entries).toHaveLength(0);
  });

  it("push caps the buffer at 5000, dropping the oldest", () => {
    const batch = Array.from({ length: 5000 }, (_, i) => log(`m${i}`));
    useLogStore.getState().pushBatch(batch);
    useLogStore.getState().push(log("overflow"));
    const entries = useLogStore.getState().entries;
    expect(entries).toHaveLength(5000);
    expect(entries[0].message).toBe("m1");
    expect(entries[4999].message).toBe("overflow");
  });

  it("pushBatch keeps only the newest 5000", () => {
    const batch = Array.from({ length: 5005 }, (_, i) => log(`m${i}`));
    useLogStore.getState().pushBatch(batch);
    const entries = useLogStore.getState().entries;
    expect(entries).toHaveLength(5000);
    expect(entries[0].message).toBe("m5");
    expect(entries[4999].message).toBe("m5004");
  });

  it("filter and follow setters update state", () => {
    useLogStore.getState().setFilterLevel("warn");
    useLogStore.getState().setFilterText("imu");
    useLogStore.getState().setFollow(false);
    const s = useLogStore.getState();
    expect(s.filterLevel).toBe("warn");
    expect(s.filterText).toBe("imu");
    expect(s.follow).toBe(false);
  });
});
