import { lazy, Suspense } from "react";
import { Route, Routes } from "react-router-dom";
import { AppShell } from "./components/layout/AppShell";

// Single loader table: lazy() consumes these AND the idle prefetch below
// warms every chunk after first paint, so switching pages never waits on
// a network/parse round-trip — the lazy split only helps initial startup.
const loaders = {
  dashboard: () => import("./pages/DashboardPage"),
  connection: () => import("./pages/ConnectionPage"),
  devices: () => import("./pages/DevicesPage"),
  trackerDetail: () => import("./pages/TrackerDetailPage"),
  haptics: () => import("./pages/HapticsPage"),
  logs: () => import("./pages/LogsPage"),
  settings: () => import("./pages/SettingsPage"),
  help: () => import("./pages/HelpPage"),
  debug: () => import("./pages/DebugPage"),
};

const DashboardPage = lazy(() => loaders.dashboard().then((m) => ({ default: m.DashboardPage })));
const ConnectionPage = lazy(() =>
  loaders.connection().then((m) => ({ default: m.ConnectionPage })),
);
const DevicesPage = lazy(() => loaders.devices().then((m) => ({ default: m.DevicesPage })));
const TrackerDetailPage = lazy(() =>
  loaders.trackerDetail().then((m) => ({ default: m.TrackerDetailPage })),
);
const HapticsPage = lazy(() => loaders.haptics().then((m) => ({ default: m.HapticsPage })));
const LogsPage = lazy(() => loaders.logs().then((m) => ({ default: m.LogsPage })));
const SettingsPage = lazy(() => loaders.settings().then((m) => ({ default: m.SettingsPage })));
const HelpPage = lazy(() => loaders.help().then((m) => ({ default: m.HelpPage })));
const DebugPage = lazy(() => loaders.debug().then((m) => ({ default: m.DebugPage })));

// Warm every page chunk once the main thread goes idle. Failures are
// ignored — the route's own lazy() retries on navigation.
function prefetchAllPages() {
  for (const load of Object.values(loaders)) {
    load().catch(() => {});
  }
}
if (typeof window.requestIdleCallback === "function") {
  window.requestIdleCallback(() => prefetchAllPages(), { timeout: 3000 });
} else {
  window.setTimeout(prefetchAllPages, 1500);
}

function RouteFallback() {
  return (
    <div className="p-6 text-xs text-[var(--fg-muted)]" aria-busy>
      Loading…
    </div>
  );
}

export function App() {
  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route
          path="/"
          element={
            <Suspense fallback={<RouteFallback />}>
              <DashboardPage />
            </Suspense>
          }
        />
        <Route
          path="/connection"
          element={
            <Suspense fallback={<RouteFallback />}>
              <ConnectionPage />
            </Suspense>
          }
        />
        <Route
          path="/devices"
          element={
            <Suspense fallback={<RouteFallback />}>
              <DevicesPage />
            </Suspense>
          }
        />
        <Route
          path="/devices/:macKey"
          element={
            <Suspense fallback={<RouteFallback />}>
              <TrackerDetailPage />
            </Suspense>
          }
        />
        <Route
          path="/haptics"
          element={
            <Suspense fallback={<RouteFallback />}>
              <HapticsPage />
            </Suspense>
          }
        />
        <Route
          path="/logs"
          element={
            <Suspense fallback={<RouteFallback />}>
              <LogsPage />
            </Suspense>
          }
        />
        <Route
          path="/settings"
          element={
            <Suspense fallback={<RouteFallback />}>
              <SettingsPage />
            </Suspense>
          }
        />
        <Route
          path="/help"
          element={
            <Suspense fallback={<RouteFallback />}>
              <HelpPage />
            </Suspense>
          }
        />
        <Route
          path="/debug"
          element={
            <Suspense fallback={<RouteFallback />}>
              <DebugPage />
            </Suspense>
          }
        />
      </Route>
    </Routes>
  );
}
