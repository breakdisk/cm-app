/**
 * Screen D — where is my order.
 *
 * Polling, not SSE. The mesh streams because a run is seconds long and the
 * cards must appear as they happen; an order takes half an hour and the
 * interesting transitions are minutes apart, so holding a connection open on a
 * phone that is backgrounding and reconnecting buys nothing and costs battery.
 *
 * The status line leads and the timeline follows, because "where is it" is the
 * question and the history is the justification.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { ActivityIndicator, Pressable, ScrollView, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams } from "expo-router";

import { openHostedCheckout } from "@/api/payment";
import { trackOrder, pollIntervalMs, type TrackResponse } from "@/api/tracking";
import { MapSurface } from "@/components/map/MapSurface";
import { Receipt } from "@/components/Receipt";
import { theme } from "@/theme";

/** What each status means to someone waiting, not to the state machine. */
const SAY: Record<TrackResponse["status"], { title: string; sub: string; tone: string }> = {
  placed:           { title: "Order placed",     sub: "Finding you a courier.",              tone: theme.muted },
  awaiting_courier: { title: "Finding a courier", sub: "This usually takes a minute or two.", tone: theme.amber },
  collecting:       { title: "Collecting",       sub: "Your courier is picking things up.",  tone: theme.cyan },
  delivering:       { title: "On the way",       sub: "Everything is collected.",            tone: theme.cyan },
  delivered:        { title: "Delivered",        sub: "Enjoy.",                              tone: theme.green },
  cancelled:        { title: "Cancelled",        sub: "This order was cancelled.",           tone: theme.red },
};

/** Timeline event types are machine names; nobody waiting wants to read them. */
const EVENT_LABEL: Record<string, string> = {
  "order.placed":        "Order placed",
  "courier.claimed":     "Courier accepted",
  "courier.reoffered":   "Looking again for a courier",
  "vendor_leg.picked_up": "Picked up",
  "vendor_leg.failed":   "A shop couldn't fulfil their part",
  "order.delivered":     "Delivered",
  "order.cancelled":     "Cancelled",
  "order.escalated":     "We're looking into a delay",
};

/** The steps every order passes through, so the list shows what is still to come. */
const FUTURE_STEPS: { key: TrackResponse["status"][]; label: string }[] = [
  { key: ["placed", "awaiting_courier"], label: "Courier accepted" },
  { key: ["placed", "awaiting_courier", "collecting"], label: "All items collected" },
  { key: ["placed", "awaiting_courier", "collecting", "delivering"], label: "Delivered" },
];

