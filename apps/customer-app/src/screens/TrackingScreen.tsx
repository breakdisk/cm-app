/**
 * Customer App — Shipment Tracking Screen
 * Shows live tracking status, timeline, and driver location for a shipment.
 * Uses dark glassmorphism design with neon status colors.
 */
import React, { useState } from "react";
import {
  View, Text, StyleSheet, ScrollView, TextInput, Pressable,
  ActivityIndicator, Linking,
} from "react-native";
import Animated, { FadeInDown, FadeInUp } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";

// ── Design tokens ─────────────────────────────────────────────────────────────
const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const AMBER   = "#FFAB00";
const RED     = "#FF3B5C";
const PURPLE  = "#A855F7";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

// ── Types ─────────────────────────────────────────────────────────────────────

interface StatusEvent {
  status:      string;
  description: string;
  location?:   string;
  occurred_at: string;
}

interface TrackingData {
  tracking_number:  string;
  status:           string;
  origin_city:      string;
  destination_city: string;
  eta?:             string;
  timeline:         StatusEvent[];
  driver_name?:     string;
}

// ── Mock fetch ────────────────────────────────────────────────────────────────

async function fetchTracking(tn: string): Promise<TrackingData | null> {
  if (!tn.startsWith("LS-")) return null;
  await new Promise((r) => setTimeout(r, 900));
  return {
    tracking_number:  tn,
    status:           "out_for_delivery",
    origin_city:      "Quezon City",
    destination_city: "Makati City",
    eta:              "Today, 2:30 – 4:00 PM",
    driver_name:      "Juan Dela Cruz",
    timeline: [
      { status: "pending",          description: "Order placed",                  occurred_at: "2026-03-17T07:00:00Z" },
      { status: "picked_up",        description: "Package collected",             occurred_at: "2026-03-17T09:15:00Z", location: "Quezon City" },
      { status: "at_hub",           description: "Arrived at sorting center",     occurred_at: "2026-03-17T10:30:00Z", location: "Caloocan Hub" },
      { status: "out_for_delivery", description: "Out for delivery",              occurred_at: "2026-03-17T11:45:00Z", location: "Makati City" },
    ],
  };
}

// ── Status config ─────────────────────────────────────────────────────────────

const STATUS_CONFIG: Record<string, { label: string; color: string; icon: keyof typeof Ionicons.glyphMap }> = {
  pending:          { label: "Processing",      color: AMBER,  icon: "time-outline" },
  picked_up:        { label: "Picked Up",       color: CYAN,   icon: "cube-outline" },
  in_transit:       { label: "In Transit",      color: PURPLE, icon: "car-outline" },
  at_hub:           { label: "At Sorting Hub",  color: PURPLE, icon: "business-outline" },
  out_for_delivery: { label: "Out for Delivery", color: GREEN,  icon: "bicycle-outline" },
  delivered:        { label: "Delivered",       color: GREEN,  icon: "checkmark-circle-outline" },
  failed:           { label: "Delivery Failed",  color: RED,    icon: "close-circle-outline" },
};

function getStatus(key: string) {
  return STATUS_CONFIG[key] ?? { label: key, color: CYAN, icon: "cube-outline" as const };
}

// ── Timeline component ────────────────────────────────────────────────────────

function TimelineItem({ event, isFirst }: { event: StatusEvent; isFirst: boolean }) {
  const cfg  = getStatus(event.status);
  const date = new Date(event.occurred_at);

  return (
    <View style={styles.timelineRow}>
      {/* Line + dot column */}
      <View style={styles.timelineLeft}>
        <View style={[styles.timelineDot, { backgroundColor: cfg.color, shadowColor: cfg.color }]}>
          <Ionicons name={cfg.icon} size={10} color={CANVAS} />
        </View>
        {!isFirst && <View style={styles.timelineLine} />}
      </View>
      {/* Content */}
      <View style={styles.timelineContent}>
        <Text style={styles.timelineDesc}>{event.description}</Text>
        {event.location && (
          <Text style={styles.timelineLocation}>
            📍 {event.location}
          </Text>
        )}
        <Text style={styles.timelineTime}>
          {date.toLocaleDateString("en-PH", { month: "short", day: "numeric" })} ·{" "}
          {date.toLocaleTimeString("en-PH", { hour: "2-digit", minute: "2-digit" })}
        </Text>
      </View>
    </View>
  );
}

// ── Main screen ───────────────────────────────────────────────────────────────

