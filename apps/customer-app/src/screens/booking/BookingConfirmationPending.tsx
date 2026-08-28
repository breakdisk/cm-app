/**
 * BookingConfirmationPendingScreen — lands right after PaymentWebViewScreen
 * hands off (gateway redirect fired, or the customer closed the checkout).
 *
 * Polls `GET /v1/shipments/:id` until `payment_status` leaves
 * `awaiting_payment` — that field only moves once order-intake's payment
 * consumer processes the payments-service webhook
 * (services/order-intake/src/infrastructure/messaging/payment_consumer.rs),
 * which is the one authoritative source for whether the charge captured.
 * Never trust the WebView redirect alone for that.
 *
 * On `paid`   → hand off to the same Collection screen every non-payment
 *               booking already lands on (BookingScreen.tsx's post-booking
 *               "Track Pickup" action navigates here identically).
 * On `payment_failed` → tell the customer and let them go back to booking
 *               (the wizard screen underneath is still mounted with its
 *               state intact — a plain goBack() returns them to it).
 * On timeout  → stop polling and point them at My Shipments rather than
 *               spinning forever; the webhook can still land after this.
 */
import React, { useCallback, useEffect, useRef, useState } from "react";
import { View, Text, ActivityIndicator, Pressable, StyleSheet } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { Ionicons } from "@expo/vector-icons";
import { getShipment } from "../../services/api/shipments";

// ── Design tokens ─────────────────────────────────────────────────────────────
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const RED    = "#FF4444";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

const POLL_INTERVAL_MS = 2_000;
// ~60s of polling. The webhook path is normally sub-second once NI posts to
// payments-service, so this just bounds how long a customer stares at a
// spinner before being told to check back instead of waiting indefinitely.
const MAX_POLLS = 30;

interface BookingConfirmationPendingScreenProps {
  route: {
    params: {
      shipmentId: string;
    };
  };
  navigation: any;
}

type PollState = "polling" | "failed" | "timed_out";

export function BookingConfirmationPendingScreen({ route, navigation }: BookingConfirmationPendingScreenProps) {
  const insets = useSafeAreaInsets();
  const { shipmentId } = route.params;
  const [state, setState] = useState<PollState>("polling");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const attemptsRef = useRef(0);

  const stopPolling = useCallback(() => {
    if (pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, []);

  const checkStatus = useCallback(async () => {
    attemptsRef.current += 1;
    try {
      const shipment = await getShipment(shipmentId);
      if (shipment.payment_status === "paid") {
        stopPolling();
        navigation.replace("Collection", {
          awb: shipment.awb,
          shipmentId: shipment.id,
          // Approximation: only Balikbayan bookings are cross-border today,
          // and this is purely a cosmetic accent color on Collection — not
          // used for any business logic there.
          type: shipment.service_type === "balikbayan" ? "international" : "local",
        });
        return;
      }
      if (shipment.payment_status === "payment_failed") {
        stopPolling();
        setState("failed");
        return;
      }
      // Still "awaiting_payment" (or the field is absent on an older
      // shipment) — keep polling until MAX_POLLS below.
    } catch {
      // Transient network/API error — stay quiet and let the next tick
      // retry. Only the shipment's own payment_status ends the poll.
    }
    if (attemptsRef.current >= MAX_POLLS) {
      stopPolling();
      setState("timed_out");
    }
  }, [shipmentId, navigation, stopPolling]);

  useEffect(() => {
    checkStatus();
    pollRef.current = setInterval(checkStatus, POLL_INTERVAL_MS);
    return stopPolling;
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <View style={[styles.container, { paddingTop: insets.top }]}>
      {state === "polling" && (
        <>
          <ActivityIndicator size="large" color={CYAN} />
          <Text style={styles.title}>Confirming your payment…</Text>
          <Text style={styles.sub}>This usually takes just a few seconds.</Text>
        </>
      )}

      {state === "failed" && (
        <>
          <Ionicons name="close-circle-outline" size={48} color={RED} />
          <Text style={styles.title}>Payment didn't go through</Text>
          <Text style={styles.sub}>
            Your shipment wasn't booked. You can try the payment again from the booking screen.
          </Text>
          <Pressable onPress={() => navigation.goBack()} style={styles.btn}>
            <Text style={styles.btnText}>Back to Booking</Text>
          </Pressable>
        </>
      )}

      {state === "timed_out" && (
        <>
          <Ionicons name="time-outline" size={48} color="rgba(255,255,255,0.3)" />
          <Text style={styles.title}>Still confirming</Text>
          <Text style={styles.sub}>
            Your payment is still being processed. We'll update your tracking status as soon as
            it clears — check My Shipments shortly.
          </Text>
          <Pressable
            onPress={() => navigation.navigate("Tabs", { screen: "History" })}
            style={styles.btn}
          >
            <Text style={styles.btnText}>Go to My Shipments</Text>
          </Pressable>
        </>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: CANVAS,
    alignItems: "center",
    justifyContent: "center",
    paddingHorizontal: 32,
    gap: 12,
  },
  title: { color: "#FFF", fontSize: 16, fontWeight: "700", marginTop: 8, textAlign: "center" },
  sub: { color: "rgba(255,255,255,0.5)", fontSize: 13, textAlign: "center", lineHeight: 18 },
  btn: {
    marginTop: 12,
    backgroundColor: GLASS,
    borderWidth: 1,
    borderColor: BORDER,
    borderRadius: 12,
    paddingVertical: 12,
    paddingHorizontal: 24,
  },
  btnText: { color: CYAN, fontSize: 14, fontWeight: "600" },
});
