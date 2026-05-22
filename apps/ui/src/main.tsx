import React from "react";
import ReactDOM from "react-dom/client";
import { BrowserRouter } from "react-router-dom";
import { App } from "./App";
import "./i18n";
// Imported for its module-load side-effect: applies the saved theme to the
// DOM before first paint. SettingsPage also imports it, but that page is
// lazy-loaded, so without this eager import the theme would stay on the
// hardcoded `dark` class until the user opens Settings.
import "./stores/useThemeStore";
import "./styles.css";

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("Root element #root not found");
}

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </React.StrictMode>,
);
