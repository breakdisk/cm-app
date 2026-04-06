/**
 * Customer App — Tracking Screen
 * Search by AWB, live status timeline, driver ETA card.
 */
import React, { useState, useEffect } from "react";
import {
  View, Text, StyleSheet, ScrollView, TextInput,
  Pressable, ActivityIndicator,
} from "react-native";
import Animated, { FadeInDown, FadeInUp } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useDispatch, useSelector } from "react-redux";
import { trackingActions } from "../../store";
import type { RootState } from "../../store";
import { useTracking } from "../../hooks/useTracking";
import { useRoute } from "@react-navigation/native";
import { useNetInfo } from "@react-native-community/netinfo";
import { getDatabase } from "../../db/sqlite";
import OfflineIndicator from "../../components/OfflineIndicator";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

type ShipmentStatus =
  | "pending" | "confirmed" | "picked_up"
  | "in_transit" | "out_for_delivery"
  | "delivery_attempted" | "delivered" | "returned" | "cancelled";

interface TimelineEvent {
  status:      ShipmentStatus;
  description: string;
  location?:   string;
  occurred_at: string;
}

interface TrackingResult {
  awb:              string;
  status:           ShipmentStatus;
  origin_city:      string;
  destination_city: string;
  eta?:             string;
  driver_name?:     string;
  driver_phone?:    string;
  timeline:         TimelineEvent[];
}

// Mock data — in production fetched from GET /v1/shipments/:awb
const MOCK_RESULTS: Record<string, TrackingResult> = {
  "LS-A1B2C3D4": {
    awb:              "LS-A1B2C3D4",
    status:           "out_for_delivery",
    origin_city:      "Pasig City",
    destination_city: "Quezon City",
    eta:              "Today, 2:00–4:00 PM",
    driver_name:      "Juan Dela Cruz",
    driver_phone:     "+639171234567",
    timeline: [
      { status: "pending",           description: "Shipment booked by merchant",             occurred_at: "Mar 17, 8:00 AM"  },
      { status: "confirmed",         description: "Order confirmed and assigned to hub",      occurred_at: "Mar 17, 8:15 AM"  },
      { status: "picked_up",         description: "Package collected from merchant",          location: "Pasig City Hub",   occurred_at: "Mar 17, 10:30 AM" },
      { status: "in_transit",        description: "Package in transit to delivery zone",      location: "QC Sorting Hub",   occurred_at: "Mar 17, 12:00 PM" },
      { status: "out_for_delivery",  description: "Package is out for delivery today",        occurred_at: "Mar 17, 1:30 PM"  },
    ],
  },
  "LS-E5F6G7H8": {
    awb:              "LS-E5F6G7H8",
    status:           "in_transit",
    origin_city:      "Makati City",
    destination_city: "Cebu City",
    eta:              "Mar 19, 2026",
    timeline: [
      { status: "pending",           description: "Shipment booked by merchant",            occurred_at: "Mar 17, 7:00 AM"  },
      { status: "confirmed",         description: "Order confirmed",                          occurred_at: "Mar 17, 7:10 AM"  },
      { status: "picked_up",         description: "Package collected from merchant",          location: "Makati Hub",        occurred_at: "Mar 17, 9:45 AM"  },
      { status: "in_transit",        description: "En route to Cebu via inter-island cargo",  location: "Manila Port Area",  occurred_at: "Mar 17, 2:00 PM"  },
    ],
  },
};

const STATUS_CONFIG: Record<ShipmentStatus, { label: string; color: string; icon: string }> = {
  pending:            { label: "Processing",         color: AMBER,  icon: "time-outline"              },
  confirmed:          { label: "Confirmed",          color: CYAN,   icon: "checkmark-circle-outline"  },
  picked_up:          { label: "Picked Up",          color: CYAN,   icon: "archive-outline"           },
  in_transit:         { label: "In Transit",         color: PURPLE, icon: "car-outline"               },
  out_for_delivery:   { label: "Out for Delivery",   color: GREEN,  icon: "bicycle-outline"           },
  delivery_attempted: { label: "Attempt Failed",     color: AMBER,  icon: "alert-circle-outline"      },
  delivered:          { label: "Delivered",          color: GREEN,  icon: "checkmark-done-outline"    },
  returned:           { label: "Returned to Sender", color: RED,    icon: "return-down-back-outline"  },
  cancelled:          { label: "Cancelled",          color: RED,    icon: "close-circle-outline"      },
};

