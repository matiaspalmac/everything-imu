import { useEffect, useRef } from "react";
import { api, events } from "../api/client";
import { macHex, macKey } from "../lib/macFormat";
import { useActivityStore } from "../stores/useActivityStore";
import { useBiasStore } from "../stores/useBiasStore";
import { useConnectionStore } from "../stores/useConnectionStore";
import { useDeviceStore } from "../stores/useDeviceStore";
import { useImuStreamStore } from "../stores/useImuStreamStore";
import { useLogStore } from "../stores/useLogStore";
import { useMetricsStore } from "../stores/useMetricsStore";
import { useToastStore } from "../stores/useToastStore";
import { useTrackerStore } from "../stores/useTrackerStore";

const BATTERY_LOW_THRESHOLD = 0.15;

/**
 * Single point where the React tree subscribes to Tauri events. Mount once
 * inside AppShell so every page reads from stores instead of installing its
 * own listener — avoids duplicate IPC traffic and lifecycle bugs when
 * navigating between routes.
 */
export function EventBridge() {
  const applyTrackers = useTrackerStore((s) => s.apply);
  const observeMetrics = useMetricsStore((s) => s.observe);
  const ingestSamples = useImuStreamStore((s) => s.ingest);
  const ingestBias = useBiasStore((s) => s.ingest);
  const setStatus = useConnectionStore((s) => s.set);
  const addDevice = useDeviceStore((s) => s.add);
  const setAllDevices = useDeviceStore((s) => s.setAll);
  const pushActivity = useActivityStore((s) => s.push);
  const pushLog = useLogStore((s) => s.push);
  const pushLogBatch = useLogStore((s) => s.pushBatch);
  const pushToast = useToastStore((s) => s.push);
  const lowBatteryNotified = useRef<Set<string>>(new Set());

  useEffect(() => {
    // First-paint snapshots.
    api.listDevices().then((res) => {
      if (res.status === "ok") setAllDevices(res.data);
    });
    api.getConnectionStatus().then((res) => {
      if (res.status === "ok") setStatus(res.data);
    });
    api.getLogBuffer().then((res) => {
      if (res.status === "ok") pushLogBatch(res.data);
    });

    const subs: Promise<() => void>[] = [
      events.trackerUpdate.listen((e) => {
        applyTrackers(e.payload);
        observeMetrics(e.payload.trackers);
        for (const t of e.payload.trackers) {
          if (
            Number.isFinite(t.battery_fraction) &&
            t.battery_fraction > 0 &&
            t.battery_fraction < BATTERY_LOW_THRESHOLD
          ) {
            const k = macKey(t.mac);
            if (!lowBatteryNotified.current.has(k)) {
              lowBatteryNotified.current.add(k);
              pushToast({
                level: "warn",
                title: "Battery low",
                message: `${macHex(t.mac)} at ${Math.round(t.battery_fraction * 100)}%`,
                ttlMs: 8000,
              });
            }
          }
        }
      }),
      events.imuSampleUpdate.listen((e) => ingestSamples(e.payload)),
      events.biasUpdate.listen((e) => ingestBias(e.payload)),
      events.connectionStatusUpdate.listen((e) => setStatus(e.payload)),
      events.deviceDiscovered.listen((e) => {
        addDevice(e.payload.metadata);
        pushActivity({
          level: "success",
          message: `Discovered ${e.payload.metadata.kind} (${macHex(e.payload.metadata.mac)})`,
        });
        pushToast({
          level: "success",
          title: "Device discovered",
          message: `${e.payload.metadata.kind} ${macHex(e.payload.metadata.mac)}`,
        });
      }),
      events.deviceStateChanged.listen((e) => {
        pushActivity({
          level: e.payload.state === "connected" ? "info" : "warn",
          message: `${macHex(e.payload.mac)} ${e.payload.state}`,
        });
        pushToast({
          level: e.payload.state === "connected" ? "info" : "warn",
          title: e.payload.state === "connected" ? "Reconnected" : "Disconnected",
          message: macHex(e.payload.mac),
        });
        if (e.payload.state === "disconnected") {
          lowBatteryNotified.current.delete(macKey(e.payload.mac));
        }
      }),
      events.logEntry.listen((e) => pushLog(e.payload)),
    ];

    return () => {
      for (const p of subs) {
        p.then((u) => u());
      }
    };
  }, [
    applyTrackers,
    observeMetrics,
    ingestSamples,
    ingestBias,
    setStatus,
    addDevice,
    setAllDevices,
    pushActivity,
    pushLog,
    pushLogBatch,
    pushToast,
  ]);

  return null;
}
