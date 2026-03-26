/**
 * Customer App — Home Screen
 * Loyalty points, recent shipments, quick track, promotional banners.
 */
import React from "react";
import {
  View, Text, StyleSheet, ScrollView, Pressable,
  TouchableOpacity,
} from "react-native";
import Animated, { FadeInDown, FadeInUp } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useSelector } from "react-redux";
import { useNavigation } from "@react-navigation/native";
import type { RootState } from "../../store";

// ── Design tokens ─────────────────────────────────────────────────────────────
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

// ── Mock recent shipments ─────────────────────────────────────────────────────

const RECENT = [
  { tracking: "LS-A1B2C3D4", desc: "Order from Lazada",    status: "out_for_delivery", color: GREEN  },
  { tracking: "LS-E5F6G7H8", desc: "Shopee package",       status: "in_transit",       color: PURPLE },
  { tracking: "LS-I9J0K1L2", desc: "Personal item",        status: "delivered",        color: GREEN  },
];

const STATUS_LABEL: Record<string, string> = {
  out_for_delivery: "Out for Delivery",
  in_transit:       "In Transit",
  delivered:        "Delivered",
  pending:          "Processing",
};

const PROMOS = [
  { title: "Book 5, get ₱50 off",     sub: "Valid until March 31",      color: [CYAN, PURPLE]  as [string, string] },
  { title: "COD now in Mindanao",      sub: "Expanded coverage",         color: [PURPLE, RED]   as [string, string] },
];

