/**
 * The deterministic path. No model is involved anywhere on this screen.
 *
 * This is what the platform's "every AI feature has a non-AI fallback" rule
 * actually means here: with Claude down, the mesh timing out, or a tenant on a
 * non-AI plan, a customer still reaches a basket they can check out.
 */
import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, FlatList, Image, Pressable, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams, useRouter } from "expo-router";

import { addLine, createBasket, type BasketView } from "@/api/basket";
import { ModifierPicker } from "@/components/ModifierPicker";
import {
  itemPhotoUrl,
  searchCatalog,
  vendorsNear,
  type SearchHit,
  type VendorSummary,
} from "@/api/catalog";
import { currentDeliveryPoint } from "@/deliveryPoint";
import { theme } from "@/theme";

function peso(cents: number): string {
  return `₱${(cents / 100).toFixed(2)}`;
}

// `currentDeliveryPoint()` is deliberately NOT read here at module scope.
// Route modules are evaluated when the bundle loads, which is before the root
// layout has awaited `loadDeliveryPoint()` — so a module-level constant captures
// the Manila *fallback* and keeps it forever. The customer sets an address in
// Cebu and still gets shops near Manila. Read it inside the effect, where the
// cache is primed. (Same shape as the session gate: a value read once and never
// again.)

export default function Browse() {
  const { vertical, vendorId } = useLocalSearchParams<{ vertical: string; vendorId?: string }>();
  // The vendor actually being browsed: the one named in the route, or the
  // nearest open one resolved on mount.
  const [resolvedVendorId, setResolvedVendorId] = useState<string | undefined>(vendorId);
  /** Every open shop of this vertical nearby. `null` until loaded. */
  const [shops, setShops] = useState<VendorSummary[] | null>(null);
  const [items, setItems] = useState<SearchHit[]>([]);
  const [vendor, setVendor] = useState<VendorSummary | null>(null);
  const [basket, setBasket] = useState<BasketView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        // Read the delivery point here, not at module scope — see the note
        // above the imports.
        const here = currentDeliveryPoint();

        // Fetched even when the route names a shop, so "Other shops" is offered
        // consistently — a deep link should not be a one-way door either. Its
        // own try/catch: failing to list the neighbours must not stop the shop
        // the customer actually asked for from loading.
        let near: VendorSummary[] = [];
        try {
          near = await vendorsNear(String(vertical), here.lat, here.lng);
        } catch {
          near = [];
        }
        if (cancelled) return;
        setShops(near);

        let id = vendorId;
        if (!id) {
          if (near.length === 0) {
            setError(`No ${vertical} shops are open near you right now.`);
            return;
          }
          // One shop is not a choice, so do not make the customer tap through a
          // list of one — the original "defaulting beats a blank page" instinct
          // was right, it was only wrong when there were others to see.
          //
          // With two or more we stop here and show the picker. Taking `near[0]`
          // and searching only that one made every other shop in the vertical
          // permanently unreachable: no picker existed, so a second restaurant
          // simply did not exist as far as the app was concerned.
          if (near.length > 1) return;

          id = near[0].id;
          setVendor(near[0]);
        }
        setResolvedVendorId(id);

        const hits = await searchCatalog(id, "");
        if (!cancelled) setItems(hits);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : "Could not load this shop");
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  // `vertical` belongs here: it decides which shops are fetched. Left out, a
  // move between verticals that reuses the component would keep the previous
  // vertical's shops.
  }, [vendorId, vertical]);

  const router = useRouter();

  /** The item whose options are being chosen, or null when none is. */
  const [picking, setPicking] = useState<SearchHit | null>(null);

  const commit = useCallback(
    async (item: SearchHit, modifiers: string[]) => {
      if (!resolvedVendorId) return;
      try {
        // The basket is created lazily on the first add, so browsing without
        // buying leaves no empty baskets behind.
        const b = basket ?? (await createBasket());
        setBasket(await addLine(b.id, resolvedVendorId, item.item_id, 1, modifiers));
      } catch (e) {
        // A rejected selection comes back as a 400 with the server's own words
        // ("Size needs at least 1 selection"), which is more use than a generic
        // failure — surface it rather than replacing it.
        setError(e instanceof Error ? e.message : "Could not add that");
      }
    },
    [basket, resolvedVendorId]
  );

  const add = useCallback(
    async (item: SearchHit) => {
      // Only interrupt the tap when there is genuinely something to choose. For
      // the majority of items — no groups — this stays a single tap.
      if ((item.modifiers?.length ?? 0) > 0) {
        setPicking(item);
        return;
      }
      await commit(item, []);
    },
    [commit]
  );

  /** Open one shop's menu. */
  const selectShop = useCallback(async (s: VendorSummary) => {
    setLoading(true);
    setError(null);
    try {
      setVendor(s);
      setResolvedVendorId(s.id);
      setItems(await searchCatalog(s.id, ""));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load that shop");
    } finally {
      setLoading(false);
    }
  }, []);

  /** Back to the list without leaving the screen — the basket footer stays. */
  const changeShop = useCallback(() => {
    setResolvedVendorId(undefined);
    setVendor(null);
    setItems([]);
    setError(null);
  }, []);

  if (loading) {
    return (
      <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, justifyContent: "center" }}>
        <ActivityIndicator color={theme.cyan} />
      </SafeAreaView>
    );
  }

  // The picker. Shown whenever no shop is chosen yet and there is more than one
  // to choose from; a single shop opens directly (see the effect).
  const choosing = !resolvedVendorId && shops !== null && shops.length > 0;

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <View style={{ padding: 20, gap: 12, flex: 1 }}>
        <Text style={{ color: theme.text, fontSize: 18, fontWeight: "600" }}>
          {choosing ? `${vertical} near you` : (vendor?.name ?? vertical)}
        </Text>
        {choosing && shops && (
          <Text style={{ color: theme.muted, fontSize: 11, marginTop: -6 }}>
            {shops.length} shop{shops.length === 1 ? "" : "s"} open
          </Text>
        )}
        {!choosing && vendor && (
          <Text style={{ color: theme.muted, fontSize: 11, marginTop: -6 }}>
            {vendor.address} · about {vendor.prep_time_minutes} min to prepare
          </Text>
        )}
        {/* Only offered when there is somewhere else to go. */}
        {!choosing && shops !== null && shops.length > 1 && (
          <Pressable onPress={changeShop} accessibilityRole="button" hitSlop={6}>
            <Text style={{ color: theme.cyan, fontSize: 12 }}>‹ Other shops</Text>
          </Pressable>
        )}

        {error && (
          <Text accessibilityRole="alert" style={{ color: theme.amber, fontSize: 12 }}>
            {error}
          </Text>
        )}

        {/* The shop list. Every open shop of this vertical, nearest first —
            which is the order the server returns them in. Before this the
            screen took the first and searched only that one, so a second
            restaurant was unreachable by any route in the app. */}
        {choosing && shops && (
          <FlatList
            data={shops}
            keyExtractor={(v) => v.id}
            ItemSeparatorComponent={() => (
              <View style={{ height: 1, backgroundColor: "rgba(255,255,255,0.06)" }} />
            )}
            renderItem={({ item: s }) => (
              <Pressable
                onPress={() => void selectShop(s)}
                accessibilityRole="button"
                accessibilityLabel={`Browse ${s.name}`}
                style={{ paddingVertical: 14, gap: 3 }}
              >
                <Text style={{ color: theme.text, fontSize: 14, fontWeight: "600" }}>
                  {s.name}
                </Text>
                <Text style={{ color: theme.muted, fontSize: 11 }}>
                  {s.address} · about {s.prep_time_minutes} min to prepare
                </Text>
              </Pressable>
            )}
          />
        )}

        {!choosing && (
        <FlatList
          data={items}
          keyExtractor={(i) => i.item_id}
          ItemSeparatorComponent={() => (
            <View style={{ height: 1, backgroundColor: "rgba(255,255,255,0.06)" }} />
          )}
          renderItem={({ item }) => (
            <View style={{ flexDirection: "row", alignItems: "center", paddingVertical: 11, gap: 10 }}>
              {/* Only rendered when the server says a photo exists. Pointing an
                  <Image> at a 404 gives every pictureless item a broken frame,
                  which reads as "this shop is broken" rather than "no photo". */}
              {item.has_photo && (
                <Image
                  source={{ uri: itemPhotoUrl(item.tenant_id, item.item_id) }}
                  style={{ width: 44, height: 44, borderRadius: 8, backgroundColor: theme.surface }}
                  accessibilityIgnoresInvertColors
                />
              )}
              <View style={{ flex: 1 }}>
                <Text style={{ color: theme.text, fontSize: 13 }}>{item.name}</Text>
                <Text style={{ color: theme.muted, fontSize: 11, marginTop: 2 }}>
                  {peso(item.price_cents)}
                  {item.availability === "out_of_stock" && "  ·  out of stock"}
                  {item.availability === "limited" && "  ·  only a few left"}
                </Text>
              </View>
              <Pressable
                accessibilityRole="button"
                accessibilityLabel={`Add ${item.name}`}
                disabled={item.availability === "out_of_stock"}
                onPress={() => void add(item)}
                style={{
                  borderRadius: 999,
                  paddingHorizontal: 14,
                  paddingVertical: 7,
                  backgroundColor:
                    item.availability === "out_of_stock" ? "rgba(255,255,255,0.06)" : theme.cyan,
                }}
              >
                <Text
                  style={{
                    color: item.availability === "out_of_stock" ? theme.faint : theme.canvas,
                    fontWeight: "700",
                    fontSize: 12,
                  }}
                >
                  Add
                </Text>
              </Pressable>
            </View>
          )}
        />
        )}

        {basket && (
          /*
           * The basket footer was a read-only total. Adding items here had no
           * exit: `/review` was reachable only from the mesh run, so anyone who
           * browsed a shop by hand filled a basket they could never check out.
           * The footer is the natural place for that door, and the total was
           * already sitting in it.
           */
          <Pressable
            onPress={() => router.push({ pathname: "/review", params: { basketId: basket.id } })}
            accessibilityRole="button"
            accessibilityLabel={`Review basket, ${peso(basket.goods_total_cents)}`}
            style={{
              borderTopWidth: 1,
              borderTopColor: theme.border,
              paddingTop: 12,
              flexDirection: "row",
              justifyContent: "space-between",
              alignItems: "center",
            }}
          >
            <Text style={{ color: theme.cyan, fontSize: 13, fontWeight: "600" }}>
              Review basket ›
            </Text>
            <Text style={{ color: theme.text, fontSize: 13, fontWeight: "700" }}>
              {peso(basket.goods_total_cents)}
            </Text>
          </Pressable>
        )}
      </View>

      {/* Rendered last and absolutely positioned, so it sits over the list
          without a Modal — which the web export does not render. */}
      {picking && (
        <ModifierPicker
          item={picking}
          onCancel={() => setPicking(null)}
          onConfirm={async (optionIds) => {
            const item = picking;
            setPicking(null);
            await commit(item, optionIds);
          }}
        />
      )}
    </SafeAreaView>
  );
}
