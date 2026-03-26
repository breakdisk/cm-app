/**
 * Driver App — Profile & Status Screen
 * Shows driver status, daily stats, sync queue status, and settings.
 */
import { View, Text, StyleSheet, ScrollView, Pressable, Switch } from "react-native";
import { useEffect } from "react";
import { useDispatch, useSelector } from "react-redux";
import Animated, { FadeInDown } from "react-native-reanimated";
import { Ionicons } from "@expo/vector-icons";
import { router } from "expo-router";
import type { RootState, AppDispatch } from "../../store";
import { authActions, complianceActions } from "../../store";
import type { SubmittedDoc } from "../../store";

const PURPLE = "#A855F7";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

// ── Compliance helpers ────────────────────────────────────────────────────────

function docSubText(doc?: SubmittedDoc): string {
  if (!doc)                         return "Not submitted · Required";
  if (doc.status === "approved")    return `Approved · Exp ${doc.expiry_date ?? "—"}`;
  if (doc.status === "under_review") return "Under review · Est. 24h";
  if (doc.status === "submitted")   return "Submitted · Awaiting review";
  if (doc.status === "rejected")    return `Rejected — ${doc.rejection_reason ?? "see reason"}`;
  if (doc.status === "expired")     return "Expired · Renewal required";
  return "—";
}

function docRowStyle(status?: string): object {
  if (!status)                                     return styles.docRowMissing;
  if (status === "approved")                       return styles.docRowOk;
  if (status === "rejected" || status === "expired") return styles.docRowWarn;
  return styles.docRowReview;
}

function docDotStyle(status?: string): object {
  if (!status)                                     return styles.docDotMissing;
  if (status === "approved")                       return styles.docDotOk;
  if (status === "rejected" || status === "expired") return styles.docDotWarn;
  return styles.docDotReview;
}

// ── ComplianceBanner ──────────────────────────────────────────────────────────

interface ComplianceBannerProps {
  status:       string;
  missingCount: number;
}

function ComplianceBanner({ status, missingCount }: ComplianceBannerProps) {
  const cfg: Record<string, { bg: string; border: string; titleColor: string; title: string; sub: string }> = {
    pending_submission: { bg: "rgba(255,171,0,0.08)",  border: "rgba(255,171,0,0.25)",  titleColor: AMBER, title: "⚠ Action Required",   sub: `${missingCount} document${missingCount !== 1 ? "s" : ""} need upload before you can receive tasks` },
    under_review:       { bg: "rgba(0,229,255,0.07)",  border: "rgba(0,229,255,0.2)",   titleColor: CYAN,  title: "⏳ Under Review",      sub: "Documents submitted · Awaiting compliance team" },
    compliant:          { bg: "rgba(0,255,136,0.07)",  border: "rgba(0,255,136,0.2)",   titleColor: GREEN, title: "✓ All Clear",          sub: "All documents verified · You are assignable" },
    expiring_soon:      { bg: "rgba(255,171,0,0.08)",  border: "rgba(255,171,0,0.25)",  titleColor: AMBER, title: "⚠ Documents Expiring", sub: "Renew soon to stay assignable" },
    expired:            { bg: "rgba(255,171,0,0.08)",  border: "rgba(255,171,0,0.25)",  titleColor: AMBER, title: "⚠ Document Expired",   sub: "Renew within grace period to stay active" },
    suspended:          { bg: "rgba(255,59,92,0.08)",  border: "rgba(255,59,92,0.25)",  titleColor: RED,   title: "✗ Account Suspended",  sub: "Contact support to reinstate your account" },
    rejected:           { bg: "rgba(255,59,92,0.08)",  border: "rgba(255,59,92,0.25)",  titleColor: RED,   title: "✗ Document Rejected",  sub: "Re-upload the rejected document to continue" },
  };
  const c = cfg[status] ?? cfg.pending_submission;
  return (
    <View style={{ marginHorizontal: 12, marginBottom: 8, borderRadius: 10, padding: 12,
                   backgroundColor: c.bg, borderWidth: 1, borderColor: c.border }}>
      <Text style={{ fontSize: 11, fontFamily: "SpaceGrotesk-SemiBold", textTransform: "uppercase",
                     letterSpacing: 0.8, color: c.titleColor, marginBottom: 4 }}>
        {c.title}
      </Text>
      <Text style={{ fontSize: 10, color: c.titleColor, opacity: 0.6, lineHeight: 15 }}>
        {c.sub}
      </Text>
    </View>
  );
}

