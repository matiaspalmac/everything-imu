import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { api, events } from "../../api/client";
import { macHex, macKey } from "../../lib/macFormat";
import { useActivityStore } from "../../stores/useActivityStore";
import { useBiasStore } from "../../stores/useBiasStore";
import { useConnectionStore } from "../../stores/useConnectionStore";
import { useDeviceStore } from "../../stores/useDeviceStore";
import { useHapticStore } from "../../stores/useHapticStore";
import { useImuStreamStore } from "../../stores/useImuStreamStore";
import { useLatencyStore } from "../../stores/useLatencyStore";
import { useLogStore } from "../../stores/useLogStore";
import { useMetricsStore } from "../../stores/useMetricsStore";
import { useToastStore } from "../../stores/useToastStore";
import { useTrackerStore } from "../../stores/useTrackerStore";

const BATTERY_LOW_THRESHOLD = 0.15;

/**
 * Single point where the React tree subscribes to Tauri events. Mount once
 * inside AppShell so every page reads from stores instead of installing its
 * own listener — avoids duplicate IPC traffic and lifecycle bugs when
 * navigating between routes.
 */
export function EventBridge() {
  // i18n.t reference can change on every render, which is poison for an
  // effect that registers Tauri listeners — we'd tear down and re-register
  // them on every render, leaking the in-flight subscriptions and slowly
  // consuming the WebView's RAM. Reading from a ref kept current via a
  // tiny effect lets the listener callbacks see the latest t() without
  // putting t in the listener-registration effect's deps.
  const { t } = useTranslation();
  const tRef = useRef(t);
  useEffect(() => {
    tRef.current = t;
  }, [t]);

  const applyTrackers = useTrackerStore((s) => s.apply);
  const observeMetrics = useMetricsStore((s) => s.observe);
  const ingestSamples = useImuStreamStore((s) => s.ingest);
  const ingestBias = useBiasStore((s) => s.ingest);
  const ingestLatency = useLatencyStore((s) => s.ingest);
  const setStatus = useConnectionStore((s) => s.set);
  const addDevice = useDeviceStore((s) => s.add);
  const setAllDevices = useDeviceStore((s) => s.setAll);
  const pushActivity = useActivityStore((s) => s.push);
  const pushLog = useLogStore((s) => s.push);
  const pushLogBatch = useLogStore((s) => s.pushBatch);
  const pushToast = useToastStore((s) => s.push);
  const addHapticAddress = useHapticStore((s) => s.add);
  const lowBatteryNotified = useRef<Set<string>>(new Set());

  // oxlint-disable-next-line react-doctor/no-cascading-set-state, react-doctor/effect-needs-cleanup -- snapshots are 3 independent stores; cleanup happens via Promise-of-unsubscribe pattern at end
  useEffect(() => {
    // First-paint snapshots.
    api
      .listDevices()
      .then((res) => {
        if (res.status === "ok") setAllDevices(res.data);
      })
      .catch(() => {});
    api
      .getConnectionStatus()
      .then((res) => {
        if (res.status === "ok") setStatus(res.data);
      })
      .catch(() => {});
    api
      .getLogBuffer()
      .then((res) => {
        if (res.status === "ok") pushLogBatch(res.data);
      })
      .catch(() => {});

    const subs: Promise<() => void>[] = [
      events.trackerUpdate.listen((e) => {
        applyTrackers(e.payload);
        observeMetrics(e.payload.trackers);
        for (const tr of e.payload.trackers) {
          if (
            Number.isFinite(tr.battery_fraction) &&
            tr.battery_fraction > 0 &&
            tr.battery_fraction < BATTERY_LOW_THRESHOLD
          ) {
            const k = macKey(tr.mac);
            if (!lowBatteryNotified.current.has(k)) {
              lowBatteryNotified.current.add(k);
              pushToast({
                level: "warn",
                title: tRef.current("toast.battery_low_title"),
                message: tRef.current("toast.battery_low_message", {
                  mac: macHex(tr.mac),
                  pct: Math.round(tr.battery_fraction * 100),
                }),
                ttlMs: 8000,
              });
            }
          }
        }
      }),
      events.imuSampleUpdate.listen((e) => ingestSamples(e.payload)),
      events.biasUpdate.listen((e) => ingestBias(e.payload)),
      events.latencyUpdate.listen((e) => ingestLatency(e.payload)),
      events.connectionStatusUpdate.listen((e) => setStatus(e.payload)),
      events.deviceDiscovered.listen((e) => {
        addDevice(e.payload.metadata);
        pushActivity({
          level: "success",
          message: `${tRef.current("toast.device_discovered_title")}: ${e.payload.metadata.kind} (${macHex(
            e.payload.metadata.mac,
          )})`,
        });
        pushToast({
          level: "success",
          title: tRef.current("toast.device_discovered_title"),
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
          title:
            e.payload.state === "connected"
              ? tRef.current("toast.reconnected")
              : tRef.current("toast.disconnected"),
          message: macHex(e.payload.mac),
        });
        if (e.payload.state === "disconnected") {
          lowBatteryNotified.current.delete(macKey(e.payload.mac));
        }
      }),
      events.logEntry.listen((e) => pushLog(e.payload)),
      events.hapticAddressDiscovered.listen((e) => addHapticAddress(e.payload.address)),
      events.updateStatus.listen((e) => {
        // Boot-time + manual updater pings the UI through this stream so a
        // user sees what's happening without having to poke Settings. We
        // surface a toast for the user-relevant transitions only; the
        // "checking" / "no_update" beats are noise.
        const stage = e.payload.stage;
        const tr = tRef.current;
        switch (stage.stage) {
          case "available":
            pushToast({
              level: "info",
              title: tr("updater.title"),
              message: tr("updater.available", {
                current: stage.current,
                latest: stage.latest,
              }),
              ttlMs: 5000,
            });
            break;
          case "installing":
            pushToast({
              level: "info",
              title: tr("updater.title"),
              message: tr("updater.applying"),
              ttlMs: 4000,
            });
            break;
          case "installed":
            pushToast({
              level: "info",
              title: tr("updater.title"),
              message: tr("updater.installed_restart", { latest: stage.latest }),
              ttlMs: 8000,
            });
            break;
          case "failed":
            pushToast({
              level: "warn",
              title: tr("updater.title"),
              message: stage.message,
              ttlMs: 6000,
            });
            break;
          default:
            break;
        }
      }),
    ];

    return () => {
      for (const p of subs) {
        p.then((u) => u()).catch(() => {});
      }
    };
  }, [
    applyTrackers,
    observeMetrics,
    ingestSamples,
    ingestBias,
    ingestLatency,
    setStatus,
    addDevice,
    setAllDevices,
    pushActivity,
    pushLog,
    pushLogBatch,
    pushToast,
    addHapticAddress,
  ]);

  return null;
}
