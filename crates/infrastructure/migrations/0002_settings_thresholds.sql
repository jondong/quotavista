-- Settings thresholds for the connection and settings flows (P1-10).
-- Threshold evaluation is later-phase work; Phase 1 persists the preferences.
ALTER TABLE settings ADD COLUMN quota_threshold TEXT NOT NULL DEFAULT '0.2';
ALTER TABLE settings ADD COLUMN balance_threshold TEXT NOT NULL DEFAULT '0';

-- The requirements set the default refresh interval to five minutes. The
-- seed row ships refresh_minutes NULL because 0001 predates the choice;
-- only rows that were never explicitly saved are upgraded, so an explicit
-- "refresh off" written by save_settings is preserved.
UPDATE settings
   SET refresh_minutes = 5
 WHERE id = 1
   AND revision = 0
   AND refresh_minutes IS NULL;
