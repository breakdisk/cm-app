/**
 * Your orders.
 *
 * This screen exists because checkout was a one-way door: it handed you a
 * tracking screen and nothing in the app ever linked back to it. Close the app
 * during a delivery and the order was gone — not cancelled, just unreachable.
 *
 * Ordered newest first and led by the shop names, because "the one from Kuya's
 * an hour ago" is how a person finds an order, not by its id.
 */
import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, Pressable, RefreshControl, ScrollView, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { router } from "expo-router";

import { listMyOrders, type OrderListItem } from "@/api/orders";
import { theme } from "@/theme";

/** What each status means to someone waiting, not to the state machine. */
const LABEL: Record<string, { text: string; tone: string }> = {
  placed:           { text: "Placed",         tone: theme.muted },
  awaiting_courier: { text: "Finding courier", tone: theme.amber },
  collecting:       { text: "Collecting",     tone: theme.cyan },
  delivering:       { text: "On the way",     tone: theme.cyan },
  delivered:        { text: "Delivered",      tone: theme.green },
  cancelled:        { text: "Cancelled",      tone: theme.red },
};

const peso = (cents: number) => `₱${(cents / 100).toFixed(2)}`;

/** "20m ago" — relative, because nobody reads a timestamp to find an order. */
function since(iso: string): string {
  const mins = Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 60000));
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

export default function Orders() {
  const [orders, setOrders] = useState<OrderListItem[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);

  const load = useCallback(async () => {
    try {
      setOrders(await listMyOrders());
      setError(null);
    } catch {
      setError("Could not load your orders.");
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  async function refresh() {
    setRefreshing(true);
    await load();
    setRefreshing(false);
  }

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <View style={{ paddingHorizontal: 24, paddingTop: 12, paddingBottom: 8 }}>
        <Text style={{ color: theme.text, fontSize: 26, fontWeight: "800" }}>Your orders</Text>
      </View>

      {orders === null && !error ? (
        <View style={{ flex: 1, alignItems: "center", justifyContent: "center" }}>
          <ActivityIndicator color={theme.cyan} />
        </View>
      ) : (
        <ScrollView
          contentContainerStyle={{ padding: 24, paddingTop: 8, gap: 12 }}
          refreshControl={
            <RefreshControl refreshing={refreshing} onRefresh={refresh} tintColor={theme.cyan} />
          }
        >
          {error && <Text style={{ color: theme.red, fontSize: 13 }}>{error}</Text>}

          {orders?.length === 0 && !error && (
            <Text style={{ color: theme.muted, fontSize: 14, lineHeight: 20 }}>
              Nothing yet. Tell us what you need on the home screen and we will put it together.
            </Text>
          )}

          {orders?.map((o) => {
            const label = LABEL[o.status] ?? { text: o.status, tone: theme.muted };
            return (
              <Pressable
                key={o.order_id}
                onPress={() => router.push(`/track/${o.order_id}`)}
                style={{
                  borderWidth: 1,
                  borderColor: "rgba(255,255,255,0.08)",
                  borderRadius: theme.radius.md,
                  padding: 16,
                  gap: 6,
                }}
              >
                <View style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between", gap: 12 }}>
                  <Text
                    numberOfLines={1}
                    style={{ color: theme.text, fontSize: 15, fontWeight: "700", flex: 1 }}
                  >
                    {/* Empty when an order has no legs. Rendered as a plain
                        fallback rather than a made-up shop name. */}
                    {o.vendor_names || "Order"}
                  </Text>
                  <Text style={{ color: label.tone, fontSize: 12, fontWeight: "700" }}>
                    {label.text}
                  </Text>
                </View>

                <Text style={{ color: theme.muted, fontSize: 12 }}>
                  {peso(o.grand_total_cents)} · {o.stops_total} {o.stops_total === 1 ? "stop" : "stops"} ·{" "}
                  {since(o.placed_at)}
                </Text>
              </Pressable>
            );
          })}
        </ScrollView>
      )}
    </SafeAreaView>
  );
}