const STATUS_ORDER: ShipmentStatus[] = [
  "pending", "confirmed", "picked_up", "in_transit",
  "out_for_delivery", "delivery_attempted", "delivered",
];

export function TrackingScreen() {
  const dispatch    = useDispatch();
  const route = useRoute();
  const { isConnected } = useNetInfo();
  const recentSearches = useSelector((s: RootState) => s.tracking.history);

  const [query,   setQuery]   = useState("");
  const [result,  setResult]  = useState<TrackingResult | null>(null);
  const [localLoading, setLocalLoading] = useState(false);
  const [localError,   setLocalError]   = useState("");
  const [currentAwb, setCurrentAwb] = useState<string>("");
  const [offlineData, setOfflineData] = useState<any>(null);
  const [lastUpdated, setLastUpdated] = useState<number | null>(null);

  // Use the tracking hook for the current AWB
  const { data: trackingData, loading: hookLoading, error: hookError, refetch } = useTracking(
    currentAwb,
    { autoload: !!currentAwb && !!isConnected }
  );

  // Load offline tracking data when offline
  useEffect(() => {
    const loadOfflineTracking = async () => {
      if (!isConnected && currentAwb) {
        try {
          const db = await getDatabase();
          const tracking = await db.getFirstAsync(
            `SELECT * FROM tracking_history WHERE awb = ?`,
            [currentAwb]
          );
          if (tracking) {
            setOfflineData(JSON.parse(tracking.events));
            setLastUpdated(new Date(tracking.lastUpdated).getTime());
          }
        } catch (err) {
          console.error('Failed to load offline tracking:', err);
        }
      } else {
        setOfflineData(null);
        setLastUpdated(null);
      }
    };

    loadOfflineTracking();
  }, [isConnected, currentAwb]);

  function handleSearch() {
    const awb = query.trim().toUpperCase();
    if (!awb) return;
    setLocalLoading(true);
    setLocalError("");
    setResult(null);
    // Simulate network delay for search
    setTimeout(() => {
      const found = MOCK_RESULTS[awb];
      if (found) {
        setResult(found);
        setCurrentAwb(awb); // Trigger the hook to load real tracking data
        dispatch(trackingActions.addToHistory({
          tracking_number: awb,
          status: found.status,
          searched_at: new Date().toLocaleTimeString("en-PH", { hour: "2-digit", minute: "2-digit" }),
        }));
      } else {
        setLocalError("No shipment found for that tracking number. Check and try again.");
      }
      setLocalLoading(false);
    }, 800);
  }

  const cfg = result ? STATUS_CONFIG[result.status] : null;

  // Use online data if connected, else use offline data
  const displayResult = isConnected ? result : (offlineData ? result : null);
  const displayLoading = isConnected ? (localLoading || hookLoading) : false;
  const displayError = isConnected ? (localError || hookError) : null;

  const handleRefresh = async () => {
    if (isConnected && currentAwb) {
      await refetch();
    }
  };

  return (
    <ScrollView style={s.container} contentContainerStyle={{ paddingBottom: 40 }} keyboardShouldPersistTaps="handled">
      {/* Offline indicator */}
      {!isConnected && <OfflineIndicator isOffline={true} lastUpdated={lastUpdated || undefined} />}

      {/* Hero */}
      <LinearGradient colors={["rgba(0,229,255,0.10)", "transparent"]} style={s.hero}>
        <Text style={s.heroTitle}>Track Shipment</Text>
        <Text style={s.heroSub}>Enter your tracking number (AWB)</Text>
      </LinearGradient>

      {/* Search */}
      <Animated.View entering={FadeInDown.springify()} style={s.searchRow}>
        <View style={s.inputWrap}>
          <Ionicons name="search-outline" size={16} color="rgba(255,255,255,0.3)" />
          <TextInput
            value={query}
            onChangeText={setQuery}
            placeholder="e.g. LS-A1B2C3D4"
            placeholderTextColor="rgba(255,255,255,0.2)"
            style={s.input}
            autoCapitalize="characters"
            returnKeyType="search"
            onSubmitEditing={handleSearch}
          />
          {query.length > 0 && (
            <Pressable onPress={() => { setQuery(""); setResult(null); setLocalError(""); setCurrentAwb(""); }}>
              <Ionicons name="close-circle" size={16} color="rgba(255,255,255,0.3)" />
            </Pressable>
          )}
        </View>
        <Pressable
          onPress={handleSearch}
          disabled={!query.trim() || localLoading}
          style={({ pressed }) => [s.searchBtn, { opacity: pressed || !query.trim() ? 0.6 : 1 }]}
        >
          <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.searchBtnGrad}>
            {localLoading ? (
              <ActivityIndicator size="small" color={CANVAS} />
            ) : (
              <Text style={s.searchBtnText}>Track</Text>
            )}
          </LinearGradient>
        </Pressable>
      </Animated.View>

      {/* Error */}
      {displayError && (
        <Animated.View entering={FadeInDown.springify()} style={s.errorCard}>
          <Ionicons name="alert-circle-outline" size={16} color={RED} />
          <Text style={s.errorText}>{displayError}</Text>
        </Animated.View>
      )}

      {/* Loading hook data */}
      {displayLoading && currentAwb && (
        <Animated.View entering={FadeInDown.springify()} style={s.resultCard}>
          <View style={{ alignItems: "center", paddingVertical: 40 }}>
            <ActivityIndicator size="large" color={CYAN} />
            <Text style={{ color: "rgba(255,255,255,0.5)", marginTop: 16, fontSize: 14 }}>Fetching tracking details...</Text>
          </View>
        </Animated.View>
      )}

      {/* Refresh button (online only) */}
      {isConnected && currentAwb && displayResult && (
        <View style={{ paddingHorizontal: 16, paddingVertical: 12, gap: 8 }}>
          <Pressable
            onPress={handleRefresh}
            disabled={displayLoading}
            style={({ pressed }) => [
              s.refreshBtn,
              { opacity: pressed || displayLoading ? 0.6 : 1 },
            ]}
          >
            <Ionicons name="refresh-outline" size={16} color={CYAN} />
            <Text style={{ color: CYAN, fontSize: 13, fontWeight: "600", marginLeft: 6 }}>
              {displayLoading ? "Refreshing..." : "Refresh"}
            </Text>
          </Pressable>
        </View>
      )}

      {/* Result */}
      {displayResult && cfg && (
        <Animated.View entering={FadeInUp.springify()} style={s.resultCard}>
          {/* AWB header */}
          <View style={s.awbRow}>
            <View>
              <Text style={s.awbLabel}>Tracking Number</Text>
              <Text style={s.awb}>{displayResult.awb}</Text>
            </View>
            <View style={[s.statusChip, { backgroundColor: cfg.color + "20", borderColor: cfg.color + "40" }]}>
              <Ionicons name={cfg.icon as any} size={14} color={cfg.color} />
              <Text style={[s.statusText, { color: cfg.color }]}>{cfg.label}</Text>
            </View>
          </View>

          {/* Route */}
          <View style={s.routeRow}>
            <View style={s.routeCity}>
              <Ionicons name="navigate-outline" size={12} color="rgba(255,255,255,0.3)" />
              <Text style={s.routeCityText}>{displayResult.origin_city}</Text>
            </View>
            <View style={s.routeLine}>
              <View style={[s.routeDot, { backgroundColor: CYAN }]} />
              <View style={s.routeLineBar} />
              <Ionicons name="chevron-forward" size={12} color={cfg.color} />
            </View>
            <View style={s.routeCity}>
              <Ionicons name="location-outline" size={12} color="rgba(255,255,255,0.3)" />
              <Text style={s.routeCityText}>{displayResult.destination_city}</Text>
            </View>
          </View>

          {/* ETA */}
          {displayResult.eta && (
            <View style={s.etaRow}>
              <Ionicons name="time-outline" size={14} color={AMBER} />
              <Text style={s.etaText}>Estimated delivery: </Text>
              <Text style={[s.etaText, { color: AMBER, fontWeight: "600" }]}>{displayResult.eta}</Text>
            </View>
          )}

          {/* Driver card */}
          {displayResult.driver_name && (
            <View style={s.driverCard}>
              <View style={s.driverAvatar}>
                <Ionicons name="person-outline" size={18} color={CYAN} />
              </View>
              <View style={{ flex: 1 }}>
                <Text style={s.driverLabel}>Your Driver</Text>
                <Text style={s.driverName}>{displayResult.driver_name}</Text>
              </View>
              {displayResult.driver_phone && (
                <Pressable style={s.callBtn}>
                  <Ionicons name="call-outline" size={16} color={GREEN} />
                </Pressable>
              )}
            </View>
          )}

          {/* Timeline */}
          <Text style={s.sectionLabel}>Timeline</Text>
          {displayResult.timeline.map((event: any, i: number) => {
            const eCfg = STATUS_CONFIG[event.status];
            const isLast = i === displayResult.timeline.length - 1;
            return (
              <View key={i} style={s.timelineRow}>
                <View style={s.timelineLeft}>
                  <View style={[s.timelineDot, { backgroundColor: isLast ? eCfg.color : "rgba(255,255,255,0.15)" }]}>
                    {isLast && <Ionicons name={eCfg.icon as any} size={10} color={CANVAS} />}
                  </View>
                  {i < displayResult.timeline.length - 1 && <View style={s.timelineLine} />}
                </View>
                <View style={[s.timelineContent, isLast && { opacity: 1 }, !isLast && { opacity: 0.5 }]}>
                  <Text style={[s.timelineStatus, { color: isLast ? eCfg.color : "rgba(255,255,255,0.7)" }]}>{eCfg.label}</Text>
                  <Text style={s.timelineDesc}>{event.description}</Text>
                  {event.location && <Text style={s.timelineLoc}>{event.location}</Text>}
                  <Text style={s.timelineTime}>{event.occurred_at}</Text>
                </View>
              </View>
            );
          })}
        </Animated.View>
      )}

      {/* Recent searches or sample numbers */}
      {!displayResult && !displayLoading && (
        <Animated.View entering={FadeInDown.delay(100).springify()} style={s.hintCard}>
          {recentSearches.length > 0 ? (
            <>
              <Text style={s.hintTitle}>Recent Searches</Text>
              {recentSearches.slice(0, 5).map((item) => {
                const cfg = STATUS_CONFIG[item.status as ShipmentStatus] ?? STATUS_CONFIG["pending"];
                return (
                  <Pressable key={item.tracking_number} onPress={() => setQuery(item.tracking_number)} style={({ pressed }) => [s.hintRow, { opacity: pressed ? 0.7 : 1 }]}>
                    <Ionicons name="time-outline" size={14} color="rgba(255,255,255,0.3)" />
                    <Text style={s.hintAWB}>{item.tracking_number}</Text>
                    <Text style={[s.hintStatus, { color: cfg.color }]}>{cfg.label}</Text>
                  </Pressable>
                );
              })}
            </>
          ) : (
            <>
              <Text style={s.hintTitle}>Try these sample numbers</Text>
              {Object.keys(MOCK_RESULTS).map((awb) => (
                <Pressable key={awb} onPress={() => setQuery(awb)} style={({ pressed }) => [s.hintRow, { opacity: pressed ? 0.7 : 1 }]}>
                  <Ionicons name="cube-outline" size={14} color={CYAN} />
                  <Text style={s.hintAWB}>{awb}</Text>
                  <Text style={[s.hintStatus, { color: STATUS_CONFIG[MOCK_RESULTS[awb].status].color }]}>
                    {STATUS_CONFIG[MOCK_RESULTS[awb].status].label}
                  </Text>
                </Pressable>
              ))}
            </>
          )}
        </Animated.View>
      )}
    </ScrollView>
  );
}

