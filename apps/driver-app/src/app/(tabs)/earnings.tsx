/**
 * Driver App — Earnings Screen
 * Shows driver type (part-time/full-time), commission config, and earnings breakdown.
 */
import { View, Text, StyleSheet, ScrollView, Platform } from "react-native";
import { useSelector } from "react-redux";
import Animated, { FadeInDown } from "react-native-reanimated";
import { Ionicons } from "@expo/vector-icons";
import type { RootState } from "../../store";
import type { EarningEntry } from "../../store";

const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const AMBER   = "#FFAB00";
const PURPLE  = "#A855F7";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

function fmt(n: number) {
  return `₱${n.toFixed(2).replace(/\B(?=(\d{3})+(?!\d))/g, ",")}`;
}

function timeAgo(iso: string) {
  const diff = Date.now() - new Date(iso).getTime();
  const m    = Math.floor(diff / 60000);
  if (m < 1)  return "just now";
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  return `${Math.floor(h / 24)}d ago`;
}

export default function EarningsScreen() {
  const earnings = useSelector((s: RootState) => s.earnings);
  const isPartTime = earnings.driverType === "part_time";

  const todayBreakdown = earnings.breakdown.filter((e) => {
    const today = new Date().toISOString().slice(0, 10);
    return e.completedAt.slice(0, 10) === today;
  });

  return (
    <ScrollView style={styles.container} contentContainerStyle={{ paddingBottom: 40 }}>

      {/* Driver type badge */}
      <Animated.View entering={FadeInDown.springify()} style={styles.typeBadgeRow}>
        <View style={[styles.typeBadge, isPartTime ? styles.typeBadgePart : styles.typeBadgeFull]}>
          <Ionicons
            name={isPartTime ? "time-outline" : "briefcase-outline"}
            size={14}
            color={isPartTime ? AMBER : CYAN}
          />
          <Text style={[styles.typeBadgeText, { color: isPartTime ? AMBER : CYAN }]}>
            {isPartTime ? "Part-Time Driver" : "Full-Time Driver"}
          </Text>
        </View>
        {isPartTime && (
          <View style={styles.rateChip}>
            <Text style={styles.rateChipText}>{fmt(earnings.commissionRate)} / delivery</Text>
          </View>
        )}
      </Animated.View>

      {/* Summary cards */}
      <Animated.View entering={FadeInDown.delay(60).springify()} style={styles.summaryRow}>
        <View style={[styles.summaryCard, { borderColor: "rgba(0,255,136,0.2)" }]}>
          <Text style={styles.summaryLabel}>Today</Text>
          <Text style={[styles.summaryValue, { color: GREEN }]}>{fmt(earnings.todayEarnings)}</Text>
          <Text style={styles.summaryCount}>{todayBreakdown.length} deliveries</Text>
        </View>
        <View style={[styles.summaryCard, { borderColor: "rgba(0,229,255,0.2)" }]}>
          <Text style={styles.summaryLabel}>This Week</Text>
          <Text style={[styles.summaryValue, { color: CYAN }]}>{fmt(earnings.weekEarnings)}</Text>
          <Text style={styles.summaryCount}>{earnings.breakdown.length} total</Text>
        </View>
      </Animated.View>

      {/* Pending payout */}
      {earnings.pendingPayout > 0 && (
        <Animated.View entering={FadeInDown.delay(100).springify()} style={styles.payoutCard}>
          <View style={styles.payoutLeft}>
            <Ionicons name="wallet-outline" size={18} color={PURPLE} />
            <View>
              <Text style={styles.payoutLabel}>Pending Payout</Text>
              <Text style={styles.payoutSub}>Released on next settlement cycle</Text>
            </View>
          </View>
          <Text style={styles.payoutAmount}>{fmt(earnings.pendingPayout)}</Text>
        </Animated.View>
      )}

      {/* Commission config (part-time only) */}
      {isPartTime && (
        <Animated.View entering={FadeInDown.delay(140).springify()} style={styles.configCard}>
          <Text style={styles.cardLabel}>Commission Structure</Text>
          <View style={styles.configRow}>
            <Text style={styles.configKey}>Base rate per delivery</Text>
            <Text style={[styles.configVal, { color: GREEN }]}>{fmt(earnings.commissionRate)}</Text>
          </View>
          <View style={[styles.configRow, { borderBottomWidth: 0 }]}>
            <Text style={styles.configKey}>COD bonus rate</Text>
            <Text style={[styles.configVal, { color: AMBER }]}>
              {(earnings.codCommissionRate * 100).toFixed(1)}%
            </Text>
          </View>
        </Animated.View>
      )}

      {/* Today's breakdown */}
      <Animated.View entering={FadeInDown.delay(180).springify()} style={styles.breakdownCard}>
        <Text style={styles.cardLabel}>Today's Deliveries</Text>
        {todayBreakdown.length === 0 ? (
          <View style={styles.emptyState}>
            <Ionicons name="receipt-outline" size={28} color="rgba(255,255,255,0.12)" />
            <Text style={styles.emptyText}>No deliveries completed today</Text>
          </View>
        ) : (
          todayBreakdown.slice().reverse().map((entry: EarningEntry, i: number) => (
            <View key={entry.taskId} style={[styles.entryRow, i === 0 && { borderTopWidth: 0 }]}>
              <View style={styles.entryLeft}>
                <Text style={styles.entryId}>{entry.shipmentId.slice(-6).toUpperCase()}</Text>
                <Text style={styles.entryTime}>{timeAgo(entry.completedAt)}</Text>
              </View>
              <View style={styles.entryRight}>
                {entry.codBonus > 0 && (
                  <View style={styles.codTag}>
                    <Text style={styles.codTagText}>+{fmt(entry.codBonus)} COD</Text>
                  </View>
                )}
                <Text style={styles.entryTotal}>{fmt(entry.total)}</Text>
              </View>
            </View>
          ))
        )}
      </Animated.View>

    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container:       { flex: 1, backgroundColor: CANVAS },

  typeBadgeRow:    { flexDirection: "row", alignItems: "center", gap: 8, margin: 12, marginBottom: 8 },
  typeBadge:       { flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: 12, paddingVertical: 6, borderRadius: 20, borderWidth: 1 },
  typeBadgePart:   { backgroundColor: "rgba(255,171,0,0.08)", borderColor: "rgba(255,171,0,0.25)" },
  typeBadgeFull:   { backgroundColor: "rgba(0,229,255,0.08)", borderColor: "rgba(0,229,255,0.25)" },
  typeBadgeText:   { fontSize: 12, fontFamily: "SpaceGrotesk-SemiBold" },
  rateChip:        { paddingHorizontal: 10, paddingVertical: 4, borderRadius: 10, backgroundColor: "rgba(0,255,136,0.08)", borderWidth: 1, borderColor: "rgba(0,255,136,0.2)" },
  rateChipText:    { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: GREEN },

  summaryRow:      { flexDirection: "row", gap: 10, marginHorizontal: 12, marginBottom: 10 },
  summaryCard:     { flex: 1, borderRadius: 14, backgroundColor: GLASS, borderWidth: 1, padding: 14 },
  summaryLabel:    { fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", color: "rgba(255,255,255,0.3)", marginBottom: 6 },
  summaryValue:    { fontSize: 22, fontFamily: "SpaceGrotesk-Bold", marginBottom: 2 },
  summaryCount:    { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)" },

  payoutCard:      { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: "rgba(168,85,247,0.07)", borderWidth: 1, borderColor: "rgba(168,85,247,0.2)", padding: 14, flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  payoutLeft:      { flexDirection: "row", alignItems: "center", gap: 10 },
  payoutLabel:     { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold", color: PURPLE },
  payoutSub:       { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(168,85,247,0.5)", marginTop: 2 },
  payoutAmount:    { fontSize: 18, fontFamily: "SpaceGrotesk-Bold", color: PURPLE },

  configCard:      { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 14 },
  cardLabel:       { fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, color: "rgba(255,255,255,0.3)", marginBottom: 12 },
  configRow:       { flexDirection: "row", justifyContent: "space-between", alignItems: "center", paddingVertical: 10, borderBottomWidth: 1, borderBottomColor: BORDER },
  configKey:       { fontSize: 13, color: "rgba(255,255,255,0.6)" },
  configVal:       { fontSize: 13, fontFamily: "JetBrainsMono-Regular", fontWeight: "600" },

  breakdownCard:   { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 14 },
  emptyState:      { alignItems: "center", paddingVertical: 24, gap: 8 },
  emptyText:       { fontSize: 13, color: "rgba(255,255,255,0.2)", fontFamily: "JetBrainsMono-Regular" },
  entryRow:        { flexDirection: "row", alignItems: "center", justifyContent: "space-between", paddingVertical: 11, borderTopWidth: 1, borderTopColor: BORDER },
  entryLeft:       { gap: 3 },
  entryId:         { fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.8)" },
  entryTime:       { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)" },
  entryRight:      { flexDirection: "row", alignItems: "center", gap: 8 },
  codTag:          { paddingHorizontal: 7, paddingVertical: 2, borderRadius: 6, backgroundColor: "rgba(255,171,0,0.1)", borderWidth: 1, borderColor: "rgba(255,171,0,0.25)" },
  codTagText:      { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: AMBER },
  entryTotal:      { fontSize: 14, fontFamily: "SpaceGrotesk-Bold", color: GREEN },
});
