/**
 * The deterministic path. No model is involved anywhere on this screen.
 *
 * This is what the platform's "every AI feature has a non-AI fallback" rule
 * actually means here: with Claude down, the mesh timing out, or a tenant on a
 * non-AI plan, a customer still reaches a basket they can check out.
 */
import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, FlatList, Pressable, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams } from "expo-router";

import { addLine, createBasket, type BasketView } from "@/api/basket";
import { searchCatalog, type SearchHit } from "@/api/catalog";
import { theme } from "@/theme";

function peso(cents: number): string {
  return `₱${(cents / 100).toFixed(2)}`;
}

export default function Browse() {
  const { vertical, vendorId } = useLocalSearchParams<{ vertical: string; vendorId?: string }>();
  const [items, setItems] = useState<SearchHit[]>([]);
  const [basket, setBasket] = useState<BasketView | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    (async () => {
      try {
        // Without a vendor there is nothing to list: vendor discovery is the
        // mesh's find_vendors today, and the browse-side vendor list is Plan 9
        // Task 4. Say so plainly rather than rendering an empty screen that
        // looks like the shop has no stock.
        if (!vendorId) {
          if (!cancelled) setError("Pick a shop first — shop browsing is not wired up yet.");
          return;
        }
        const hits = await searchCatalog(vendorId, "");
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

  const add = useCallback(
    async (item: SearchHit) => {
      if (!vendorId) return;
      try {
        // The basket is created lazily on the first add, so browsing without
        // buying leaves no empty baskets behind.
        const b = basket ?? (await createBasket());
        setBasket(await addLine(b.id, vendorId, item.item_id));
      } catch (e) {
        setError(e instanceof Error ? e.message : "Could not add that");
      }
    },
    [basket, vendorId]
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
          {vertical}
        </Text>

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
    </SafeAreaView>
  );
}