const s = StyleSheet.create({
  container:      { flex: 1, backgroundColor: CANVAS },
  hero:           { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 20 },
  heroTitle:      { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:        { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },

  searchRow:      { flexDirection: "row", gap: 10, paddingHorizontal: 16, marginBottom: 16 },
  inputWrap:      { flex: 1, flexDirection: "row", alignItems: "center", gap: 10, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingHorizontal: 14, paddingVertical: 12 },
  input:          { flex: 1, fontSize: 14, color: "#FFF", fontFamily: "JetBrainsMono-Regular" },
  searchBtn:      { borderRadius: 12, overflow: "hidden" },
  searchBtnGrad:  { paddingHorizontal: 18, paddingVertical: 14, alignItems: "center", justifyContent: "center", minWidth: 72 },
  searchBtnText:  { fontSize: 13, fontWeight: "700", color: CANVAS },

  refreshBtn:     { flexDirection: "row", alignItems: "center", justifyContent: "center", paddingVertical: 10, paddingHorizontal: 14, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12 },

  errorCard:      { flexDirection: "row", alignItems: "center", gap: 8, marginHorizontal: 16, padding: 12, backgroundColor: RED + "10", borderWidth: 1, borderColor: RED + "30", borderRadius: 12, marginBottom: 16 },
  errorText:      { flex: 1, fontSize: 13, color: "rgba(255,255,255,0.6)" },

  resultCard:     { marginHorizontal: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 20, gap: 14 },

  awbRow:         { flexDirection: "row", justifyContent: "space-between", alignItems: "flex-start" },
  awbLabel:       { fontSize: 10, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, marginBottom: 4 },
  awb:            { fontSize: 18, fontWeight: "700", color: "#FFF", fontFamily: "JetBrainsMono-Regular" },
  statusChip:     { flexDirection: "row", alignItems: "center", gap: 5, paddingHorizontal: 10, paddingVertical: 6, borderRadius: 20, borderWidth: 1 },
  statusText:     { fontSize: 11, fontWeight: "600" },

  routeRow:       { flexDirection: "row", alignItems: "center", gap: 8 },
  routeCity:      { flex: 1, flexDirection: "row", alignItems: "center", gap: 4 },
  routeCityText:  { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.6)", fontWeight: "500" },
  routeLine:      { flexDirection: "row", alignItems: "center", flex: 0.4 },
  routeDot:       { width: 6, height: 6, borderRadius: 3 },
  routeLineBar:   { flex: 1, height: 1, backgroundColor: "rgba(255,255,255,0.1)", marginHorizontal: 2 },

  etaRow:         { flexDirection: "row", alignItems: "center", gap: 6 },
  etaText:        { fontSize: 13, color: "rgba(255,255,255,0.5)" },

  driverCard:     { flexDirection: "row", alignItems: "center", gap: 12, backgroundColor: CYAN + "08", borderWidth: 1, borderColor: CYAN + "20", borderRadius: 12, padding: 14 },
  driverAvatar:   { width: 38, height: 38, borderRadius: 10, backgroundColor: CYAN + "15", alignItems: "center", justifyContent: "center" },
  driverLabel:    { fontSize: 10, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 0.5 },
  driverName:     { fontSize: 14, fontWeight: "600", color: "#FFF", marginTop: 2 },
  callBtn:        { width: 36, height: 36, borderRadius: 10, backgroundColor: GREEN + "15", alignItems: "center", justifyContent: "center" },

  sectionLabel:   { fontSize: 11, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1 },

  timelineRow:    { flexDirection: "row", gap: 12 },
  timelineLeft:   { width: 24, alignItems: "center" },
  timelineDot:    { width: 24, height: 24, borderRadius: 12, alignItems: "center", justifyContent: "center" },
  timelineLine:   { flex: 1, width: 1, backgroundColor: "rgba(255,255,255,0.08)", marginTop: 4 },
  timelineContent:{ flex: 1, paddingBottom: 16 },
  timelineStatus: { fontSize: 13, fontWeight: "600", marginBottom: 2 },
  timelineDesc:   { fontSize: 12, color: "rgba(255,255,255,0.5)", lineHeight: 16 },
  timelineLoc:    { fontSize: 11, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", marginTop: 2 },
  timelineTime:   { fontSize: 10, color: "rgba(255,255,255,0.2)", fontFamily: "JetBrainsMono-Regular", marginTop: 4 },

  hintCard:       { marginHorizontal: 16, marginTop: 8, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 16 },
  hintTitle:      { fontSize: 11, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, marginBottom: 12 },
  hintRow:        { flexDirection: "row", alignItems: "center", gap: 10, paddingVertical: 10, borderBottomWidth: 1, borderBottomColor: "rgba(255,255,255,0.05)" },
  hintAWB:        { flex: 1, fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "#FFF" },
  hintStatus:     { fontSize: 11, fontWeight: "600" },
});
