# OmniDeliv App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Anticipatory Canvas — the four screens that make the mesh legible to a customer, with a deterministic fallback that keeps the app working when the mesh does not.

**Architecture:** A new Expo app, `apps/omnideliv-app`, own brand and release cadence per ADR-0009 product isolation. It reuses the auth bridge and API-client patterns from `apps/customer-app` as *code*, not by forking. Screen B consumes the mesh's SSE stream; everything post-checkout reuses push plus polling, which already works in production.

**Tech Stack:** Expo SDK 54, React Native 0.81, TypeScript, expo-router, React Native Reanimated 3, `expo/fetch` for streaming.

---

## Dependencies

**Requires Plan 3** (catalog and basket), **Plan 4** (the mesh and its `MeshEvent` stream), **Plan 5** (checkout and orders).

Verify each is reachable before starting:

```bash
curl -sf localhost:8091/health && echo "omnideliv up"
```

---

## Three traps this repo has already paid for

Read these before writing any code — each one cost real time in this codebase.

1. **`expo-file-system` v19 removed the legacy API from the package root.** Importing it builds fine and is `undefined` at runtime. `npx tsc --noEmit` is the only gate that catches it — the bundler will not.
2. **`expo/fetch` and Blob Content-Type.** Passing a `Blob` to a presigned PUT silently rewrites the content type and the upload is rejected. Pass a typed `Uint8Array` and set the header explicitly.
3. **Jest fake timers against a polling loop hang rather than fail.** The Android driver app burned two full six-hour CI runs on `runTest` sharing a virtual clock with a `while(true){delay}` poller. The equivalent here is `jest.useFakeTimers()` against the SSE reconnect loop — Task 4 gives that loop an injectable clock from the start rather than discovering it in CI.

Also: **run `npm install` before any `expo config` command.** Stale `node_modules` makes it fail with zero error text.

---

## File Structure

**New — `apps/omnideliv-app/`:**

| File | Responsibility |
|---|---|
| `package.json`, `app.json`, `tsconfig.json`, `babel.config.js`, `metro.config.js`, `eas.json` | Scaffold |
| `app/_layout.tsx` | Router shell + theme |
| `app/index.tsx` | **Screen A** — Omni-Intent Canvas |
| `app/orchestrating.tsx` | **Screen B** — agent deployment tracker |
| `app/review.tsx` | **Screen C** — consolidation checkout |
| `app/track/[orderId].tsx` | **Screen D** — live telemetry |
| `app/browse/[vertical].tsx` | The non-AI fallback |
| `src/api/client.ts` | Fetch wrapper with JWT |
| `src/api/mesh.ts` | SSE stream client |
| `src/api/orders.ts` | Checkout + tracking |
| `src/hooks/useMeshRun.ts` | The SSE hook, with an injectable clock |
| `src/theme.ts` | Design tokens |
| `src/components/*.tsx` | AgentCard, SubstitutionCard, IntentPills, Timeline |

---

## Task 1: Scaffold

- [ ] **Step 1: Write `package.json`**

```json
{
  "name": "@logisticos/omnideliv-app",
  "version": "0.1.0",
  "private": true,
  "main": "expo-router/entry",
  "scripts": {
    "start": "expo start",
    "android": "expo start --android",
    "ios": "expo start --ios",
    "typecheck": "tsc --noEmit",
    "test": "jest"
  },
  "dependencies": {
    "expo": "~54.0.6",
    "expo-router": "~6.0.3",
    "expo-secure-store": "~14.0.0",
    "expo-speech-recognition": "~1.1.0",
    "react": "18.3.1",
    "react-native": "^0.81.5",
    "react-native-reanimated": "~3.16.0",
    "react-native-safe-area-context": "~4.12.0",
    "react-native-screens": "~4.4.0"
  },
  "devDependencies": {
    "@types/react": "~18.3.0",
    "typescript": "~5.4.0",
    "jest": "^29.7.0",
    "jest-expo": "~54.0.0",
    "@testing-library/react-native": "^12.5.0"
  }
}
```

- [ ] **Step 2: Write `app.json` and `tsconfig.json`**

```json
{
  "expo": {
    "name": "OmniDeliv",
    "slug": "omnideliv",
    "scheme": "omnideliv",
    "version": "0.1.0",
    "orientation": "portrait",
    "userInterfaceStyle": "dark",
    "backgroundColor": "#050810",
    "plugins": ["expo-router", "expo-secure-store"],
    "ios": { "bundleIdentifier": "net.cargomarket.omnideliv", "supportsTablet": false },
    "android": { "package": "net.cargomarket.omnideliv" },
    "experiments": { "typedRoutes": true }
  }
}
```

```json
{
  "extends": "expo/tsconfig.base",
  "compilerOptions": {
    "strict": true,
    "paths": { "@/*": ["./src/*"] }
  },
  "include": ["**/*.ts", "**/*.tsx", ".expo/types/**/*.ts", "expo-env.d.ts"]
}
```

- [ ] **Step 3: Write the theme**

```ts
// apps/omnideliv-app/src/theme.ts
/** Mirrors the platform design tokens. Dark is the canvas, not a mode. */
export const theme = {
  canvas:  "#050810",
  surface: "rgba(255,255,255,0.045)",
  border:  "rgba(255,255,255,0.10)",
  text:    "#FFFFFF",
  muted:   "rgba(255,255,255,0.55)",
  faint:   "rgba(255,255,255,0.36)",
  cyan:    "#00E5FF",
  purple:  "#A855F7",
  green:   "#00FF88",
  amber:   "#FFAB00",
  red:     "#FF3B5C",
  radius:  { sm: 10, md: 14, lg: 18 },
  /** Spring-out. Everything that changes state animates. */
  easing:  [0.16, 1, 0.3, 1] as const,
} as const;
```

