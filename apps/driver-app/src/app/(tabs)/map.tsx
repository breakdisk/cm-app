/**
 * Driver App — Route Map Screen
 * Native: react-native-maps with dark theme + numbered markers.
 * Web: Rich interactive simulation — SVG route + tap-to-expand stop cards.
 */
import { useRef, useState } from "react";
import { View, Text, StyleSheet, Platform, Pressable, ScrollView } from "react-native";
import Animated, { FadeInDown, FadeInUp } from "react-native-reanimated";
import { Ionicons } from "@expo/vector-icons";
import { router } from "expo-router";
import { useSelector } from "react-redux";
import type { RootState } from "../../store";
import type { DeliveryTask } from "../../store";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const RED    = "#FF3B5C";
const PURPLE = "#A855F7";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

const STATUS_COLORS: Record<string, string> = {
  awaiting_pickup:  PURPLE,
  pickup_confirmed: GREEN,
  assigned:         CYAN,
  navigating:       AMBER,
  arrived:          AMBER,
  pod_pending:      AMBER,
  completed:        GREEN,
  failed:           RED,
};

// ── Native map (unchanged) ────────────────────────────────────────────────────

function NativeMap({ tasks }: { tasks: DeliveryTask[] }) {
  const MapView  = require("react-native-maps").default;
  const { Marker, Polyline } = require("react-native-maps");
  const mapRef = useRef(null);

  const MAP_STYLE = [
    { elementType: "geometry",             stylers: [{ color: "#0d1422" }] },
    { elementType: "labels.text.fill",     stylers: [{ color: "#ffffff40" }] },
    { elementType: "labels.text.stroke",   stylers: [{ color: "#050810" }] },
    { featureType: "road",         elementType: "geometry", stylers: [{ color: "#1a2235" }] },
    { featureType: "road.highway", elementType: "geometry", stylers: [{ color: "#1e2a40" }] },
    { featureType: "water",        elementType: "geometry", stylers: [{ color: "#050810" }] },
    { featureType: "poi",          stylers: [{ visibility: "off" }] },
    { featureType: "transit",      stylers: [{ visibility: "off" }] },
  ];

  const routeCoords = tasks
    .filter((t) => t.task_type === "delivery" && t.status !== "completed" && t.status !== "failed")
    .sort((a, b) => a.sequence - b.sequence)
    .map((t) => ({ latitude: t.lat, longitude: t.lng }));

  return (
    <View style={styles.container}>
      <MapView
        ref={mapRef}
        style={StyleSheet.absoluteFillObject}
        initialRegion={{ latitude: 14.5995, longitude: 120.9842, latitudeDelta: 0.15, longitudeDelta: 0.10 }}
        customMapStyle={MAP_STYLE}
        showsUserLocation
        userInterfaceStyle="dark"
      >
        {routeCoords.length > 1 && (
          <Polyline coordinates={routeCoords} strokeColor={`${CYAN}60`} strokeWidth={2} lineDashPattern={[6, 4]} />
        )}
        {tasks.map((task) => {
          const color = STATUS_COLORS[task.status] ?? CYAN;
          return (
            <Marker key={task.id} coordinate={{ latitude: task.lat, longitude: task.lng }}
              title={task.task_type === "pickup" ? task.sender_name : task.recipient_name}
            >
              <View style={[styles.pin, { backgroundColor: color }]}>
                <Text style={styles.pinText}>{task.task_type === "pickup" ? "↑" : task.sequence}</Text>
              </View>
            </Marker>
          );
        })}
      </MapView>
      <MapLegend />
    </View>
  );
}

// ── Map legend ────────────────────────────────────────────────────────────────

function MapLegend() {
  return (
    <View style={styles.legend}>
      {[
        { label: "To Deliver", color: CYAN },
        { label: "Pickup",     color: PURPLE },
        { label: "Delivered",  color: GREEN },
        { label: "Failed",     color: RED },
      ].map((item) => (
        <View key={item.label} style={styles.legendItemNative}>
          <View style={[styles.legendDotNative, { backgroundColor: item.color }]} />
          <Text style={styles.legendTextNative}>{item.label}</Text>
        </View>
      ))}
    </View>
  );
}

// ── Web simulation map ────────────────────────────────────────────────────────
// Deterministic pin positions based on a simplified Metro Manila layout.

interface PinLayout {
  topPct:  number;
  leftPct: number;
}

