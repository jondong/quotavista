import { LOCALES, useI18n, type Locale, type MessageKey } from "../i18n";
import { useSettings } from "../SettingsContext";

export const LOCALE_LABEL_KEYS: Record<Locale, MessageKey> = {
  en: "settings.language.en",
  "zh-CN": "settings.language.zhCN",
};
/**
 * Language selector shared by the header and Settings. Switching is immediate
 * in memory and persisted through the versioned `ui_locale` setting.
 */
export default function LanguageSelect({ idPrefix }: { idPrefix: string }) {
  const { t, locale } = useI18n();
  const { selectLocale } = useSettings();
  const id = `${idPrefix}-language`;
  return (
    <>
      <label htmlFor={id}>{t("settings.language.label")}</label>
      <select
        id={id}
        value={locale}
        onChange={(event) => selectLocale(event.target.value as Locale)}
      >
        {LOCALES.map((available) => (
          <option key={available} value={available}>
            {t(LOCALE_LABEL_KEYS[available])}
          </option>
        ))}
      </select>
    </>
  );
}
