# OmniDeliv Vendor Console Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give vendors a console to manage their catalog and — critically — to declare stock, because the freshness model the whole substitution design rests on only works if the people who know the stock are the ones updating it.

**Architecture:** A new Next.js 14 App Router application, `apps/vendor-console`, following the existing portal patterns (dark glassmorphism design tokens, JWT via the auth bridge, middleware-guarded routes). Small by design: three pages. The availability toggle is the load-bearing screen and gets optimistic UI with rollback, because a vendor who thinks they marked something out of stock and didn't will send a courier to nothing.

**Tech Stack:** Next.js 14 App Router, TypeScript, TailwindCSS with the shared design tokens, React Server Components where the data is static and client components where it is not.

---

## Dependencies

**Requires Plan 3** — `services/omnideliv` with the catalog API and `set_availability`.

Verify: `curl -sf localhost:8091/health` returns 200.

---

## Why a separate app rather than a page in admin-portal

The cheaper option is to let Partner ops manage vendor catalogs from `apps/admin-portal`. For a five-vendor pilot that works, and it saves an entire application.

It is still the wrong call, for one reason: **the freshness model depends on the declaration being recent.** `Availability::confidence` downgrades an in-stock flag to `Uncertain` after 30 minutes, and the Nutritionist proposes substitutes defensively when it does. If Partner ops toggles stock on behalf of every restaurant, declarations are made by someone who is not standing in the kitchen, at whatever cadence ops can manage — so almost every flag is stale, almost every item warrants a substitute, and the substitution review degrades from "the one decision blocking checkout" to noise the customer learns to dismiss.

The console exists so the person who knows the stock is the one declaring it. That is a product requirement, not a convenience.

**What is deferred:** vendor self-signup, payout dashboards, order acceptance flows. Slice one onboards vendors by hand and this console manages what they sell.

---

## File Structure

**New — `apps/vendor-console/`:**

| File | Responsibility |
|---|---|
| `package.json`, `next.config.js`, `tsconfig.json`, `tailwind.config.ts`, `postcss.config.js` | Scaffold |
| `src/app/layout.tsx`, `globals.css` | Shell + theme |
| `src/app/login/page.tsx` | Sign-in |
| `src/app/(console)/layout.tsx` | Authenticated shell |
| `src/app/(console)/catalog/page.tsx` | Item list + availability toggles |
| `src/app/(console)/profile/page.tsx` | Prep time, hours, status |
| `src/lib/api/client.ts` | Fetch wrapper with JWT |
| `src/lib/api/catalog.ts` | Catalog + availability calls |
| `src/components/AvailabilityToggle.tsx` | The load-bearing control |
| `src/components/FreshnessBadge.tsx` | Shows the vendor how stale their own declaration is |
| `middleware.ts` | Route guard |

---

## Task 1: Scaffold

**Files:**
- Create the files listed above under `apps/vendor-console/`

- [ ] **Step 1: Write `package.json`**

```json
{
  "name": "@logisticos/vendor-console",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "next dev -p 3005",
    "build": "next build",
    "start": "next start -p 3005",
    "lint": "next lint",
    "typecheck": "tsc --noEmit",
    "test": "vitest run"
  },
  "dependencies": {
    "next": "^14.2.0",
    "react": "^18.3.0",
    "react-dom": "^18.3.0"
  },
  "devDependencies": {
    "@types/node": "^20",
    "@types/react": "^18",
    "@types/react-dom": "^18",
    "autoprefixer": "^10.4.0",
    "postcss": "^8.4.0",
    "tailwindcss": "^3.4.0",
    "typescript": "^5.4.0",
    "vitest": "^1.6.0",
    "@testing-library/react": "^15.0.0",
    "@testing-library/jest-dom": "^6.4.0",
    "jsdom": "^24.0.0"
  }
}
```

- [ ] **Step 2: Write the config files**

```js
// apps/vendor-console/next.config.js
/** @type {import('next').NextConfig} */
module.exports = {
  reactStrictMode: true,
  env: {
    NEXT_PUBLIC_OMNIDELIV_API: process.env.NEXT_PUBLIC_OMNIDELIV_API,
    NEXT_PUBLIC_PLATFORM_API: process.env.NEXT_PUBLIC_PLATFORM_API,
  },
};
```

