# Auth Bridge — Secret Rotation Runbook

**Owner:** Senior Rust Engineer — Identity & Auth
**Backup Owner:** Staff Platform Engineer / SRE Lead
**Last Reviewed:** 2026-04-15
**Related ADRs:** ADR-0011 (Firebase → LogisticOS JWT bridge), ADR-0008 (RLS)

---

## What this runbook covers

The Firebase → LogisticOS JWT bridge relies on a small set of long-lived secrets. This runbook covers rotation for each, plus the emergency revocation path.

| Secret | Held by | Blast radius on compromise |
|---|---|---|
| `LOGISTICOS_INTERNAL_SECRET` | identity + landing + 4 portals | Attacker can mint LoS JWTs for any Firebase UID → **full tenant impersonation** |
| `LOGISTICOS_JWT_SIGNING_KEY` | identity only | Attacker can forge any LoS JWT → full platform takeover |
| Firebase Admin service account JSON | identity (ID token verification) | Attacker can mint Firebase ID tokens → can then call the exchange endpoint |

> **Rule of thumb:** treat `LOGISTICOS_INTERNAL_SECRET` like a production database password. Any leak is a pageable incident.

---

## 1. Routine rotation (quarterly)

Target cadence: **every 90 days**, plus immediately on any team member offboarding who had Vault access.

### 1.1 Pre-rotation checklist

- [ ] Confirm change window — prefer Tuesday/Wednesday 10:00–14:00 SGT (see `deployment.md`).
- [ ] Confirm `scripts/auth-bridge-smoke.sh` passes against current staging (baseline).
- [ ] Confirm latest `staging` deploys of identity, landing, and all four portals are on the **same commit or newer** of `libs/auth` — older builds may not support dual-secret verification.
- [ ] Notify `#eng-platform` and `#eng-frontend` channels 24h in advance.

### 1.2 Rotation steps

Identity supports **dual-secret verification**: if both `LOGISTICOS_INTERNAL_SECRET` and `LOGISTICOS_INTERNAL_SECRET_NEXT` are set, either one is accepted on inbound requests. Signing still uses the primary.

1. **Generate the new secret** (64 bytes, base64):

   ```bash
   openssl rand -base64 48
   ```

2. **Write as `_NEXT` in Vault** on every environment (staging first, then prod):

   ```bash
   vault kv patch secret/logisticos/identity LOGISTICOS_INTERNAL_SECRET_NEXT=<new>
   vault kv patch secret/logisticos/landing  LOGISTICOS_INTERNAL_SECRET_NEXT=<new>
   vault kv patch secret/logisticos/merchant-portal LOGISTICOS_INTERNAL_SECRET_NEXT=<new>
   vault kv patch secret/logisticos/admin-portal    LOGISTICOS_INTERNAL_SECRET_NEXT=<new>
   vault kv patch secret/logisticos/partner-portal  LOGISTICOS_INTERNAL_SECRET_NEXT=<new>
   vault kv patch secret/logisticos/customer-portal LOGISTICOS_INTERNAL_SECRET_NEXT=<new>
   ```

3. **Trigger rolling restart** of identity + all portals so they pick up `_NEXT`:

   ```bash
   kubectl -n logisticos rollout restart deploy/identity deploy/landing \
     deploy/merchant-portal deploy/admin-portal deploy/partner-portal deploy/customer-portal
   ```

   Wait for all rollouts to complete. `auth-bridge-smoke.sh` should still pass (it uses the **current** primary secret).

4. **Promote `_NEXT` to primary** — swap the two values in Vault:

   ```bash
   # Read current values
   CURRENT=$(vault kv get -field=LOGISTICOS_INTERNAL_SECRET secret/logisticos/identity)
   NEXT=$(vault kv get -field=LOGISTICOS_INTERNAL_SECRET_NEXT secret/logisticos/identity)

   # For each secret path:
   vault kv patch secret/logisticos/identity \
     LOGISTICOS_INTERNAL_SECRET="$NEXT" \
     LOGISTICOS_INTERNAL_SECRET_NEXT="$CURRENT"
   # …repeat for landing + 4 portals
   ```

5. **Rolling restart again.** Now all components sign with the new secret and still accept the old one.

