/**
 * PaymentWebViewScreen — hosts Network International's redirect checkout.
 *
 * order-intake opens an NI payment intent when a booking's quote_token
 * carries a fee that must be collected up front (see
 * `ShipmentService::create` → `payments_client.create_shipping_fee_intent`
 * in services/order-intake) and returns its hosted `checkout_url` on the
 * shipment response. This screen just displays that URL.
 *
 * NI redirects the WebView itself back to the `return_url` order-intake
 * generated (`.../payment/return?shipment_id=...`) once the checkout flow
 * ends — whether the charge succeeded, failed, or the customer bounced
 * through an intermediate gateway page. That redirect is a UX signal only:
 * the customer can also back out of the WebView before it ever fires, and
 * either way the URL itself says nothing about whether money actually moved.
 * The authoritative answer is the payments webhook writing the shipment's
 * own `payment_status` (services/order-intake/src/infrastructure/messaging/payment_consumer.rs),
 * so all this screen does on seeing the return URL is hand off to
 * BookingConfirmationPendingScreen, which polls that field directly.
 */
import React, { useCallback, useRef } from "react";
import { View, Text, Pressable, ActivityIndicator, StyleSheet } from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { WebView, WebViewNavigation } from "react-native-webview";

// ── Design tokens ─────────────────────────────────────────────────────────────
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

interface PaymentWebViewScreenProps {
  route: {
    params: {
      /** Hosted checkout URL from `ShipmentResponse.checkout_url`. */
      checkoutUrl: string;
      /** UUID from `ShipmentResponse.id` — used to recognize the gateway's
       *  return redirect and to hand off to the polling screen. */
      shipmentId: string;
    };
  };
  navigation: any;
}

export function PaymentWebViewScreen({ route, navigation }: PaymentWebViewScreenProps) {
  const insets = useSafeAreaInsets();
  const { checkoutUrl, shipmentId } = route.params;
  // Guards against firing the hand-off twice — NI's checkout can touch the
  // return_url path more than once while it settles (e.g. an intermediate
  // 3DS bounce), and onNavigationStateChange fires on every navigation.
  const handedOff = useRef(false);

  const handleNavigationChange = useCallback((navState: WebViewNavigation) => {
    if (handedOff.current) return;
    if (navState.url.includes("/payment/return") && navState.url.includes(`shipment_id=${shipmentId}`)) {
      handedOff.current = true;
      navigation.replace("BookingConfirmationPending", { shipmentId });
    }
  }, [navigation, shipmentId]);

  return (
    <View style={styles.container}>
      {/* ── Header ───────────────────────────────────────────────────────── */}
      <LinearGradient
        colors={["rgba(0,229,255,0.08)", CANVAS]}
        style={[styles.header, { paddingTop: insets.top + 8 }]}
      >
        <Pressable onPress={() => navigation.goBack()} hitSlop={12} style={styles.backBtn}>
          <Ionicons name="chevron-back" size={22} color="#FFF" />
        </Pressable>
        <View style={{ flex: 1 }}>
          <Text style={styles.headerTitle}>Secure Payment</Text>
          <Text style={styles.headerSub} numberOfLines={1}>Powered by Network International</Text>
        </View>
      </LinearGradient>

      <WebView
        source={{ uri: checkoutUrl }}
        onNavigationStateChange={handleNavigationChange}
        startInLoadingState
        renderLoading={() => (
          <View style={styles.loading}>
            <ActivityIndicator size="large" color={CYAN} />
          </View>
        )}
        style={styles.webview}
      />
    </View>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1, backgroundColor: CANVAS },
  header: {
    flexDirection: "row",
    alignItems: "center",
    paddingHorizontal: 16,
    paddingBottom: 12,
    borderBottomWidth: 1,
    borderBottomColor: BORDER,
  },
  backBtn: {
    width: 36,
    height: 36,
    borderRadius: 18,
    backgroundColor: GLASS,
    alignItems: "center",
    justifyContent: "center",
    marginRight: 12,
  },
  headerTitle: { color: "#FFF", fontSize: 16, fontWeight: "700" },
  headerSub: { color: "rgba(255,255,255,0.4)", fontSize: 11, marginTop: 2 },
  webview: { flex: 1, backgroundColor: CANVAS },
  loading: {
    ...StyleSheet.absoluteFillObject,
    alignItems: "center",
    justifyContent: "center",
    backgroundColor: CANVAS,
  },
});