// Approximate relative positions for Manila metro zones
const ZONE_LAYOUTS: Record<string, PinLayout> = {
  "Makati City":     { topPct: 48, leftPct: 44 },
  "Taguig City":     { topPct: 60, leftPct: 54 },
  "Pasig City":      { topPct: 38, leftPct: 62 },
  "Quezon City":     { topPct: 22, leftPct: 52 },
  "Mandaluyong":     { topPct: 32, leftPct: 44 },
  "Makati CBD":      { topPct: 45, leftPct: 42 },
  "BGC Taguig":      { topPct: 58, leftPct: 52 },
  "Sampaloc, Manila":{ topPct: 30, leftPct: 30 },
  "Las Piñas":       { topPct: 72, leftPct: 38 },
  "Parañaque City":  { topPct: 68, leftPct: 44 },
};

function getPinLayout(task: DeliveryTask, index: number): PinLayout {
  const city = task.address_city;
  if (ZONE_LAYOUTS[city]) return ZONE_LAYOUTS[city];
  // Deterministic fallback spread
  return {
    topPct:  15 + ((task.sequence * 13 + index * 19) % 62),
    leftPct: 12 + ((task.sequence * 17 + index * 11) % 70),
  };
}

function WebMap({ tasks }: { tasks: DeliveryTask[] }) {
  const [selected, setSelected] = useState<string | null>(null);
  const selectedTask = tasks.find((t) => t.id === selected);

  const activeTasks = tasks.filter((t) => t.status !== "completed" && t.status !== "failed");
  const routeTasks  = tasks
    .filter((t) => t.task_type === "delivery" && t.status !== "completed" && t.status !== "failed")
    .sort((a, b) => a.sequence - b.sequence);

  // Compute pin positions for route line rendering
  const layouts = tasks.map((t, i) => getPinLayout(t, i));
  const routeLayouts = routeTasks.map((t) => layouts[tasks.indexOf(t)]);

  return (
    <View style={styles.container}>
      {/* ── Legend row — sits above the map frame ──────────────────── */}
      <View style={styles.legendRow}>
        {[
          { label: "To Deliver", color: CYAN },
          { label: "Pickup",     color: PURPLE },
          { label: "Delivered",  color: GREEN },
          { label: "Failed",     color: RED },
        ].map((item) => (
          <View key={item.label} style={styles.legendItem}>
            <View style={[styles.legendDot, { backgroundColor: item.color }]} />
            <Text style={styles.legendText}>{item.label}</Text>
          </View>
        ))}
      </View>

      {/* ── Map grid canvas ─────────────────────────────────────────── */}
      <View style={styles.webMapBg}>
        {/* Grid lines */}
        {Array.from({ length: 9 }).map((_, i) => (
          <View key={`h${i}`} style={[styles.gridLine, styles.gridLineH, { top: `${(i + 1) * 10}%` as any }]} />
        ))}
        {Array.from({ length: 6 }).map((_, i) => (
          <View key={`v${i}`} style={[styles.gridLine, styles.gridLineV, { left: `${(i + 1) * 14}%` as any }]} />
        ))}

        {/* Road suggestions */}
        <View style={[styles.road, { top: "45%", left: "10%", right: "10%", height: 2 }]} />
        <View style={[styles.road, { top: "25%", left: "10%", right: "10%", height: 1 }]} />
        <View style={[styles.road, { left: "40%", top: "10%", bottom: "10%", width: 2 }]} />
        <View style={[styles.road, { left: "60%", top: "15%", bottom: "20%", width: 1 }]} />

        {/* Route polyline — connect sequential delivery stops */}
        {routeLayouts.slice(0, -1).map((layout, i) => {
          const next  = routeLayouts[i + 1];
          const x1    = layout.leftPct + 2;
          const y1    = layout.topPct  + 2;
          const x2    = next.leftPct   + 2;
          const y2    = next.topPct    + 2;
          const len   = Math.sqrt(Math.pow(x2 - x1, 2) + Math.pow(y2 - y1, 2));
          const angle = Math.atan2(y2 - y1, x2 - x1) * (180 / Math.PI);
          return (
            <View
              key={`route-${i}`}
              style={{
                position:  "absolute",
                left:      `${x1}%` as any,
                top:       `${y1}%` as any,
                width:     `${len}%` as any,
                height:    2,
                backgroundColor: `${CYAN}40`,
                transformOrigin: "left center",
                transform: [{ rotate: `${angle}deg` }],
              }}
            />
          );
        })}

        {/* Task pins */}
        {tasks.map((task, i) => {
          const color   = STATUS_COLORS[task.status] ?? CYAN;
          const layout  = layouts[i];
          const isPickup = task.task_type === "pickup";
          const isSel   = selected === task.id;
          return (
            <Pressable
              key={task.id}
              onPress={() => setSelected(isSel ? null : task.id)}
              style={[
                styles.webPin,
                {
                  top:  `${layout.topPct}%` as any,
                  left: `${layout.leftPct}%` as any,
                  backgroundColor: color,
                  borderColor: isSel ? "#fff" : CANVAS,
                  transform: [{ scale: isSel ? 1.35 : 1 }],
                  zIndex: isSel ? 20 : 10,
                },
              ]}
            >
              <Text style={styles.webPinText}>
                {isPickup ? "↑" : task.status === "completed" ? "✓" : task.sequence}
              </Text>
            </Pressable>
          );
        })}

        {/* Map watermark */}
        <View style={styles.mapLabel}>
          <Text style={styles.mapLabelText}>METRO MANILA</Text>
          <Text style={styles.mapLabelSub}>Route simulation · native map on device</Text>
        </View>
      </View>

      {/* ── Bottom stop detail card ──────────────────────────────────── */}
      {selectedTask ? (
        <Animated.View entering={FadeInUp.springify()} style={styles.stopCard}>
          <View style={styles.stopCardHeader}>
            <View style={[styles.stopTypeBadge, { backgroundColor: `${STATUS_COLORS[selectedTask.status]}18`, borderColor: `${STATUS_COLORS[selectedTask.status]}40` }]}>
              <Ionicons
                name={selectedTask.task_type === "pickup" ? "archive-outline" : "location-outline"}
                size={12}
                color={STATUS_COLORS[selectedTask.status]}
              />
              <Text style={[styles.stopTypeTxt, { color: STATUS_COLORS[selectedTask.status] }]}>
                {selectedTask.task_type === "pickup" ? "First-Mile Pickup" : `Stop #${selectedTask.sequence}`}
              </Text>
            </View>
            <Pressable onPress={() => setSelected(null)} style={styles.stopClose}>
              <Ionicons name="close" size={14} color="rgba(255,255,255,0.4)" />
            </Pressable>
          </View>

          <Text style={styles.stopName}>
            {selectedTask.task_type === "pickup" ? selectedTask.sender_name : selectedTask.recipient_name}
          </Text>
          <Text style={styles.stopAddress}>{selectedTask.address_line1} · {selectedTask.address_city}</Text>
          <Text style={styles.stopAwb}>{selectedTask.tracking_number}</Text>

          <View style={styles.stopFooter}>
            {selectedTask.cod_amount != null && (
              <View style={styles.codTag}>
                <Text style={styles.codTagText}>COD ₱{selectedTask.cod_amount.toLocaleString()}</Text>
              </View>
            )}
            {selectedTask.eta_minutes != null && selectedTask.status === "assigned" && (
              <View style={styles.etaTag}>
                <Ionicons name="time-outline" size={11} color={AMBER} />
                <Text style={styles.etaText}>{selectedTask.eta_minutes}m away</Text>
              </View>
            )}
            {(selectedTask.status === "assigned" || selectedTask.status === "navigating" || selectedTask.status === "awaiting_pickup") && (
              <Pressable
                onPress={() => router.push(`/task/${selectedTask.id}`)}
                style={styles.goBtn}
              >
                <Ionicons name="navigate" size={12} color={CANVAS} />
                <Text style={styles.goBtnText}>Go</Text>
              </Pressable>
            )}
          </View>
        </Animated.View>
      ) : (
        <View style={styles.statsBar}>
          <View style={styles.statItem}>
            <Text style={[styles.statValue, { color: CYAN }]}>{activeTasks.filter(t => t.task_type === "delivery").length}</Text>
            <Text style={styles.statLabel}>Deliveries</Text>
          </View>
          <View style={styles.statDivider} />
          <View style={styles.statItem}>
            <Text style={[styles.statValue, { color: PURPLE }]}>{activeTasks.filter(t => t.task_type === "pickup").length}</Text>
            <Text style={styles.statLabel}>Pickups</Text>
          </View>
          <View style={styles.statDivider} />
          <View style={styles.statItem}>
            <Text style={[styles.statValue, { color: GREEN }]}>{tasks.filter(t => t.status === "completed").length}</Text>
            <Text style={styles.statLabel}>Done</Text>
          </View>
          <View style={styles.tapHint}>
            <Text style={styles.tapHintText}>Tap pin for details</Text>
          </View>
        </View>
      )}

    </View>
  );
}

