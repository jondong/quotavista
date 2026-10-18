# State ownership

Backend state is authoritative for connections, settings revisions, current metrics, published history, operations, and coverage. Frontend state owns navigation, selected filters, transient form input, and display loading state. No frontend state-management library has been selected.

Preserve screen, range, selected accounts, and nonsecret form values during language switching. Clear credential input after submission; never send secrets into generic application state persistence or diagnostics.

Follow [interface languages](../../../docs/requirements.md#interface-languages): bundle en and zh-CN, choose zh-CN only for explicitly Simplified Chinese system locales on first launch, otherwise en, and persist explicit preference. Reporting zone and native currency are independent settings. Store alert template keys and parameters rather than translated episode identities.
