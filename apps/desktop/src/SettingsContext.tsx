import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { ipc, isIpcError } from "./ipc";
import type { SettingsDto, SettingsPatch } from "./generated/contracts";
import { useI18n, type Locale, type MessageKey } from "./i18n";

/**
 * Canonical empty patch: the update_settings contract treats an all-null
 * patch as a pure read of the current settings, so the renderer has one
 * well-shaped way to load them.
 */
export const EMPTY_PATCH: SettingsPatch = {
  ui_locale: null,
  reporting_zone: null,
  display_currency: null,
  polling: null,
  quota_threshold: null,
  balance_threshold: null,
};

function isConflict(error: unknown): boolean {
  return isIpcError(error) && error.code === "conflict";
}

interface Settings {
  /** The persisted settings, or null until the first read settles. */
  readonly settings: SettingsDto | null;
  /** The read or the last persist failed; the key is localized at render. */
  readonly error: MessageKey | null;
  /**
   * Apply a locale choice: the in-memory switch is immediate; the persisted
   * `ui_locale` setting follows through the optimistic revision check.
   */
  readonly selectLocale: (next: Locale) => void;
  /**
   * Persist a patch with the current settings revision. Resolves to the
   * saved settings, or rejects with the typed IPC error (a conflict means
   * the caller should reload).
   */
  readonly save: (changes: SettingsPatch) => Promise<SettingsDto>;
  /** Re-read the persisted settings (used after a revision conflict). */
  readonly reload: () => Promise<void>;
  /** Drop the error note (does not touch settings state). */
  readonly dismissError: () => void;
}

const SettingsContext = createContext<Settings | null>(null);

export function SettingsProvider({ children }: { children: React.ReactNode }) {
  const { t, locale, setLocale } = useI18n();
  const [settings, setSettings] = useState<SettingsDto | null>(null);
  const [error, setError] = useState<MessageKey | null>(null);
  // The newest read wins; an older in-flight response must never overwrite
  // a newer one.
  const generation = useRef(0);

  const read = useCallback(async (): Promise<SettingsDto | null> => {
    const request = generation.current + 1;
    generation.current = request;
    try {
      const loaded = await ipc.updateSettings({
        expected_settings_revision: "0",
        changes: EMPTY_PATCH,
      });
      if (request !== generation.current) return null;
      setSettings(loaded);
      setError(null);
      return loaded;
    } catch {
      if (request !== generation.current) return null;
      setError("settings.error.loadFailed");
      return null;
    }
  }, [t]);

  // Apply the persisted locale once the first read settles. Only an
  // explicitly saved choice (revision > 0) outranks system detection; the
  // seeded settings row never overrides the first-launch detection rule.
  useEffect(() => {
    let disposed = false;
    void (async () => {
      const loaded = await read();
      if (disposed || !loaded) return;
      if (loaded.settings_revision !== "0" && loaded.ui_locale !== locale) {
        setLocale(loaded.ui_locale);
      }
    })();
    return () => {
      disposed = true;
    };
    // Run once: the locale it compares against is the startup locale.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const selectLocale = useCallback(
    (next: Locale) => {
      // The switch itself is immediate and never blocked by the backend.
      setLocale(next);
      void (async () => {
        const current = settings ?? (await read());
        if (!current) {
          // Unreadable settings cannot hold the choice either; the switch
          // itself already applied. Surface that it was not saved.
          setError("settings.error.localeNotSaved");
          return;
        }
        const expected = current.settings_revision;
        try {
          const saved = await ipc.updateSettings({
            expected_settings_revision: expected,
            changes: { ...EMPTY_PATCH, ui_locale: next },
          });
          setSettings(saved);
        } catch (persistError) {
          if (isConflict(persistError)) {
            // Someone else moved the revision: reload once and retry with
            // the fresh value, never overwriting a newer choice blindly.
            try {
              const fresh = await read();
              if (!fresh || fresh.ui_locale === next) return;
              const saved = await ipc.updateSettings({
                expected_settings_revision: fresh.settings_revision,
                changes: { ...EMPTY_PATCH, ui_locale: next },
              });
              setSettings(saved);
              return;
            } catch {
              // fall through to the error note
            }
          }
          setError("settings.error.localeNotSaved");
        }
      })();
    },
    [read, setLocale, settings, t],
  );

  const save = useCallback(
    async (changes: SettingsPatch): Promise<SettingsDto> => {
      const current = settings ?? (await read());
      if (!current) throw new Error(t("settings.error.loadFailed"));
      const saved = await ipc.updateSettings({
        expected_settings_revision: current.settings_revision,
        changes,
      });
      setSettings(saved);
      setError(null);
      return saved;
    },
    [read, settings, t],
  );

  const dismissError = useCallback(() => setError(null), []);

  const reload = useCallback(async (): Promise<void> => {
    await read();
  }, [read]);

  const value = useMemo<Settings>(
    () => ({ settings, error, selectLocale, save, dismissError, reload }),
    [settings, error, selectLocale, save, dismissError, reload],
  );

  return <SettingsContext.Provider value={value}>{children}</SettingsContext.Provider>;
}

export function useSettings(): Settings {
  const context = useContext(SettingsContext);
  if (!context) throw new Error("useSettings must be used within SettingsProvider");
  return context;
}
