import { lazy, Suspense } from "react";
import { Route, Routes } from "react-router-dom";
import { AppShell } from "./components/AppShell";

const DashboardPage = lazy(() =>
  import("./pages/DashboardPage").then((m) => ({ default: m.DashboardPage })),
);
const ConnectionPage = lazy(() =>
  import("./pages/ConnectionPage").then((m) => ({ default: m.ConnectionPage })),
);
const DevicesPage = lazy(() =>
  import("./pages/DevicesPage").then((m) => ({ default: m.DevicesPage })),
);
const TrackerDetailPage = lazy(() =>
  import("./pages/TrackerDetailPage").then((m) => ({ default: m.TrackerDetailPage })),
);
const HapticsPage = lazy(() =>
  import("./pages/HapticsPage").then((m) => ({ default: m.HapticsPage })),
);
const LogsPage = lazy(() => import("./pages/LogsPage").then((m) => ({ default: m.LogsPage })));
const SettingsPage = lazy(() =>
  import("./pages/SettingsPage").then((m) => ({ default: m.SettingsPage })),
);
const HelpPage = lazy(() => import("./pages/HelpPage").then((m) => ({ default: m.HelpPage })));
const DebugPage = lazy(() => import("./pages/DebugPage").then((m) => ({ default: m.DebugPage })));

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