```ts
// apps/vendor-console/tailwind.config.ts
import type { Config } from "tailwindcss";

// Mirrors apps/merchant-portal/src/lib/design-system/tokens.ts. Kept in sync by
// hand for now — extracting @logisticos/ui is a separate piece of work.
const config: Config = {
  content: ["./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        canvas: { DEFAULT: "#050810", 100: "#0d1422", 200: "#111827", 300: "#1a2235" },
        cyan:   { neon: "#00E5FF", glow: "#00B8D9" },
        green:  { signal: "#00FF88" },
        amber:  { signal: "#FFAB00" },
        red:    { signal: "#FF3B5C" },
      },
      boxShadow: {
        "glow-cyan":  "0 0 24px rgba(0,229,255,0.28)",
        "glow-amber": "0 0 24px rgba(255,171,0,0.28)",
      },
      transitionTimingFunction: {
        "spring-out": "cubic-bezier(0.16, 1, 0.3, 1)",
      },
    },
  },
  plugins: [],
};
export default config;
```

```json
// apps/vendor-console/tsconfig.json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["dom", "dom.iterable", "esnext"],
    "allowJs": false,
    "skipLibCheck": true,
    "strict": true,
    "noEmit": true,
    "esModuleInterop": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "jsx": "preserve",
    "incremental": true,
    "plugins": [{ "name": "next" }],
    "paths": { "@/*": ["./src/*"] }
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
  "exclude": ["node_modules"]
}
```

```js
// apps/vendor-console/postcss.config.js
module.exports = { plugins: { tailwindcss: {}, autoprefixer: {} } };
```

- [ ] **Step 3: Install and verify**

```bash
cd apps/vendor-console && npm install && npx tsc --noEmit
```

Expected: type-check passes with no source files yet (or reports missing `src/` — that resolves in Task 2).

- [ ] **Step 4: Commit**

```bash
git add apps/vendor-console/
git commit -m "feat(vendor-console): scaffold Next.js app with shared design tokens"
```

---

## Task 2: API client and auth

**Files:**
- Create: `src/lib/api/client.ts`, `src/lib/api/catalog.ts`, `middleware.ts`

- [ ] **Step 1: Write the client**

```ts
// apps/vendor-console/src/lib/api/client.ts
/**
 * Fetch wrapper for the OmniDeliv API.
 *
 * The JWT lives in an httpOnly cookie set by the auth bridge, so it is never
 * readable from JS and `credentials: "include"` is what carries it. Nothing
 * here reads or stores a token.
 */

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

const BASE = process.env.NEXT_PUBLIC_OMNIDELIV_API ?? "http://localhost:8091";

export async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    credentials: "include",
    headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
  });

  if (res.status === 401) {
    // The session expired. Bounce to login rather than rendering an empty
    // catalog, which a vendor would reasonably read as "I have no items".
    if (typeof window !== "undefined") window.location.href = "/login";
    throw new ApiError(401, "Session expired");
  }

  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new ApiError(res.status, body || res.statusText);
  }

  return res.json() as Promise<T>;
}
```

- [ ] **Step 2: Write the catalog calls**

```ts
// apps/vendor-console/src/lib/api/catalog.ts
import { apiFetch } from "./client";

export type AvailabilityState = "available" | "limited" | "out_of_stock";

export interface CatalogItem {
  item_id: string;
  name: string;
  price_cents: number;
  availability: AvailabilityState;
  /** ISO 8601. How long ago this vendor last declared the state. */
  availability_updated_at: string;
}

export function listItems(vendorId: string): Promise<CatalogItem[]> {
  return apiFetch<CatalogItem[]>(`/v1/omnideliv/catalog/items?vendor_id=${vendorId}`);
}

export function setAvailability(itemId: string, state: AvailabilityState): Promise<void> {
  return apiFetch<void>(`/v1/omnideliv/catalog/items/${itemId}/availability`, {
    method: "PUT",
    body: JSON.stringify({ state }),
  });
}
```

