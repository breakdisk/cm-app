/**
 * Customer App — Shipment History Screen
 * Lists all booked shipments from Redux, grouped by active / delivered / other.
 */
import React, { useState } from "react";
import {
  View, Text, StyleSheet, ScrollView, Pressable,
} from "react-native";
import Animated, { FadeInDown, FadeInUp } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useSelector } from "react-redux";
import type { RootState } from "../../store";
import type { ShipmentRecord, ShipmentStatus } from "../../store";
import { AwbQRCode } from "../../components/AwbQRCode";
import { useShipments } from "../../hooks/useShipments";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

const STATUS_CONFIG: Record<ShipmentStatus, { label: string; color: string; icon: string }> = {
  pending:            { label: "Processing",         color: AMBER,  icon: "time-outline"             },
  confirmed:          { label: "Confirmed",          color: CYAN,   icon: "checkmark-circle-outline" },
  picked_up:          { label: "Picked Up",          color: CYAN,   icon: "archive-outline"          },
  in_transit:         { label: "In Transit",         color: PURPLE, icon: "car-outline"              },
  out_for_delivery:   { label: "Out for Delivery",   color: GREEN,  icon: "bicycle-outline"          },
  delivery_attempted: { label: "Attempt Failed",     color: AMBER,  icon: "alert-circle-outline"     },
  delivered:          { label: "Delivered",          color: GREEN,  icon: "checkmark-done-outline"   },
  returned:           { label: "Returned",           color: RED,    icon: "return-down-back-outline" },
  cancelled:          { label: "Cancelled",          color: RED,    icon: "close-circle-outline"     },
};

const ACTIVE_STATUSES: ShipmentStatus[]   = ["pending", "confirmed", "picked_up", "in_transit", "out_for_delivery", "delivery_attempted"];
const DONE_STATUSES:   ShipmentStatus[]   = ["delivered", "returned", "cancelled"];

type FilterTab = "all" | "active" | "done";

// Demo shipments shown when the Redux list is empty
const DEMO_SHIPMENTS: ShipmentRecord[] = [
  {
    awb: "LS-A1B2C3D4", type: "local", status: "out_for_delivery",
    origin: "123 Kalayaan Ave, Pasig City", destination: "45 Timog Ave, Quezon City",
    description: "Electronics", weight: "1.2", isCOD: false,
    bookedAt: "Mar 17, 2026, 08:00 AM", estimatedDelivery: "Mar 17, 2026", totalFee: 95,
  },
  {
    awb: "LS-E5F6G7H8", type: "international", status: "in_transit",
    origin: "88 Ayala Ave, Makati City", destination: "221B Baker St, London",
    destCountry: "GB", description: "Balikbayan Box — Clothes & food", weight: "22",
    isCOD: false, freightMode: "sea",
    bookedAt: "Mar 17, 2026, 07:00 AM", estimatedDelivery: "30–45 days", totalFee: 720,
  },
  {
    awb: "LS-I9J0K1L2", type: "local", status: "delivered",
    origin: "9 Dela Rosa St, BGC", destination: "12 Dapitan St, Sampaloc, Manila",
    description: "Personal item", weight: "0.8", isCOD: true, codAmount: "1500",
    bookedAt: "Mar 15, 2026, 09:30 AM", estimatedDelivery: "Mar 15, 2026", totalFee: 85,
  },
];

// Statuses where driver pickup QR is relevant
const PICKUP_STATUSES: ShipmentStatus[] = ["pending", "confirmed"];

