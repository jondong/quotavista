import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { initialLocale } from "./i18n";
import "./styles.css";

// Sync the document language before first paint so a stored zh-CN preference
// never flashes an English <html lang>.
document.documentElement.lang = initialLocale();

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
