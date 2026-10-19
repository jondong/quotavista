# Bilingual validation

Use [A25](../../../docs/traceability.md) and [interface-language requirements](../../../docs/requirements.md#interface-languages). Ship matching en and zh-CN catalog keys, including accessible labels, errors, charts, empty states, and subsequent notifications; retain English runtime fallback.

The exact renderer checks are `npm run i18n:check` (catalog parity), `npm test` (Vitest), `npm run lint` (ESLint), `npm run typecheck` (strict TypeScript), and `npm run build` (production build), all from `apps/desktop`, mirrored in `.github/workflows/ci.yml` and listed in `CONTRIBUTING.md`.

Verify language switching during a pending read preserves filters and nonsecret input, does not trigger provider calls or duplicate notifications, and leaves numeric values and reporting boundaries unchanged. Exercise unknown, zero, stale, partial-permission, missing-history, local-storage-error, and no-tray states with synthetic data.

Browser checks validate renderer behavior; native WebView, secret-store, tray, and notification behavior require macOS/Linux evidence separately.
