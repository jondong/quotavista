import { useEffect, useRef, useState } from "react";
import LanguageSelect, { LOCALE_LABEL_KEYS } from "./components/LanguageSelect";
import DesktopIntegration from "./components/DesktopIntegration";
import { I18nProvider, useI18n, type MessageKey } from "./i18n";
import OverviewScreen from "./screens/OverviewScreen";
import StatisticsScreen from "./screens/StatisticsScreen";
import { SettingsProvider } from "./SettingsContext";
import ConnectionsScreen from "./screens/ConnectionsScreen";
import SettingsScreen from "./screens/SettingsScreen";

const SCREENS = [
  { id: "overview", labelKey: "nav.overview" },
  { id: "statistics", labelKey: "nav.statistics" },
  { id: "connections", labelKey: "nav.connections" },
  { id: "settings", labelKey: "nav.settings" },
] as const satisfies ReadonlyArray<{
  id: string;
  labelKey: MessageKey;
}>;

type ScreenId = (typeof SCREENS)[number]["id"];

function Shell() {
  const { t, locale } = useI18n();
  // Announce every real language change, including back to the initial one.
  const previousLocaleRef = useRef(locale);
  const [announcement, setAnnouncement] = useState("");
  useEffect(() => {
    if (previousLocaleRef.current === locale) return;
    previousLocaleRef.current = locale;
    setAnnouncement(t("a11y.langSelected", { language: t(LOCALE_LABEL_KEYS[locale]) }));
  }, [locale, t]);
  // Screen selection is frontend-owned display state; language switching
  // re-renders in place and never resets it.
  const [screen, setScreen] = useState<ScreenId>("overview");

  return (
    <>
      <a className="skip-link" href="#main-content">
        {t("a11y.skipToContent")}
      </a>
      <header className="app-header">
        <div className="app-header-row">
          <h1>{t("app.title")}</h1>
          <div className="header-language">
            <LanguageSelect idPrefix="header" />
          </div>
        </div>
        <p className="synthetic-banner" role="status">
          {t("app.syntheticBanner")}
        </p>
      </header>
      {/* Shell-level live region: header switches announce everywhere, not just on Settings. */}
      <p role="status" aria-live="polite" className="visually-hidden">
        {announcement}
      </p>
      <nav aria-label={t("nav.region")}>
        <ul className="nav-list">
          {SCREENS.map((entry) => (
            <li key={entry.id}>
              <button
                type="button"
                className="nav-button"
                aria-current={screen === entry.id ? "page" : undefined}
                onClick={() => setScreen(entry.id)}
              >
                {t(entry.labelKey)}
              </button>
            </li>
          ))}
        </ul>
      </nav>
      <main id="main-content" className="container" tabIndex={-1}>
        {screen === "overview" && <OverviewScreen />}
        {screen === "statistics" && <StatisticsScreen />}
        {screen === "connections" && <ConnectionsScreen />}
        {screen === "settings" && <SettingsScreen />}
      </main>
      <DesktopIntegration />
    </>
  );
}

export default function App() {
  return (
    <I18nProvider>
      <SettingsProvider>
        <Shell />
      </SettingsProvider>
    </I18nProvider>
  );
}
