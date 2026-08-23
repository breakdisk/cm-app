/**
 * Screen C — review the basket and place the order.
 *
 * This screen used to end with "Reviewing swaps and placing the order arrive
 * with the checkout service." That service arrived: `checkout()` has been sitting
 * fully written in `api/orders.ts`, error classification and all, with **nothing
 * calling it**. The whole funnel dead-ended here — you could browse, add to a
 * basket, and then had no way to buy anything.
 *
 * It also showed a total and no itemisation. A checkout screen that tells you
 * what you owe but not what for, and offers no way to remove a line, is the one
 * shape a checkout screen must not have.
 *
 * The tip is the only money the client chooses. Goods prices, modifier deltas
 * and the delivery fee are all computed server-side — see `orders.ts`.
 */
import { useCallback, useEffect, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, Text, TextInput, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams, useRouter } from "expo-router";

import { getBasket, removeLine, type BasketView } from "@/api/basket";
import { checkout, classifyCheckoutError } from "@/api/orders";
import { currentDeliveryPoint, hasDeliveryPoint } from "@/deliveryPoint";
import { theme } from "@/theme";

function peso(cents: number): string {
  return `₱${(cents / 100).toFixed(2)}`;
}

export default function Review() {
  const { basketId } = useLocalSearchParams<{ basketId: string }>();
  const router = useRouter();
  const [basket, setBasket] = useState<BasketView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tip, setTip] = useState("0");
  // The courier reads this at the door. See `checkout` in api/orders.ts.
  const [note, setNote] = useState("");
  const [placing, setPlacing] = useState(false);

  const load = useCallback(async () => {
    if (!basketId) return;
    try {
      setBasket(await getBasket(basketId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not load your basket");
    }
  }, [basketId]);

  useEffect(() => {
    void load();
  }, [load]);

  const lines = basket?.lines ?? [];
  const blocked = (basket?.lines_awaiting_review ?? 0) > 0;
  const empty = !basket || basket.goods_total_cents === 0;

  async function remove(lineId: string) {
    if (!basketId) return;
    setError(null);
    try {
      setBasket(await removeLine(basketId, lineId));
    } catch (e) {
      setError(e instanceof Error ? e.message : "Could not remove that");
    }
  }

  async function place() {
    if (!basketId) return;
    // Refuse before the round trip rather than after. Checkout would answer 400,
    // but "where are we delivering this" is a question this screen can answer by
    // sending the customer to the address screen instead of showing an error.
    if (!hasDeliveryPoint()) {
      setError("Set a delivery address first.");
      router.push("/address");
      return;
    }

    setPlacing(true);
    setError(null);
    try {
      const point = currentDeliveryPoint();
      const tipCents = Math.max(0, Math.round(parseFloat(tip || "0") * 100));
      const res = await checkout(basketId, tipCents, point.lat, point.lng, note);
      router.replace(`/track/${res.order_id}`);
    } catch (e) {
      // Each of these is a different thing for the customer to do, which is why
      // `classifyCheckoutError` exists rather than one apology.
      switch (classifyCheckoutError(e)) {
        case "awaiting_review":
          setError("Some swaps still need your OK before we can place this.");
          await load();
          break;
        case "no_courier":
          // Nothing was charged and the basket is intact — say so, or the
          // customer assumes they have to start again.
          setError("No courier is free right now. Nothing was charged — try again shortly.");
          break;
        case "rejected":
          setError(e instanceof Error ? e.message : "We could not place this order.");
          await load();
          break;
        default:
          setError("Something went wrong placing your order.");
      }
    } finally {
      setPlacing(false);
    }
  }

  if (!basket && !error) {
    return (
      <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, justifyContent: "center" }}>
        <ActivityIndicator color={theme.cyan} />
      </SafeAreaView>
    );
  }

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <ScrollView contentContainerStyle={{ padding: 20, gap: 14 }}>
        <Text style={{ color: theme.text, fontSize: 18, fontWeight: "600" }}>Your basket</Text>

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
                  backgroundColor: c.blocking ? "rgba(255,59,92,0.07)" : "rgba(255,171,0,0.07)",
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

        {/* An empty basket is a result, not a blank screen.
            It happens legitimately — every candidate refused because no shop
            had stated its contents and the customer named an allergy — and
            without this it reads as the app failing rather than the check
            working. */}
        {empty && (
          <View
            style={{
              borderLeftWidth: 2,
              borderLeftColor: theme.amber,
              backgroundColor: "rgba(255,171,0,0.07)",
              borderRadius: theme.radius.sm,
              padding: 12,
              gap: 4,
            }}
          >
            <Text style={{ color: theme.amber, fontSize: 13, fontWeight: "700" }}>
              Nothing made it into your basket
            </Text>
            <Text style={{ color: "rgba(255,255,255,0.72)", fontSize: 12, lineHeight: 17 }}>
              Everything we found was left out for the reasons above. If you
              mentioned something to avoid, try browsing shops directly — we
              only skip items when we can&apos;t be sure.
            </Text>
          </View>
        )}

        {lines.length > 0 && (
          <View style={{ gap: 8 }}>
            {lines.map((l) => (
              <View
                key={l.id}
                style={{
                  backgroundColor: theme.surface,
                  borderColor: l.state === "substituted" ? theme.amber : theme.border,
                  borderWidth: 1,
                  borderRadius: theme.radius.md,
                  padding: 12,
                  flexDirection: "row",
                  alignItems: "flex-start",
                  gap: 10,
                }}
              >
                <View style={{ flex: 1, gap: 3 }}>
                  <Text style={{ color: theme.text, fontSize: 13 }}>
                    {l.qty > 1 ? `${l.qty} × ` : ""}
                    {l.name}
                  </Text>

                  {/* The chosen options, priced. Without these a line reads as
                      the wrong price — the delta is already inside the unit
                      price, so "Adobo ₱200" when the menu says ₱180 looks like
                      an error rather than the large one they picked. */}
                  {l.modifiers.map((m) => (
                    <Text key={m.option_id} style={{ color: theme.faint, fontSize: 11 }}>
                      {m.option_name}
                      {m.price_delta_cents !== 0
                        ? ` (${m.price_delta_cents > 0 ? "+" : "−"}${peso(
                            Math.abs(m.price_delta_cents),
                          )})`
                        : ""}
                    </Text>
                  ))}

                  {l.state === "substituted" && (
                    <Text style={{ color: theme.amber, fontSize: 11 }}>
                      Swapped — needs your OK
                    </Text>
                  )}
                </View>

                <Text style={{ color: theme.text, fontSize: 13, fontWeight: "600" }}>
                  {peso(l.subtotal_cents)}
                </Text>

                <Pressable
                  onPress={() => void remove(l.id)}
                  accessibilityRole="button"
                  accessibilityLabel={`Remove ${l.name}`}
                  hitSlop={8}
                >
                  <Text style={{ color: theme.faint, fontSize: 16 }}>×</Text>
                </Pressable>
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
              gap: 10,
            }}
          >
            <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
              <Text style={{ color: theme.muted, fontSize: 12 }}>Goods</Text>
              <Text style={{ color: theme.text, fontSize: 14, fontWeight: "700" }}>
                {peso(basket.goods_total_cents)}
              </Text>
            </View>

            {blocked && (
              <Text style={{ color: theme.amber, fontSize: 12 }}>
                {basket.lines_awaiting_review === 1
                  ? "1 swap needs your OK"
                  : `${basket.lines_awaiting_review} swaps need your OK`}
              </Text>
            )}

            <View style={{ gap: 6 }}>
              <Text style={{ color: theme.muted, fontSize: 12 }}>
                Delivery note (optional)
              </Text>
              <TextInput
                value={note}
                onChangeText={setNote}
                placeholder="Unit 12B, gate code 4417, ring twice"
                placeholderTextColor={theme.muted}
                multiline
                maxLength={280}
                accessibilityLabel="Delivery note for the courier"
                style={{
                  color: theme.text,
                  fontSize: 14,
                  minHeight: 64,
                  textAlignVertical: "top",
                  paddingVertical: 8,
                  paddingHorizontal: 10,
                  borderRadius: theme.radius.sm,
                  borderWidth: 1,
                  borderColor: theme.border,
                }}
              />
            </View>

            <View
              style={{ flexDirection: "row", alignItems: "center", justifyContent: "space-between" }}
            >
              <Text style={{ color: theme.muted, fontSize: 12 }}>Tip for your courier</Text>
              <View style={{ flexDirection: "row", alignItems: "center", gap: 4 }}>
                <Text style={{ color: theme.muted, fontSize: 13 }}>₱</Text>
                <TextInput
                  value={tip}
                  onChangeText={setTip}
                  keyboardType="decimal-pad"
                  accessibilityLabel="Tip amount in pesos"
                  style={{
                    color: theme.text,
                    fontSize: 14,
                    minWidth: 64,
                    textAlign: "right",
                    paddingVertical: 6,
                    paddingHorizontal: 8,
                    borderRadius: theme.radius.sm,
                    borderWidth: 1,
                    borderColor: theme.border,
                  }}
                />
              </View>
            </View>

            {/* Deliberately not a total. The delivery fee is computed at
                checkout, so any number shown here would be a guess the
                customer would read as a promise. */}
            <Text style={{ color: theme.faint, fontSize: 11 }}>
              Delivery is worked out when you place the order.
            </Text>
          </View>
        )}

        {basket && !empty && (
          <Pressable
            onPress={() => void place()}
            disabled={placing || blocked}
            accessibilityRole="button"
            accessibilityState={{ disabled: placing || blocked }}
            accessibilityLabel="Place order"
            style={{
              paddingVertical: 15,
              borderRadius: theme.radius.md,
              alignItems: "center",
              borderWidth: 1,
              backgroundColor: placing || blocked ? "rgba(255,255,255,0.05)" : "rgba(0,229,255,0.15)",
              borderColor: placing || blocked ? theme.border : theme.cyan,
            }}
          >
            <Text
              style={{
                color: placing || blocked ? theme.faint : theme.cyan,
                fontSize: 14,
                fontWeight: "700",
              }}
            >
              {placing ? "Placing…" : blocked ? "Resolve the swaps first" : "Place order"}
            </Text>
          </Pressable>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}