export default function TrackingScreen() {
  const [query,    setQuery]    = useState("");
  const [loading,  setLoading]  = useState(false);
  const [result,   setResult]   = useState<TrackingData | null>(null);
  const [notFound, setNotFound] = useState(false);

  async function handleSearch() {
    const tn = query.trim().toUpperCase();
    if (!tn) return;
    setLoading(true);
    setResult(null);
    setNotFound(false);
    try {
      const data = await fetchTracking(tn);
      if (data) setResult(data);
      else      setNotFound(true);
    } finally {
      setLoading(false);
    }
  }

  const cfg = result ? getStatus(result.status) : null;

  return (
    <ScrollView style={styles.container} contentContainerStyle={{ paddingBottom: 40 }}>
      {/* Hero gradient header */}
      <LinearGradient
        colors={["rgba(0,229,255,0.12)", "transparent"]}
        style={styles.hero}
      >
        <Animated.View entering={FadeInDown.springify()}>
          <Text style={styles.heroTitle}>Track Package</Text>
          <Text style={styles.heroSub}>Enter your tracking number below</Text>
        </Animated.View>
      </LinearGradient>

      {/* Search input */}
      <View style={styles.searchRow}>
        <View style={[styles.searchInput, result && styles.searchInputActive]}>
          <Ionicons name="search-outline" size={16} color="rgba(255,255,255,0.3)" />
          <TextInput
            value={query}
            onChangeText={(t) => setQuery(t.toUpperCase())}
            placeholder="LS-A1B2C3D4"
            placeholderTextColor="rgba(255,255,255,0.2)"
            style={styles.searchText}
            autoCapitalize="characters"
            returnKeyType="search"
            onSubmitEditing={handleSearch}
          />
        </View>
        <Pressable
          onPress={handleSearch}
          disabled={loading || !query.trim()}
          style={({ pressed }) => [styles.searchBtn, { opacity: pressed || !query.trim() ? 0.6 : 1 }]}
        >
          {loading
            ? <ActivityIndicator color={CANVAS} size="small" />
            : <Text style={styles.searchBtnText}>Track</Text>
          }
        </Pressable>
      </View>

      {/* Not found */}
      {notFound && (
        <Animated.View entering={FadeInUp.springify()} style={styles.notFound}>
          <Ionicons name="search-outline" size={28} color="rgba(255,59,92,0.5)" />
          <Text style={styles.notFoundTitle}>Tracking number not found</Text>
          <Text style={styles.notFoundSub}>Check the number and try again</Text>
        </Animated.View>
      )}

      {/* Result */}
      {result && cfg && (
        <Animated.View entering={FadeInUp.springify()} style={styles.resultCard}>
          {/* Status header */}
          <LinearGradient
            colors={[`${cfg.color}1A`, "transparent"]}
            style={[styles.statusHeader, { borderTopColor: `${cfg.color}80` }]}
          >
            <View style={[styles.statusIconWrap, { backgroundColor: `${cfg.color}20` }]}>
              <Ionicons name={cfg.icon} size={22} color={cfg.color} />
            </View>
            <View style={{ flex: 1 }}>
              <Text style={styles.statusLabel}>Current Status</Text>
              <Text style={[styles.statusText, { color: cfg.color }]}>{cfg.label}</Text>
            </View>
            {result.eta && (
              <View style={styles.etaWrap}>
                <Text style={styles.etaLabel}>ETA</Text>
                <Text style={styles.etaValue}>{result.eta}</Text>
              </View>
            )}
          </LinearGradient>

          {/* Tracking number + route */}
          <View style={styles.routeRow}>
            <View>
              <Text style={styles.routeLabel}>Tracking No.</Text>
              <Text style={styles.trackingNo}>{result.tracking_number}</Text>
            </View>
            <View style={styles.routeArrow}>
              <Text style={styles.routeCity}>{result.origin_city}</Text>
              <View style={styles.routeLine} />
              <Text style={styles.routeCity}>{result.destination_city}</Text>
            </View>
          </View>

          {result.driver_name && (
            <View style={styles.courierRow}>
              <Ionicons name="bicycle-outline" size={14} color="rgba(255,255,255,0.3)" />
              <Text style={styles.courierText}>Courier: {result.driver_name}</Text>
            </View>
          )}

          {/* Timeline */}
          <View style={styles.timeline}>
            <Text style={styles.timelineTitle}>Shipment Timeline</Text>
            {[...result.timeline].reverse().map((event, i) => (
              <TimelineItem key={i} event={event} isFirst={i === result.timeline.length - 1} />
            ))}
          </View>

          {/* CTAs */}
          <View style={styles.ctaRow}>
            <Pressable style={styles.ctaBtn}>
              <Text style={styles.ctaBtnText}>Reschedule</Text>
            </Pressable>
            <Pressable style={styles.ctaBtn}>
              <Text style={styles.ctaBtnText}>Get Help</Text>
            </Pressable>
          </View>
        </Animated.View>
      )}
    </ScrollView>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container:         { flex: 1, backgroundColor: CANVAS },
  hero:              { paddingHorizontal: 20, paddingTop: 28, paddingBottom: 20 },
  heroTitle:         { fontSize: 26, fontWeight: "700", color: "#FFFFFF", fontFamily: "SpaceGrotesk-Bold", marginBottom: 4 },
  heroSub:           { fontSize: 13, color: "rgba(255,255,255,0.4)" },
  searchRow:         { flexDirection: "row", gap: 10, paddingHorizontal: 16, marginBottom: 16 },
  searchInput:       { flex: 1, flexDirection: "row", alignItems: "center", gap: 8, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, paddingHorizontal: 14, paddingVertical: 12 },
  searchInputActive: { borderColor: "rgba(0,229,255,0.4)" },
  searchText:        { flex: 1, fontSize: 14, fontFamily: "JetBrainsMono-Regular", color: "#FFFFFF", letterSpacing: 0.5 },
  searchBtn:         { borderRadius: 12, paddingHorizontal: 20, paddingVertical: 12, alignItems: "center", justifyContent: "center", minWidth: 64, background: "linear-gradient(135deg, #00E5FF, #A855F7)" },
  searchBtnText:     { fontSize: 14, fontWeight: "600", color: CANVAS },
  notFound:          { alignItems: "center", padding: 32, gap: 8 },
  notFoundTitle:     { fontSize: 16, fontWeight: "600", color: "#FFFFFF", fontFamily: "SpaceGrotesk-SemiBold" },
  notFoundSub:       { fontSize: 13, color: "rgba(255,255,255,0.4)" },
  resultCard:        { marginHorizontal: 16, borderRadius: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, overflow: "hidden" },
  statusHeader:      { flexDirection: "row", alignItems: "center", gap: 12, padding: 16, borderTopWidth: 2 },
  statusIconWrap:    { width: 44, height: 44, borderRadius: 12, alignItems: "center", justifyContent: "center" },
  statusLabel:       { fontSize: 10, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, marginBottom: 2 },
  statusText:        { fontSize: 18, fontWeight: "700", fontFamily: "SpaceGrotesk-Bold" },
  etaWrap:           { alignItems: "flex-end" },
  etaLabel:          { fontSize: 9, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase" },
  etaValue:          { fontSize: 12, color: "#FFFFFF", fontWeight: "600" },
  routeRow:          { flexDirection: "row", alignItems: "center", justifyContent: "space-between", paddingHorizontal: 16, paddingVertical: 12, borderTopWidth: 1, borderTopColor: BORDER },
  routeLabel:        { fontSize: 9, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", marginBottom: 2 },
  trackingNo:        { fontSize: 15, fontFamily: "JetBrainsMono-Bold", color: CYAN, letterSpacing: 1 },
  routeArrow:        { flexDirection: "row", alignItems: "center", gap: 6 },
  routeCity:         { fontSize: 11, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular" },
  routeLine:         { width: 24, height: 1, backgroundColor: "rgba(0,229,255,0.3)" },
  courierRow:        { flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: 16, paddingBottom: 12 },
  courierText:       { fontSize: 12, color: "rgba(255,255,255,0.4)" },
  timeline:          { borderTopWidth: 1, borderTopColor: BORDER, padding: 16 },
  timelineTitle:     { fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1.5, color: "rgba(255,255,255,0.25)", marginBottom: 16 },
  timelineRow:       { flexDirection: "row", gap: 12, marginBottom: 4 },
  timelineLeft:      { alignItems: "center", width: 24 },
  timelineDot:       { width: 24, height: 24, borderRadius: 12, alignItems: "center", justifyContent: "center", shadowOffset: { width: 0, height: 0 }, shadowOpacity: 0.6, shadowRadius: 6 },
  timelineLine:      { flex: 1, width: 1, backgroundColor: BORDER, minHeight: 16, marginVertical: 2 },
  timelineContent:   { flex: 1, paddingBottom: 16 },
  timelineDesc:      { fontSize: 13, color: "#FFFFFF", fontWeight: "500", marginBottom: 2 },
  timelineLocation:  { fontSize: 11, color: "rgba(255,255,255,0.4)", marginBottom: 2 },
  timelineTime:      { fontSize: 10, color: "rgba(255,255,255,0.2)", fontFamily: "JetBrainsMono-Regular" },
  ctaRow:            { flexDirection: "row", gap: 10, padding: 16, borderTopWidth: 1, borderTopColor: BORDER },
  ctaBtn:            { flex: 1, alignItems: "center", paddingVertical: 12, borderRadius: 10, borderWidth: 1, borderColor: BORDER, backgroundColor: GLASS },
  ctaBtnText:        { fontSize: 13, color: "rgba(255,255,255,0.6)" },
});
