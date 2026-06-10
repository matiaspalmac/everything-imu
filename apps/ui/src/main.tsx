import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { App } from "./App";
import { ErrorBoundary } from "./components/layout/ErrorBoundary";
import "./i18n";
// Imported for its module-load side-effect: applies the saved theme to the
// DOM before first paint. SettingsPage also imports it, but that page is
// lazy-loaded, so without this eager import the theme would stay on the
// hardcoded `dark` class until the user opens Settings.
import "./stores/useThemeStore";
import { useToastStore } from "./stores/useToastStore";
import "./styles.css";

// Global escape hatches for errors that never reach React: listener
// callbacks, timers, rejected promises. Surface them as a toast (and the
// console, which the dev harness mirrors into the backend log) instead of
// failing silently. Dedupe consecutive repeats so a tight failure loop
// can't flood the toast stack.
let lastGlobalError = "";
function reportGlobalError(message: string) {
  if (message === lastGlobalError) return;
  lastGlobalError = message;
  console.error("[ui] uncaught:", message);
  useToastStore.getState().push({
    level: "error",
    title: "Unexpected error",
    message: message.slice(0, 200),
    ttlMs: 8000,
  });
}

window.addEventListener("error", (event) => {
  reportGlobalError(event.message);
});
window.addEventListener("unhandledrejection", (event) => {
  reportGlobalError(event.reason instanceof Error ? event.reason.message : String(event.reason));
});

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Root element #root not found");
}

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <ErrorBoundary>
      <BrowserRouter>
        <App />
      </BrowserRouter>
    </ErrorBoundary>
  </React.StrictMode>,
);
