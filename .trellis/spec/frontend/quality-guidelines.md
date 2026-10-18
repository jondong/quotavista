# Bilingual validation

Use [A25](../../../docs/traceability.md) and [interface-language requirements](../../../docs/requirements.md#interface-languages). Ship matching en and zh-CN catalog keys, including accessible labels, errors, charts, empty states, and subsequent notifications; retain English runtime fallback.

After scaffolding, establish exact type-check, lint, catalog-parity, production-build, and interaction-check commands in the contributor guide. They cannot pass before application code exists.

Verify language switching during a pending read preserves filters and nonsecret input, does not trigger provider calls or duplicate notifications, and leaves numeric values and reporting boundaries unchanged. Exercise unknown, zero, stale, partial-permission, missing-history, local-storage-error, and no-tray states with synthetic data.

Browser checks validate renderer behavior; native WebView, secret-store, tray, and notification behavior require macOS/Linux evidence separately.
