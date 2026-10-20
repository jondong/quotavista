import LanguageSelect from "../components/LanguageSelect";
import { useI18n } from "../i18n";


// Display-only for the prototype; the reporting zone and currency are
// independent of the interface language and arrive with persisted settings.
const REPORTING_ZONE = "UTC";
const DISPLAY_CURRENCY = "USD";

export default function SettingsScreen() {
  const { t } = useI18n();
  return (
    <section aria-labelledby="settings-heading">
      <h2 id="settings-heading">{t("settings.heading")}</h2>

      <h3>{t("settings.language.label")}</h3>
      <div className="language-control">
        <LanguageSelect idPrefix="settings" />
        <p className="note">{t("settings.language.hint")}</p>
      </div>
      <h3>{t("settings.reporting.heading")}</h3>
      <dl>
        <dt>{t("settings.reporting.zone")}</dt>
        <dd>{REPORTING_ZONE}</dd>
        <dt>{t("settings.reporting.currency")}</dt>
        <dd>{DISPLAY_CURRENCY}</dd>
      </dl>
      <p className="note">{t("settings.reporting.note")}</p>

      <h3>{t("settings.prototype.heading")}</h3>
      <p className="note">{t("settings.prototype.note")}</p>
    </section>
  );
}