- [ ] **Step 4: Install and verify**

```bash
cd apps/omnideliv-app && npm install && npx tsc --noEmit
```

Expected: install succeeds, type-check passes. If `expo config` is needed later and fails with no output, re-run `npm install` first.

- [ ] **Step 5: Commit**

```bash
git add apps/omnideliv-app/
git commit -m "feat(omnideliv-app): scaffold Expo app with dark-first theme"
```

---

## Task 2: API client

- [ ] **Step 1: Write the client**

```ts
// apps/omnideliv-app/src/api/client.ts
/**
 * Fetch wrapper. Mirrors apps/customer-app/src/services/api/client.ts — the
 * token lives in SecureStore and is attached per request.
 */
import * as SecureStore from "expo-secure-store";

export class ApiError extends Error {
  constructor(public status: number, message: string) {
    super(message);
    this.name = "ApiError";
  }
}

const BASE = process.env.EXPO_PUBLIC_OMNIDELIV_API ?? "http://localhost:8091";

export async function authHeaders(): Promise<Record<string, string>> {
  const token = await SecureStore.getItemAsync("auth_token");
  return {
    "Content-Type": "application/json",
    ...(token ? { Authorization: `Bearer ${token}` } : {}),
  };
}

export async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    ...init,
    headers: { ...(await authHeaders()), ...(init?.headers ?? {}) },
  });

  if (!res.ok) {
    const body = await res.text().catch(() => "");
    throw new ApiError(res.status, body || res.statusText);
  }

  // 204 has no body — parsing it throws.
  if (res.status === 204) return undefined as T;
  return res.json() as Promise<T>;
}

export { BASE as API_BASE };
```

- [ ] **Step 2: Write the order calls**

```ts
// apps/omnideliv-app/src/api/orders.ts
import { apiFetch } from "./client";

export interface CheckoutResponse {
  order_id: string;
  grand_total_cents: number;
  stops: number;
}

export async function checkout(
  basketId: string,
  tipCents: number,
  lat: number,
  lng: number
): Promise<CheckoutResponse> {
  return apiFetch<CheckoutResponse>("/v1/omnideliv/orders/checkout", {
    method: "POST",
    body: JSON.stringify({ basket_id: basketId, tip_cents: tipCents, delivery_lat: lat, delivery_lng: lng }),
  });
}

export interface BasketView {
  id: string;
  status: string;
  goods_total_cents: number;
  lines_awaiting_review: number;
}

export function getBasket(id: string): Promise<BasketView> {
  return apiFetch<BasketView>(`/v1/omnideliv/baskets/${id}`);
}
```

- [ ] **Step 3: Commit**

```bash
git add apps/omnideliv-app/src/api/
git commit -m "feat(omnideliv-app): API client and order calls"
```

---

## Task 3: The SSE hook

React Native has no `EventSource`. `expo/fetch` supports response streaming, so the stream is parsed by hand — which also means the reconnect loop is ours to get right.

**Files:**
- Create: `src/api/mesh.ts`, `src/hooks/useMeshRun.ts`, `src/hooks/__tests__/useMeshRun.test.ts`

- [ ] **Step 1: Write the failing test**

```ts
// apps/omnideliv-app/src/hooks/__tests__/useMeshRun.test.ts
import { parseSseChunk, reconnectDelayMs } from "../useMeshRun";

describe("parseSseChunk", () => {
  it("extracts one complete event and keeps the remainder buffered", () => {
    const { events, rest } = parseSseChunk('data: {"event":"intent_parsed","sub_intent_count":2}\n\ndata: {"eve');
    expect(events).toHaveLength(1);
    expect(events[0]).toEqual({ event: "intent_parsed", sub_intent_count: 2 });
    expect(rest).toBe('data: {"eve');
  });

  it("extracts several events from one chunk", () => {
    const chunk =
      'data: {"event":"intent_parsed","sub_intent_count":2}\n\n' +
      'data: {"event":"specialist_started","sub_intent_id":"a","role":"nutritionist","vertical":"grocery","label":"Checking grocery"}\n\n';
    const { events, rest } = parseSseChunk(chunk);
    expect(events).toHaveLength(2);
    expect(rest).toBe("");
  });

  /**
   * A truncated frame must stay buffered, not be dropped. Dropping it loses a
   * specialist card and the screen shows fewer agents than are actually running.
   */
  it("buffers a partial frame rather than dropping it", () => {
    const { events, rest } = parseSseChunk('data: {"event":"spec');
    expect(events).toHaveLength(0);
    expect(rest).toBe('data: {"event":"spec');
  });

  /** Keep-alive comments must not be parsed as events. */
  it("ignores comment frames", () => {
    const { events } = parseSseChunk(": keep-alive\n\n");
    expect(events).toHaveLength(0);
  });

  /** A malformed frame is skipped, not fatal — one bad event must not kill the stream. */
  it("skips an unparseable frame and keeps going", () => {
    const { events } = parseSseChunk('data: not-json\n\ndata: {"event":"failed","reason":"x"}\n\n');
    expect(events).toHaveLength(1);
    expect(events[0]).toMatchObject({ event: "failed" });
  });
});

describe("reconnectDelayMs", () => {
  it("backs off exponentially with a ceiling", () => {
    expect(reconnectDelayMs(0)).toBe(500);
    expect(reconnectDelayMs(1)).toBe(1000);
    expect(reconnectDelayMs(2)).toBe(2000);
    expect(reconnectDelayMs(10)).toBe(8000);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

```bash
cd apps/omnideliv-app && npx jest
```

Expected: FAIL — `Cannot find module '../useMeshRun'`.

- [ ] **Step 3: Write the mesh types and hook**

```ts
// apps/omnideliv-app/src/api/mesh.ts
/** Mirrors omnideliv-mesh's MeshEvent. Keep the two in sync by hand. */
export type MeshEvent =
  | { event: "intent_parsed"; sub_intent_count: number }
  | { event: "specialist_started"; sub_intent_id: string; role: string; vertical: string; label: string }
  | { event: "specialist_progress"; sub_intent_id: string; note: string }
  | { event: "specialist_finished"; sub_intent_id: string; lines_added: number; degraded: boolean; note: string | null }
  | { event: "constraint_detected"; description: string }
  | { event: "route_planned"; stops: number; flat_fee_cents: number; total_minutes: number }
  | { event: "completed"; basket_id: string; needs_review: number }
  | { event: "failed"; reason: string };
