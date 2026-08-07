/**
 * Screen C — consolidation review.
 *
 * INCOMPLETE BY DESIGN: the substitution cards and the Place Order action are
 * Plan 5's (checkout, orders and the three-leg settlement). What is here is the
 * part the backend already supports — the real basket, its total, and how many
 * lines are waiting on a decision.
 *
 * There is deliberately no disabled "Place order" button. A control that looks
 * like checkout and does nothing is worse than its absence: it reads as a bug
 * to the customer and as done to the next reader.
 */
import { useEffect, useState } from "react";
import { ActivityIndicator, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams } from "expo-router";

import { getBasket, type BasketView } from "@/api/basket";
import { theme } from "@/theme";

function peso(cents: number): string {
  return `₱${(cents / 100).toFixed(2)}`;
}

export default function Review() {
  const { basketId } = useLocalSearchParams<{ basketId: string }>();
  const [basket, setBasket] = useState<BasketView | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    if (!basketId) return;

    getBasket(basketId)
      .then((b) => { if (!cancelled) setBasket(b); })
      .catch((e: unknown) => {
        if (!cancelled) setError(e instanceof Error ? e.message : "Could not load your basket");
      });

    return () => { cancelled = true; };
  }, [basketId]);

  if (!basket && !error) {
    return (
      <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, justifyContent: "center" }}>
        <ActivityIndicator color={theme.cyan} />
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <View style={{ padding: 20, gap: 14 }}>
        <Text style={{ color: theme.text, fontSize: 18, fontWeight: "600" }}>
          Your basket
        </Text>

        {error && (
          <Text accessibilityRole="alert" style={{ color: theme.amber, fontSize: 12 }}>
            {error}
          </Text>
        )}

        {/* Above the totals, not below: these are things the customer needs
            before deciding, not afterwards. Screen B streamed them while the
            run was in flight, but anyone who tapped through quickly never
            read them — and a blocking conflict means a line they asked for is
            missing from what they are about to pay for. */}
        {basket?.conflicts && basket.conflicts.length > 0 && (
          <View style={{ gap: 8 }}>
            {basket.conflicts.map((c, i) => (
              <View
                key={i}
                accessibilityRole="alert"
                style={{
                  borderLeftWidth: 2,
                  borderLeftColor: c.blocking ? theme.red : theme.amber,
                  backgroundColor: c.blocking
                    ? "rgba(255,59,92,0.07)"
                    : "rgba(255,171,0,0.07)",
                  borderRadius: theme.radius.sm,
                  padding: 11,
                }}
              >
                <Text
                  style={{
                    color: c.blocking ? theme.red : theme.amber,
                    fontSize: 9.5,
                    letterSpacing: 1,
                    marginBottom: 4,
                  }}
                >
                  {c.blocking ? "WE CHANGED SOMETHING" : "WORTH KNOWING"}
                </Text>
                <Text style={{ color: "rgba(255,255,255,0.82)", fontSize: 12 }}>
                  {c.description}
                </Text>
              </View>
            ))}
          </View>
        )}

        {basket && (
          <View
            style={{
              backgroundColor: theme.surface,
              borderColor: theme.border,
              borderWidth: 1,
              borderRadius: theme.radius.md,
              padding: 14,
              gap: 8,
            }}
          >
            <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
              <Text style={{ color: theme.muted, fontSize: 12 }}>Goods</Text>
              <Text style={{ color: theme.text, fontSize: 14, fontWeight: "700" }}>
                {peso(basket.goods_total_cents)}
              </Text>
            </View>

            {basket.lines_awaiting_review > 0 && (
              <Text style={{ color: theme.amber, fontSize: 12 }}>
                {basket.lines_awaiting_review === 1
                  ? "1 swap needs your OK"
                  : `${basket.lines_awaiting_review} swaps need your OK`}
              </Text>
            )}
          </View>
        )}

        <Text style={{ color: theme.faint, fontSize: 11, lineHeight: 16 }}>
          Reviewing swaps and placing the order arrive with the checkout service.
        </Text>
      </View>
    </SafeAreaView>
  );
}
