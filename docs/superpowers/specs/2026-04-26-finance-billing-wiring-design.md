# Finance, Billing & Invoicing Wiring — Design Spec
**Date:** 2026-04-26  
**Scope:** Merchant Portal billing page, Partner Portal payouts page, Customer App wallet screen  
**Out of scope:** Admin Portal (already live), Driver App (follow-on sprint)

---

## 1. Goals

Connect the three remaining finance surfaces to the live payments service. All required backend endpoints already exist. No new Rust code needed. This is purely frontend wiring + API client creation for Partner Portal.

---

## 2. Architecture & Data Flow

All three apps talk to the payments service via HTTP. JWT claims scope data server-side (tenant isolation, role filtering). No new backend endpoints.

```
Merchant Portal  →  billing.ts (exists, 8 methods, unused)   →  /v1/invoices, /v1/wallet, /v1/cod
Partner Portal   →  payments.ts (new, 4 methods)             →  /v1/invoices, /v1/wallet, /v1/wallet/transactions, /v1/wallet/withdraw
Customer App     →  payments.ts (exists, unused)             →  /v1/wallet, /v1/wallet/transactions, /v1/wallet/withdraw
```

**Write path:** Only `POST /v1/wallet/withdraw` is a mutation. Pattern: disable button → call API → refresh balance on success, show inline error on failure (no optimistic update — balance must reflect server state).

---

## 3. Merchant Portal — `apps/merchant-portal/src/app/(dashboard)/billing/page.tsx`

### Current state
100% static mock. `billing.ts` API client exists with all required methods but is never imported by the page.

### Changes

**Summary cards (top row, 3 cards):**
| Card | API call | Field |
|------|----------|-------|
| COD Balance | `getCodBalance()` | `balance_cents` → formatted ₱ |
| Wallet Balance | `getWallet()` | `balance_cents` → formatted ₱ |
| Outstanding | `listInvoices({ status: 'pending' })` | `total` count |

**Invoice table:**
- Replace hardcoded array with `listInvoices()` response
- Columns: Invoice #, Date, Amount, Type, Status — same as current mock layout
- Client-side filter tabs: All / Pending / Paid / Overdue (filter `status` field locally)
- Pagination: show first 50, add "Load more" if `total > 50`

**States:**
- Loading: skeleton rows (3 cards + 5 table rows)
- Error: red banner at top with message + retry button
- Empty: "No invoices yet" placeholder in table body

**Files touched:**
- `apps/merchant-portal/src/app/(dashboard)/billing/page.tsx` — full rewrite, keep existing layout structure

---

## 4. Partner Portal — `apps/partner-portal/src/app/(dashboard)/payouts/page.tsx`

### Current state
Hybrid mock — attempts an API call, catches the error, falls back to `PAYOUT_HISTORY_DEFAULT` hardcoded array. No payments API client exists in the partner portal.

### New file: `apps/partner-portal/src/lib/api/payments.ts`

Four methods, mirroring merchant-portal's billing.ts pattern:

```typescript
getWallet(): Promise<Wallet>                          // GET /v1/wallet
getTransactions(): Promise<WalletTransaction[]>       // GET /v1/wallet/transactions
getInvoices(): Promise<Invoice[]>                     // GET /v1/invoices
withdraw(amount_cents: number): Promise<Wallet>       // POST /v1/wallet/withdraw
```

Types: `Wallet { balance_cents, currency }`, `WalletTransaction { id, amount_cents, type, description, created_at }`, `Invoice { id, number, amount_cents, status, type, created_at }`.

### Page layout (top → bottom)

1. **Wallet card** — balance display (large ₱ figure) + "Request Withdrawal" button
2. **Withdrawal modal** — amount input (₱, max = wallet balance), Confirm / Cancel. On confirm: call `withdraw()`, close modal, refresh wallet balance. Show inline error if rejected.
3. **Transaction history** — scrollable list of recent wallet transactions (last 20)
4. **Invoice table** — same filter tabs as merchant (All / Pending / Paid), columns: Invoice #, Date, Amount, Status

**Remove:** All `PAYOUT_HISTORY_DEFAULT` fallback data and the try/catch that silently swallows errors.

**Files touched:**
- `apps/partner-portal/src/lib/api/payments.ts` — new file
- `apps/partner-portal/src/app/(dashboard)/payouts/page.tsx` — full rewrite

---

## 5. Customer App — new `WalletScreen`

### Current state
`payments.ts` exists with `getWallet()`, `getTransactions()`, `withdraw()` — all unused. No wallet screen exists. Invoices screen is live.

### New file: `apps/customer-app/src/screens/wallet/WalletScreen.tsx`

**Layout:**
- **Balance card** — large centered ₱ balance, currency label, subtle glow consistent with app design system
- **"Withdraw" button** — below balance card; opens bottom sheet
- **Bottom sheet** — amount input, confirm button, inline error display
- **Transaction FlatList** — below the balance card, most recent first, each row: type icon + description + amount + date

**Navigation:** Invoices are a Stack screen (not a tab) in `AppNavigator.tsx` — Wallet follows the same pattern. Add as a Stack screen accessible via a "Wallet" row on the Profile screen (existing tab). Avoids crowding the already-full 6-tab bar.

**States:**
- Loading: ActivityIndicator centered
- Error: inline error with retry
- Empty transactions: "No transactions yet" text

**Files touched:**
- `apps/customer-app/src/screens/wallet/WalletScreen.tsx` — new file
- `apps/customer-app/src/navigation/AppNavigator.tsx` — add `Wallet` Stack screen + deep-link route
- `apps/customer-app/src/screens/profile/ProfileScreen.tsx` — add "Wallet" navigation row

---

## 6. Shared Patterns

- All monetary values stored as cents (integer), displayed as ₱N,NNN using a shared `fmtPhp(cents)` helper (already exists in `apps/partner-portal/src/lib/api/carriers.ts` — import or duplicate per app)
- All pages use the existing `GlassCard` / `NeonBadge` design system components — no new UI primitives
- Error messages surfaced verbatim from API response `message` field; fallback to generic string if absent

---

## 7. Out of Scope

- Driver App earnings/wallet screens — follow-on sprint
- Admin Portal finance — already live, no changes
- New backend endpoints — all required endpoints exist
- Pagination beyond "Load more" (infinite scroll, page controls) — not needed at current data volumes
- Push notifications on withdrawal approval — engagement engine work, separate feature