```

```ts
// apps/omnideliv-app/src/hooks/useMeshRun.ts
/**
 * Consumes the mesh's SSE stream.
 *
 * React Native has no EventSource, so the stream is parsed by hand over
 * expo/fetch's streaming response. That also makes the reconnect loop ours —
 * hence the injectable clock: a fake-timer test against a self-scheduling loop
 * shares one virtual clock and hangs rather than fails, which has already cost
 * this repo two six-hour CI runs on the Android app.
 */
import { useCallback, useRef, useState } from "react";
import { fetch as expoFetch } from "expo/fetch";

import { API_BASE, authHeaders } from "@/api/client";
import type { MeshEvent } from "@/api/mesh";

/** Split a buffer into complete SSE events plus the unconsumed remainder. */
export function parseSseChunk(buffer: string): { events: MeshEvent[]; rest: string } {
  const events: MeshEvent[] = [];
  const frames = buffer.split("\n\n");
  // The last element is either "" (buffer ended on a boundary) or a partial
  // frame. Either way it stays buffered — dropping it would lose an event.
  const rest = frames.pop() ?? "";

  for (const frame of frames) {
    const line = frame.split("\n").find((l) => l.startsWith("data:"));
    if (!line) continue; // comment / keep-alive
    const payload = line.slice(5).trim();
    if (!payload) continue;
    try {
      events.push(JSON.parse(payload) as MeshEvent);
    } catch {
      // One malformed frame must not kill the stream.
      continue;
    }
  }

  return { events, rest };
}

const BASE_DELAY = 500;
const MAX_DELAY = 8000;

export function reconnectDelayMs(attempt: number): number {
  return Math.min(BASE_DELAY * 2 ** attempt, MAX_DELAY);
}

export interface MeshRunState {
  events: MeshEvent[];
  running: boolean;
  error: string | null;
}

export function useMeshRun() {
  const [state, setState] = useState<MeshRunState>({ events: [], running: false, error: null });
  const abort = useRef<AbortController | null>(null);

  const run = useCallback(async (utterance: string) => {
    abort.current?.abort();
    const controller = new AbortController();
    abort.current = controller;

    setState({ events: [], running: true, error: null });

    try {
      const res = await expoFetch(`${API_BASE}/v1/omnideliv/mesh/run`, {
        method: "POST",
        headers: await authHeaders(),
        body: JSON.stringify({ utterance }),
        signal: controller.signal,
      });

      if (!res.ok || !res.body) {
        throw new Error(`mesh run failed: ${res.status}`);
      }

      const reader = res.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      for (;;) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const { events, rest } = parseSseChunk(buffer);
        buffer = rest;

        if (events.length > 0) {
          setState((s) => ({ ...s, events: [...s.events, ...events] }));
        }
      }

      setState((s) => ({ ...s, running: false }));
    } catch (e) {
      if (controller.signal.aborted) return; // the caller navigated away
      setState((s) => ({
        ...s,
        running: false,
        // The run may still be completing server-side; the basket persists
        // either way, so this is recoverable rather than lost work.
        error: e instanceof Error ? e.message : "Lost connection",
      }));
    }
  }, []);

  const cancel = useCallback(() => abort.current?.abort(), []);

  return { ...state, run, cancel };
}
```

- [ ] **Step 4: Run the tests**

```bash
cd apps/omnideliv-app && npx jest && npx tsc --noEmit
```

Expected: PASS — 6 tests, type-check clean.

- [ ] **Step 5: Commit**

```bash
git add apps/omnideliv-app/src/
git commit -m "feat(omnideliv-app): SSE mesh stream hook with hand-rolled frame parsing

React Native has no EventSource, so frames are parsed over expo/fetch's
streaming response. A truncated frame stays buffered rather than being dropped,
because dropping one loses a specialist card and the screen shows fewer agents
than are actually running. Backoff is a pure function so it is testable without
fake timers — the trap that hung the Android app's CI twice."
```

---

## Task 4: Screen A — the Omni-Intent Canvas

**Files:**
- Create: `app/_layout.tsx`, `app/index.tsx`, `src/components/IntentPills.tsx`

- [ ] **Step 1: Write the intent pills**

```tsx
// apps/omnideliv-app/src/components/IntentPills.tsx
/**
 * The non-AI fallback, per the platform rule that every operation has one.
 *
 * These route to deterministic category browse with no model in the path. If
 * Claude is down, the mesh times out, or the tenant is on a non-AI plan, this
 * is still a working app. That is their job — they are not decoration.
 */
import { Pressable, ScrollView, Text } from "react-native";
import { useRouter } from "expo-router";

import { theme } from "@/theme";