function ShipmentCard({ item, onShowQR }: { item: ShipmentRecord; onShowQR: (awb: string) => void }) {
  const cfg       = STATUS_CONFIG[item.status];
  const isIntl    = item.type === "international";
  const showQRBtn = PICKUP_STATUSES.includes(item.status);
  const accent    = isIntl ? PURPLE : CYAN;

  return (
    <Animated.View entering={FadeInUp.springify()} style={s.card}>
      {/* Header */}
      <View style={s.cardHeader}>
        <View style={{ flex: 1 }}>
          <Text style={s.awb}>{item.awb}</Text>
          <View style={s.typeBadgeRow}>
            <View style={[s.typeBadge, isIntl ? s.typeBadgeIntl : s.typeBadgeLocal]}>
              <Ionicons name={isIntl ? "globe-outline" : "home-outline"} size={10} color={isIntl ? PURPLE : GREEN} />
              <Text style={[s.typeBadgeText, { color: isIntl ? PURPLE : GREEN }]}>
                {isIntl ? `Balikbayan · ${item.freightMode === "sea" ? "Sea" : "Air"}` : "Local"}
              </Text>
            </View>
          </View>
        </View>
        <View style={[s.statusChip, { backgroundColor: cfg.color + "18", borderColor: cfg.color + "40" }]}>
          <Ionicons name={cfg.icon as any} size={12} color={cfg.color} />
          <Text style={[s.statusText, { color: cfg.color }]}>{cfg.label}</Text>
        </View>
      </View>

      {/* Route */}
      <View style={s.routeRow}>
        <View style={s.routeCity}>
          <Ionicons name="navigate-circle-outline" size={12} color="rgba(255,255,255,0.25)" />
          <Text style={s.routeText} numberOfLines={1}>{item.origin}</Text>
        </View>
        <Ionicons name="chevron-forward" size={12} color="rgba(255,255,255,0.2)" />
        <View style={s.routeCity}>
          <Ionicons name="location-outline" size={12} color="rgba(255,255,255,0.25)" />
          <Text style={s.routeText} numberOfLines={1}>
            {item.destination}{isIntl ? ` · ${item.destCountry}` : ""}
          </Text>
        </View>
      </View>

      {/* Meta */}
      <View style={s.metaRow}>
        <Text style={s.metaText}>Booked {item.bookedAt}</Text>
        {item.isCOD && (
          <View style={s.codTag}>
            <Text style={s.codTagText}>COD ₱{item.codAmount}</Text>
          </View>
        )}
        <Text style={[s.metaFee, { color: isIntl ? PURPLE : CYAN }]}>₱{item.totalFee.toFixed(2)}</Text>
      </View>

      {item.estimatedDelivery && item.status !== "delivered" && item.status !== "cancelled" && (
        <View style={s.etaRow}>
          <Ionicons name="time-outline" size={11} color={AMBER} />
          <Text style={s.etaText}>ETA: {item.estimatedDelivery}</Text>
        </View>
      )}

      {/* Show QR button — only for pending/confirmed (awaiting pickup) */}
      {showQRBtn && (
        <Pressable
          onPress={() => onShowQR(item.awb)}
          style={[s.qrBtn, { borderColor: accent + "40", backgroundColor: accent + "0C" }]}
        >
          <Ionicons name="qr-code-outline" size={14} color={accent} />
          <Text style={[s.qrBtnText, { color: accent }]}>Show Pickup QR Code</Text>
          <Ionicons name="chevron-forward" size={12} color={accent + "80"} />
        </Pressable>
      )}
    </Animated.View>
  );
}

