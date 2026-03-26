/**
 * Driver App — Compliance Checklist Screen
 * Full-screen document checklist with status overview and navigation to upload.
 */
import { View, Text, StyleSheet, ScrollView, Pressable } from "react-native";
import { useSelector, useDispatch } from "react-redux";
import { useEffect } from "react";
import Animated, { FadeInDown } from "react-native-reanimated";
import { Ionicons } from "@expo/vector-icons";
import { router } from "expo-router";
import type { RootState, AppDispatch } from "../../store";
import type { SubmittedDoc } from "../../store";
import { complianceActions } from "../../store";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

const STATUS_LABEL: Record<string, { label: string; color: string; bg: string; border: string }> = {
  compliant:          { label: "All Clear",          color: GREEN, bg: "rgba(0,255,136,0.07)",  border: "rgba(0,255,136,0.2)"  },
  under_review:       { label: "Under Review",       color: CYAN,  bg: "rgba(0,229,255,0.07)",  border: "rgba(0,229,255,0.2)"  },
  expiring_soon:      { label: "Expiring Soon",      color: AMBER, bg: "rgba(255,171,0,0.08)",  border: "rgba(255,171,0,0.25)" },
  expired:            { label: "Expired",            color: AMBER, bg: "rgba(255,171,0,0.08)",  border: "rgba(255,171,0,0.25)" },
  suspended:          { label: "Suspended",          color: RED,   bg: "rgba(255,59,92,0.08)",  border: "rgba(255,59,92,0.25)" },
  pending_submission: { label: "Action Required",    color: AMBER, bg: "rgba(255,171,0,0.08)",  border: "rgba(255,171,0,0.25)" },
};

function docSubText(doc?: SubmittedDoc): string {
  if (!doc)                          return "Not submitted · Tap to upload";
  if (doc.status === "approved")     return `Approved · Exp ${doc.expiry_date ?? "—"}`;
  if (doc.status === "under_review") return "Under review · Est. 24h";
  if (doc.status === "submitted")    return "Submitted · Awaiting review";
  if (doc.status === "rejected")     return `Rejected — ${doc.rejection_reason ?? "see reason"}`;
  if (doc.status === "expired")      return "Expired · Renewal required";
  return "—";
}

function dotColor(status?: string): string {
  if (!status)                                       return "rgba(255,255,255,0.15)";
  if (status === "approved")                         return GREEN;
  if (status === "rejected" || status === "expired") return AMBER;
  return CYAN;
}

export default function ComplianceScreen() {
  const dispatch   = useDispatch<AppDispatch>();
  const compliance = useSelector((s: RootState) => s.compliance);
  const statusCfg  = STATUS_LABEL[compliance.overall_status] ?? STATUS_LABEL.pending_submission;

  // Seed mock data when navigating directly to this screen (store not yet populated)
  useEffect(() => {
    if (compliance.required_types.length > 0) return;
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

  return (
    <ScrollView style={styles.container} contentContainerStyle={{ paddingBottom: 40 }}>
      {/* Header */}
      <Animated.View entering={FadeInDown.springify()} style={styles.header}>
        <Pressable onPress={() => router.back()}>
          <Ionicons name="chevron-back" size={20} color="rgba(255,255,255,0.5)" />
        </Pressable>
        <View style={{ flex: 1, marginLeft: 8 }}>
          <Text style={styles.headerTitle}>Compliance</Text>
          <Text style={styles.headerSub}>{compliance.jurisdiction}</Text>
        </View>
      </Animated.View>

      {/* Overall status card */}
      <Animated.View
        entering={FadeInDown.delay(60).springify()}
        style={[styles.statusCard, { backgroundColor: statusCfg.bg, borderColor: statusCfg.border }]}
      >
        <Text style={[styles.statusLabel, { color: statusCfg.color }]}>{statusCfg.label}</Text>
        <Text style={[styles.statusSub, { color: statusCfg.color }]}>
          {compliance.documents.filter((d) => d.status === "approved").length}
          {" / "}
          {compliance.required_types.length} documents verified
        </Text>
      </Animated.View>

      {/* Document list */}
      <Animated.View entering={FadeInDown.delay(100).springify()} style={styles.docCard}>
        <Text style={styles.cardLabel}>Required Documents</Text>
        {compliance.required_types.map((dt, i) => {
          const doc = compliance.documents.find(
            (d) => d.document_type_id === dt.id && d.status !== "superseded"
          );
          const isLast = i === compliance.required_types.length - 1;
          return (
            <Pressable
              key={dt.id}
              onPress={() => router.push(`/compliance/upload/${dt.code}` as any)}
              style={({ pressed }) => [
                styles.docRow,
                !isLast && styles.docRowBorder,
                pressed && { opacity: 0.7 },
              ]}
            >
              <View style={[styles.docDot, { backgroundColor: dotColor(doc?.status) }]} />
              <View style={styles.docInfo}>
                <Text style={styles.docName}>{dt.name}</Text>
                <Text style={styles.docSub}>{docSubText(doc)}</Text>
                {doc?.status === "rejected" && (
                  <Text style={styles.docRejectNote}>Tap to re-upload</Text>
                )}
              </View>
              <Ionicons name="chevron-forward" size={14} color="rgba(255,255,255,0.2)" />
            </Pressable>
          );
        })}
      </Animated.View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container:    { flex: 1, backgroundColor: CANVAS },
  header:       { flexDirection: "row", alignItems: "center", padding: 16, paddingTop: 20 },
  headerTitle:  { fontSize: 16, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff" },
  headerSub:    { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", marginTop: 2 },
  statusCard:   { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, borderWidth: 1, padding: 16 },
  statusLabel:  { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", marginBottom: 4 },
  statusSub:    { fontSize: 11, fontFamily: "JetBrainsMono-Regular", opacity: 0.7 },
  docCard:      { marginHorizontal: 12, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 14 },
  cardLabel:    { fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, color: "rgba(255,255,255,0.3)", marginBottom: 12 },
  docRow:       { flexDirection: "row", alignItems: "center", gap: 12, paddingVertical: 12 },
  docRowBorder: { borderBottomWidth: 1, borderBottomColor: BORDER },
  docDot:       { width: 8, height: 8, borderRadius: 4, flexShrink: 0 },
  docInfo:      { flex: 1 },
  docName:      { fontSize: 13, color: "#FFFFFF", fontFamily: "SpaceGrotesk-SemiBold" },
  docSub:       { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", marginTop: 2 },
  docRejectNote:{ fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: AMBER, marginTop: 2 },
});
