import { describe, expect, it } from "vitest";
import type { ConnectionStatusUpdate } from "../api/client";
import { useConnectionStore } from "./useConnectionStore";

const status: ConnectionStatusUpdate = {
  server_addr: "127.0.0.1:6969",
  server_supports_bundle: true,
  packets_sent: 42,
  last_send_ms_ago: 5,
  last_handshake_ms_ago: null,
};

describe("useConnectionStore", () => {
  it("set stores the latest status snapshot", () => {
    useConnectionStore.getState().set(status);
    expect(useConnectionStore.getState().status).toEqual(status);
  });

  it("clear resets to null", () => {
    useConnectionStore.getState().set(status);
    useConnectionStore.getState().clear();
    expect(useConnectionStore.getState().status).toBeNull();
  });
});
