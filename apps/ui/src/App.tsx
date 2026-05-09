import { Route, Routes } from "react-router-dom";
import { AppShell } from "./components/AppShell";
import { ConnectionPage } from "./pages/ConnectionPage";
import { DashboardPage } from "./pages/DashboardPage";
import { DevicesPage } from "./pages/DevicesPage";
import { HelpPage } from "./pages/HelpPage";
import { LogsPage } from "./pages/LogsPage";
import { SettingsPage } from "./pages/SettingsPage";
import { TrackerDetailPage } from "./pages/TrackerDetailPage";

export function App() {
  return (
    <Routes>
      <Route element={<AppShell />}>
        <Route path="/" element={<DashboardPage />} />
        <Route path="/connection" element={<ConnectionPage />} />
        <Route path="/devices" element={<DevicesPage />} />
        <Route path="/devices/:macKey" element={<TrackerDetailPage />} />
        <Route path="/logs" element={<LogsPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/help" element={<HelpPage />} />
      </Route>
    </Routes>
  );
}
