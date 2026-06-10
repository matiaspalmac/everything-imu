import { TrashIcon } from "@phosphor-icons/react";
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { UdpHapticTarget } from "../../api/bindings";
import { api } from "../../api/client";
import { macHex } from "../../lib/macFormat";
import { useToastStore } from "../../stores/useToastStore";

/// UI for forwarded UDP haptic targets. Lets the user register
/// `host:port` endpoints, fire a test pulse, and delete entries. The
/// synthesized MAC is shown read-only so users wiring OSC rules later
/// know what identifier to bind against.
export function UdpHaptics() {
  const { t } = useTranslation();
  const pushToast = useToastStore((s) => s.push);
  const [list, setList] = useState<UdpHapticTarget[]>([]);
  const [alias, setAlias] = useState("");
  const [host, setHost] = useState("");
  const [port, setPort] = useState(7000);

  const reload = useCallback(async () => {
    const res = await api.udpHapticList();
    if (res.status === "ok") setList(res.data);
  }, []);

  useEffect(() => {
    void reload();
  }, [reload]);

  async function add() {
    if (!host.trim()) return;
    const res = await api.udpHapticUpsert(alias || `${host}:${port}`, host, port);
    if (res.status === "ok") {
      setAlias("");
      setHost("");
      setPort(7000);
      await reload();
    } else {
      pushToast({
        level: "warn",
        title: t("udp_haptics.title"),
        message: "message" in res.error ? res.error.message : res.error.type,
      });
    }
  }

  async function remove(mac: [number, number, number, number, number, number]) {
    await api.udpHapticRemove(mac);
    await reload();
  }

  async function test(mac: [number, number, number, number, number, number]) {
    const res = await api.udpHapticTest(mac, 0.8, 400);
    if (res.status !== "ok") {
      pushToast({
        level: "warn",
        title: t("udp_haptics.title"),
        message: "message" in res.error ? res.error.message : res.error.type,
      });
    }
  }

  return (
    <section className="rounded-[var(--radius-xl)] border border-[var(--border-subtle)] bg-[var(--bg-panel)] p-5 transition-colors hover:border-[var(--border-strong)]">
      <header className="flex flex-col gap-1 pb-3">
        <h3 className="text-sm font-semibold uppercase tracking-[0.12em] text-[var(--fg-section-header)]">
          {t("udp_haptics.title")}
        </h3>
        <span className="text-[11px] text-[var(--fg-muted)]">{t("udp_haptics.body")}</span>
      </header>

      <div className="flex flex-wrap items-end gap-2 pb-3">
        <label className="flex flex-col gap-1 text-[11px] text-[var(--fg-muted)]">
          {t("udp_haptics.alias")}
          <input
            type="text"
            aria-label={t("udp_haptics.alias")}
            value={alias}
            onChange={(e) => setAlias(e.target.value)}
            placeholder="vest-front"
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 text-xs text-[var(--fg-primary)]"
          />
        </label>
        <label className="flex flex-col gap-1 text-[11px] text-[var(--fg-muted)]">
          {t("udp_haptics.host")}
          <input
            type="text"
            aria-label={t("udp_haptics.host")}
            value={host}
            onChange={(e) => setHost(e.target.value)}
            placeholder="192.168.1.42"
            className="rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 text-xs text-[var(--fg-primary)]"
          />
        </label>
        <label className="flex flex-col gap-1 text-[11px] text-[var(--fg-muted)]">
          {t("udp_haptics.port")}
          <input
            type="number"
            aria-label={t("udp_haptics.port")}
            min={1}
            max={65535}
            value={port}
            onChange={(e) => setPort(Number.parseInt(e.target.value, 10) || 0)}
            className="w-24 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-2 py-1 text-xs text-[var(--fg-primary)]"
          />
        </label>
        <button
          type="button"
          onClick={() => void add()}
          className="rounded-[var(--radius-sm)] bg-[var(--accent)] px-3 py-1 text-xs font-semibold text-[var(--fg-inverse)] hover:bg-[var(--accent-bright)]"
        >
          {t("udp_haptics.add")}
        </button>
      </div>

      {list.length === 0 ? (
        <p className="text-[11px] text-[var(--fg-muted)]">{t("udp_haptics.empty")}</p>
      ) : (
        <ul className="flex flex-col gap-1.5">
          {list.map((t) => (
            <li
              key={macHex(t.mac)}
              className="flex items-center gap-3 rounded-[var(--radius-sm)] border border-[var(--border-subtle)] bg-[var(--bg-elevated)] px-3 py-2 text-xs"
            >
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate text-sm text-[var(--fg-primary)]">{t.alias}</span>
                <span className="metric-num font-mono text-[10px] text-[var(--fg-muted)]">
                  {t.host}:{t.port} · {macHex(t.mac)}
                </span>
              </span>
              <button
                type="button"
                onClick={() => void test(t.mac)}
                className="rounded-[var(--radius-sm)] bg-[var(--bg-panel)] px-2 py-1 text-[11px] text-[var(--fg-secondary)] hover:bg-[var(--accent-soft)] hover:text-[var(--accent)]"
              >
                test
              </button>
              <button
                type="button"
                onClick={() => void remove(t.mac)}
                className="rounded-[var(--radius-sm)] bg-[var(--bg-panel)] p-1 text-[var(--fg-muted)] hover:bg-[var(--warn-soft)] hover:text-[var(--warn)]"
                aria-label="remove"
              >
                <TrashIcon size={14} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
