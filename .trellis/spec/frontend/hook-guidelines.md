# Asynchronous reads

No hooks exist yet. The following requirements come from [application contracts](../../../docs/architecture.md#application-contracts) and [time and coverage](../../../docs/architecture.md#time-and-coverage).

Read local state on launch and on revision events. Events are invalidation hints; skipped revisions or reconnects require a complete read. Cleanup subscriptions on unmount.

Match completed display queries to the active request and filters before applying them. Cancel obsolete reads cooperatively. Navigation, locale changes, and filtering must not validate credentials, initiate a separate polling loop, or reset alert episodes.

Retain usable cached content during refresh and keep its original timestamps. Distinguish a local display-query failure from a provider dataset failure.