> **Two endpoints Plan 3 did not build.** `GET /v1/omnideliv/catalog/items?vendor_id=` and `PUT /v1/omnideliv/catalog/items/:id/availability` are vendor-facing; Plan 3 built only the agent-facing `GET /v1/omnideliv/catalog/search`. Add both in Task 3 Step 1 before wiring the UI — they are a thin pass-through to `CatalogRepository::list_for_vendor` and `CatalogService::set_availability`, both of which already exist.

- [ ] **Step 3: Write the route guard**

```ts
// apps/vendor-console/middleware.ts
import { NextResponse } from "next/server";
import type { NextRequest } from "next/server";

export function middleware(req: NextRequest) {
  const session = req.cookies.get("session");
  if (!session) {
    const url = req.nextUrl.clone();
    url.pathname = "/login";
    // Preserve where they were going so login can return them there.
    url.searchParams.set("returnTo", req.nextUrl.pathname);
    return NextResponse.redirect(url);
  }
  return NextResponse.next();
}

export const config = {
  matcher: ["/catalog/:path*", "/profile/:path*"],
};
```

- [ ] **Step 4: Commit**

```bash
git add apps/vendor-console/
git commit -m "feat(vendor-console): API client, catalog calls and route guard"
```

---

## Task 3: The availability toggle

The load-bearing screen. A vendor who believes they marked something out of stock and didn't will send a courier to nothing — so the control must never show a state the server has not accepted.

**Files:**
- Modify: `services/omnideliv/src/api/http/catalog.rs` (two vendor-facing endpoints)
- Create: `src/components/FreshnessBadge.tsx`, `src/components/AvailabilityToggle.tsx`, `src/components/__tests__/AvailabilityToggle.test.tsx`

- [ ] **Step 1: Add the two vendor-facing endpoints**

In `services/omnideliv/src/api/http/catalog.rs`, add alongside the existing `search` route:

```rust
    Router::new()
        .route("/v1/omnideliv/catalog/search", get(search))
        .route("/v1/omnideliv/catalog/items", get(list_items))
        .route("/v1/omnideliv/catalog/items/:id/availability", put(set_availability))
```

`list_items` calls `CatalogRepository::list_for_vendor` and returns `item_id`, `name`, `price_cents`, `availability` and `availability_updated_at`. `set_availability` parses the state string and calls `CatalogService::set_availability`, returning `204`. Both read `tenant_id` from validated `Claims`, never from the request.

- [ ] **Step 2: Write the freshness badge**

```tsx
// apps/vendor-console/src/components/FreshnessBadge.tsx
"use client";

/**
 * Shows the vendor how stale their own declaration is.
 *
 * This is not decoration. The agent downgrades an in-stock flag to uncertain
 * after the freshness window and starts proposing substitutes — so a vendor who
 * can see their flag going stale has a reason to refresh it. Hiding the age
 * would leave them wondering why customers keep getting offered alternatives.
 */

const FRESH_WINDOW_MINUTES = 30;

export function FreshnessBadge({ updatedAt }: { updatedAt: string }) {
  const ageMins = Math.floor((Date.now() - new Date(updatedAt).getTime()) / 60_000);
  const stale = ageMins > FRESH_WINDOW_MINUTES;

  const label =
    ageMins < 1 ? "just now"
    : ageMins < 60 ? `${ageMins}m ago`
    : ageMins < 1440 ? `${Math.floor(ageMins / 60)}h ago`
    : `${Math.floor(ageMins / 1440)}d ago`;

  return (
    <span
      data-testid="freshness"
      data-stale={stale}
      title={
        stale
          ? "Customers are being offered substitutes for this item because the stock information is old. Confirm it to stop that."
          : "Recent enough that customers see this as reliably in stock."
      }
      className={
        stale
          ? "text-xs text-amber-signal/90 border border-amber-signal/40 rounded-full px-2 py-0.5"
          : "text-xs text-white/40"
      }
    >
      {label}
    </span>
  );
}
```

- [ ] **Step 3: Write the failing toggle test**

