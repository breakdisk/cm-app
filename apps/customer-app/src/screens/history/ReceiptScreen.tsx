/**
 * Customer App — Delivery Receipt Screen
 * Shows a detailed receipt for a single shipment.
 * Data is sourced from the shipment record already in Redux + tracking data.
 * Navigated to from HistoryScreen card "View Receipt" button.
 */
import React, { useEffect, useState } from "react";
import {
  View, Text, StyleSheet, ScrollView, Pressable, ActivityIndicator, Share,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { FadeInView } from "../../components/FadeInView";
import { trackingApi } from "../../services/api/tracking";
import type { ShipmentRecord } from "../../store";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

interface ReceiptScreenProps {
  route: {
    params: {
      shipment: ShipmentRecord;
    };
  };
  navigation: any;
}

function ReceiptRow({ label, value, highlight }: { label: string; value: string; highlight?: string }) {
  return (
    <View style={s.row}>
      <Text style={s.rowLabel}>{label}</Text>
      <Text style={[s.rowValue, highlight ? { color: highlight } : undefined]}>{value}</Text>
    </View>
  );
}

function Divider() {
  return <View style={s.divider} />;
}

export function ReceiptScreen({ route, navigation }: ReceiptScreenProps) {
  const insets = useSafeAreaInsets();
  const { shipment } = route.params;
  const isDelivered = shipment.status === "delivered";

  const [deliveredAt, setDeliveredAt] = useState<string | null>(null);
  const [driverName, setDriverName] = useState<string | null>(null);
  const [loadingExtra, setLoadingExtra] = useState(isDelivered);

  useEffect(() => {
    if (!isDelivered) return;
    trackingApi.getLive(shipment.awb, "")
      .then(res => {
        const data = (res.data as any)?.data ?? res.data as any;
        const deliveryEvent = data.events?.find((e: any) => e.status === "delivered");
        if (deliveryEvent) setDeliveredAt(deliveryEvent.occurred_at);
        if (data.driver?.name) setDriverName(data.driver.name);
      })
      .catch(() => {/* non-critical — skip silently */})
      .finally(() => setLoadingExtra(false));
  }, [shipment.awb, isDelivered]);

  const totalFee = shipment.totalFee ?? 0;
  const baseFee = totalFee * 0.85;
  const taxFee  = totalFee * 0.12;
  const fuelFee = totalFee - baseFee - taxFee;

  async function handleShare() {
    const lines = [
      `LogisticOS — Delivery Receipt`,
      `AWB: ${shipment.awb}`,
      `Status: ${shipment.status.replace(/_/g, " ").toUpperCase()}`,
      `From: ${shipment.origin}`,
      `To:   ${shipment.destination}`,
      `Fee:  ₱${totalFee.toFixed(2)}`,
      isDelivered && deliveredAt ? `Delivered: ${deliveredAt}` : "",
    ].filter(Boolean).join("\n");
    await Share.share({ message: lines, title: `Receipt — ${shipment.awb}` });
  }

  return (
    <View style={{ flex: 1, backgroundColor: CANVAS }}>
      {/* Back + Share header */}
      <View style={[s.header, { paddingTop: insets.top + 8 }]}>
        <Pressable onPress={() => navigation.goBack()} hitSlop={12} style={s.backBtn}>
          <Ionicons name="chevron-back" size={22} color="#FFF" />
        </Pressable>
        <Text style={s.headerTitle}>Receipt</Text>
        <Pressable onPress={handleShare} hitSlop={12} style={s.shareBtn}>
          <Ionicons name="share-outline" size={20} color={CYAN} />
        </Pressable>
      </View>

      <ScrollView contentContainerStyle={{ paddingBottom: insets.bottom + 32 }}>
        {/* Hero gradient band */}
        <LinearGradient
          colors={isDelivered ? ["rgba(0,255,136,0.10)", "transparent"] : ["rgba(0,229,255,0.08)", "transparent"]}
          style={s.hero}
        >
          <FadeInView fromY={-12}>
            <View style={s.statusBadge}>
              <Ionicons
                name={isDelivered ? "checkmark-done-circle" : "time"}
                size={18}
                color={isDelivered ? GREEN : AMBER}
              />
              <Text style={[s.statusText, { color: isDelivered ? GREEN : AMBER }]}>
                {isDelivered ? "Delivered" : shipment.status.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase())}
              </Text>
            </View>
            <Text style={s.awb}>{shipment.awb}</Text>
            <Text style={s.bookedAt}>Booked {shipment.bookedAt}</Text>
          </FadeInView>
        </LinearGradient>

        {/* Shipment details card */}
        <FadeInView delay={60} fromY={12} style={s.card}>
          <Text style={s.cardTitle}>Shipment Details</Text>
          <Divider />
          <ReceiptRow label="Service Type" value={
            shipment.type === "international"
              ? `International · ${shipment.freightMode === "sea" ? "Sea" : "Air"}`
              : "Local Standard"
          } />
          <ReceiptRow label="Contents" value={shipment.description || "—"} />
          <ReceiptRow label="Weight" value={shipment.weight ? `${shipment.weight} kg` : "—"} />
          <Divider />
          <ReceiptRow label="From" value={shipment.origin} />
          <ReceiptRow label="To"   value={shipment.destination + (shipment.destCountry ? ` · ${shipment.destCountry}` : "")} />
          {shipment.estimatedDelivery && !isDelivered && (
            <ReceiptRow label="Estimated Delivery" value={shipment.estimatedDelivery} highlight={AMBER} />
          )}
          {loadingExtra && (
            <View style={{ paddingVertical: 8, alignItems: "center" }}>
              <ActivityIndicator size="small" color={CYAN} />
            </View>
          )}
          {isDelivered && deliveredAt && !loadingExtra && (
            <ReceiptRow label="Delivered On" value={deliveredAt} highlight={GREEN} />
          )}
          {driverName && (
            <ReceiptRow label="Delivered By" value={driverName} />
          )}
        </FadeInView>

        {/* Fee breakdown card */}
        <FadeInView delay={120} fromY={12} style={s.card}>
          <Text style={s.cardTitle}>Fee Breakdown</Text>
          <Divider />
          <ReceiptRow label="Base Freight"  value={`₱${baseFee.toFixed(2)}`} />
          <ReceiptRow label="Fuel Surcharge" value={`₱${fuelFee.toFixed(2)}`} />
          <ReceiptRow label="VAT (12%)"     value={`₱${taxFee.toFixed(2)}`} />
          {shipment.isCOD && shipment.codAmount && (
            <ReceiptRow
              label="COD Amount"
              value={`₱${parseFloat(String(shipment.codAmount)).toFixed(2)}`}
              highlight={AMBER}
            />
          )}
          <Divider />
          <View style={[s.row, s.totalRow]}>
            <Text style={s.totalLabel}>Total</Text>
            <Text style={[s.totalValue, { color: shipment.type === "international" ? PURPLE : CYAN }]}>
              ₱{totalFee.toFixed(2)}
            </Text>
          </View>
        </FadeInView>

        {/* COD status card — only shown for COD shipments */}
        {shipment.isCOD && (
          <FadeInView delay={180} fromY={12} style={s.card}>
            <Text style={s.cardTitle}>Cash on Delivery</Text>
            <Divider />
            <ReceiptRow label="COD Amount Due"   value={`₱${parseFloat(String(shipment.codAmount ?? 0)).toFixed(2)}`} />
            <ReceiptRow
              label="Collection Status"
              value={isDelivered ? "Collected" : "Pending collection"}
              highlight={isDelivered ? GREEN : AMBER}
            />
          </FadeInView>
        )}

        {/* Share CTA */}
        <FadeInView delay={240} fromY={12} style={s.shareCard}>
          <Pressable
            onPress={handleShare}
            style={({ pressed }) => [s.shareButton, { opacity: pressed ? 0.7 : 1 }]}
          >
            <LinearGradient
              colors={[CYAN + "20", PURPLE + "20"]}
              start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }}
              style={s.shareGrad}
            >
              <Ionicons name="share-outline" size={18} color={CYAN} />
              <Text style={s.shareText}>Share Receipt</Text>
            </LinearGradient>
          </Pressable>
        </FadeInView>
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  header:      { flexDirection: "row", alignItems: "center", paddingHorizontal: 16, paddingBottom: 12, backgroundColor: CANVAS },
  backBtn:     { width: 36, height: 36, alignItems: "center", justifyContent: "center" },
  headerTitle: { flex: 1, textAlign: "center", fontSize: 16, fontWeight: "600", color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold" },
  shareBtn:    { width: 36, height: 36, alignItems: "center", justifyContent: "center" },

  hero:        { paddingHorizontal: 20, paddingVertical: 24 },
  statusBadge: { flexDirection: "row", alignItems: "center", gap: 6, marginBottom: 10 },
  statusText:  { fontSize: 14, fontWeight: "600", fontFamily: "SpaceGrotesk-SemiBold" },
  awb:         { fontSize: 22, fontWeight: "700", color: "#FFF", fontFamily: "JetBrainsMono-Regular", letterSpacing: 1 },
  bookedAt:    { fontSize: 12, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", marginTop: 6 },

  card:        { marginHorizontal: 16, marginBottom: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 20, gap: 10 },
  cardTitle:   { fontSize: 11, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1 },

  divider:     { height: 1, backgroundColor: BORDER, marginVertical: 4 },

  row:         { flexDirection: "row", alignItems: "flex-start", gap: 8 },
  rowLabel:    { flex: 1, fontSize: 13, color: "rgba(255,255,255,0.45)", fontFamily: "JetBrainsMono-Regular" },
  rowValue:    { fontSize: 13, color: "#FFF", fontFamily: "SpaceGrotesk-Regular", flexShrink: 1, textAlign: "right", maxWidth: "55%" },

  totalRow:    { marginTop: 4 },
  totalLabel:  { flex: 1, fontSize: 15, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  totalValue:  { fontSize: 18, fontWeight: "700", fontFamily: "SpaceGrotesk-Bold" },

  shareCard:   { marginHorizontal: 16, marginTop: 8 },
  shareButton: { borderRadius: 14, overflow: "hidden", borderWidth: 1, borderColor: CYAN + "30" },
  shareGrad:   { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 10, paddingVertical: 14 },
  shareText:   { fontSize: 14, fontWeight: "600", color: CYAN, fontFamily: "SpaceGrotesk-SemiBold" },
});
