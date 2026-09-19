/**
 * The whole-home intake in progress, shared by the six home screens.
 *
 * A module store rather than navigation params: the rooms screen and each
 * room screen edit one inventory, and passing it back and forth through
 * params is how edits get lost. It holds only what the customer said; every
 * price comes from the server.
 */
import { useSyncExternalStore } from 'react';
import type { HomeProperty, Inventory, TruckPlan } from '../../../services/api/homeMove';

export interface ReadBack {
  sentence: string;
  chips: string[];
  confident: boolean;
}

export interface HomeDraft {
  origin: { line1: string; city: string };
  destination: { line1: string; city: string };
  property: HomeProperty;
  inventory: Inventory;
  declared: boolean;
  plan: TruckPlan;
  read: ReadBack | null;
  /** What the prompt box read, sent with the booking. */
  intake: unknown;
}

export function emptyDraft(): HomeDraft {
  return {
    origin: { line1: '', city: '' },
    destination: { line1: '', city: '' },
    property: {
      type: 'apartment', size: '2 bedroom',
      pickup_floor: 0, pickup_has_lift: true, dropoff_floor: 0, dropoff_has_lift: true, long_carry: false,
    },
    inventory: {},
    declared: false,
    plan: 'trucks',
    read: null,
    intake: undefined,
  };
}

let draft: HomeDraft = emptyDraft();
const listeners = new Set<() => void>();

export function getDraft(): HomeDraft {
  return draft;
}

export function setDraft(patch: Partial<HomeDraft> | ((d: HomeDraft) => Partial<HomeDraft>)) {
  const next = typeof patch === 'function' ? patch(draft) : patch;
  draft = { ...draft, ...next };
  listeners.forEach((l) => l());
}

export function resetDraft(seed?: Partial<HomeDraft>) {
  draft = { ...emptyDraft(), ...seed };
  listeners.forEach((l) => l());
}

/** Change one item's line in one room. A quantity of zero removes it. */
export function setItem(room: string, key: string, patch: Partial<{ qty: number; dismantle: boolean; packing: boolean }>, defaults?: { dismantle: boolean; packing: boolean }) {
  setDraft((d) => {
    const roomItems = { ...(d.inventory[room] ?? {}) };
    const current = roomItems[key] ?? { qty: 0, dismantle: defaults?.dismantle ?? false, packing: defaults?.packing ?? false };
    const next = { ...current, ...patch };
    if (next.qty <= 0) delete roomItems[key];
    else roomItems[key] = next;
    // Any change to the inventory un-declares it: the customer confirms the
    // list they are pricing, not an earlier one.
    return { inventory: { ...d.inventory, [room]: roomItems }, declared: false };
  });
}

function subscribe(l: () => void) {
  listeners.add(l);
  return () => { listeners.delete(l); };
}

export function useHomeDraft(): HomeDraft {
  return useSyncExternalStore(subscribe, getDraft, getDraft);
}