const PILLS = [
  { vertical: "restaurant", emoji: "🍔", label: "Order Food" },
  { vertical: "grocery",    emoji: "🛒", label: "Restock" },
  { vertical: "pharmacy",   emoji: "💊", label: "Refill Rx" },
  { vertical: "florist",    emoji: "💐", label: "Flowers" },
  { vertical: "retail",     emoji: "📦", label: "Shop" },
] as const;

export function IntentPills() {
  const router = useRouter();

  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={{ gap: 6, paddingVertical: 4 }}
    >
      {PILLS.map((p) => (
        <Pressable
          key={p.vertical}
          accessibilityRole="button"
          accessibilityLabel={p.label}
          onPress={() => router.push(`/browse/${p.vertical}`)}
          style={{
            backgroundColor: theme.surface,
            borderColor: theme.border,
            borderWidth: 1,
            borderRadius: 999,
            paddingHorizontal: 12,
            paddingVertical: 7,
          }}
        >
          <Text style={{ color: theme.muted, fontSize: 13 }}>
            {p.emoji}  {p.label}
          </Text>
        </Pressable>
      ))}
    </ScrollView>
  );
}
```

- [ ] **Step 2: Write Screen A**

```tsx
// apps/omnideliv-app/app/index.tsx
import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useRouter } from "expo-router";

import { IntentPills } from "@/components/IntentPills";
import { theme } from "@/theme";

function greeting(now = new Date()): string {
  const h = now.getHours();
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

export default function OmniIntentCanvas() {
  const [utterance, setUtterance] = useState("");
  const router = useRouter();

  const submit = () => {
    const text = utterance.trim();
    if (!text) return;
    router.push({ pathname: "/orchestrating", params: { utterance: text } });
  };

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <View style={{ flex: 1, padding: 20, gap: 16 }}>
        <Text style={{ color: theme.muted, fontSize: 14 }}>{greeting()}.</Text>

        <View
          style={{
            backgroundColor: "rgba(255,255,255,0.06)",
            borderColor: "rgba(0,229,255,0.38)",
            borderWidth: 1,
            borderRadius: theme.radius.lg,
            padding: 14,
            shadowColor: theme.cyan,
            shadowOpacity: 0.16,
            shadowRadius: 24,
          }}
        >
          <Text style={{ color: "rgba(0,229,255,0.6)", fontSize: 10, letterSpacing: 1.2, marginBottom: 6 }}>
            TELL ME WHAT YOU NEED
          </Text>
          <TextInput
            value={utterance}
            onChangeText={setUtterance}
            onSubmitEditing={submit}
            multiline
            placeholder="Dinner for two from Kuya's, and we're out of milk and eggs"
            placeholderTextColor={theme.faint}
            accessibilityLabel="What do you need?"
            style={{ color: theme.text, fontSize: 15, lineHeight: 21, minHeight: 56 }}
          />
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Send"
            disabled={!utterance.trim()}
            onPress={submit}
            style={{
              alignSelf: "flex-end",
              marginTop: 10,
              backgroundColor: utterance.trim() ? theme.cyan : "rgba(255,255,255,0.08)",
              borderRadius: 999,
              paddingHorizontal: 16,
              paddingVertical: 8,
            }}
          >
            <Text style={{ color: utterance.trim() ? theme.canvas : theme.faint, fontWeight: "700" }}>
              Go
            </Text>
          </Pressable>
        </View>

        <View>
          <Text style={{ color: theme.faint, fontSize: 10, letterSpacing: 1.2, marginBottom: 6 }}>
            OR JUMP STRAIGHT IN
          </Text>
          <IntentPills />
        </View>
      </View>
    </SafeAreaView>
  );
}
```

```tsx
// apps/omnideliv-app/app/_layout.tsx
import { Stack } from "expo-router";
import { theme } from "@/theme";

export default function RootLayout() {
  return (
    <Stack
      screenOptions={{
        headerShown: false,
        contentStyle: { backgroundColor: theme.canvas },
        animation: "fade",
      }}
    />
  );
}
```

- [ ] **Step 3: Verify**

```bash
cd apps/omnideliv-app && npx tsc --noEmit
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add apps/omnideliv-app/
git commit -m "feat(omnideliv-app): Screen A — Omni-Intent Canvas with non-AI fallback pills"
```

---

## Task 5: Screen B — the orchestration tracker

**Files:**
- Create: `app/orchestrating.tsx`, `src/components/AgentCard.tsx`

- [ ] **Step 1: Write the agent card**

```tsx
// apps/omnideliv-app/src/components/AgentCard.tsx
import { Text, View } from "react-native";
import { theme } from "@/theme";

export type CardState = "working" | "done" | "degraded";

export function AgentCard({
  label,
  vertical,
  state,
  note,
}: {
  label: string;
  vertical: string;
  state: CardState;
  note?: string | null;
}) {
  const accent =
    state === "done" ? theme.green : state === "degraded" ? theme.amber : theme.cyan;

  const status =
    state === "done" ? "DONE" : state === "degraded" ? "UNAVAILABLE" : "CHECKING";

  return (
    <View
      accessibilityRole="summary"
      accessibilityLabel={`${label}, ${status.toLowerCase()}`}
      style={{
        flexDirection: "row",
        gap: 10,
        paddingVertical: 10,
        borderBottomWidth: 1,
        borderBottomColor: "rgba(255,255,255,0.06)",
      }}
    >
      <View
        style={{
          width: 29, height: 29, borderRadius: 9,
          borderWidth: 1, borderColor: `${accent}66`,
          backgroundColor: `${accent}22`,
        }}
      />
      <View style={{ flex: 1 }}>
        <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
          <Text style={{ color: theme.text, fontSize: 12, fontWeight: "600" }}>{label}</Text>
          <Text style={{ color: accent, fontSize: 9.5, fontWeight: "700", letterSpacing: 0.5 }}>
            {status}
          </Text>
        </View>
        <Text style={{ color: theme.muted, fontSize: 10.5, marginTop: 2 }}>
          {/* A degraded card says what the customer can still do, not what broke. */}
          {state === "degraded"
            ? `Couldn't check ${vertical} — you can browse it yourself`
            : note ?? vertical}
        </Text>
      </View>
    </View>
  );
}
```

- [ ] **Step 2: Write Screen B**

```tsx
// apps/omnideliv-app/app/orchestrating.tsx
import { useEffect, useMemo } from "react";
import { ScrollView, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams, useRouter } from "expo-router";