export function HistoryScreen() {
  const { list: hookShipments, loading } = useShipments();
  const reduxShipments = useSelector((s: RootState) => s.shipments.list);

  // Use hook shipments if available, otherwise fall back to Redux, otherwise use demo data
  const shipments = hookShipments.length > 0
    ? hookShipments as ShipmentRecord[]
    : (reduxShipments.length > 0 ? reduxShipments : DEMO_SHIPMENTS);

  const [filter,  setFilter]  = useState<FilterTab>("all");
  const [qrAwb,   setQrAwb]   = useState<string | null>(null);

  const filtered = shipments.filter((s) => {
    if (filter === "active") return ACTIVE_STATUSES.includes(s.status);
    if (filter === "done")   return DONE_STATUSES.includes(s.status);
    return true;
  });

  const activeCount = shipments.filter(s => ACTIVE_STATUSES.includes(s.status)).length;
  const doneCount   = shipments.filter(s => DONE_STATUSES.includes(s.status)).length;

  const qrShipment = qrAwb ? shipments.find(s => s.awb === qrAwb) : null;

  // Show loading state if hook is loading
  if (loading && hookShipments.length === 0) {
    return (
      <View style={{ flex: 1, backgroundColor: CANVAS, justifyContent: "center", alignItems: "center" }}>
        <Ionicons name="cube-outline" size={44} color="rgba(255,255,255,0.1)" />
        <Text style={{ color: "rgba(255,255,255,0.5)", marginTop: 16, fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold" }}>Loading shipments...</Text>
      </View>
    );
  }

  return (
    <View style={{ flex: 1, backgroundColor: CANVAS }}>
    <ScrollView style={s.container} contentContainerStyle={{ paddingBottom: 40 }}>
      {/* Hero */}
      <LinearGradient colors={["rgba(168,85,247,0.10)", "transparent"]} style={s.hero}>
        <Animated.View entering={FadeInDown.springify()}>
          <Text style={s.heroTitle}>My Shipments</Text>
          <Text style={s.heroSub}>{shipments.length} total · {activeCount} active</Text>
        </Animated.View>
      </LinearGradient>

      {/* Filter tabs */}
      <Animated.View entering={FadeInDown.delay(60).springify()} style={s.filterRow}>
        {([
          { key: "all",    label: `All (${shipments.length})`  },
          { key: "active", label: `Active (${activeCount})`    },
          { key: "done",   label: `Done (${doneCount})`        },
        ] as const).map((tab) => (
          <Pressable
            key={tab.key}
            onPress={() => setFilter(tab.key)}
            style={[s.filterTab, filter === tab.key && s.filterTabActive]}
          >
            <Text style={[s.filterTabText, filter === tab.key && { color: CYAN }]}>{tab.label}</Text>
          </Pressable>
        ))}
      </Animated.View>

      {/* List */}
      {filtered.length === 0 ? (
        <Animated.View entering={FadeInUp.springify()} style={s.emptyCard}>
          <Ionicons name="cube-outline" size={36} color="rgba(255,255,255,0.1)" />
          <Text style={s.emptyText}>No shipments yet</Text>
          <Text style={s.emptySub}>Book your first shipment from the Book tab</Text>
        </Animated.View>
      ) : (
        <View style={s.list}>
          {filtered.map((item) => (
            <ShipmentCard key={item.awb} item={item} onShowQR={setQrAwb} />
          ))}
        </View>
      )}
    </ScrollView>

    {/* QR overlay */}
    {qrAwb && (
      <AwbQRCode
        awb={qrAwb}
        accent={qrShipment?.type === "international" ? PURPLE : CYAN}
        fullscreen
        onClose={() => setQrAwb(null)}
      />
    )}
    </View>
  );
}

const s = StyleSheet.create({
  container:      { flex: 1, backgroundColor: CANVAS },
  hero:           { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 20 },
  heroTitle:      { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:        { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },

  filterRow:      { flexDirection: "row", gap: 8, paddingHorizontal: 16, marginBottom: 16 },
  filterTab:      { paddingHorizontal: 14, paddingVertical: 7, borderRadius: 20, borderWidth: 1, borderColor: BORDER, backgroundColor: GLASS },
  filterTabActive:{ borderColor: CYAN + "50", backgroundColor: CYAN + "0F" },
  filterTabText:  { fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.4)" },

  list:           { paddingHorizontal: 16, gap: 10 },

  card:           { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 16, gap: 10 },
  cardHeader:     { flexDirection: "row", alignItems: "flex-start", gap: 10 },
  awb:            { fontSize: 15, fontFamily: "JetBrainsMono-Regular", color: "#FFF", fontWeight: "700", marginBottom: 4 },
  typeBadgeRow:   { flexDirection: "row" },
  typeBadge:      { flexDirection: "row", alignItems: "center", gap: 4, paddingHorizontal: 7, paddingVertical: 3, borderRadius: 6, borderWidth: 1 },
  typeBadgeLocal: { backgroundColor: "rgba(0,255,136,0.08)", borderColor: "rgba(0,255,136,0.2)" },
  typeBadgeIntl:  { backgroundColor: "rgba(168,85,247,0.08)", borderColor: "rgba(168,85,247,0.2)" },
  typeBadgeText:  { fontSize: 10, fontFamily: "JetBrainsMono-Regular" },
  statusChip:     { flexDirection: "row", alignItems: "center", gap: 4, paddingHorizontal: 9, paddingVertical: 5, borderRadius: 16, borderWidth: 1 },
  statusText:     { fontSize: 10, fontWeight: "600" },

  routeRow:       { flexDirection: "row", alignItems: "center", gap: 6 },
  routeCity:      { flex: 1, flexDirection: "row", alignItems: "center", gap: 5 },
  routeText:      { flex: 1, fontSize: 11, color: "rgba(255,255,255,0.45)", fontFamily: "JetBrainsMono-Regular" },

  metaRow:        { flexDirection: "row", alignItems: "center", gap: 8 },
  metaText:       { flex: 1, fontSize: 10, color: "rgba(255,255,255,0.25)", fontFamily: "JetBrainsMono-Regular" },
  codTag:         { paddingHorizontal: 7, paddingVertical: 3, borderRadius: 6, backgroundColor: "rgba(255,171,0,0.12)", borderWidth: 1, borderColor: "rgba(255,171,0,0.25)" },
  codTagText:     { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: AMBER },
  metaFee:        { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold" },

  etaRow:         { flexDirection: "row", alignItems: "center", gap: 5 },
  etaText:        { fontSize: 10, color: AMBER, fontFamily: "JetBrainsMono-Regular" },

  qrBtn:          { flexDirection: "row", alignItems: "center", gap: 8, borderWidth: 1, borderRadius: 10, paddingHorizontal: 12, paddingVertical: 9, marginTop: 4 },
  qrBtnText:      { flex: 1, fontSize: 12, fontFamily: "SpaceGrotesk-SemiBold" },

  emptyCard:      { marginHorizontal: 16, alignItems: "center", gap: 8, paddingVertical: 48 },
  emptyText:      { fontSize: 16, fontWeight: "600", color: "rgba(255,255,255,0.3)", fontFamily: "SpaceGrotesk-SemiBold" },
  emptySub:       { fontSize: 12, color: "rgba(255,255,255,0.2)", fontFamily: "JetBrainsMono-Regular", textAlign: "center" },
});
