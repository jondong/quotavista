-- Settings thresholds for the connection and settings flows (P1-10).
-- Threshold evaluation is later-phase work; Phase 1 persists the preferences.
ALTER TABLE settings ADD COLUMN quota_threshold TEXT NOT NULL DEFAULT '0.2';
ALTER TABLE settings ADD COLUMN balance_threshold TEXT NOT NULL DEFAULT '0';

-- refresh_minutes deliberately stays NULL on the never-saved seed row: the
-- prototype ships with automatic refresh off (P1-11 semantics on main), and
-- an explicit interval choice — including Off — is persisted by save_settings
-- with its revision, so no data repair is needed or wanted here.