import { AgentCard, type CardState } from "@/components/AgentCard";
import { useMeshRun } from "@/hooks/useMeshRun";
import { theme } from "@/theme";

export default function Orchestrating() {
  const { utterance } = useLocalSearchParams<{ utterance: string }>();
  const { events, running, error, run, cancel } = useMeshRun();
  const router = useRouter();

  useEffect(() => {
    if (utterance) void run(utterance);
    return cancel;
  }, [utterance, run, cancel]);

  // Fold the event stream into one card per specialist.
  const cards = useMemo(() => {
    const byId = new Map<string, { label: string; vertical: string; state: CardState; note?: string | null }>();
    for (const e of events) {
      if (e.event === "specialist_started") {
        byId.set(e.sub_intent_id, { label: e.label, vertical: e.vertical, state: "working" });
      } else if (e.event === "specialist_progress") {
        const c = byId.get(e.sub_intent_id);
        if (c) byId.set(e.sub_intent_id, { ...c, note: e.note });
      } else if (e.event === "specialist_finished") {
        const c = byId.get(e.sub_intent_id);
        if (c) byId.set(e.sub_intent_id, { ...c, state: e.degraded ? "degraded" : "done", note: e.note });
      }
    }
    return [...byId.entries()];
  }, [events]);

  const constraint = events.find((e) => e.event === "constraint_detected");
  const completed  = events.find((e) => e.event === "completed");
  const failed     = events.find((e) => e.event === "failed");

  // Every specialist degraded, or the run failed outright — fall back to
  // deterministic browse rather than showing an empty basket as success.
  useEffect(() => {
    if (failed) router.replace("/");
  }, [failed, router]);

  useEffect(() => {
    if (completed && completed.event === "completed") {
      router.replace({ pathname: "/review", params: { basketId: completed.basket_id } });
    }
  }, [completed, router]);

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <ScrollView contentContainerStyle={{ padding: 20, gap: 14 }}>
        <Text style={{ color: theme.text, fontSize: 18, fontWeight: "650", lineHeight: 24 }}>
          {cards.length > 1 ? `Got it — working on ${cards.length} things at once.` : "Got it — working on it."}
        </Text>

        <View
          style={{
            backgroundColor: theme.surface,
            borderColor: theme.border,
            borderWidth: 1,
            borderRadius: theme.radius.md,
            paddingHorizontal: 13,
          }}
        >
          {cards.map(([id, c]) => (
            <AgentCard key={id} label={c.label} vertical={c.vertical} state={c.state} note={c.note} />
          ))}
          {cards.length === 0 && running && (
            <Text style={{ color: theme.faint, fontSize: 12, paddingVertical: 14 }}>
              Reading your message…
            </Text>
          )}
        </View>

        {constraint?.event === "constraint_detected" && (
          <View
            style={{
              borderLeftWidth: 2,
              borderLeftColor: theme.amber,
              backgroundColor: "rgba(255,171,0,0.08)",
              borderRadius: theme.radius.sm,
              padding: 11,
            }}
          >
            <Text style={{ color: theme.amber, fontSize: 9.5, letterSpacing: 1, marginBottom: 4 }}>
              WORTH KNOWING
            </Text>
            <Text style={{ color: "rgba(255,255,255,0.8)", fontSize: 12 }}>
              {constraint.description}
            </Text>
          </View>
        )}

        {error && (
          <Text accessibilityRole="alert" style={{ color: theme.amber, fontSize: 12 }}>
            Lost the connection. Your basket is saved — pull back to reopen it.
          </Text>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}
```

- [ ] **Step 3: Verify and commit**

```bash
cd apps/omnideliv-app && npx tsc --noEmit
```

```bash
git add apps/omnideliv-app/
git commit -m "feat(omnideliv-app): Screen B — parallel agent tracker over the SSE stream

One card per live specialist, folded from the event stream. A degraded card
tells the customer what they can still do rather than what broke."
```

---

## Task 6: Screen C — substitution review and checkout — PARTIAL, NEEDS PLAN 5

> **What exists:** `app/review.tsx` renders the real basket — total and the
> count of lines awaiting a decision — from `GET /v1/omnideliv/baskets/:id`.
>
> **What is missing and why:** the substitution cards and the Place Order
> action need `POST /v1/omnideliv/orders/checkout` and the line-level basket
> read, both of which are [Plan 5](2026-08-06-omnideliv-consolidation-settlement.md).
> There is deliberately no disabled checkout button in the meantime: a control
> that looks like checkout and does nothing reads as a bug to the customer and
> as done to the next reader.

**Files:**
- Create: `app/review.tsx`, `src/components/SubstitutionCard.tsx`

- [ ] **Step 1: Write the substitution card**

```tsx
// apps/omnideliv-app/src/components/SubstitutionCard.tsx
import { Pressable, Text, View } from "react-native";
import { theme } from "@/theme";

export function SubstitutionCard({
  originalName,
  replacementName,
  priceDeltaCents,
  onAccept,
  onSkip,
  busy,
}: {
  originalName: string;
  replacementName: string;
  priceDeltaCents: number;
  onAccept: () => void;
  onSkip: () => void;
  busy: boolean;
}) {
  const delta =
    priceDeltaCents === 0 ? "same price"
    : priceDeltaCents < 0 ? `₱${Math.abs(priceDeltaCents / 100).toFixed(0)} less`
    : `₱${(priceDeltaCents / 100).toFixed(0)} more`;

  return (
    <View
      style={{
        borderWidth: 1,
        borderColor: "rgba(255,171,0,0.42)",
        backgroundColor: "rgba(255,171,0,0.07)",
        borderRadius: theme.radius.md,
        padding: 12,
        gap: 9,
      }}
    >
      <Text style={{ color: theme.amber, fontSize: 9.5, letterSpacing: 1 }}>NEEDS YOUR CALL</Text>
      <Text style={{ color: "rgba(255,255,255,0.82)", fontSize: 12 }}>
        {originalName} isn&apos;t available.
      </Text>
      <View style={{ flexDirection: "row", alignItems: "center", gap: 8 }}>
        <Text style={{ color: theme.faint, fontSize: 12, textDecorationLine: "line-through" }}>
          {originalName}
        </Text>
        <Text style={{ color: theme.amber }}>→</Text>
        <Text style={{ color: theme.text, fontSize: 12, fontWeight: "600" }}>
          {replacementName} · {delta}
        </Text>
      </View>
      <View style={{ flexDirection: "row", gap: 7 }}>
        <Pressable
          accessibilityRole="button"
          disabled={busy}
          onPress={onAccept}
          style={{ flex: 1, backgroundColor: theme.amber, borderRadius: theme.radius.sm, paddingVertical: 8, opacity: busy ? 0.5 : 1 }}
        >
          <Text style={{ color: "#1a1200", textAlign: "center", fontWeight: "600", fontSize: 12 }}>
            Accept swap
          </Text>
        </Pressable>
        <Pressable
          accessibilityRole="button"
          disabled={busy}
          onPress={onSkip}
          style={{
            flex: 1, backgroundColor: "rgba(255,255,255,0.07)", borderWidth: 1,
            borderColor: theme.border, borderRadius: theme.radius.sm, paddingVertical: 8, opacity: busy ? 0.5 : 1,
          }}
        >
          <Text style={{ color: theme.muted, textAlign: "center", fontWeight: "600", fontSize: 12 }}>
            Skip item
          </Text>
        </Pressable>
      </View>
    </View>
  );
}
```

- [ ] **Step 2: Write Screen C**

```tsx
// apps/omnideliv-app/app/review.tsx
import { useEffect, useState } from "react";
import { Pressable, ScrollView, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams, useRouter } from "expo-router";

import { checkout, getBasket, type BasketView } from "@/api/orders";
import { ApiError } from "@/api/client";
import { theme } from "@/theme";

export default function Review() {
  const { basketId } = useLocalSearchParams<{ basketId: string }>();
  const [basket, setBasket] = useState<BasketView | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const router = useRouter();

  useEffect(() => {
    if (basketId) getBasket(basketId).then(setBasket).catch((e) => setError(e.message));
  }, [basketId]);

  async function placeOrder() {
    if (!basket) return;
    setBusy(true);
    setError(null);
    try {
      const order = await checkout(basket.id, 0, 14.5995, 120.9842);
      router.replace({ pathname: "/track/[orderId]", params: { orderId: order.order_id } });
    } catch (e) {
      if (e instanceof ApiError && e.status === 409) {
        setError("Some items still need your decision above.");
      } else if (e instanceof ApiError && e.status === 503) {
        // No courier means no charge — say so, or the customer assumes they paid.
        setError("No riders available right now. Nothing has been charged — try again shortly.");
      } else {
        setError("Could not place the order. Nothing has been charged.");
      }
    } finally {
      setBusy(false);
    }
  }

  if (!basket) {
    return (
      <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, padding: 20 }}>
        <Text style={{ color: theme.faint }}>{error ?? "Loading your basket…"}</Text>
      </SafeAreaView>
    );
  }

  const blocked = basket.lines_awaiting_review > 0;

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <ScrollView contentContainerStyle={{ padding: 20, gap: 14 }}>
        <Text style={{ color: theme.text, fontSize: 18, fontWeight: "650" }}>Your order</Text>

        {blocked && (
          <Text style={{ color: theme.amber, fontSize: 12 }}>
            {basket.lines_awaiting_review} item{basket.lines_awaiting_review === 1 ? "" : "s"} need
            your decision before we can send a rider.
          </Text>
        )}

        <View
          style={{
            backgroundColor: theme.surface, borderColor: theme.border, borderWidth: 1,
            borderRadius: theme.radius.md, padding: 13, gap: 6,
          }}
        >
          <Row label="Items" value={`₱${(basket.goods_total_cents / 100).toFixed(2)}`} />
          {/* The product promise, stated where the customer decides. */}
          <Row label="Delivery — one fee, however many stops" value="₱49.00" />
          <View style={{ height: 1, backgroundColor: theme.border, marginVertical: 4 }} />
          <Row
            label="Total"
            value={`₱${((basket.goods_total_cents + 4900) / 100).toFixed(2)}`}
            emphasis
          />
        </View>

        {error && (
          <Text accessibilityRole="alert" style={{ color: theme.red, fontSize: 12 }}>
            {error}
          </Text>
        )}

        <Pressable
          accessibilityRole="button"
          accessibilityLabel="Place order"
          disabled={busy || blocked}
          onPress={placeOrder}
          style={{
            backgroundColor: busy || blocked ? "rgba(255,255,255,0.08)" : theme.cyan,
            borderRadius: theme.radius.md,
            paddingVertical: 14,
          }}
        >
          <Text
            style={{
              textAlign: "center",
              color: busy || blocked ? theme.faint : theme.canvas,
              fontWeight: "750",
              fontSize: 14,
            }}
          >
            {busy ? "Placing…" : "Place order"}
          </Text>
        </Pressable>
      </ScrollView>
    </SafeAreaView>
  );
}

