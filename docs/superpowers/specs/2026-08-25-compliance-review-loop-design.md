# Compliance — closing the review loop

**Date:** 2026-08-25
**Status:** shipped and verified in production 2026-08-25 (PRs #140-#143)
**Follows:** PR #138 (`ddf2d67f`, courier compliance gate, observe-only) and
PR #139 (`478b80bd`, courier document-upload screens)

## The problem

PR #138 gave couriers compliance profiles. PR #139 gave them a way to submit
documents. Both merged. The loop still does not close, because the two ends
that consume those submissions were never built:

1. **The reviewer cannot see the document they are approving.**
   `document-detail-panel.tsx` renders `<a href={doc.file_url}>`, and `file_url`
   is always `s3://bucket/compliance/<tenant>/<uuid>` — both
   `DocumentStorage::upload` and `confirm_document` construct it that way. A
   browser does nothing with an `s3://` href. The only presign route is
   `GET /me/documents/:doc_id/url`, which hard-checks
   `profile.entity_id != claims.user_id`, so every admin gets 403.

   Approve and Reject are therefore blind decisions today, and the courier APK
   is about to start filling that queue with real licences.

2. **The reviewer cannot tell what kind of document it is.** The panel renders
   `doc.document_type_id.slice(0, 12)` and the queue `.slice(0, 8)` — a raw UUID
   prefix where "Driver's Licence" belongs. No admin route exposes the document
   type catalogue.

3. **Queue rows carry no identity.** `list_pending_review` returns bare
   `DriverDocument` rows, so the queue reads "Profile 3f2a1b9c".

4. **A rejection never reaches the courier.** The OmniDeliv courier app's shift
   screen has an unadorned `TextButton("Documents")`. `outstandingCount()` exists
   in the domain layer and its own KDoc says *"Drives the badge on the shift
   screen"* — it has no caller outside `ComplianceViewModel`'s derived getter and
   one test. `ShiftViewModel` never touches `ComplianceApi`. A courier whose
   licence was refused is told nothing until they happen to tap through.

Items 1–3 make the admin half unusable. Item 4 means the decision the admin
finally makes does not travel.

## The load-bearing decision: where labels come from

**The server enriches what it owns; the client joins what it does not.**

compliance owns `compliance_profiles` and `driver_documents`, so joining them is
legal and cheap. It does **not** own courier names — those live in
`field_ops.couriers` and `driver_ops.drivers`. Cross-service DB joins are banned
by the architecture principles; a synchronous HTTP call would couple compliance
to a tier it deliberately does not know about; and *which* tier is a fork
compliance would have to branch on. The admin portal already fetches both
rosters, so the name join belongs there.

Rejected alternatives:

- **Denormalize the name onto the profile via `driver.registered`.** Needs a
  migration, a field-ops publisher change and a backfill, and creates a second
  source of truth for a value that changes.
- **compliance calls field-ops at query time.** Synchronous coupling, plus the
  field-ops/driver-ops fork compliance should not own.

`profile.entity_id` is the identity user on both creation paths — lazily it is
`claims.user_id`, and via the event it is `driver_id`, which `register_courier`
sets from `user_id` while also forcing `courier.id = user_id` (the ADR-0015
collapse). So the portal matches `courier.user_id` first, `courier.id` second.
Rows predating the collapse may differ; driver-ops' own `id`/`user_id`
split-brain is still open and is out of scope here.

## Backend — `services/compliance`

| Route | Permission | Notes |
|---|---|---|
| `GET /admin/documents/:doc_id/content` | `compliance:review` | doc → profile → tenant check, then `storage.get_object`. Streams the bytes, typed by magic number. **Shipped as `/content`; the design below said `/url` and that was wrong — see the correction under Outcome.** |
| `GET /admin/document-types` | `compliance:review` | New `DocumentTypeRepository::list_all()`. Client caches `id → {code, name}`. |
| `GET /admin/queue` *(changed)* | `compliance:review` | Rows gain `entity_id`, `entity_type`, `jurisdiction`, `overall_status` via one SQL join — no N+1. |

Two deliberate choices:

- **The tenant-ownership check becomes a shared helper.** `approve_document` and
  `reject_document` each open-code the same doc → profile → tenant three-step;
  the document route would be the third copy. One `authorize_document` returning
  `(doc, profile)`, used by all three, so the copies cannot drift.

- **The document is fetched on click, not on panel render.** A reviewer opening
  someone's licence is a privacy-relevant read under PDPA/GDPR and gets an audit
  row (`doc_viewed`). Fetching on render would make that log noise rather than
  evidence.

## Frontend — `apps/admin-portal`

- `lib/api/compliance.ts` — `fetchDocumentBlobUrl`, `fetchDocumentTypes`,
  enriched queue row type.
- `document-detail-panel.tsx` — "View" becomes a button that fetches the bytes
  into a blob and renders them inline, fixing the dead `s3://` href. An `<img>`
  tag sends no `Authorization` header and a KYC document cannot be public, which
  is why it is a blob and not a `src`. Document type renders its name.
- `review-queue.tsx` — rows show the courier's name, resolved against
  `fetchCouriers()`, and the document type name.

**Degradation — each failure keeps the console usable:**

| Failure | Behaviour |
|---|---|
| Roster fetch fails | Names fall back to the short entity id; queue still works |
| Unknown document type id | Renders the short id, not blank |
| `file_url` is not a stored `s3://` object (legacy rows, `#` mocks) | Button disabled, no broken tab |
| The fetch fails | Error surfaces with a Retry; no silent blank tab |
| The bytes are not an image (a PDF) | Falls back to an "Open document" link |

## Courier app — `apps/omnideliv-driver-android`

Wire the badge the domain layer was already written for: the shift screen's
"Documents" link carries the outstanding count, so a refusal reaches the courier
without them going looking for it.

- `ShiftScreen` obtains `ComplianceViewModel`, loads once on entry, and passes
  `outstanding` into `DutyBar`.
- `ShiftViewModel` is left alone. It is a polling offer machine; compliance is
  not its job, and the existing view model already computes exactly this number.

Two invariants this must not break:

- **Nothing may claim the courier cannot work.** Gating still ships off, so a
  courier with four outstanding documents *is* still getting jobs; a message
  describing a rule not in force is one they can immediately disprove. The badge
  is a count, and `ComplianceTest`'s forbidden-phrasings test is extended to
  cover its label.
- **A compliance outage must not break the shift screen.** The load already
  degrades to `failed = true`; the badge simply does not render.

A side effect worth having: `GET /me/profile` creates the profile lazily, so
every courier who opens the app onto the shift screen gets one. That is the
backfill happening organically.

> **Correction, 2026-08-25, found by deploying and running it.** That paragraph
> was false when written. `GET /me/profile` did **not** create anything — only
> `upload_document` and `confirm_document` called `resolve_profile`; the read
> path did two `find_by_entity` lookups and 404'd. Against production it 404'd
> for every courier alive, because `driver.registered` had no publisher before
> #138 and nothing was backfilled.
>
> It was a dead end, not an error: the courier app's compliance screen calls
> only this route on load, so a 404 left it `failed = true` with an empty
> checklist — no row to tap, no upload form behind it, and therefore no way to
> reach the one route that would have created the profile. The screen said "try
> again shortly", which was never going to become true.
>
> `get_my_profile` now falls through to `ensure_profile`, keeping both existing
> lookups ahead of it so the 17 historical `customer`-typed profiles are not
> stranded behind a new empty one. The paragraph above is true as of that fix.

## Explicitly out of scope

- **Admin upload-on-behalf** (`POST /admin/profiles/:id/documents/upload`).
  Still open; a field worker with no app cannot leave `pending_submission`.
- **Telling a courier that compliance is what is withholding work.** field-ops
  computes `block_reason` for the admin roster, but there is no
  `GET /couriers/me` for the courier's own copy. It only matters once
  `ENFORCE_COMPLIANCE=true`, and the app deliberately makes no such claim today.
- **Enforcement itself.** `ENFORCE_COMPLIANCE` stays `false`. Nothing here
  changes when it flips.

## Testing

compliance is in the CI test matrix (fixed 2026-08-23) but has no handler-level
mocks — its 16 tests are pure-function. So:

- Rust: unit tests on the extracted authorization helper, the stored-object
  predicate and the content-type sniffer; `cargo check --all-targets` for the SQL
  join, which cannot be unit-tested without a database.
- End to end: `python scripts/e2e-compliance-loop.py` against a live deployment.
  This is the only check that would have caught any of the three dead ends
  below, and none of the others did.
- Portal: `tsc --noEmit`, clearing `tsconfig.tsbuildinfo` first.
- Android: `testDebugUnitTest`, counted from the XML results rather than from
  `BUILD SUCCESSFUL`.


## Outcome — what shipped, and what this design got wrong

Shipped across four PRs, all merged and deployed 2026-08-25: #140 (`4c9cb778`)
the loop plus a CI fix its own run exposed, #141 (`5bde5913`), #142
(`26d37a66`), #143 (`d1212299`) the e2e script.

**Three dead ends, each of which compiled, passed review, passed CI and deployed
clean. All three were found by running it, none by reading it.**

1. The one this design was written to fix: `file_url` is `s3://bucket/key`,
   rendered into an `<a href>`. The anchor rendered; clicking did nothing.
2. **This design's own fix was equally unreachable.** A presigned URL signs
   against `STORAGE__ENDPOINT`, which is `http://minio:9000` here — a
   compose-network host with no published port and no Traefik route. The
   reviewer's browser cannot resolve it. Replaced with the byte route in #142;
   the OmniDeliv catalog-photo work had hit this wall first and explicitly
   flagged compliance's presign flow as suspect without investigating it.
3. `GET /me/profile` 404'd for anyone with no profile — every courier alive.
   The app calls only that route on load, so PR #139's entire upload surface was
   unreachable for its whole audience. Fixed in #141; see the correction above.

Two further consequences worth keeping:

- **Content type is now sniffed from the file's magic number.** This design said
  PDF-vs-image "cannot be known up front" because there is no `content_type`
  column. That was a failure of imagination — the bytes carry the answer, and
  they are more trustworthy than a declared type would have been.
- **The audit row now marks a real view** rather than the minting of a link that
  may never have been opened.

The governing lesson, recorded in memory as the session's takeaway: *if a
feature has never been exercised against a live deployment, treat it as unbuilt
regardless of what CI says.*
