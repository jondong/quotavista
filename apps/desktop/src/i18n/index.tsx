import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
} from "react";
import en from "./en.json";
import zhCN from "./zh-CN.json";

/** Locales shipped with the application. English is the runtime fallback. */
export const LOCALES = ["en", "zh-CN"] as const;
export type Locale = (typeof LOCALES)[number];
export type MessageKey = keyof typeof en;

// zh-CN may lag during development; `translate` falls back to the en entry.
export const catalogs: Record<Locale, Partial<Record<MessageKey, string>>> = {
  en,
  "zh-CN": zhCN,
};

export function translate(
  locale: Locale,
  key: MessageKey,
  params?: Record<string, string>,
): string {
  const template = catalogs[locale][key] ?? en[key];
  return template.replace(/\{(\w+)\}/g, (_match, name: string) =>
    params?.[name] !== undefined ? params[name] : `{${name}}`,
  );
}

/**
 * True only for explicitly Simplified Chinese locales (zh-CN, zh-SG, zh-Hans*).
 * Plain "zh", zh-TW/HK/MO, and zh-Hant* are not explicit Simplified Chinese.
 */
export function isExplicitSimplifiedChinese(tag: string): boolean {
  const t = tag.toLowerCase();
  if (!t.startsWith("zh-")) return false;
  if (t.startsWith("zh-hans")) return true;
  const region = t.split("-")[1];
  return region === "cn" || region === "sg";
}

/**
 * First-launch rule from the requirements: an explicitly Simplified Chinese
 * system locale selects zh-CN; every other locale selects en.
 */
export function detectLocale(languages: readonly string[]): Locale {
  for (const tag of languages) {
    if (isExplicitSimplifiedChinese(tag)) return "zh-CN";
    if (tag.toLowerCase().startsWith("zh")) return "en";
  }
  return "en";
}

/** Locale-aware integer grouping for token counts; exact, never Number-rounded. */
export function formatTokens(value: string, locale: Locale): string {
  if (!/^-?\d+$/.test(value)) return value;
  return new Intl.NumberFormat(locale).format(BigInt(value));
}

/**
 * Exact decimal display for money amounts: groups the integer part only and
 * keeps every fractional digit verbatim, so the underlying value is unchanged.
 */
export function formatDecimal(value: string, locale: Locale): string {
  const match = /^(-?)(\d+)(?:\.(\d+))?$/.exec(value);
  if (!match) return value;
  const [, sign, integer, fraction] = match;
  const grouped = new Intl.NumberFormat(locale).format(BigInt(integer));
  return fraction ? `${sign}${grouped}.${fraction}` : `${sign}${grouped}`;
}

/** Display-locale date/time formatting in an independently selected zone. */
export function formatDateTime(instant: Date, locale: Locale, zone: string): string {
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: zone,
  }).format(instant);
}

// shortcut: renderer-local persistence only; P1-10 moves this into the
// versioned ui_locale setting with the shared optimistic revision check.
const STORAGE_KEY = "quotavista.ui_locale";

// Reads are failure-isolated: a broken storage engine falls back to system
// detection instead of taking down the shell. Exported for the pre-paint
// document language sync in main.tsx.
export function initialLocale(): Locale {
  let stored: string | null = null;
  try {
    stored = window.localStorage.getItem(STORAGE_KEY);
  } catch {
    stored = null;
  }
  if (stored === "en" || stored === "zh-CN") return stored;
  return detectLocale(window.navigator.languages ?? [window.navigator.language]);
}

interface I18n {
  locale: Locale;
  setLocale: (next: Locale) => void;
  t: (key: MessageKey, params?: Record<string, string>) => string;
  formatTokens: (value: string) => string;
  formatDecimal: (value: string) => string;
  formatDateTime: (instant: Date, zone: string) => string;
}

const I18nContext = createContext<I18n | null>(null);

export function I18nProvider({ children }: { children: React.ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(initialLocale);
  const setLocale = useCallback((next: Locale) => {
    setLocaleState(next);
    // The in-memory switch always applies; a failed write must not block it.
    try {
      window.localStorage.setItem(STORAGE_KEY, next);
    } catch {
      // shortcut: silent until P1-10 persists via the versioned setting
    }
  }, []);
  useEffect(() => {
    document.documentElement.lang = locale;
  }, [locale]);
  const value = useMemo<I18n>(
    () => ({
      locale,
      setLocale,
      t: (key, params) => translate(locale, key, params),
      formatTokens: (value) => formatTokens(value, locale),
      formatDecimal: (value) => formatDecimal(value, locale),
      formatDateTime: (instant, zone) => formatDateTime(instant, locale, zone),
    }),
    [locale, setLocale],
  );
  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>;
}

export function useI18n(): I18n {
  const context = useContext(I18nContext);
  if (!context) throw new Error("useI18n must be used within I18nProvider");
  return context;
}