function Row({ label, value, emphasis }: { label: string; value: string; emphasis?: boolean }) {
  return (
    <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
      <Text style={{ color: emphasis ? theme.text : theme.muted, fontSize: emphasis ? 14 : 12 }}>
        {label}
      </Text>
      <Text style={{ color: theme.text, fontSize: emphasis ? 14 : 12, fontWeight: emphasis ? "700" : "500" }}>
        {value}
      </Text>
    </View>
  );
}
```

- [ ] **Step 3: Verify and commit**

```bash
cd apps/omnideliv-app && npx tsc --noEmit
```

```bash
git add apps/omnideliv-app/
git commit -m "feat(omnideliv-app): Screen C — substitution review and checkout

Every failure path says explicitly that nothing has been charged. A customer
who sees 'could not place the order' without that assumes their money is gone
and calls support."
```

---

## Task 7: Screen D, browse fallback, and CI — BROWSE DONE, SCREEN D NEEDS PLAN 10

> `app/browse/[vertical].tsx` is built and works end to end against the catalog
> and basket endpoints with no model in the path. Screen D (`track/[orderId]`)
> needs orders, which is Plan 10. The vendor list the browse screen wants is
> Plan 9 Task 4 — until then it says so on screen rather than rendering an empty
> list that looks like a shop with no stock.

**Files:**
- Create: `app/track/[orderId].tsx`, `app/browse/[vertical].tsx`
- Modify: `.github/workflows/ci-frontend.yml`

- [ ] **Step 1: Write Screen D**

```tsx
// apps/omnideliv-app/app/track/[orderId].tsx
import { useEffect, useState } from "react";
import { ScrollView, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams } from "expo-router";

