/**
 * The customer's money on the home canvas.
 *
 * Deliberately not a wallet. There is no balance, no top-up and no withdraw,
 * because OmniDeliv has no rail behind any of them — every order is cash on
 * delivery, payment capture is deferred and refunds do not exist. The nearest
 * thing to a balance a customer has is the cash they are about to hand over,
 * so that is what this shows.
 */
import { useCallback, useEffect, useState } from "react";
import { Pressable, Text, View } from "react-native";
import { router } from "expo-router";

import { listMyOrders, type OrderListItem } from "@/api/orders";
import { cashDue, monthToDate, peso } from "@/money";
import { theme } from "@/theme";

export function MoneyPanel() {
  const [orders, setOrders] = useState<OrderListItem[] | null>(null);

  const load = useCallback(async () => {
    try {
      setOrders(await listMyOrders());
    } catch {
      // Silent. This panel is context on a screen whose job is taking an
      // order; a failed rollup must not put an error in front of that.
      setOrders([]);
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  if (orders === null) return null;

  const due = cashDue(orders);
  const month = monthToDate(orders);

  // Nothing owed and nothing spent yet — say nothing rather than showing a
  // ₱0.00 that looks like an empty balance.
  if (!due && month.count === 0) return null;

  return (
    <View
      style={{
        borderWidth: 1,
        borderColor: "rgba(255,255,255,0.08)",
        borderRadius: theme.radius.md,
        overflow: "hidden",
      }}
    >
      {due && (
        <Pressable
          onPress={() => router.push(`/track/${due.order_id}`)}
          accessibilityRole="button"
          accessibilityLabel={`Cash due at the door, ${peso(due.grand_total_cents)}`}
          style={{ padding: 14, gap: 2 }}
        >
          <Text style={{ color: theme.amber, fontSize: 10, letterSpacing: 1.2 }}>
            CASH DUE AT THE DOOR
          </Text>
          <Text style={{ color: theme.text, fontSize: 24, fontWeight: "800" }}>
            {peso(due.grand_total_cents)}
          </Text>
          <Text numberOfLines={1} style={{ color: theme.muted, fontSize: 12 }}>
            {due.vendor_names || "Order"} · on the way
          </Text>
        </Pressable>
      )}

      {month.count > 0 && (
        <View
          style={{
            padding: 14,
            borderTopWidth: due ? 1 : 0,
            borderTopColor: "rgba(255,255,255,0.08)",
            flexDirection: "row",
            justifyContent: "space-between",
            alignItems: "center",
          }}
        >
          <Text style={{ color: theme.muted, fontSize: 12 }}>
            This month · {month.count} {month.count === 1 ? "order" : "orders"}
          </Text>
          <Text style={{ color: theme.text, fontSize: 14, fontWeight: "700" }}>
            {peso(month.cents)}
          </Text>
        </View>
      )}
    </View>
  );
}