```tsx
// apps/vendor-console/src/components/__tests__/AvailabilityToggle.test.tsx
import { describe, it, expect, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AvailabilityToggle } from "../AvailabilityToggle";

describe("AvailabilityToggle", () => {
  it("shows the new state immediately so the control feels responsive", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    render(<AvailabilityToggle itemId="i1" initial="available" onSave={save} />);

    await userEvent.click(screen.getByRole("button", { name: /out of stock/i }));

    expect(screen.getByRole("button", { name: /out of stock/i })).toHaveAttribute("aria-pressed", "true");
    expect(save).toHaveBeenCalledWith("i1", "out_of_stock");
  });

  /**
   * The critical case. A vendor who believes they marked something out of stock
   * and didn't will send a courier to nothing — so a failed save must visibly
   * revert, not silently leave the optimistic state on screen.
   */
  it("reverts and surfaces an error when the save fails", async () => {
    const save = vi.fn().mockRejectedValue(new Error("network"));
    render(<AvailabilityToggle itemId="i1" initial="available" onSave={save} />);

    await userEvent.click(screen.getByRole("button", { name: /out of stock/i }));

    await waitFor(() => {
      expect(screen.getByRole("button", { name: /^available$/i })).toHaveAttribute("aria-pressed", "true");
    });
    expect(screen.getByRole("alert")).toHaveTextContent(/not saved/i);
  });

  it("is disabled while a save is in flight so a double-tap cannot race", async () => {
    let resolve!: () => void;
    const save = vi.fn(() => new Promise<void>((r) => { resolve = r; }));
    render(<AvailabilityToggle itemId="i1" initial="available" onSave={save} />);

    await userEvent.click(screen.getByRole("button", { name: /out of stock/i }));
    expect(screen.getByRole("button", { name: /limited/i })).toBeDisabled();

    resolve();
    await waitFor(() => expect(screen.getByRole("button", { name: /limited/i })).toBeEnabled());
  });

  it("does not call save when the state is unchanged", async () => {
    const save = vi.fn().mockResolvedValue(undefined);
    render(<AvailabilityToggle itemId="i1" initial="available" onSave={save} />);

    await userEvent.click(screen.getByRole("button", { name: /^available$/i }));
    expect(save).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 4: Run it to verify it fails**

```bash
cd apps/vendor-console && npx vitest run
```

Expected: FAIL — `Cannot find module '../AvailabilityToggle'`.

- [ ] **Step 5: Write the toggle**

```tsx
// apps/vendor-console/src/components/AvailabilityToggle.tsx
"use client";

import { useState } from "react";
import type { AvailabilityState } from "@/lib/api/catalog";

const OPTIONS: { value: AvailabilityState; label: string }[] = [
  { value: "available",    label: "Available" },
  { value: "limited",      label: "Limited" },
  { value: "out_of_stock", label: "Out of stock" },
];

