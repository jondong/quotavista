import { useCallback, useRef, useState } from "react";

import { useI18n, type MessageKey } from "../i18n";
import {
  ipc,
  type NotificationCapabilityDto,
  type NotificationPreviewDto,
  type PlatformCapabilitiesDto,
} from "../ipc";

// Tray availability and notification delivery are detections made on this
// machine, never assumptions about a desktop environment. This region exists so
// that detection is visible: when a capability is missing the interface says so
// and keeps the flow here in the main window instead of pretending it worked.
//
// Both reads are user-triggered. The region deliberately issues no IPC call on
// mount, so opening the application never contacts the composition by itself
// and never sends a notification nobody asked for. Until the user asks, the
// rows say "not checked yet" rather than guessing a value.

const TRAY_MESSAGE_KEYS: Record<PlatformCapabilitiesDto["tray"], MessageKey> = {
  available: "desktop.tray.available",
  unavailable: "desktop.tray.unavailable",
};

const CAPABILITY_MESSAGE_KEYS: Record<NotificationCapabilityDto, MessageKey> = {
  available: "desktop.notification.available",
  permission_denied: "desktop.notification.permissionDenied",
  unsupported: "desktop.notification.unsupported",
};

/** Which request is in flight, if any. */
type Busy = "capabilities" | "notification" | null;

interface Status {
  readonly capabilities: PlatformCapabilitiesDto | null;
  readonly preview: NotificationPreviewDto | null;
  readonly busy: Busy;
  readonly failure: MessageKey | null;
}

const INITIAL_STATUS: Status = {
  capabilities: null,
  preview: null,
  busy: null,
  failure: null,
};

export default function DesktopIntegration() {
  const { t, locale } = useI18n();
  const [status, setStatus] = useState<Status>(INITIAL_STATUS);
  // A later request supersedes an earlier one, so a slow or failed read can
  // never overwrite the answer to a newer one.
  const capabilitiesRequest = useRef(0);
  const previewRequest = useRef(0);

  const checkCapabilities = useCallback(async () => {
    const request = ++capabilitiesRequest.current;
    setStatus((current) => ({ ...current, busy: "capabilities", failure: null }));
    try {
      const capabilities = await ipc.getPlatformCapabilities();
      if (request !== capabilitiesRequest.current) return;
      setStatus((current) => ({ ...current, capabilities, busy: null }));
    } catch {
      if (request !== capabilitiesRequest.current) return;
      setStatus((current) => ({
        ...current,
        busy: null,
        failure: "desktop.capabilities.error",
      }));
    }
  }, []);

  const sendPreview = useCallback(async () => {
    const request = ++previewRequest.current;
    setStatus((current) => ({ ...current, busy: "notification", failure: null }));
    try {
      const preview = await ipc.previewSystemNotification({
        template_key: "synthetic_probe",
        locale,
      });
      if (request !== previewRequest.current) return;
      setStatus((current) => ({ ...current, preview, busy: null }));
    } catch {
      if (request !== previewRequest.current) return;
      setStatus((current) => ({
        ...current,
        preview: null,
        busy: null,
        failure: "desktop.notification.error",
      }));
    }
  }, [locale]);

  // Unknown stays unknown: nothing here may read as a value the platform never
  // reported.
  const notChecked = t("desktop.status.notChecked");
  const trayText =
    status.capabilities === null ? notChecked : t(TRAY_MESSAGE_KEYS[status.capabilities.tray]);
  const notificationText =
    status.capabilities === null
      ? notChecked
      : t(CAPABILITY_MESSAGE_KEYS[status.capabilities.notification]);

  return (
    <footer className="desktop-integration" aria-labelledby="desktop-integration-heading">
      <h2 className="desktop-integration-heading" id="desktop-integration-heading">
        {t("desktop.heading")}
      </h2>
      <p className="desktop-integration-note note">{t("desktop.note")}</p>

      <dl className="desktop-integration-status">
        <div className="desktop-integration-row">
          <dt>{t("desktop.tray.label")}</dt>
          <dd>{trayText}</dd>
        </div>
        <div className="desktop-integration-row">
          <dt>{t("desktop.notification.label")}</dt>
          <dd>{notificationText}</dd>
        </div>
        {status.preview !== null && (
          <div className="desktop-integration-row">
            <dt>{t("desktop.notification.preview")}</dt>
            <dd className="desktop-integration-preview">
              <span className="desktop-integration-preview-title">{status.preview.title}</span>
              <span className="desktop-integration-preview-body">{status.preview.body}</span>
            </dd>
          </div>
        )}
      </dl>

      <div className="desktop-integration-actions">
        <button
          type="button"
          className="desktop-integration-action"
          onClick={() => void checkCapabilities()}
          disabled={status.busy !== null}
        >
          {t("desktop.capabilities.check")}
        </button>
        <button
          type="button"
          className="desktop-integration-action"
          onClick={() => void sendPreview()}
          disabled={status.busy !== null}
        >
          {t("desktop.notification.send")}
        </button>
      </div>

      {(status.preview !== null || status.failure !== null) && (
        <div className="desktop-integration-result" role="status">
          {status.preview !== null && (
            <p className="desktop-integration-verdict">
              {t(
                status.preview.delivered
                  ? "desktop.notification.delivered"
                  : "desktop.notification.explained",
              )}
            </p>
          )}
          {status.failure !== null && (
            <p className="desktop-integration-failure">{t(status.failure)}</p>
          )}
        </div>
      )}
    </footer>
  );
}
