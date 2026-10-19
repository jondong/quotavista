import { LOCALES, useI18n, type Locale, type MessageKey } from "../i18n";

const LABEL_KEYS: Record<Locale, MessageKey> = {
  en: "settings.language.en",
  "zh-CN": "settings.language.zhCN",
};

/** Language selector shared by the header and Settings; switching is immediate. */
export default function LanguageSelect({ idPrefix }: { idPrefix: string }) {
  const { locale, setLocale, t } = useI18n();
  const id = `${idPrefix}-language`;
  return (
    <>
      <label htmlFor={id}>{t("settings.language.label")}</label>
      <select
        id={id}
        value={locale}
        onChange={(event) => setLocale(event.target.value as Locale)}
      >
        {LOCALES.map((available) => (
          <option key={available} value={available}>
            {t(LABEL_KEYS[available])}
          </option>
        ))}
      </select>
    </>
  );
}
