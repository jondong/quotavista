# Sanitized provider fixture corpus

Shape evidence for the Phase 2 capability reports. Every file here is
**documentation**, not a capture: the shapes come from public provider
documentation, and every value is invented.

- [`docs/validation/provider-capability-openrouter.md`](../provider-capability-openrouter.md)
- [`docs/validation/provider-capability-kimi.md`](../provider-capability-kimi.md)

Nothing in this corpus was retrieved from a live account. No credential, cookie,
session token, authorization header, email address, phone number, or real account
identifier appears in any file, and no file may be replaced with a live capture
without redacting it first.

## Layout

```text
docs/validation/fixtures/
  README.md                     this file
  openrouter/<name>.json        wire-shape bodies for the four OpenRouter surfaces
  openrouter/<name>.meta.json   provenance for the fixture of the same name
  kimi/<name>.json              wire-shape bodies for the Kimi local-server envelope
  kimi/<name>.meta.json         provenance for the fixture of the same name
  kimi/subscription_page_observed_shape.md
                                the login-gated page, as a field list (no JSON API exists)
```

The OpenRouter adapter also has its own **adapter-local** fixtures at
`crates/providers/src/openrouter/fixtures/`, landed by P2-02 and parsed by its
tests. The two sets have different jobs and are not duplicates:

| | `crates/providers/src/openrouter/fixtures/` | `docs/validation/fixtures/` |
| --- | --- | --- |
| Owner | The adapter that parses them | The capability reports |
| Parsed by | Rust tests | `scripts/check-validation-fixtures.mjs` only |
| Purpose | Prove a parser behaves correctly | Record what a surface returns and what it cannot |
| Coverage | The one endpoint the adapter calls today | All four OpenRouter surfaces and the Kimi envelope |

P2-03 owns `crates/providers/src/kimi/**` and may keep its own adapter-local Kimi
fixtures. This corpus touched nothing under `crates/`.

## Sanitization rules

1. **Shapes from documentation, values from imagination.** Field names, nesting,
   types, and enumerations follow the published schema. Numbers, identifiers,
   timestamps, and labels are invented.
2. **Provenance lives beside the body, never inside it.** Each fixture has a
   sibling `<name>.meta.json`. Bodies carry no `_fixture`, `_note`, or other
   comment key, so a future adapter can parse them byte-for-byte as if they were
   wire responses.
3. **Undocumented sub-shapes are omitted, not invented.** Where the documentation
   names a field but does not publish its shape — OpenRouter's `filters`,
   `order_by`, and `group_limit` items, for example — the fixture leaves the field
   out and the meta file says so. A plausible invention reads as authority.
4. **Synthetic prose is labelled.** Where an envelope structure is documented but
   its message string is not published, the message begins `Synthetic fixture:`
   and the meta `origin` field records that the prose is invented.
5. **Personally identifying fields are redacted by construction.** In
   `kimi/oauth_userinfo_shape_only.json`, every identifying value is the literal
   sentinel `<redacted-at-capture>`, and the documented optional fields `avatar`,
   `bio`, `username`, `globalId`, `createdTime`, and `lastLoginTime` are omitted
   entirely — an absent field cannot leak. `userId` is invented rather than
   redacted because a scope identity has to be stable to be useful.
6. **No key-shaped strings.** The OpenRouter `label` field is masked by the
   provider itself, and the published example uses the provider's real key prefix.
   Fixtures here use `synthetic-key-…` instead so that secret scanners, including
   this repository's own, stay clean.
7. **`validated: false` everywhere.** No fixture in this corpus has been compared
   against a live response. The check script fails if any meta file claims
   otherwise, because that claim would be false today.

## Smoke check

```sh
node scripts/check-validation-fixtures.mjs
```

The script is dependency-free, collects every failure across all nine checks, prints
them grouped by check, and exits non-zero if any remain. It
asserts that:

- every file is valid UTF-8, at most 64 KiB, and every `.json` file parses;
- every fixture has a sibling meta file and every meta file has a fixture, in both
  directions;
- each meta file carries the required provenance keys, names an existing report,
  and honestly declares `sanitized: true`, `validated: false`, and
  `captured_from_live_account: false`;
- no fixture body carries a top-level `_`-prefixed comment key;
- no file under this directory, and neither capability report, matches any of nine
  secret-like patterns;
- the personally identifying fields in the Kimi userinfo fixture are exactly the
  redaction sentinel, and the omitted optional fields are still absent;
- every `request_id` is a 26-character ULID-*shaped* string — length and character
  class only. The invented values are deliberately readable (`01SYNTHETIC…`) and are
  **not** valid Crockford-base32 ULIDs, so nothing may sort or timestamp-decode them;
- every `fixtures/…` link in the capability reports resolves on disk;
- every fixture in this corpus is cited by at least one capability report, so a
  file cannot be added or renamed without the documentation noticing.

The nine secret-like patterns are: an `Authorization`-style header carrying a
`Bearer` or `Basic` credential; the OpenRouter key prefix; the Anthropic key
prefix; the OpenAI project key prefix; a generic `sk-`-prefixed run of 20 or more
characters; a `Cookie` or `Set-Cookie` assignment; a PEM private-key block; a JSON
Web Token; and any JSON field named `password`, `api_key`, `secret`,
`access_token`, `refresh_token`, `client_secret`, `private_key`, or
`session_token`. Key-prefix patterns require a plausible-length body, so prose
that merely names a prefix does not trip the gate.

## What this corpus does not prove

These files show what a surface is documented to return. They are not evidence
that any adapter retrieves real data, and they cannot move A01, A02, A03, or A04
off `Pending`. Both capability reports list the open questions a live
reconciliation still has to answer.
