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
import { useLocalSearchParams } from "expo-router";

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

/** Slice-one placeholder, mirroring the service's DEFAULT_LAT/DEFAULT_LNG. */
// One shared definition — see src/deliveryPoint.ts for why these must not be
// per-screen constants.
const { lat: DEFAULT_LAT, lng: DEFAULT_LNG } = currentDeliveryPoint();

export default function Browse() {
  const { vertical, vendorId } = useLocalSearchParams<{ vertical: string; vendorId?: string }>();
  // The vendor actually being browsed: the one named in the route, or the
  // nearest open one resolved on mount.
  const [resolvedVendorId, setResolvedVendorId] = useState<string | undefined>(vendorId);
  const [items, setItems] = useState<SearchHit[]>([]);
  const [vendor, setVendor] = useState<VendorSummary | null>(null);
  const [basket, setBasket] = useState<BasketView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        // Resolve a vendor when the caller did not name one. Nearest first, so
        // browsing a vertical lands in the closest open store rather than
        // dead-ending — picking between stores is a screen this slice does not
        // have, and defaulting beats a blank page.
        let id = vendorId;
        if (!id) {
          const near = await vendorsNear(String(vertical), DEFAULT_LAT, DEFAULT_LNG);
          if (near.length === 0) {
            if (!cancelled) setError(`No ${vertical} shops are open near you right now.`);
            return;
          }
          id = near[0].id;
          if (!cancelled) setVendor(near[0]);
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
  }, [vendorId]);

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

  if (loading) {
    return (
      <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, justifyContent: "center" }}>
        <ActivityIndicator color={theme.cyan} />
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <View style={{ padding: 20, gap: 12, flex: 1 }}>
        <Text style={{ color: theme.text, fontSize: 18, fontWeight: "600" }}>
          {vendor?.name ?? vertical}
        </Text>
        {vendor && (
          <Text style={{ color: theme.muted, fontSize: 11, marginTop: -6 }}>
            {vendor.address} · about {vendor.prep_time_minutes} min to prepare
          </Text>
        )}

        {error && (
          <Text accessibilityRole="alert" style={{ color: theme.amber, fontSize: 12 }}>
            {error}
          </Text>
        )}

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

        {basket && (
          <View
            style={{
              borderTopWidth: 1,
              borderTopColor: theme.border,
              paddingTop: 12,
              flexDirection: "row",
              justifyContent: "space-between",
            }}
          >
            <Text style={{ color: theme.muted, fontSize: 12 }}>Basket</Text>
            <Text style={{ color: theme.text, fontSize: 13, fontWeight: "700" }}>
              {peso(basket.goods_total_cents)}
            </Text>
          </View>
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