export function HomeScreen() {
  const name       = useSelector((s: RootState) => s.auth.name);
  const loyaltyPts = useSelector((s: RootState) => s.auth.loyaltyPts);
  const navigation = useNavigation<any>();

  return (
    <ScrollView style={s.container} contentContainerStyle={{ paddingBottom: 40 }}>
      {/* Hero */}
      <LinearGradient colors={["rgba(0,229,255,0.12)", "transparent"]} style={s.hero}>
        <Animated.View entering={FadeInDown.springify()}>
          <Text style={s.greeting}>Hello, {name ?? "there"} 👋</Text>
          <Text style={s.heroSub}>What would you like to do today?</Text>
        </Animated.View>
      </LinearGradient>

      {/* Loyalty card */}
      {!!(name) && (
        <Animated.View entering={FadeInUp.delay(100).springify()} style={s.loyaltyCard}>
          <LinearGradient colors={[PURPLE + "22", CYAN + "11"]} style={s.loyaltyGradient}>
            <View style={s.loyaltyRow}>
              <View>
                <Text style={s.loyaltyLabel}>Loyalty Points</Text>
                <Text style={s.loyaltyPts}>{loyaltyPts.toLocaleString()} pts</Text>
              </View>
              <View style={[s.loyaltyBadge, { backgroundColor: PURPLE + "30" }]}>
                <Ionicons name="star" size={14} color={PURPLE} />
                <Text style={[s.loyaltyTier, { color: PURPLE }]}>Gold</Text>
              </View>
            </View>
            <View style={s.progressBar}>
              <View style={[s.progressFill, { width: "62%", backgroundColor: PURPLE }]} />
            </View>
            <Text style={s.progressLabel}>380 pts to Platinum</Text>
          </LinearGradient>
        </Animated.View>
      )}

      {/* Quick actions */}
      <Animated.View entering={FadeInUp.delay(150).springify()} style={s.section}>
        <Text style={s.sectionTitle}>Quick Actions</Text>
        <View style={s.quickGrid}>
          {[
            { icon: "cube-outline",          label: "Track",   color: CYAN,   onPress: () => navigation.navigate("Track")   },
            { icon: "add-circle-outline",    label: "Book",    color: GREEN,  onPress: () => navigation.navigate("Book")    },
            { icon: "document-text-outline", label: "History", color: PURPLE, onPress: () => navigation.navigate("History") },
            { icon: "chatbubble-outline",    label: "Support", color: AMBER,  onPress: () => navigation.navigate("Support") },
          ].map((q) => (
            <Pressable key={q.label} onPress={q.onPress} style={({ pressed }) => [s.quickBtn, { opacity: pressed ? 0.7 : 1 }]}>
              <View style={[s.quickIcon, { backgroundColor: q.color + "20" }]}>
                <Ionicons name={q.icon as any} size={22} color={q.color} />
              </View>
              <Text style={s.quickLabel}>{q.label}</Text>
            </Pressable>
          ))}
        </View>
      </Animated.View>

      {/* Recent shipments */}
      {RECENT.length > 0 && (
        <Animated.View entering={FadeInUp.delay(200).springify()} style={s.section}>
          <View style={s.sectionHeader}>
            <Text style={s.sectionTitle}>Recent Shipments</Text>
            <TouchableOpacity onPress={() => navigation.navigate("History")}>
              <Text style={[s.sectionAction, { color: CYAN }]}>See all</Text>
            </TouchableOpacity>
          </View>
          {RECENT.map((item) => (
            <Pressable key={item.tracking} onPress={() => navigation.navigate("Track")} style={({ pressed }) => [s.shipmentRow, { opacity: pressed ? 0.8 : 1 }]}>
              <View style={[s.shipmentDot, { backgroundColor: item.color }]} />
              <View style={{ flex: 1 }}>
                <Text style={s.shipmentDesc}>{item.desc}</Text>
                <Text style={s.shipmentTracking}>{item.tracking}</Text>
              </View>
              <Text style={[s.shipmentStatus, { color: item.color }]}>
                {STATUS_LABEL[item.status] ?? item.status}
              </Text>
            </Pressable>
          ))}
        </Animated.View>
      )}

      {/* Promo banners */}
      <Animated.View entering={FadeInUp.delay(250).springify()} style={s.section}>
        <Text style={s.sectionTitle}>Offers</Text>
        {PROMOS.map((p) => (
          <LinearGradient key={p.title} colors={p.color.map(c => c + "33") as [string, string]} style={s.promoBanner}>
            <Text style={s.promoTitle}>{p.title}</Text>
            <Text style={s.promoSub}>{p.sub}</Text>
          </LinearGradient>
        ))}
      </Animated.View>
    </ScrollView>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────
const s = StyleSheet.create({
  container:      { flex: 1, backgroundColor: CANVAS },
  hero:           { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 20 },
  greeting:       { fontSize: 24, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:        { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },
  loyaltyCard:    { marginHorizontal: 16, marginBottom: 16, borderRadius: 16, overflow: "hidden", borderWidth: 1, borderColor: BORDER },
  loyaltyGradient:{ padding: 16 },
  loyaltyRow:     { flexDirection: "row", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 12 },
  loyaltyLabel:   { fontSize: 10, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1 },
  loyaltyPts:     { fontSize: 28, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold", marginTop: 2 },
  loyaltyBadge:   { flexDirection: "row", alignItems: "center", gap: 4, paddingHorizontal: 10, paddingVertical: 5, borderRadius: 20 },
  loyaltyTier:    { fontSize: 11, fontWeight: "600" },
  progressBar:    { height: 4, borderRadius: 2, backgroundColor: "rgba(255,255,255,0.08)", marginBottom: 6 },
  progressFill:   { height: "100%", borderRadius: 2 },
  progressLabel:  { fontSize: 10, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular" },
  section:        { paddingHorizontal: 16, marginBottom: 16 },
  sectionHeader:  { flexDirection: "row", justifyContent: "space-between", alignItems: "center", marginBottom: 12 },
  sectionTitle:   { fontSize: 14, fontWeight: "600", color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold", marginBottom: 12 },
  sectionAction:  { fontSize: 12, fontFamily: "JetBrainsMono-Regular" },
  quickGrid:      { flexDirection: "row", gap: 10 },
  quickBtn:       { flex: 1, alignItems: "center", gap: 8 },
  quickIcon:      { width: 56, height: 56, borderRadius: 16, alignItems: "center", justifyContent: "center" },
  quickLabel:     { fontSize: 11, color: "rgba(255,255,255,0.6)", fontWeight: "500" },
  shipmentRow:    { flexDirection: "row", alignItems: "center", gap: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, padding: 14, marginBottom: 8 },
  shipmentDot:    { width: 8, height: 8, borderRadius: 4 },
  shipmentDesc:   { fontSize: 13, color: "#FFF", fontWeight: "500", marginBottom: 2 },
  shipmentTracking:{ fontSize: 11, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular" },
  shipmentStatus: { fontSize: 11, fontWeight: "600", textAlign: "right" },
  promoBanner:    { borderRadius: 12, padding: 16, marginBottom: 8, borderWidth: 1, borderColor: BORDER },
  promoTitle:     { fontSize: 14, fontWeight: "600", color: "#FFF", marginBottom: 4 },
  promoSub:       { fontSize: 12, color: "rgba(255,255,255,0.4)" },
});
