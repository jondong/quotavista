# Components and accessibility

Follow the independent states in [Requirements](../../../docs/requirements.md#separate-state-dimensions) and the metric DTO in [Architecture](../../../docs/architecture.md#metric-and-statistics-transport).

Use labels and units for remaining quota versus consumption, account balance versus key budget, freshness, and history coverage. Unknown and unavailable values must not render as zero; an empty chart needs an explanation. Communicate state with text as well as color.

Keep primary interactions accessible from the main window even without a tray. Support keyboard navigation, visible focus, accessible form/error/chart labels, and localized status text. User aliases and official names retain their contents.

Charts render backend-prepared series. They must not recalculate financial totals or disguise UTC-only buckets as local calendar days.