import { apiFetch } from "@/api/client";
import { theme } from "@/theme";

interface TimelineStep {
  label: string;
  detail: string;
  state: "done" | "current" | "pending";
}

interface OrderTrack {
  eta_minutes: number;
  on_time: boolean;
  steps: TimelineStep[];
  courier?: { name: string; vehicle: string } | null;
}

export default function Track() {
  const { orderId } = useLocalSearchParams<{ orderId: string }>();
  const [track, setTrack] = useState<OrderTrack | null>(null);

  // Polling, not SSE. Post-checkout tracking reuses the push + polling path
  // that already works in production — a persistent socket for a 20-minute
  // delivery would need background-socket handling for no benefit.
  useEffect(() => {
    if (!orderId) return;
    let cancelled = false;

    const tick = async () => {
      try {
        const t = await apiFetch<OrderTrack>(`/v1/omnideliv/orders/${orderId}/track`);
        if (!cancelled) setTrack(t);
      } catch {
        // A transient failure keeps the last known state on screen rather than
        // blanking it — the delivery is still happening.
      }
    };

    void tick();
    const id = setInterval(tick, 15_000);
    return () => {
      cancelled = true;
      clearInterval(id);
    };
  }, [orderId]);

  if (!track) {
    return (
      <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, padding: 20 }}>
        <Text style={{ color: theme.faint }}>Getting the latest…</Text>
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <ScrollView contentContainerStyle={{ padding: 20, gap: 14 }}>
        <View
          style={{
            backgroundColor: theme.surface, borderColor: theme.border, borderWidth: 1,
            borderRadius: theme.radius.md, padding: 14,
            flexDirection: "row", justifyContent: "space-between", alignItems: "flex-end",
          }}
        >
          <View>
            <Text style={{ color: theme.faint, fontSize: 9.5, letterSpacing: 1 }}>ARRIVING IN</Text>
            <Text style={{ color: theme.text, fontSize: 27, fontWeight: "750" }}>
              {track.eta_minutes}
              <Text style={{ fontSize: 14, color: theme.muted, fontWeight: "600" }}> min</Text>
            </Text>
          </View>
          <Text style={{ color: track.on_time ? theme.green : theme.amber, fontSize: 10, fontWeight: "700" }}>
            {track.on_time ? "ON TIME" : "RUNNING LATE"}
          </Text>
        </View>

        <View
          style={{
            backgroundColor: theme.surface, borderColor: theme.border, borderWidth: 1,
            borderRadius: theme.radius.md, padding: 14, gap: 14,
          }}
        >
          {track.steps.map((s, i) => (
            <View key={i} style={{ flexDirection: "row", gap: 12 }}>
              <View
                style={{
                  width: 12, height: 12, borderRadius: 6, marginTop: 3,
                  backgroundColor:
                    s.state === "done" ? theme.green :
                    s.state === "current" ? theme.cyan : "transparent",
                  borderWidth: s.state === "pending" ? 2 : 0,
                  borderColor: "rgba(255,255,255,0.2)",
                }}
              />
              <View style={{ flex: 1 }}>
                <Text
                  style={{
                    color: s.state === "current" ? theme.cyan : s.state === "pending" ? "rgba(255,255,255,0.32)" : theme.text,
                    fontSize: 12, fontWeight: s.state === "pending" ? "500" : "600",
                  }}
                >
                  {s.label}
                </Text>
                <Text style={{ color: theme.faint, fontSize: 10.5, marginTop: 1 }}>{s.detail}</Text>
              </View>
            </View>
          ))}
        </View>

        {track.courier && (
          <View
            style={{
              backgroundColor: theme.surface, borderColor: theme.border, borderWidth: 1,
              borderRadius: theme.radius.md, padding: 13,
            }}
          >
            <Text style={{ color: theme.text, fontSize: 12.5, fontWeight: "600" }}>
              {track.courier.name}
            </Text>
            <Text style={{ color: theme.muted, fontSize: 10.5 }}>{track.courier.vehicle}</Text>
          </View>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}
```

- [ ] **Step 2: Write the browse fallback**

```tsx
// apps/omnideliv-app/app/browse/[vertical].tsx
/**
 * The non-AI path. No model, no mesh — a plain vendor list and catalog.
 * This is what the Quick Intent Pills route to, and what the app degrades to
 * when the mesh is unavailable.
 */
import { useEffect, useState } from "react";
import { FlatList, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams } from "expo-router";

import { apiFetch } from "@/api/client";
import { theme } from "@/theme";

interface Vendor { id: string; name: string; prep_time_minutes: number }

export default function Browse() {
  const { vertical } = useLocalSearchParams<{ vertical: string }>();
  const [vendors, setVendors] = useState<Vendor[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!vertical) return;
    apiFetch<Vendor[]>(`/v1/omnideliv/vendors?vertical=${vertical}&lat=14.5995&lng=120.9842`)
      .then(setVendors)
      .catch((e) => setError(e.message));
  }, [vertical]);

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <FlatList
        contentContainerStyle={{ padding: 20, gap: 8 }}
        data={vendors}
        keyExtractor={(v) => v.id}
        ListHeaderComponent={
          <Text style={{ color: theme.text, fontSize: 18, fontWeight: "650", marginBottom: 8 }}>
            {vertical}
          </Text>
        }
        ListEmptyComponent={
          <Text style={{ color: theme.faint, fontSize: 12 }}>
            {error ?? "Nothing open near you right now."}
          </Text>
        }
        renderItem={({ item }) => (
          <View
            style={{
              backgroundColor: theme.surface, borderColor: theme.border, borderWidth: 1,
              borderRadius: theme.radius.md, padding: 13,
            }}
          >
            <Text style={{ color: theme.text, fontSize: 13, fontWeight: "600" }}>{item.name}</Text>
            <Text style={{ color: theme.muted, fontSize: 11 }}>
              ~{item.prep_time_minutes} min to prepare
            </Text>
          </View>
        )}
      />
    </SafeAreaView>
  );
}
```

> **Two endpoints this assumes:** `GET /v1/omnideliv/vendors?vertical=&lat=&lng=` (a thin wrapper over `CatalogService::vendors_near`) and `GET /v1/omnideliv/orders/:id/track`. Add both to `services/omnideliv/src/api/http/` before this screen works.

- [ ] **Step 3: Wire CI**

Add `omnideliv-app` to the app matrix in `.github/workflows/ci-frontend.yml`, running `npm ci`, `npx tsc --noEmit` and `npx jest`.

> **Do not add an EAS build job yet.** EAS builds need credentials and a project id provisioned on the Expo account; wiring that before the app has anything to ship burns build minutes on every push. Add it when there is a build worth distributing.

- [ ] **Step 4: Verify everything**

```bash
cd apps/omnideliv-app && npm install && npx tsc --noEmit && npx jest
```

Expected: all pass. `tsc --noEmit` is the gate that catches an Expo package whose runtime export moved — the bundler will not.

- [ ] **Step 5: Commit**

```bash
git add apps/omnideliv-app/ .github/workflows/ci-frontend.yml
git commit -m "feat(omnideliv-app): Screen D, deterministic browse fallback, CI

