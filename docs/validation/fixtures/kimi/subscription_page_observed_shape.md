# Kimi subscription page: documented field list

**This file is not an observation.** Nobody opened a member's subscription page while
writing it. It records what the public help documentation says the page exposes, so
that the A03 reconciliation has a written reference to compare against, and so that
nobody mistakes the absence of a JSON fixture for an oversight.

Provider: Kimi membership (Moonshot AI). Surfaces:

- Web: `https://www.kimi.com/membership/subscription`, quota tab
  `https://www.kimi.com/membership/subscription?tab=quota`
  (reached from the avatar or "Mine" at the bottom left, then Settings, then
  Subscription)
- Mobile app: Settings tab, then Settings, then Membership plan, then My subscription

Both are **login-gated user-interface screens**. The public documentation publishes
**no JSON API** for membership quota, which is why this file is a field list rather
than a fixture, and why the retrieval path for the shared pool is an open integration
question instead of a chosen design.

## Fields the documentation says are visible

| Field | Documented form | Notes for reconciliation |
| --- | --- | --- |
| Plan name | A tier name | The documented China lineup is Go, Plus, Pro, Max and Ultra; the legacy tiers Andante, Moderato, Allegretto and Allegro remain valid for existing subscribers. Public documentation disagrees on the tier count, and the lineup is region-scoped, so the name must be read from the account and never guessed from quota size. |
| Remaining quota | **A percentage** | No token amount and no capacity is published. Tokens must never be derived from this percentage, from a model price, or from a guessed plan capacity. |
| Next refresh time | A date and time | Refreshes monthly by **subscription cycle, not calendar month**: subscribing at 3:00 PM on December 1 means the next refresh is at 3:00 PM on January 1. The documentation says to check the subscription page for the exact time, so it must not be inferred from a payment date. |
| Usage detail | The **most recent 1000 records** | Each record carries a usage time, a feature, and a consumption ratio or percentage. This 1000-record bound is the documented history limit and is the ceiling on any per-feature backfill. |
| Extra Usage module | A balance in CNY, an optional monthly spending cap, and a progress bar | A separate wallet, P1 scope. Only subscription members can enable it, the toggle is web-only, and it is shared across web, browser extension, desktop, mobile and Kimi Code. |

## Documented delay

The help documentation carries an explicit warning that usage-detail data may be
briefly delayed and that the current quota display should be treated as
authoritative. That is the delay A03 asks to be recorded, and it means the quota
percentage and the usage-detail list can legitimately disagree at the moment of
capture. An adapter must record both a value time and a collection time rather than
collapsing them.

## Documented quota rules that constrain interpretation

- One shared pool: every member feature draws on it, deducted by actual token
  consumption, so exhausting it in one feature affects the others. Web, mobile app and
  Kimi Code are one pool and must be counted once.
- Kimi Code additionally has a rolling 5-hour usage limit on every tier that includes
  it, and legacy plans also have a weekly limit that resets every 7 days from the
  subscription start time. These are **independent limits** and cannot substitute for
  the membership pool.
- Unused quota does not roll over and cannot be converted to cash; it is cleared at the
  end of the period.
- Payment frequency, membership validity and quota refresh are three different things.
  A remaining percentage does not guarantee that every feature, including Kimi Code, is
  currently available.
- The Kimi Open Platform API wallet is a different product with pay-as-you-go billing
  and no subscription plans, and its API is independent of the Kimi Code API. Neither
  can substitute for membership quota.

## What a human observer should record during A03 reconciliation

Capture the following beside the automatically retrieved values, for the same period
and the same scope. Do not paste screenshots, cookies, session data, order numbers,
email addresses or phone numbers into the repository; transcribe only the fields below.

1. The plan name exactly as displayed, and the account's region.
2. The remaining quota percentage, and the direction it is labelled in (remaining or
   used) - the display direction has to be read, not assumed.
3. The next refresh time as displayed, including its zone.
4. The subscription period boundaries if the page shows them.
5. The timestamp of the most recent usage-detail record, and how many records are
   listed, to confirm the 1000-record bound in practice.
6. The wall-clock time of the observation and the wall-clock time of the automated
   read, so the documented delay can be measured rather than assumed.
7. Whether Extra Usage is enabled, and its balance and currency if so.
8. Whether the page offers any export, download or machine-readable affordance at all.

Items 1 to 3 and 6 are the minimum needed to state whether an automated retrieval path
agrees with the official page, and therefore the minimum needed before A01 or A03 can
move off Pending.