export default function ProfileScreen() {
  const dispatch   = useDispatch<AppDispatch>();
  const auth       = useSelector((s: RootState) => s.auth);
  const tasks      = useSelector((s: RootState) => s.tasks);
  const earnings   = useSelector((s: RootState) => s.earnings);
  const compliance = useSelector((s: RootState) => s.compliance);
  const isPartTime = earnings.driverType === "part_time";

  // Seed mock compliance data on first mount
  useEffect(() => {
    dispatch(complianceActions.setComplianceProfile({
      overall_status: "pending_submission",
      jurisdiction:   "UAE",
      required_types: [
        { id: "dt1", code: "UAE_DRIVING_LICENSE",  name: "UAE Driving License",           has_expiry: true, warn_days_before: 30 },
        { id: "dt2", code: "UAE_EMIRATES_ID",       name: "Emirates ID",                   has_expiry: true, warn_days_before: 60 },
        { id: "dt3", code: "UAE_VEHICLE_MULKIYA",   name: "Vehicle Registration (Mulkiya)", has_expiry: true, warn_days_before: 30 },
        { id: "dt4", code: "UAE_VEHICLE_INSURANCE", name: "Third-Party Insurance",         has_expiry: true, warn_days_before: 30 },
      ],
      documents: [
        {
          id: "doc1", document_type_id: "dt3", document_number: "REG-DXB-A12345",
          expiry_date: "2026-03-01", status: "approved", rejection_reason: null,
          submitted_at: new Date().toISOString(),
        },
        {
          id: "doc2", document_type_id: "dt4", document_number: "POL-AXA-88821",
          expiry_date: "2025-12-31", status: "approved", rejection_reason: null,
          submitted_at: new Date().toISOString(),
        },
      ],
    }));
  }, []);

  const missingCount = compliance.required_types.filter(
    (dt) => !compliance.documents.find(
      (d) => d.document_type_id === dt.id && d.status !== "superseded"
    )
  ).length;

  const completed  = tasks.tasks.filter((t) => t.status === "completed").length;
  const failed     = tasks.tasks.filter((t) => t.status === "failed").length;
  const total      = tasks.tasks.length;

  function toggleOnline() {
    dispatch(authActions.setOnlineStatus(!auth.isOnline));
  }

  return (
    <ScrollView style={styles.container} contentContainerStyle={{ paddingBottom: 40 }}>

      {/* Driver card */}
      <Animated.View entering={FadeInDown.springify()} style={styles.driverCard}>
        <View style={styles.avatar}>
          <Text style={styles.avatarText}>{(auth.name ?? "DR").substring(0, 2).toUpperCase()}</Text>
        </View>
        <View style={styles.driverInfo}>
          <Text style={styles.driverName}>{auth.name ?? "Driver"}</Text>
          <Text style={styles.driverId}>ID: {auth.driverId ?? "—"}</Text>
          <View style={[styles.driverTypeBadge, isPartTime ? styles.driverTypePart : styles.driverTypeFull]}>
            <Text style={[styles.driverTypeText, { color: isPartTime ? AMBER : CYAN }]}>
              {isPartTime ? "Part-Time" : "Full-Time"}
            </Text>
          </View>
        </View>
        {/* Online toggle */}
        <View style={styles.onlineToggle}>
          <Text style={[styles.onlineLabel, { color: auth.isOnline ? GREEN : "rgba(255,255,255,0.3)" }]}>
            {auth.isOnline ? "Online" : "Offline"}
          </Text>
          <Switch
            value={auth.isOnline}
            onValueChange={toggleOnline}
            trackColor={{ false: "rgba(255,255,255,0.1)", true: `${GREEN}60` }}
            thumbColor={auth.isOnline ? GREEN : "rgba(255,255,255,0.4)"}
          />
        </View>
      </Animated.View>

      {/* Daily stats */}
      <Animated.View entering={FadeInDown.delay(80).springify()} style={styles.statsCard}>
        <Text style={styles.cardLabel}>Today's Performance</Text>
        <View style={styles.statsRow}>
          <View style={styles.statItem}>
            <Text style={[styles.statValue, { color: GREEN }]}>{completed}</Text>
            <Text style={styles.statLabel}>Delivered</Text>
          </View>
          <View style={styles.statDivider} />
          <View style={styles.statItem}>
            <Text style={[styles.statValue, { color: RED }]}>{failed}</Text>
            <Text style={styles.statLabel}>Failed</Text>
          </View>
          <View style={styles.statDivider} />
          <View style={styles.statItem}>
            <Text style={[styles.statValue, { color: CYAN }]}>{total}</Text>
            <Text style={styles.statLabel}>Assigned</Text>
          </View>
          <View style={styles.statDivider} />
          <View style={styles.statItem}>
            <Text style={[styles.statValue, { color: total > 0 ? GREEN : "rgba(255,255,255,0.3)" }]}>
              {total > 0 ? Math.round((completed / total) * 100) : 0}%
            </Text>
            <Text style={styles.statLabel}>Rate</Text>
          </View>
        </View>
      </Animated.View>

      {/* Vehicle card */}
      <Animated.View entering={FadeInDown.delay(100).springify()} style={styles.vehicleCard}>
        <Text style={styles.cardLabel}>Assigned Vehicle</Text>
        <View style={styles.vehicleRow}>
          {/* Class badge */}
          <View style={styles.vehicleIconWrap}>
            <Ionicons name="car-sport-outline" size={22} color={CYAN} />
          </View>
          <View style={styles.vehicleInfo}>
            <View style={styles.vehicleNameRow}>
              <Text style={styles.vehicleModel}>Toyota Vios 1.3 E</Text>
              <View style={styles.vehicleClassBadge}>
                <Ionicons name="car-outline" size={10} color={CYAN} />
                <Text style={styles.vehicleClassText}>Sedan</Text>
              </View>
            </View>
            <Text style={styles.vehiclePlate}>Plate: DXB • A12345</Text>
            <Text style={styles.vehicleSpec}>5–25 kg  ·  COD-capable  ·  Fragile-rated</Text>
          </View>
          <View style={styles.vehicleStatus}>
            <View style={styles.vehicleActiveDot} />
            <Text style={styles.vehicleActiveText}>Active</Text>
          </View>
        </View>
      </Animated.View>

      {/* Compliance Banner */}
      <Animated.View entering={FadeInDown.delay(130).springify()} style={styles.compBanner}>
        <ComplianceBanner
          status={compliance.overall_status}
          missingCount={missingCount}
        />
      </Animated.View>

      {/* Document Checklist */}
      <Animated.View entering={FadeInDown.delay(150).springify()} style={styles.docCard}>
        <Text style={styles.cardLabel}>Required Documents</Text>
        {compliance.required_types.map((dt) => {
          const doc = compliance.documents.find(
            (d) => d.document_type_id === dt.id && d.status !== "superseded"
          );
          return (
            <Pressable
              key={dt.id}
              onPress={() => router.push(`/compliance/upload/${dt.code}` as any)}
              style={[styles.docRow, docRowStyle(doc?.status)]}
            >
              <View style={[styles.docDot, docDotStyle(doc?.status)]} />
              <View style={styles.docRowInfo}>
                <Text style={styles.docRowName}>{dt.name}</Text>
                <Text style={styles.docRowSub}>{docSubText(doc)}</Text>
              </View>
              <Ionicons name="chevron-forward" size={14} color="rgba(255,255,255,0.2)" />
            </Pressable>
          );
        })}
      </Animated.View>

      {/* Sync status */}
      {tasks.syncPending > 0 && (
        <Animated.View entering={FadeInDown.delay(120).springify()} style={styles.syncCard}>
          <Ionicons name="cloud-upload-outline" size={16} color={AMBER} />
          <View style={styles.syncInfo}>
            <Text style={styles.syncTitle}>{tasks.syncPending} action{tasks.syncPending !== 1 ? "s" : ""} pending sync</Text>
            <Text style={styles.syncSub}>Will sync automatically when online</Text>
          </View>
          <View style={styles.syncDot} />
        </Animated.View>
      )}

      {/* Settings menu */}
      <Animated.View entering={FadeInDown.delay(160).springify()} style={styles.menuCard}>
        <Text style={styles.cardLabel}>Settings</Text>
        {[
          { label: "Notifications",    icon: "notifications-outline", value: "On" },
          { label: "GPS Background",   icon: "location-outline",      value: "Enabled" },
          { label: "App Version",      icon: "information-circle-outline", value: "1.0.0" },
        ].map((item) => (
          <View key={item.label} style={styles.menuRow}>
            <Ionicons name={item.icon as never} size={16} color="rgba(255,255,255,0.35)" />
            <Text style={styles.menuLabel}>{item.label}</Text>
            <Text style={styles.menuValue}>{item.value}</Text>
            <Ionicons name="chevron-forward" size={12} color="rgba(255,255,255,0.2)" />
          </View>
        ))}
      </Animated.View>

      {/* Sign out */}
      <Animated.View entering={FadeInDown.delay(200).springify()} style={{ marginHorizontal: 12 }}>
        <Pressable
          onPress={() => dispatch(authActions.logout())}
          style={({ pressed }) => [styles.signOutBtn, { opacity: pressed ? 0.7 : 1 }]}
        >
          <Ionicons name="log-out-outline" size={16} color={RED} />
          <Text style={styles.signOutText}>Sign Out</Text>
        </Pressable>
      </Animated.View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container:    { flex: 1, backgroundColor: CANVAS },
  driverCard:   { margin: 12, borderRadius: 14, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 16, flexDirection: "row", alignItems: "center", gap: 12 },
  avatar:       { width: 48, height: 48, borderRadius: 24, backgroundColor: "rgba(0,229,255,0.15)", borderWidth: 2, borderColor: "rgba(0,229,255,0.3)", alignItems: "center", justifyContent: "center" },
  avatarText:   { fontSize: 16, fontFamily: "SpaceGrotesk-Bold", color: CYAN },
  driverInfo:   { flex: 1 },
  driverName:   { fontSize: 16, fontFamily: "SpaceGrotesk-SemiBold", color: "#FFFFFF" },
  driverId:     { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", marginTop: 2 },
  onlineToggle: { alignItems: "center", gap: 4 },
  onlineLabel:  { fontSize: 9, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase" },
  statsCard:    { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 14 },
  cardLabel:    { fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, color: "rgba(255,255,255,0.3)", marginBottom: 12 },
  statsRow:     { flexDirection: "row", justifyContent: "space-around" },
  statItem:     { alignItems: "center", gap: 4 },
  statValue:    { fontSize: 24, fontFamily: "SpaceGrotesk-Bold" },
  statLabel:    { fontSize: 9, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase" },
  statDivider:  { width: 1, backgroundColor: BORDER, alignSelf: "stretch" },
  vehicleCard:      { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: "rgba(0,229,255,0.04)", borderWidth: 1, borderColor: "rgba(0,229,255,0.15)", padding: 14 },
  vehicleRow:       { flexDirection: "row", alignItems: "center", gap: 12 },
  vehicleIconWrap:  { width: 44, height: 44, borderRadius: 12, backgroundColor: "rgba(0,229,255,0.1)", borderWidth: 1, borderColor: "rgba(0,229,255,0.25)", alignItems: "center", justifyContent: "center" },
  vehicleInfo:      { flex: 1 },
  vehicleNameRow:   { flexDirection: "row", alignItems: "center", gap: 8 },
  vehicleModel:     { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: "#FFFFFF" },
  vehicleClassBadge:{ flexDirection: "row", alignItems: "center", gap: 4, paddingHorizontal: 7, paddingVertical: 2, borderRadius: 8, backgroundColor: "rgba(0,229,255,0.08)", borderWidth: 1, borderColor: "rgba(0,229,255,0.25)" },
  vehicleClassText: { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: CYAN, textTransform: "uppercase", letterSpacing: 0.5 },
  vehiclePlate:     { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.35)", marginTop: 3 },
  vehicleSpec:      { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.2)", marginTop: 2 },
  vehicleStatus:    { alignItems: "center", gap: 4 },
  vehicleActiveDot: { width: 8, height: 8, borderRadius: 4, backgroundColor: GREEN },
  vehicleActiveText:{ fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: GREEN, textTransform: "uppercase" },
  syncCard:     { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: "rgba(255,171,0,0.06)", borderWidth: 1, borderColor: "rgba(255,171,0,0.2)", padding: 14, flexDirection: "row", alignItems: "center", gap: 10 },
  syncInfo:     { flex: 1 },
  syncTitle:    { fontSize: 13, color: AMBER, fontFamily: "SpaceGrotesk-SemiBold" },
  syncSub:      { fontSize: 11, color: "rgba(255,171,0,0.5)", fontFamily: "JetBrainsMono-Regular", marginTop: 2 },
  syncDot:      { width: 8, height: 8, borderRadius: 4, backgroundColor: AMBER },
  menuCard:     { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 14 },
  menuRow:      { flexDirection: "row", alignItems: "center", gap: 10, paddingVertical: 12, borderBottomWidth: 1, borderBottomColor: BORDER },
  menuLabel:    { flex: 1, fontSize: 13, color: "rgba(255,255,255,0.7)" },
  menuValue:    { fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)" },
  signOutBtn:   { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 8, borderRadius: 12, borderWidth: 1, borderColor: "rgba(255,59,92,0.2)", backgroundColor: "rgba(255,59,92,0.06)", paddingVertical: 14 },
  signOutText:     { fontSize: 14, color: RED, fontFamily: "SpaceGrotesk-SemiBold" },
  driverTypeBadge: { alignSelf: "flex-start", marginTop: 4, paddingHorizontal: 8, paddingVertical: 2, borderRadius: 8, borderWidth: 1 },
  driverTypePart:  { backgroundColor: "rgba(255,171,0,0.08)", borderColor: "rgba(255,171,0,0.25)" },
  driverTypeFull:  { backgroundColor: "rgba(0,229,255,0.08)", borderColor: "rgba(0,229,255,0.2)" },
  driverTypeText:  { fontSize: 9, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 0.5 },
  // Compliance
  compBanner:      { marginBottom: 0 },
  docCard:         { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 14 },
  docRow:          { flexDirection: "row", alignItems: "center", gap: 10, paddingVertical: 10, borderBottomWidth: 1, borderBottomColor: BORDER },
  docRowMissing:   { opacity: 0.65 },
  docRowOk:        {},
  docRowWarn:      {},
  docRowReview:    {},
  docDot:          { width: 8, height: 8, borderRadius: 4 },
  docDotMissing:   { backgroundColor: "rgba(255,255,255,0.15)" },
  docDotOk:        { backgroundColor: GREEN },
  docDotWarn:      { backgroundColor: AMBER },
  docDotReview:    { backgroundColor: CYAN },
  docRowInfo:      { flex: 1 },
  docRowName:      { fontSize: 13, color: "#FFFFFF", fontFamily: "SpaceGrotesk-SemiBold" },
  docRowSub:       { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", marginTop: 2 },
});
