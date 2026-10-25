import { useEffect, useRef, useState } from "react";
import LanguageSelect from "../components/LanguageSelect";
import { useI18n, type MessageKey } from "../i18n";
import { isIpcError, type IpcError } from "../ipc";
import { EMPTY_PATCH, useSettings } from "../SettingsContext";
import type { Polling, SettingsDto, SettingsPatch } from "../generated/contracts";

const POLLING_OPTIONS: ReadonlyArray<Polling> = [
  "disabled",
  "one_minute",
  "five_minutes",
  "fifteen_minutes",
];

const POLLING_LABELS: Record<Polling, MessageKey> = {
  disabled: "settings.polling.disabled",
  one_minute: "settings.polling.oneMinute",
  five_minutes: "settings.polling.fiveMinutes",
  fifteen_minutes: "settings.polling.fifteenMinutes",
} as const;

interface Drafts {
  readonly zone: string;
  readonly currency: string;
  readonly polling: Polling;
  readonly quotaThreshold: string;
  readonly balanceThreshold: string;
}

function draftsFromSettings(settings: NonNullable<ReturnType<typeof useSettings>["settings"]>): Drafts {
  return {
    zone: settings.reporting_zone,
    currency: settings.display_currency,
    polling: settings.polling,
    quotaThreshold: settings.quota_threshold,
    balanceThreshold: settings.balance_threshold,
  };
}

