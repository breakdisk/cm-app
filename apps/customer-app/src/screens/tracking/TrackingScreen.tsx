/**
 * Customer App — Tracking Screen
 * Search by AWB, live status timeline, driver ETA card.
 */
import React, { useState, useEffect } from "react";
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FadeInView } from '../../components/FadeInView';
import {
  View, Text, StyleSheet, ScrollView, TextInput,
  Pressable, ActivityIndicator,
} from "react-native";
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
import { LiveDriverMap } from "../../components/LiveDriverMap";
import { trackingApi } from "../../services/api/tracking";

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
  driver_location?: { lat: number; lng: number };
  timeline:         TimelineEvent[];
}


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
            const row = tracking as Record<string, string>;
            setOfflineData(JSON.parse(row['events'] ?? '{}'));
            setLastUpdated(new Date(row['lastUpdated'] ?? Date.now()).getTime());
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

  async function handleSearch() {
    const awb = query.trim().toUpperCase();
    if (!awb) return;
    setLocalLoading(true);
    setLocalError("");
    setResult(null);
    try {
      const tracking = await trackingApi.getByTrackingNumber(awb);
      const data = (tracking.data as any)?.data ?? tracking.data as any;
      const mapped: TrackingResult = {
        awb: data.tracking_number ?? awb,
        status: (data.status ?? "pending") as ShipmentStatus,
        origin_city: data.origin ?? data.origin_city ?? "",
        destination_city: data.destination ?? data.destination_city ?? "",
        eta: data.estimated_delivery ?? data.eta,
        driver_name: data.driver?.name,
        driver_phone: undefined,
        driver_location: data.driver_location
          ? { lat: Number(data.driver_location.lat), lng: Number(data.driver_location.lng) }
          : undefined,
        timeline: (data.history ?? data.events ?? []).map((e: any) => ({
          status: (e.status ?? "pending") as ShipmentStatus,
          description: e.description ?? e.status_label ?? "",
          location: e.location,
          occurred_at: e.occurred_at ?? e.timestamp ?? "",
        })),
      };
      setResult(mapped);
      setCurrentAwb(awb);
      dispatch(trackingActions.addToHistory({
        awb,
        currentStatus: data.status ?? "pending",
        events: data.events ?? [],
      } as any));
    } catch (err: any) {
      if (err?.status === 404) {
        setLocalError("No shipment found for that tracking number. Check and try again.");
      } else {
        setLocalError(err?.message ?? "Failed to fetch tracking. Please try again.");
      }
    } finally {
      setLocalLoading(false);
    }
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
      <FadeInView fromY={-16} style={s.searchRow}>
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
      </FadeInView>

      {/* Error */}
      {displayError && (
        <FadeInView fromY={-16} style={s.errorCard}>
          <Ionicons name="alert-circle-outline" size={16} color={RED} />
          <Text style={s.errorText}>{displayError}</Text>
        </FadeInView>
      )}

      {/* Loading hook data */}
      {displayLoading && currentAwb && (
        <FadeInView fromY={-16} style={s.resultCard}>
          <View style={{ alignItems: "center", paddingVertical: 40 }}>
            <ActivityIndicator size="large" color={CYAN} />
            <Text style={{ color: "rgba(255,255,255,0.5)", marginTop: 16, fontSize: 14 }}>Fetching tracking details...</Text>
          </View>
        </FadeInView>
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
        <FadeInView fromY={16} style={s.resultCard}>
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

          {/* Live driver map — shown only when driver_location is present */}
          {displayResult.driver_location && (
            <LiveDriverMap
              driverLocation={displayResult.driver_location}
              driverName={displayResult.driver_name}
            />
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
        </FadeInView>
      )}

      {/* Recent searches or sample numbers */}
      {!displayResult && !displayLoading && (
        <FadeInView delay={100} fromY={-16} style={s.hintCard}>
          {recentSearches.length > 0 ? (
            <>
              <Text style={s.hintTitle}>Recent Searches</Text>
              {recentSearches.slice(0, 5).map((item) => {
                const cfg = STATUS_CONFIG[item.awb as ShipmentStatus] ?? STATUS_CONFIG["pending"];
                return (
                  <Pressable key={item.awb} onPress={() => setQuery(item.awb)} style={({ pressed }) => [s.hintRow, { opacity: pressed ? 0.7 : 1 }]}>
                    <Ionicons name="time-outline" size={14} color="rgba(255,255,255,0.3)" />
                    <Text style={s.hintAWB}>{item.awb}</Text>
                    <Text style={[s.hintStatus, { color: cfg?.color ?? AMBER }]}>{cfg?.label ?? item.awb}</Text>
                  </Pressable>
                );
              })}
            </>
          ) : (
            <>
              <Text style={s.hintTitle}>Enter a tracking number above</Text>
              <Text style={{ fontSize: 12, color: "rgba(255,255,255,0.25)", fontFamily: "JetBrainsMono-Regular", marginTop: 4 }}>
                e.g. LS-PH1-S0001234X
              </Text>
            </>
          )}
        </FadeInView>
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