// ── Screen ────────────────────────────────────────────────────────────────────

export default function RouteMapScreen() {
  const tasks = useSelector((s: RootState) => s.tasks.tasks);
  if (Platform.OS === "web") return <WebMap tasks={tasks} />;
  return <NativeMap tasks={tasks} />;
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container:    { flex: 1, backgroundColor: CANVAS },

  // Native
  pin:          { width: 28, height: 28, borderRadius: 14, alignItems: "center", justifyContent: "center", borderWidth: 2, borderColor: CANVAS },
  pinText:      { fontSize: 11, fontFamily: "JetBrainsMono-Bold", color: CANVAS },
  // Native map legend (absolute, inside map)
  legend:       { position: "absolute", bottom: 24, left: 12, backgroundColor: "rgba(5,8,16,0.88)", borderRadius: 10, padding: 10, gap: 6, borderWidth: 1, borderColor: BORDER },
  legendItemNative: { flexDirection: "row" as const, alignItems: "center" as const, gap: 6 },
  legendDotNative:  { width: 8, height: 8, borderRadius: 4 },
  legendTextNative: { fontSize: 10, color: "rgba(255,255,255,0.5)", fontFamily: "JetBrainsMono-Regular" },
  // Web legend row (above map frame)
  legendRow:    { flexDirection: "row" as const, alignItems: "center", gap: 16, paddingHorizontal: 16, paddingVertical: 8, borderBottomWidth: 1, borderBottomColor: BORDER, backgroundColor: "rgba(5,8,16,0.6)" },
  legendItem:   { flexDirection: "row" as const, alignItems: "center", gap: 5 },
  legendDot:    { width: 7, height: 7, borderRadius: 4 },
  legendText:   { fontSize: 10, color: "rgba(255,255,255,0.45)", fontFamily: "JetBrainsMono-Regular" },

  // Web map bg
  webMapBg:     { flex: 1, backgroundColor: "#060c1a", position: "relative", overflow: "hidden" },
  gridLine:     { position: "absolute", backgroundColor: "rgba(255,255,255,0.04)" },
  gridLineH:    { left: 0, right: 0, height: 1 },
  gridLineV:    { top: 0, bottom: 0, width: 1 },
  road:         { position: "absolute", backgroundColor: "rgba(255,255,255,0.07)", borderRadius: 1 },

  // Web pins
  webPin:       { position: "absolute", width: 28, height: 28, borderRadius: 14, alignItems: "center", justifyContent: "center", borderWidth: 2 },
  webPinText:   { fontSize: 10, fontFamily: "JetBrainsMono-Bold", color: CANVAS },

  mapLabel:     { position: "absolute", bottom: "50%", alignSelf: "center", alignItems: "center", gap: 4 },
  mapLabelText: { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: "rgba(0,229,255,0.2)", letterSpacing: 4, textTransform: "uppercase" },
  mapLabelSub:  { fontSize: 8, color: "rgba(255,255,255,0.12)", fontFamily: "JetBrainsMono-Regular" },

  // Stop card
  stopCard:       { backgroundColor: "rgba(10,15,28,0.98)", borderTopWidth: 1, borderTopColor: BORDER, padding: 16, gap: 6 },
  stopCardHeader: { flexDirection: "row", alignItems: "center", justifyContent: "space-between", marginBottom: 2 },
  stopTypeBadge:  { flexDirection: "row", alignItems: "center", gap: 5, paddingHorizontal: 8, paddingVertical: 3, borderRadius: 999, borderWidth: 1 },
  stopTypeTxt:    { fontSize: 10, fontFamily: "JetBrainsMono-Regular" },
  stopClose:      { padding: 4 },
  stopName:       { fontSize: 16, fontFamily: "SpaceGrotesk-SemiBold", color: "#FFF" },
  stopAddress:    { fontSize: 11, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular" },
  stopAwb:        { fontSize: 11, color: "rgba(255,255,255,0.25)", fontFamily: "JetBrainsMono-Regular" },
  stopFooter:     { flexDirection: "row", alignItems: "center", gap: 8, marginTop: 4 },
  codTag:         { paddingHorizontal: 8, paddingVertical: 3, borderRadius: 6, backgroundColor: "rgba(255,171,0,0.1)", borderWidth: 1, borderColor: "rgba(255,171,0,0.25)" },
  codTagText:     { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: AMBER },
  etaTag:         { flexDirection: "row", alignItems: "center", gap: 4 },
  etaText:        { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: AMBER },
  goBtn:          { marginLeft: "auto" as any, flexDirection: "row", alignItems: "center", gap: 5, backgroundColor: CYAN, paddingHorizontal: 14, paddingVertical: 6, borderRadius: 8 },
  goBtnText:      { fontSize: 12, fontFamily: "SpaceGrotesk-SemiBold", color: CANVAS },

  // Stats bar
  statsBar:       { flexDirection: "row", alignItems: "center", backgroundColor: "rgba(10,15,28,0.98)", borderTopWidth: 1, borderTopColor: BORDER, paddingHorizontal: 20, paddingVertical: 14 },
  statItem:       { alignItems: "center", gap: 2, paddingHorizontal: 16 },
  statValue:      { fontSize: 20, fontFamily: "SpaceGrotesk-Bold" },
  statLabel:      { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", textTransform: "uppercase", letterSpacing: 1 },
  statDivider:    { width: 1, height: 30, backgroundColor: BORDER },
  tapHint:        { flex: 1, alignItems: "flex-end" },
  tapHintText:    { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.2)" },
});