6. **Re-run smoke** with the new secret exported locally:

   ```bash
   export LOGISTICOS_INTERNAL_SECRET=<new>
   ./scripts/auth-bridge-smoke.sh
   ```

7. **Wait 24h**, then remove `_NEXT`:

   ```bash
   vault kv patch secret/logisticos/identity LOGISTICOS_INTERNAL_SECRET_NEXT=
   # …repeat for all services
   kubectl -n logisticos rollout restart deploy/identity ...
   ```

8. **Update CI secrets** in GitHub Actions:
   - `STAGING_INTERNAL_SECRET` → new value
   - `PROD_INTERNAL_SECRET` → new value (if we publish it)

### 1.3 Rollback

If smoke fails at step 6: re-swap Vault values back (old is now `_NEXT`, still accepted), rolling-restart, investigate. No user-visible impact because both secrets remained accepted throughout.

---

## 2. Emergency rotation (suspected compromise)

**When to trigger:** any leak of `LOGISTICOS_INTERNAL_SECRET` — accidentally committed, seen in logs, shared in a forwarded ticket, or unusual `/v1/internal/auth/exchange-firebase` traffic in Grafana.

### 2.1 Immediate actions (within 15 min)

1. **Page the on-call Identity engineer and CISO.** Open incident in Linear.
2. **Generate a new secret** (`openssl rand -base64 48`).
3. **Set it as the primary** in Vault on all services — skip the `_NEXT` dance.
4. **Rolling restart** identity + landing + 4 portals in parallel:

   ```bash
   kubectl -n logisticos rollout restart deploy/identity deploy/landing \
     deploy/merchant-portal deploy/admin-portal deploy/partner-portal deploy/customer-portal
   ```

5. **Accept the ~60s window** where some pods still have the old secret. Existing user sessions are unaffected — they rely on already-issued `__los_at` / `__los_rt` cookies, not the internal secret.

### 2.2 Follow-up actions (within 1h)

- [ ] Revoke all LogisticOS refresh tokens issued in the compromise window (SQL: `UPDATE refresh_tokens SET revoked_at = now() WHERE created_at > <leak_time>`). Users re-authenticate via Firebase; legitimate sessions recover transparently.
- [ ] Audit identity logs for `exchange-firebase` calls during the window. Flag any Firebase UID whose associated tenant was changed or created.
- [ ] Run `scripts/auth-bridge-smoke.sh` against prod and staging.
- [ ] Post-mortem started in `docs/incidents/<date>-internal-secret-leak.md`.

---

## 3. JWT signing key rotation

`LOGISTICOS_JWT_SIGNING_KEY` is rotated less frequently — **every 180 days or on compromise**. The procedure is structurally identical to §1 (dual-key verification via `LOGISTICOS_JWT_SIGNING_KEY_NEXT`), but with one extra consideration:

- Existing LoS access tokens (~15m lifetime) and refresh tokens (~30d) signed with the old key will continue to verify until the `_NEXT` slot is cleared. Clearing `_NEXT` after <30d would cut off active sessions — schedule the final cleanup step **31 days** after promotion.

## 4. Firebase Admin service account rotation

Handled in Firebase Console → IAM → Service Accounts. Generate a new JSON key, upload to `secret/logisticos/identity/firebase-admin.json`, rolling-restart identity. Old key remains valid until explicitly deleted in the console; delete it only after verifying `exchange-firebase` smoke passes.

---

## 5. Verification checklist (post-rotation)

Run these against prod regardless of rotation type:

- [ ] `./scripts/auth-bridge-smoke.sh` returns exit 0 with `LOGISTICOS_INTERNAL_SECRET` set to the new value
- [ ] A fresh Firebase sign-in on `os.cargomarket.net/merchant` lands on the dashboard (no "Not authenticated" error)
- [ ] The identity Grafana dashboard shows no spike in `exchange_firebase_auth_failures_total`
- [ ] The E2E auth workflow in GitHub Actions is green on the latest `main` commit

---

## References

- ADR-0011 — architectural rationale for the bridge
- `scripts/auth-bridge-smoke.sh` — curl-based post-rotation check
- `scripts/e2e-seed.sh` — token seeding used by Playwright
- `.github/workflows/ci-e2e-auth.yml` — CI coverage for the bridge
