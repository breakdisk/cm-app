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
import { ActivityIndicator, ScrollView, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams } from "expo-router";

import { trackOrder, type TrackResponse } from "@/api/tracking";
import { theme } from "@/theme";

const POLL_MS = 8000;

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

const peso = (c: number) => `₱${(c / 100).toFixed(2)}`;

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

      // Stop polling once nothing more will change. A screen left open on a
      // delivered order should not keep waking the radio all evening.
      if (next.status !== "delivered" && next.status !== "cancelled") {
        timer.current = setTimeout(() => void poll(), POLL_MS);
      }
    } catch {
      // Keep the last known state on screen and keep trying: a dropped request
      // in a lift is not a reason to blank the page.
      setError("Couldn't refresh just now — still trying.");
      timer.current = setTimeout(() => void poll(), POLL_MS);
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

  const say = SAY[order.status];

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <ScrollView contentContainerStyle={{ padding: 22, gap: 18 }}>
        <View style={{ gap: 6 }}>
          <Text style={{ color: say.tone, fontSize: 26, fontWeight: "800" }}>{say.title}</Text>
          <Text style={{ color: theme.muted, fontSize: 14 }}>{say.sub}</Text>
        </View>

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
          <Row label="Total" value={peso(order.grand_total_cents)} />
          <Row
            label="Stops collected"
            value={`${order.stops_collected} of ${order.stops_total}`}
          />
          {/* Cash on delivery: say the number, so nobody is surprised at the door. */}
          {order.status !== "delivered" && (
            <Text style={{ color: theme.amber, fontSize: 12, marginTop: 2 }}>
              Please have {peso(order.grand_total_cents)} in cash ready.
            </Text>
          )}
        </View>

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
        </View>
      </ScrollView>
    </SafeAreaView>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
      <Text style={{ color: theme.muted, fontSize: 13 }}>{label}</Text>
      <Text style={{ color: theme.text, fontSize: 13, fontWeight: "700" }}>{value}</Text>
    </View>
  );
}
