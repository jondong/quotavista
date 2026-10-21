import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { initialLocale } from "./i18n";
import "./styles.css";

// Sync the document language before first paint from the system locale; the
// persisted `ui_locale` setting is applied once the first settings read
// settles.
document.documentElement.lang = initialLocale();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