export function AvailabilityToggle({
  itemId,
  initial,
  onSave,
}: {
  itemId: string;
  initial: AvailabilityState;
  onSave: (itemId: string, state: AvailabilityState) => Promise<void>;
}) {
  const [state, setState] = useState<AvailabilityState>(initial);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function select(next: AvailabilityState) {
    if (next === state || saving) return;

    const previous = state;
    // Optimistic: the control must feel instant, because a vendor tapping
    // through twenty items during a rush will not wait on a round trip.
    setState(next);
    setSaving(true);
    setError(null);

    try {
      await onSave(itemId, next);
    } catch {
      // Revert visibly. Leaving the optimistic state on screen would let the
      // vendor believe stock is marked when the server never heard about it —
      // and a courier would be sent to collect something that is not there.
      setState(previous);
      setError("Not saved — check your connection and try again.");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex flex-col gap-1">
      <div className="flex gap-1" role="group" aria-label="Stock status">
        {OPTIONS.map((o) => {
          const active = state === o.value;
          return (
            <button
              key={o.value}
              type="button"
              aria-pressed={active}
              disabled={saving}
              onClick={() => select(o.value)}
              className={[
                "px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 ease-spring-out",
                "disabled:opacity-50 disabled:cursor-not-allowed",
                active && o.value === "available"    ? "bg-green-signal/15 text-green-signal border border-green-signal/40" : "",
                active && o.value === "limited"      ? "bg-amber-signal/15 text-amber-signal border border-amber-signal/40" : "",
                active && o.value === "out_of_stock" ? "bg-red-signal/15 text-red-signal border border-red-signal/40" : "",
                !active ? "bg-white/5 text-white/50 border border-white/10 hover:border-white/25" : "",
              ].join(" ")}
            >
              {o.label}
            </button>
          );
        })}
      </div>
      {error && (
        <p role="alert" className="text-xs text-red-signal">
          {error}
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 6: Run the tests**

```bash
cd apps/vendor-console && npx vitest run
```

Expected: PASS — 4 passed.

- [ ] **Step 7: Commit**

```bash
git add apps/vendor-console/ services/omnideliv/src/api/
git commit -m "feat(vendor-console): availability toggle with rollback on failure

A failed save reverts visibly rather than leaving the optimistic state on
screen — a vendor who believes stock is marked when the server never heard
about it will send a courier to collect something that is not there. The
freshness badge shows the vendor how stale their own declaration is, which is
what gives them a reason to refresh it."
```

---

## Task 4: Catalog page and shell

**Files:**
- Create: `src/app/layout.tsx`, `src/app/globals.css`, `src/app/(console)/layout.tsx`, `src/app/(console)/catalog/page.tsx`, `src/app/login/page.tsx`

- [ ] **Step 1: Write the shell**

```tsx
// apps/vendor-console/src/app/layout.tsx
import type { Metadata } from "next";
import "./globals.css";

export const metadata: Metadata = {
  title: "Vendor Console",
  description: "Manage your menu and stock",
};

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html lang="en">
      <body className="bg-canvas text-white min-h-screen antialiased">{children}</body>
    </html>
  );
}
```

```css
/* apps/vendor-console/src/app/globals.css */
@tailwind base;
@tailwind components;
@tailwind utilities;

:root { color-scheme: dark; }

body {
  background-image:
    radial-gradient(60% 40% at 10% 0%, rgba(0, 229, 255, 0.06), transparent 70%),
    radial-gradient(50% 35% at 95% 5%, rgba(168, 85, 247, 0.06), transparent 70%);
}
```

- [ ] **Step 2: Write the catalog page**

```tsx
// apps/vendor-console/src/app/(console)/catalog/page.tsx
"use client";

import { useEffect, useState } from "react";
import { AvailabilityToggle } from "@/components/AvailabilityToggle";
import { FreshnessBadge } from "@/components/FreshnessBadge";
import { listItems, setAvailability, type CatalogItem } from "@/lib/api/catalog";

export default function CatalogPage() {
  const [items, setItems] = useState<CatalogItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  // The vendor id comes from the session's claims via the API — the client
  // never supplies it, or one vendor could read another's catalog.
  useEffect(() => {
    listItems("me")
      .then(setItems)
      .catch((e) => setError(e.message));
  }, []);

  if (error) {
    return (
      <div role="alert" className="p-8 text-red-signal">
        Could not load your menu: {error}
      </div>
    );
  }

  if (!items) {
    return <div className="p-8 text-white/40">Loading your menu…</div>;
  }

  const stale = items.filter(
    (i) => Date.now() - new Date(i.availability_updated_at).getTime() > 30 * 60_000
  ).length;

  return (
    <div className="p-6 max-w-4xl mx-auto">
      <header className="mb-6">
        <h1 className="text-2xl font-semibold">Your menu</h1>
        {stale > 0 && (
          <p className="mt-2 text-sm text-amber-signal">
            {stale} item{stale === 1 ? "" : "s"} haven&apos;t been confirmed in a while. Customers
            are being offered substitutes for {stale === 1 ? "it" : "them"} — confirm to stop that.
          </p>
        )}
      </header>

      <ul className="space-y-2">
        {items.map((item) => (
          <li
            key={item.item_id}
            className="flex items-center justify-between gap-4 rounded-xl border border-white/10 bg-white/[0.03] backdrop-blur px-4 py-3"
          >
            <div className="min-w-0">
              <p className="truncate font-medium">{item.name}</p>
              <p className="text-sm text-white/40">
                ₱{(item.price_cents / 100).toFixed(2)}{" "}
                <FreshnessBadge updatedAt={item.availability_updated_at} />
              </p>
            </div>
            <AvailabilityToggle
              itemId={item.item_id}
              initial={item.availability}
              onSave={setAvailability}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}
```

- [ ] **Step 3: Write the console layout and login**

```tsx
// apps/vendor-console/src/app/(console)/layout.tsx
export default function ConsoleLayout({ children }: { children: React.ReactNode }) {
  return (
    <div className="min-h-screen">
      <nav className="border-b border-white/10 px-6 py-3 flex gap-6 text-sm">
        <a href="/catalog" className="hover:text-cyan-neon transition-colors">Menu</a>
        <a href="/profile" className="hover:text-cyan-neon transition-colors">Store details</a>
      </nav>
      {children}
    </div>
  );
}
```

```tsx
// apps/vendor-console/src/app/login/page.tsx
"use client";

import { useState } from "react";

export default function LoginPage() {
  const [email, setEmail] = useState("");
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function submit(e: React.FormEvent) {
    e.preventDefault();
    setError(null);
    try {
      const res = await fetch("/api/auth/magic-link", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        // Normalised to lowercase: the identity service matches emails
        // case-sensitively, so a capitalised address silently fails to link.
        body: JSON.stringify({ email: email.trim().toLowerCase() }),
      });
      if (!res.ok) throw new Error(await res.text());
      setSent(true);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Could not send the link");
    }
  }

  if (sent) {
    return (
      <main className="min-h-screen grid place-items-center p-6">
        <p className="text-white/70">Check your email for a sign-in link.</p>
      </main>
    );
  }

  return (
    <main className="min-h-screen grid place-items-center p-6">
      <form onSubmit={submit} className="w-full max-w-sm space-y-4">
        <h1 className="text-xl font-semibold">Vendor sign-in</h1>
        <input
          type="email"
          required
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          placeholder="you@yourstore.com"
          className="w-full rounded-lg bg-white/5 border border-white/10 px-3 py-2 outline-none focus:border-cyan-neon/50"
        />
        <button
          type="submit"
          className="w-full rounded-lg bg-cyan-neon text-canvas font-semibold py-2 shadow-glow-cyan"
        >
          Send sign-in link
        </button>
        {error && <p role="alert" className="text-sm text-red-signal">{error}</p>}
      </form>
    </main>
  );
}
```

- [ ] **Step 4: Verify it builds**

```bash
cd apps/vendor-console && npx tsc --noEmit && npm run build
```

Expected: both pass.

- [ ] **Step 5: Commit**

```bash
git add apps/vendor-console/
git commit -m "feat(vendor-console): catalog page, shell and magic-link sign-in

The catalog page surfaces a count of stale items and says plainly what stale
means for the vendor — that customers are being offered substitutes — because
the freshness model only works if the vendor has a reason to keep it current."
```

---

## Task 5: Store details page and CI

**Files:**
- Create: `src/app/(console)/profile/page.tsx`
- Modify: `.github/workflows/ci-frontend.yml`

- [ ] **Step 1: Write the profile page**

```tsx
// apps/vendor-console/src/app/(console)/profile/page.tsx
"use client";

import { useEffect, useState } from "react";
import { apiFetch } from "@/lib/api/client";

interface VendorProfile {
  name: string;
  address: string;
  prep_time_minutes: number;
  status: "onboarding" | "active" | "paused" | "offboarded";
}

export default function ProfilePage() {
  const [profile, setProfile] = useState<VendorProfile | null>(null);
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    apiFetch<VendorProfile>("/v1/omnideliv/vendors/me").then(setProfile).catch(() => setProfile(null));
  }, []);

  async function save(next: Partial<VendorProfile>) {
    if (!profile) return;
    setSaving(true);
    setMessage(null);
    try {
      await apiFetch("/v1/omnideliv/vendors/me", { method: "PATCH", body: JSON.stringify(next) });
      setProfile({ ...profile, ...next });
      setMessage("Saved.");
    } catch {
      setMessage("Could not save — try again.");
    } finally {
      setSaving(false);
    }
  }

  if (!profile) return <div className="p-8 text-white/40">Loading…</div>;

  return (
    <div className="p-6 max-w-2xl mx-auto space-y-6">
      <h1 className="text-2xl font-semibold">{profile.name}</h1>

      <section className="rounded-xl border border-white/10 bg-white/[0.03] p-4 space-y-3">
        <label className="block text-sm text-white/60" htmlFor="prep">
          Typical preparation time
        </label>
        <input
          id="prep"
          type="number"
          min={0}
          max={180}
          disabled={saving}
          value={profile.prep_time_minutes}
          onChange={(e) => setProfile({ ...profile, prep_time_minutes: Number(e.target.value) })}
          onBlur={() => save({ prep_time_minutes: profile.prep_time_minutes })}
          className="w-28 rounded-lg bg-white/5 border border-white/10 px-3 py-2 outline-none focus:border-cyan-neon/50"
        />
        <p className="text-xs text-white/40">
          Minutes from order to ready. Used to decide the courier&apos;s pickup order —
          if this is too low the courier arrives before the food is ready; too high
          and it sits waiting.
        </p>
      </section>

      <section className="rounded-xl border border-white/10 bg-white/[0.03] p-4">
        <div className="flex items-center justify-between">
          <div>
            <p className="text-sm font-medium">
              {profile.status === "active" ? "Accepting orders" : "Not accepting orders"}
            </p>
            <p className="text-xs text-white/40">
              Pausing stops new orders immediately. Orders already placed still stand.
            </p>
          </div>
          <button
            type="button"
            disabled={saving}
            onClick={() => save({ status: profile.status === "active" ? "paused" : "active" })}
            className={
              profile.status === "active"
                ? "px-4 py-2 rounded-lg text-sm bg-white/5 border border-white/15"
                : "px-4 py-2 rounded-lg text-sm bg-green-signal/15 text-green-signal border border-green-signal/40"
            }
          >
            {profile.status === "active" ? "Pause" : "Resume"}
          </button>
        </div>
      </section>

      {message && <p className="text-sm text-white/60">{message}</p>}
    </div>
  );
}
```

> **`GET`/`PATCH /v1/omnideliv/vendors/me` are not yet built.** Add them to `services/omnideliv/src/api/http/` alongside the catalog routes, resolving the vendor from the session's claims — the same "never take the identity from the client" rule as `list_items`.

- [ ] **Step 2: Add the app to frontend CI**

In `.github/workflows/ci-frontend.yml`, add `vendor-console` to the app matrix alongside the existing portals.

- [ ] **Step 3: Verify**

```bash
cd apps/vendor-console && npx tsc --noEmit && npx vitest run && npm run build
```

Expected: all three pass.

- [ ] **Step 4: Commit**

```bash
git add apps/vendor-console/ .github/workflows/ci-frontend.yml
git commit -m "feat(vendor-console): store details page and CI wiring

The prep-time field explains what it is actually used for, because a vendor
who under-reports it gets a courier waiting in their doorway and a vendor who
over-reports it gets cold food."
```

---

## Definition of done

- [ ] `npx vitest run` — 4 tests pass
- [ ] `npx tsc --noEmit` — clean
- [ ] `npm run build` — succeeds
- [ ] A vendor can sign in, see their menu, and change an item's stock state
- [ ] Killing the API mid-toggle reverts the control and shows an error rather than leaving the optimistic state

## Follow-on work this surfaces

1. **Four endpoints this plan assumes.** `GET /v1/omnideliv/catalog/items`, `PUT /v1/omnideliv/catalog/items/:id/availability`, `GET /v1/omnideliv/vendors/me`, `PATCH /v1/omnideliv/vendors/me` are added here rather than in Plan 3 because Plan 3 built only the agent-facing surface. All four resolve identity from claims.
2. **Design tokens are duplicated by hand.** `tailwind.config.ts` mirrors `apps/merchant-portal/src/lib/design-system/tokens.ts`. Extracting `@logisticos/ui` as a real package is overdue and now has a third consumer arguing for it.
3. **Vendor self-signup.** Slice one onboards by hand. A vendor who can sign up needs verification, payout-account capture and a terms flow — its own plan.