export default function SettingsScreen() {
  const { t } = useI18n();
  const { settings, save, reload, error, dismissError } = useSettings();
  const [drafts, setDrafts] = useState<Drafts | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [busy, setBusy] = useState(false);

  // Drafts reinitialize when the persisted *content* changes (initial load,
  // a successful save, a conflict reload). A revision bump that only moves
  // the locale leaves them alone: unsaved non-secret drafts survive a
  // language switch, and the zone the user is still editing is not reset by
  // the locale write that advanced the revision.
  const previousSettings = useRef<SettingsDto | null>(null);
  const revision = settings?.settings_revision;
  useEffect(() => {
    if (!settings) return;
    const previous = previousSettings.current;
    previousSettings.current = settings;
    if (previous) {
      const sameContent =
        previous.reporting_zone === settings.reporting_zone &&
        previous.display_currency === settings.display_currency &&
        previous.polling === settings.polling &&
        previous.quota_threshold === settings.quota_threshold &&
        previous.balance_threshold === settings.balance_threshold;
      if (sameContent) return;
    }
    setDrafts(draftsFromSettings(settings));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision]);

  if (!settings || !drafts) {
    return (
      <section className="panel" aria-labelledby="settings-heading">
        <h2 id="settings-heading">{t("settings.heading")}</h2>
        {error ? (
          <p role="alert" className="error">
            {t(error)}{" "}
            <button type="button" onClick={() => void reload()}>
              {t("common.action.retry")}
            </button>
          </p>
        ) : (
          <p role="status" className="note">{t("common.loading")}</p>
        )}
      </section>
    );
  }

  const submit = async (): Promise<void> => {
    if (busy) return;
    setBusy(true);
    setSaved(false);
    // Only changed fields enter the patch; the canonical sender shape still
    // carries every field, with null meaning "leave unchanged".
    const changes: SettingsPatch = { ...EMPTY_PATCH };
    if (drafts.zone !== settings.reporting_zone) changes.reporting_zone = drafts.zone;
    if (drafts.currency !== settings.display_currency) {
      changes.display_currency = drafts.currency;
    }
    if (drafts.polling !== settings.polling) changes.polling = drafts.polling;
    if (drafts.quotaThreshold !== settings.quota_threshold) {
      changes.quota_threshold = drafts.quotaThreshold;
    }
    if (drafts.balanceThreshold !== settings.balance_threshold) {
      changes.balance_threshold = drafts.balanceThreshold;
    }
    try {
      await save(changes);
      setSaveError(null);
      setSaved(true);
    } catch (failure) {
      setSaved(false);
      if (isIpcError(failure)) {
        const ipcError = failure as IpcError;
        if (ipcError.code === "conflict") {
          setSaveError(t("settings.error.saveConflict"));
          // A stale revision never overwrites the newer persisted choice:
          // reload it so the drafts show the truth again.
          await reload();
          return;
        }
        if (ipcError.code === "validation") {
          setSaveError(t("settings.error.saveValidation"));
          return;
        }
      }
      setSaveError(t("settings.error.loadFailed"));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="panel" aria-labelledby="settings-heading">
      <h2 id="settings-heading">{t("settings.heading")}</h2>

      <section className="subsection">
      <h3>{t("settings.language.label")}</h3>
      <div className="language-control">
        <LanguageSelect idPrefix="settings" />
        <p className="note">{t("settings.language.hint")}</p>
      </div>
      </section>

      <section className="subsection">
      <h3>{t("settings.reporting.heading")}</h3>
      <div>
        <label htmlFor="settings-zone">{t("settings.reporting.zone")}</label>
        <input
          id="settings-zone"
          type="text"
          value={drafts.zone}
          onChange={(event) => setDrafts({ ...drafts, zone: event.target.value })}
        />
        <p className="note">{t("settings.reporting.zone.hint")}</p>
      </div>
      <div>
        <label htmlFor="settings-currency">{t("settings.reporting.currency")}</label>
        <input
          id="settings-currency"
          type="text"
          value={drafts.currency}
          onChange={(event) => setDrafts({ ...drafts, currency: event.target.value })}
        />
        <p className="note">{t("settings.reporting.currency.hint")}</p>
      </div>
      <p className="note">{t("settings.reporting.note")}</p>

      </section>
      <section className="subsection">
      <h3>{t("settings.polling.heading")}</h3>
      <div>
        <label htmlFor="settings-polling">{t("settings.polling.label")}</label>
        <select
          id="settings-polling"
          value={drafts.polling}
          onChange={(event) =>
            setDrafts({ ...drafts, polling: event.target.value as Polling })
          }
        >
          {POLLING_OPTIONS.map((option) => (
            <option key={option} value={option}>
              {t(POLLING_LABELS[option])}
            </option>
          ))}
        </select>
        <p className="note">{t("settings.polling.note")}</p>
      </div>
      </section>

      <section className="subsection">
      <h3>{t("settings.thresholds.heading")}</h3>
      <div>
        <label htmlFor="settings-quota-threshold">{t("settings.thresholds.quota")}</label>
        <input
          id="settings-quota-threshold"
          type="text"
          value={drafts.quotaThreshold}
          onChange={(event) =>
            setDrafts({ ...drafts, quotaThreshold: event.target.value })
          }
        />
        <p className="note">{t("settings.thresholds.quota.hint")}</p>
      </div>
      <div>
        <label htmlFor="settings-balance-threshold">{t("settings.thresholds.balance")}</label>
        <input
          id="settings-balance-threshold"
          type="text"
          value={drafts.balanceThreshold}
          onChange={(event) =>
            setDrafts({ ...drafts, balanceThreshold: event.target.value })
          }
        />
        <p className="note">{t("settings.thresholds.balance.hint")}</p>
      </div>
      <p className="note">{t("settings.thresholds.note")}</p>
      </section>

      <div className="subsection form-actions">
        <button
          type="button"
          className="button-primary"
          disabled={busy}
          onClick={() => void submit()}
        >
          {t("settings.save")}
        </button>
        {saved ? <p role="status">{t("settings.saved")}</p> : null}
      </div>
      {saveError ? <p role="alert" className="error">{saveError}</p> : null}
      {error ? (
        <p role="alert" className="error">
          {t(error)}{" "}
          <button type="button" onClick={() => dismissError()}>
            {t("common.action.retry")}
          </button>
        </p>
      ) : null}

      <section className="subsection">
      <h3>{t("settings.prototype.heading")}</h3>
      <p className="note">{t("settings.prototype.note")}</p>
      </section>
    </section>
  );
}