export default function Track() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const [order, setOrder] = useState<TrackResponse | null>(null);
  const [error, setError] = useState<string | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const poll = useCallback(async () => {
    if (!id) return;
    try {
      const next = await trackOrder(id);
      setOrder(next);
      setError(null);

      // The schedule this screen should always have used: 5s while the courier
      // is moving and the map wants freshness, 15s otherwise, and nothing at
      // all once the order is terminal.
      const next_ms = pollIntervalMs(next.status, next);
      if (next_ms !== null) {
        timer.current = setTimeout(() => void poll(), next_ms);
      }
    } catch {
      // Keep the last known state on screen and keep trying: a dropped request
      // in a lift is not a reason to blank the page.
      setError("Couldn't refresh just now — still trying.");
      timer.current = setTimeout(() => void poll(), 15_000);
    }
  }, [id]);

  useEffect(() => {
    void poll();
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, [poll]);

  if (!order) {
    return (
      <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas, justifyContent: "center" }}>
        <ActivityIndicator color={theme.cyan} />
      </SafeAreaView>
    );
  }

  // `status` alone is a lie for an order whose payment never completed: the
  // server leaves it `placed` and offers it to nobody, so "Finding you a
  // courier" would be a promise nothing is working on.
  const awaitingPayment =
    order.payment_method === "online" && order.payment_status === "pending" &&
    order.status !== "delivered" && order.status !== "cancelled";

  const say = awaitingPayment
    ? {
        title: "Waiting for payment",
        sub: "We look for a courier as soon as your card goes through.",
        tone: theme.amber,
      }
    : SAY[order.status];

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <ScrollView contentContainerStyle={{ padding: 22, gap: 18 }}>
        <View style={{ gap: 6 }}>
          <Text style={{ color: say.tone, fontSize: 26, fontWeight: "800" }}>{say.title}</Text>
          <Text style={{ color: theme.muted, fontSize: 14 }}>{say.sub}</Text>
          {order.eta && (
            <Text style={{ color: theme.text, fontSize: 15, fontWeight: "600" }}>
              {order.eta.low_minutes === order.eta.high_minutes
                ? `Arriving in about ${order.eta.low_minutes} min`
                : `Arriving in ${order.eta.low_minutes}–${order.eta.high_minutes} min`}
            </Text>
          )}
        </View>

        {/* Four states, not one. A finished delivery does not need a map, and
            the backend returns no position for one anyway. */}
        {order.destination &&
          order.status !== "delivered" &&
          order.status !== "cancelled" && (
            <View style={{ gap: 6 }}>
              <MapSurface
                courier={order.courier}
                stops={order.stops}
                destination={order.destination}
              />
              {!order.courier && (
                <Text style={{ color: theme.faint, fontSize: 12 }}>
                  Waiting for the courier&apos;s location.
                </Text>
              )}
            </View>
          )}

        {!awaitingPayment && (
          <Text style={{ color: theme.muted, fontSize: 13 }}>
            {order.stops_collected} of {order.stops_total} stops collected
          </Text>
        )}

        {/* The way back into an unfinished payment. Without it, anything that
            took the customer off the card page — a call, backgrounding the app,
            tapping Back — left an order that could not be paid for and no route
            to it; it simply sat there until the server cancelled it. */}
        {awaitingPayment && (
          <View
            style={{
              borderLeftWidth: 2,
              borderLeftColor: theme.amber,
              backgroundColor: "rgba(255,171,0,0.07)",
              borderRadius: theme.radius.sm,
              padding: 13,
              gap: 9,
            }}
          >
            <Text style={{ color: "rgba(255,255,255,0.82)", fontSize: 12, lineHeight: 17 }}>
              {order.resume_payment_url
                ? "Your payment hasn't gone through yet. Nothing has been charged, and no courier has been asked to collect this."
                : "This payment wasn't completed in time. Nothing has been charged — place the order again when you're ready."}
            </Text>
            {order.resume_payment_url && (
              <Pressable
                onPress={() => void openHostedCheckout(order.resume_payment_url!)}
                accessibilityRole="button"
                accessibilityLabel="Finish paying for this order"
                style={{
                  paddingVertical: 12,
                  borderRadius: theme.radius.sm,
                  alignItems: "center",
                  borderWidth: 1,
                  borderColor: theme.amber,
                  backgroundColor: "rgba(255,171,0,0.12)",
                }}
              >
                <Text style={{ color: theme.amber, fontSize: 13, fontWeight: "700" }}>
                  Finish paying
                </Text>
              </Pressable>
            )}
          </View>
        )}

        <Receipt
          goods_total_cents={order.goods_total_cents}
          delivery_fee_cents={order.delivery_fee_cents}
          tip_cents={order.tip_cents}
          grand_total_cents={order.grand_total_cents}
          cod_amount_cents={order.cod_amount_cents}
          payment_status={order.payment_status}
          settled={order.status === "delivered"}
        />

        {error && (
          <Text style={{ color: theme.faint, fontSize: 12 }}>{error}</Text>
        )}

        <View style={{ gap: 10 }}>
          {order.timeline.map((e, i) => (
            <View key={i} style={{ flexDirection: "row", gap: 10 }}>
              <View
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: 3,
                  backgroundColor: i === 0 ? theme.cyan : theme.faint,
                  marginTop: 6,
                }}
              />
              <View style={{ flex: 1 }}>
                <Text style={{ color: theme.text, fontSize: 13 }}>
                  {EVENT_LABEL[e.event_type] ?? e.event_type}
                </Text>
                <Text style={{ color: theme.faint, fontSize: 11 }}>
                  {new Date(e.at).toLocaleTimeString()}
                </Text>
              </View>
            </View>
          ))}

          {order.status !== "cancelled" &&
            FUTURE_STEPS.filter((s) => s.key.includes(order.status)).map((s) => (
              <View key={s.label} style={{ flexDirection: "row", gap: 10 }}>
                <View
                  style={{
                    width: 6,
                    height: 6,
                    borderRadius: 3,
                    backgroundColor: theme.faint,
                    marginTop: 6,
                    opacity: 0.4,
                  }}
                />
                <Text style={{ color: theme.faint, fontSize: 13, opacity: 0.6 }}>{s.label}</Text>
              </View>
            ))}
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}