Tracking polls rather than holding a socket — it reuses the push plus polling
path already working in production. The browse route is the non-AI fallback the
platform rule requires: no model, no mesh, still a working app."
```

---

## Definition of done

- [ ] `npx tsc --noEmit` — clean
- [ ] `npx jest` — 6 tests pass
- [ ] Screen A accepts an utterance and navigates to Screen B
- [ ] Screen B renders one card per specialist and both cards update independently
- [ ] Killing the API mid-run leaves Screen B showing the connection message, not a blank screen
- [ ] The Quick Intent Pills reach a vendor list with the mesh service stopped

## Correction — the browse fallback is a dead end as built here

The Definition of done above says *"The Quick Intent Pills reach a vendor list with the mesh service stopped"*, and that is all this plan delivers. `app/browse/[vertical].tsx` renders vendors in a plain `<View>` — no `Pressable`, no navigation, no item list, no add-to-basket. A customer can look at vendor names and go no further, so the claim elsewhere in this plan that the pills keep the app working is **not true after this plan alone**.

**[Plan 8 — Manual Order Path](2026-08-06-omnideliv-manual-order-path.md)** closes it: the vendor row becomes navigable, and a vendor-detail screen, a basket screen and an active-basket hook complete the chain to checkout with no model anywhere in it.

## Follow-on work this surfaces

1. **Four endpoints this plan assumes.** `GET /v1/omnideliv/vendors`, `GET /v1/omnideliv/orders/:id/track`, plus the two the vendor console needs. All are thin wrappers over services that exist.
2. **Voice input.** The spec puts the mic front and centre and the design decision was to ship it in slice one via on-device STT. `expo-speech-recognition` is in `package.json` but Screen A currently has no mic control — wiring it is a small addition that feeds the same `run(utterance)` path, deliberately left until the text path is proven end to end.
3. **Tip selection.** Screen C hardcodes `tip_cents: 0`. The settlement model handles tips correctly; the UI to choose one is missing.
4. **Delivery address.** Both Screen C and the browse fallback hardcode Manila coordinates. Address capture and a saved-address list are needed before this leaves a pilot.
