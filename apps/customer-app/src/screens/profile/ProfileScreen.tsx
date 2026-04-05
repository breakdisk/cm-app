/**
 * Customer App — Profile Screen
 * Wired to Redux: auth, shipments, tracking history, notification prefs.
 */
import React, { useState } from "react";
import {
  View, Text, StyleSheet, ScrollView, Pressable, Switch, Alert,
} from "react-native";
import Animated, { FadeInDown, FadeInUp } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useSelector, useDispatch } from "react-redux";
import { useNavigation } from "@react-navigation/native";
import type { RootState, AppDispatch } from "../../store";
import { authActions, prefsActions } from "../../store";
import type { KycStatus } from "../../store";

const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const PURPLE  = "#A855F7";
const AMBER   = "#FFAB00";
const RED     = "#FF3B5C";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

// ── Loyalty tier logic ─────────────────────────────────────────────────────────

const TIERS = [
  { label: "Bronze",   min: 0,    max: 199,  color: "#CD7F32", icon: "ribbon-outline"    },
  { label: "Silver",   min: 200,  max: 499,  color: "#C0C0C0", icon: "ribbon-outline"    },
  { label: "Gold",     min: 500,  max: 999,  color: AMBER,     icon: "star-outline"      },
  { label: "Platinum", min: 1000, max: null, color: CYAN,      icon: "diamond-outline"   },
];

function getTier(pts: number) {
  return TIERS.find(t => pts >= t.min && (t.max === null || pts <= t.max)) ?? TIERS[0];
}

function getNextTier(pts: number) {
  const idx = TIERS.findIndex(t => pts >= t.min && (t.max === null || pts <= t.max));
  return idx < TIERS.length - 1 ? TIERS[idx + 1] : null;
}

// ── KYC badge ─────────────────────────────────────────────────────────────────

const KYC_CONFIG: Record<KycStatus, { label: string; color: string; icon: string }> = {
  none:     { label: "Not Verified",         color: "rgba(255,255,255,0.3)", icon: "shield-outline"            },
  pending:  { label: "Verification Pending", color: AMBER,                  icon: "time-outline"              },
  verified: { label: "Identity Verified",    color: GREEN,                  icon: "shield-checkmark-outline"  },
  rejected: { label: "Verification Failed",  color: RED,                    icon: "shield-outline"            },
};

// ── Main screen ────────────────────────────────────────────────────────────────

// ── Terms & Privacy Modal ─────────────────────────────────────────────────────

const TERMS_SECTIONS = [
  {
    title: "1. Acceptance of Terms",
    body: "By downloading, installing, or using the LogisticOS Customer App, you agree to be bound by these Terms of Service and our Privacy Policy. If you do not agree, please discontinue use of the app immediately.",
  },
  {
    title: "2. Service Description",
    body: "LogisticOS provides last-mile delivery logistics services including shipment booking, real-time tracking, cash-on-delivery (COD), and Balikbayan Box international freight. Service availability may vary by region.",
  },
  {
    title: "3. Account & Verification",
    body: "You must provide accurate personal information during registration. Identity verification (KYC) is required for international shipments and COD services. You are responsible for maintaining the confidentiality of your account credentials.",
  },
  {
    title: "4. Shipment & Liability",
    body: "LogisticOS is not liable for delays caused by force majeure, incorrect addresses, or customs clearance. Prohibited items (firearms, hazardous materials, perishables without declaration) are strictly not allowed. Declared value is used as the basis for any claims.",
  },
  {
    title: "5. Cash on Delivery (COD)",
    body: "COD is available for domestic shipments only. The declared COD amount must match the actual amount to be collected. LogisticOS remits COD collections to merchants within 3–5 business days after successful delivery.",
  },
  {
    title: "6. Loyalty Program",
    body: "Loyalty points are earned per completed booking and have no cash value. Points may be redeemed for shipping discounts per the current reward schedule. LogisticOS reserves the right to modify the loyalty program with 30 days' notice.",
  },
  {
    title: "7. Privacy & Data Collection",
    body: "We collect your name, phone number, email, and shipment data to provide our services. Location data is used only during active deliveries. We do not sell your personal data to third parties. You may request data deletion by contacting support.",
  },
  {
    title: "8. Data Retention",
    body: "Shipment records are retained for 5 years for regulatory compliance. Account data is deleted within 30 days of account closure. Anonymised analytics data may be retained indefinitely for service improvement.",
  },
  {
    title: "9. Cookies & Tracking",
    body: "The app uses session tokens for authentication and anonymised analytics (crash reporting, feature usage). No advertising trackers are used. Push notification tokens are stored only to deliver service alerts you have opted into.",
  },
  {
    title: "10. Governing Law",
    body: "These Terms are governed by the laws of the Republic of the Philippines. Disputes shall be resolved in the courts of Pasig City, Metro Manila. For UAE and GCC operations, local consumer protection regulations additionally apply.",
  },
  {
    title: "11. Changes to Terms",
    body: "We may update these Terms at any time. Continued use of the app after changes constitutes acceptance. Significant changes will be communicated via in-app notification at least 14 days in advance.",
  },
  {
    title: "12. Contact",
    body: "For questions about these Terms or your data, contact us at: legal@logisticos.ph or through the Support tab in the app.",
  },
];

function TermsPanel({ visible, onClose }: { visible: boolean; onClose: () => void }) {
  const [tab, setTab] = useState<"terms" | "privacy">("terms");
  if (!visible) return null;
  return (
    <View style={tm.overlay}>
      <Pressable style={tm.backdrop} onPress={onClose} />
      <View style={tm.sheet}>
        {/* Header */}
        <View style={tm.header}>
          <Text style={tm.headerTitle}>Terms & Privacy</Text>
          <Pressable onPress={onClose} style={tm.closeBtn}>
            <Ionicons name="close" size={20} color="rgba(255,255,255,0.6)" />
          </Pressable>
        </View>

        {/* Tab switcher */}
        <View style={tm.tabRow}>
          <Pressable onPress={() => setTab("terms")} style={[tm.tabBtn, tab === "terms" && tm.tabActive]}>
            <Text style={[tm.tabText, tab === "terms" && { color: CYAN }]}>Terms of Service</Text>
          </Pressable>
          <Pressable onPress={() => setTab("privacy")} style={[tm.tabBtn, tab === "privacy" && tm.tabActive]}>
            <Text style={[tm.tabText, tab === "privacy" && { color: CYAN }]}>Privacy Policy</Text>
          </Pressable>
        </View>

        <ScrollView contentContainerStyle={{ padding: 20, paddingBottom: 40 }}>
          {tab === "terms" ? (
            <>
              <Text style={tm.effectiveDate}>Effective: 1 March 2026 · Version 1.0</Text>
              {TERMS_SECTIONS.slice(0, 6).map((sec) => (
                <View key={sec.title} style={{ marginBottom: 18 }}>
                  <Text style={tm.secTitle}>{sec.title}</Text>
                  <Text style={tm.secBody}>{sec.body}</Text>
                </View>
              ))}
            </>
          ) : (
            <>
              <Text style={tm.effectiveDate}>Effective: 1 March 2026 · GDPR & PDPA Compliant</Text>
              {TERMS_SECTIONS.slice(6).map((sec) => (
                <View key={sec.title} style={{ marginBottom: 18 }}>
                  <Text style={tm.secTitle}>{sec.title}</Text>
                  <Text style={tm.secBody}>{sec.body}</Text>
                </View>
              ))}
              <View style={tm.rightsCard}>
                <Text style={tm.rightsTitle}>Your Data Rights</Text>
                {[
                  { icon: "eye-outline",       right: "Access",  desc: "Request a copy of your data"     },
                  { icon: "create-outline",    right: "Correct", desc: "Fix inaccurate information"       },
                  { icon: "trash-outline",     right: "Delete",  desc: "Request account & data deletion"  },
                  { icon: "download-outline",  right: "Export",  desc: "Receive your data in JSON format" },
                  { icon: "hand-left-outline", right: "Object",  desc: "Opt out of marketing processing"  },
                ].map((r) => (
                  <View key={r.right} style={tm.rightRow}>
                    <View style={tm.rightIcon}>
                      <Ionicons name={r.icon as any} size={14} color={CYAN} />
                    </View>
                    <View style={{ flex: 1 }}>
                      <Text style={tm.rightLabel}>{r.right}</Text>
                      <Text style={tm.rightDesc}>{r.desc}</Text>
                    </View>
                  </View>
                ))}
              </View>
            </>
          )}
        </ScrollView>
      </View>
    </View>
  );
}

const tm = StyleSheet.create({
  overlay:        { position: "absolute", top: 0, left: 0, right: 0, bottom: 0, justifyContent: "flex-end", zIndex: 100 },
  backdrop:       { position: "absolute", top: 0, left: 0, right: 0, bottom: 0, backgroundColor: "rgba(0,0,0,0.75)" },
  sheet:          { backgroundColor: "#0A0E1A", borderTopLeftRadius: 24, borderTopRightRadius: 24, borderWidth: 1, borderColor: "rgba(255,255,255,0.08)", maxHeight: "88%", flexShrink: 1 },
  header:         { flexDirection: "row", alignItems: "center", paddingHorizontal: 20, paddingTop: 20, paddingBottom: 12, borderBottomWidth: 1, borderBottomColor: "rgba(255,255,255,0.06)" },
  headerTitle:    { flex: 1, fontSize: 17, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  closeBtn:       { width: 32, height: 32, borderRadius: 16, backgroundColor: "rgba(255,255,255,0.06)", alignItems: "center", justifyContent: "center" },
  tabRow:         { flexDirection: "row", marginHorizontal: 20, marginTop: 12, backgroundColor: "rgba(255,255,255,0.04)", borderRadius: 10, padding: 3 },
  tabBtn:         { flex: 1, paddingVertical: 8, alignItems: "center", borderRadius: 8 },
  tabActive:      { backgroundColor: "rgba(0,229,255,0.10)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)" },
  tabText:        { fontSize: 12, fontFamily: "SpaceGrotesk-SemiBold", color: "rgba(255,255,255,0.4)" },
  effectiveDate:  { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)", marginBottom: 4 },
  secTitle:       { fontSize: 13, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold", marginBottom: 6 },
  secBody:        { fontSize: 13, color: "rgba(255,255,255,0.5)", lineHeight: 21 },
  rightsCard:     { backgroundColor: "rgba(0,229,255,0.05)", borderWidth: 1, borderColor: "rgba(0,229,255,0.15)", borderRadius: 14, padding: 16, gap: 12 },
  rightsTitle:    { fontSize: 13, fontWeight: "700", color: CYAN, fontFamily: "SpaceGrotesk-SemiBold", marginBottom: 4 },
  rightRow:       { flexDirection: "row", alignItems: "flex-start", gap: 10 },
  rightIcon:      { width: 28, height: 28, borderRadius: 8, backgroundColor: "rgba(0,229,255,0.1)", alignItems: "center", justifyContent: "center" },
  rightLabel:     { fontSize: 12, fontWeight: "600", color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold" },
  rightDesc:      { fontSize: 11, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular", marginTop: 1 },
});

// ── Main screen ────────────────────────────────────────────────────────────────

export function ProfileScreen() {
  const dispatch   = useDispatch<AppDispatch>();
  const navigation = useNavigation<any>();

  const { name, phone, email, customerId, loyaltyPts, isGuest, kycStatus, verificationTier } =
    useSelector((s: RootState) => s.auth);
  const { notifDelivery, notifPromos } = useSelector((s: RootState) => s.prefs);
  const shipments     = useSelector((s: RootState) => s.shipments.list);
  const trackHistory  = useSelector((s: RootState) => s.tracking.history);

  const [showPersonalInfo, setShowPersonalInfo] = useState(false);
  const [showTerms,        setShowTerms]        = useState(false);

  const tier     = getTier(loyaltyPts);
  const nextTier = getNextTier(loyaltyPts);
  const progress = nextTier
    ? Math.min(((loyaltyPts - tier.min) / (nextTier.min - tier.min)) * 100, 100)
    : 100;

  const activeShipments = shipments.filter(
    s => !["delivered", "returned", "cancelled"].includes(s.status)
  ).length;
  const kycCfg = KYC_CONFIG[kycStatus];

  function handleSignOut() {
    Alert.alert("Sign Out", "Are you sure?", [
      { text: "Cancel", style: "cancel" },
      { text: "Sign Out", style: "destructive", onPress: () => dispatch(authActions.logout()) },
    ]);
  }

  // Demo: simulate KYC approval
  function handleDemoKycApprove() {
    Alert.alert("Demo", "Simulate KYC approval?", [
      { text: "Cancel", style: "cancel" },
      { text: "Approve", onPress: () => dispatch(authActions.approveKyc()) },
    ]);
  }

  return (
    <View style={{ flex: 1, backgroundColor: CANVAS }}>
    <ScrollView style={s.container} contentContainerStyle={{ paddingBottom: 40 }}>

      {/* Hero / Avatar */}
      <LinearGradient colors={["rgba(168,85,247,0.12)", "transparent"]} style={s.hero}>
        <Animated.View entering={FadeInDown.springify()} style={s.avatarRow}>
          <LinearGradient colors={[PURPLE, CYAN]} style={s.avatar}>
            <Text style={s.avatarInitial}>{name ? name[0].toUpperCase() : "?"}</Text>
          </LinearGradient>
          <View style={{ flex: 1 }}>
            <Text style={s.nameText}>{isGuest ? "Guest User" : (name ?? "Customer")}</Text>
            <Text style={s.phoneText}>{phone ?? "Not signed in"}</Text>
            {email ? <Text style={s.emailText}>{email}</Text> : null}
          </View>
          {/* KYC badge */}
          <View style={[s.kycBadge, { borderColor: kycCfg.color + "50" }]}>
            <Ionicons name={kycCfg.icon as any} size={11} color={kycCfg.color} />
            <Text style={[s.kycBadgeText, { color: kycCfg.color }]}>{kycCfg.label}</Text>
          </View>
        </Animated.View>
      </LinearGradient>

      {/* Loyalty card */}
      {!isGuest && (
        <Animated.View entering={FadeInUp.delay(80).springify()} style={s.loyaltyCard}>
          <LinearGradient colors={[PURPLE + "20", tier.color + "15"]} style={s.loyaltyGrad}>
            <View style={s.loyaltyRow}>
              <View>
                <Text style={s.loyaltyLabel}>Loyalty Points</Text>
                <Text style={s.loyaltyPts}>{loyaltyPts.toLocaleString()} pts</Text>
              </View>
              <View style={[s.tierBadge, { backgroundColor: tier.color + "25" }]}>
                <Ionicons name={tier.icon as any} size={13} color={tier.color} />
                <Text style={[s.tierText, { color: tier.color }]}>{tier.label}</Text>
              </View>
            </View>
            {nextTier && (
              <>
                <View style={s.progressBar}>
                  <View style={[s.progressFill, { width: `${progress}%` as any, backgroundColor: tier.color }]} />
                </View>
                <Text style={s.progressLabel}>
                  {nextTier.min - loyaltyPts} pts to {nextTier.label}
                </Text>
              </>
            )}
            {!nextTier && (
              <Text style={[s.progressLabel, { color: CYAN }]}>Maximum tier reached!</Text>
            )}
          </LinearGradient>
        </Animated.View>
      )}

      {/* Stats row */}
      {!isGuest && (
        <Animated.View entering={FadeInUp.delay(120).springify()} style={s.statsRow}>
          {[
            { value: shipments.length,   label: "Shipments",  color: CYAN   },
            { value: activeShipments,    label: "Active",     color: GREEN  },
            { value: trackHistory.length, label: "Tracked",   color: PURPLE },
          ].map((st) => (
            <View key={st.label} style={s.statCard}>
              <Text style={[s.statValue, { color: st.color }]}>{st.value}</Text>
              <Text style={s.statLabel}>{st.label}</Text>
            </View>
          ))}
        </Animated.View>
      )}

      {/* Personal Info */}
      {!isGuest && (
        <Animated.View entering={FadeInUp.delay(140).springify()} style={s.section}>
          <Pressable onPress={() => setShowPersonalInfo(v => !v)} style={s.menuRow}>
            <View style={[s.menuIcon, { backgroundColor: CYAN + "20" }]}>
              <Ionicons name="person-outline" size={16} color={CYAN} />
            </View>
            <Text style={s.menuLabel}>Personal Info</Text>
            <Ionicons name={showPersonalInfo ? "chevron-up" : "chevron-down"} size={14} color="rgba(255,255,255,0.25)" />
          </Pressable>
          {showPersonalInfo && (
            <Animated.View entering={FadeInDown.duration(180)} style={s.infoBlock}>
              {[
                { label: "Name",        value: name       ?? "—" },
                { label: "Phone",       value: phone      ?? "—" },
                { label: "Email",       value: email      ?? "—" },
                { label: "Customer ID", value: customerId ?? "—" },
                { label: "KYC Tier",    value: verificationTier.replace("_", " ") },
              ].map((row) => (
                <View key={row.label} style={s.infoRow}>
                  <Text style={s.infoLabel}>{row.label}</Text>
                  <Text style={s.infoValue}>{row.value}</Text>
                </View>
              ))}
            </Animated.View>
          )}
        </Animated.View>
      )}

      {/* KYC / Identity */}
      {!isGuest && (
        <Animated.View entering={FadeInUp.delay(160).springify()} style={s.section}>
          <Pressable
            onPress={kycStatus === "pending" ? handleDemoKycApprove : undefined}
            style={[s.menuRow, { borderColor: kycCfg.color + "30" }]}
          >
            <View style={[s.menuIcon, { backgroundColor: kycCfg.color + "18" }]}>
              <Ionicons name={kycCfg.icon as any} size={16} color={kycCfg.color} />
            </View>
            <View style={{ flex: 1 }}>
              <Text style={s.menuLabel}>Identity Verification</Text>
              <Text style={[s.menuSub, { color: kycCfg.color }]}>{kycCfg.label}</Text>
            </View>
            {kycStatus === "pending" && (
              <View style={s.demoPill}>
                <Text style={s.demoPillText}>Tap to approve (demo)</Text>
              </View>
            )}
            {kycStatus === "verified" && <Ionicons name="checkmark-circle" size={18} color={GREEN} />}
          </Pressable>
        </Animated.View>
      )}

      {/* Notifications */}
      <Animated.View entering={FadeInUp.delay(180).springify()} style={s.section}>
        <Text style={s.sectionTitle}>Notifications</Text>
        {([
          { label: "Delivery updates", sub: "Status changes, ETA alerts", value: notifDelivery, action: (v: boolean) => { dispatch(prefsActions.setNotifDelivery(v)); } },
          { label: "Promotions & offers", sub: "Discount codes, loyalty rewards", value: notifPromos, action: (v: boolean) => { dispatch(prefsActions.setNotifPromos(v)); } },
        ] as const).map((n) => (
          <View key={n.label} style={s.toggleRow}>
            <View style={{ flex: 1 }}>
              <Text style={s.toggleLabel}>{n.label}</Text>
              <Text style={s.toggleSub}>{n.sub}</Text>
            </View>
            <Switch
              value={n.value}
              onValueChange={n.action}
              trackColor={{ false: BORDER, true: CYAN + "60" }}
              thumbColor={n.value ? CYAN : "rgba(255,255,255,0.3)"}
            />
          </View>
        ))}
      </Animated.View>

      {/* Account links */}
      <Animated.View entering={FadeInUp.delay(200).springify()} style={s.section}>
        <Text style={s.sectionTitle}>Account</Text>
        {[
          { icon: "card-outline",         label: "Saved Addresses",  sub: `${shipments.length} locations used`,   color: PURPLE },
          { icon: "wallet-outline",       label: "Payment Methods",  sub: "Add credit/debit card",               color: GREEN  },
          { icon: "shield-checkmark-outline", label: "Security",     sub: `Tier: ${verificationTier.replace("_"," ")}`, color: AMBER  },
        ].map((item) => (
          <Pressable key={item.label} style={({ pressed }) => [s.menuRow, { opacity: pressed ? 0.7 : 1 }]}>
            <View style={[s.menuIcon, { backgroundColor: item.color + "20" }]}>
              <Ionicons name={item.icon as any} size={16} color={item.color} />
            </View>
            <View style={{ flex: 1 }}>
              <Text style={s.menuLabel}>{item.label}</Text>
              <Text style={s.menuSub}>{item.sub}</Text>
            </View>
            <Ionicons name="chevron-forward" size={14} color="rgba(255,255,255,0.2)" />
          </Pressable>
        ))}
      </Animated.View>

      {/* Support & App */}
      <Animated.View entering={FadeInUp.delay(220).springify()} style={s.section}>
        <Text style={s.sectionTitle}>Help & App</Text>
        {[
          {
            icon: "chatbubble-ellipses-outline", label: "Chat with Support", sub: "AI agent + live chat",
            color: GREEN,  onPress: () => navigation.navigate("Support"),
          },
          {
            icon: "document-text-outline",  label: "Terms & Privacy", sub: "Read our policies",
            color: AMBER,  onPress: () => setShowTerms(true),
          },
          {
            icon: "information-circle-outline", label: "About LogisticOS", sub: "Version 1.0.0",
            color: CYAN,   onPress: undefined,
          },
        ].map((item) => (
          <Pressable key={item.label} onPress={item.onPress} style={({ pressed }) => [s.menuRow, { opacity: pressed ? 0.7 : 1 }]}>
            <View style={[s.menuIcon, { backgroundColor: item.color + "20" }]}>
              <Ionicons name={item.icon as any} size={16} color={item.color} />
            </View>
            <View style={{ flex: 1 }}>
              <Text style={s.menuLabel}>{item.label}</Text>
              <Text style={s.menuSub}>{item.sub}</Text>
            </View>
            <Ionicons name="chevron-forward" size={14} color="rgba(255,255,255,0.2)" />
          </Pressable>
        ))}
      </Animated.View>

      {/* Sign out */}
      <View style={s.section}>
        <Pressable onPress={handleSignOut} style={({ pressed }) => [s.signOutBtn, { opacity: pressed ? 0.7 : 1 }]}>
          <Ionicons name="log-out-outline" size={16} color={RED} />
          <Text style={s.signOutText}>{isGuest ? "Sign In" : "Sign Out"}</Text>
        </Pressable>
      </View>

    </ScrollView>
    <TermsPanel visible={showTerms} onClose={() => setShowTerms(false)} />
    </View>
  );
}

const s = StyleSheet.create({
  container:      { flex: 1, backgroundColor: CANVAS },
  hero:           { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 20 },
  avatarRow:      { flexDirection: "row", alignItems: "center", gap: 14 },
  avatar:         { width: 58, height: 58, borderRadius: 29, alignItems: "center", justifyContent: "center" },
  avatarInitial:  { fontSize: 22, fontWeight: "700", color: "#FFF" },
  nameText:       { fontSize: 17, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  phoneText:      { fontSize: 12, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular", marginTop: 2 },
  emailText:      { fontSize: 11, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", marginTop: 1 },
  kycBadge:       { flexDirection: "row", alignItems: "center", gap: 4, paddingHorizontal: 7, paddingVertical: 4, borderRadius: 8, borderWidth: 1, backgroundColor: GLASS },
  kycBadgeText:   { fontSize: 9, fontFamily: "JetBrainsMono-Regular" },

  loyaltyCard:    { marginHorizontal: 16, marginBottom: 12, borderRadius: 16, overflow: "hidden", borderWidth: 1, borderColor: BORDER },
  loyaltyGrad:    { padding: 16, gap: 10 },
  loyaltyRow:     { flexDirection: "row", justifyContent: "space-between", alignItems: "flex-start" },
  loyaltyLabel:   { fontSize: 10, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, marginBottom: 2 },
  loyaltyPts:     { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  tierBadge:      { flexDirection: "row", alignItems: "center", gap: 5, paddingHorizontal: 10, paddingVertical: 5, borderRadius: 20 },
  tierText:       { fontSize: 12, fontWeight: "600" },
  progressBar:    { height: 4, borderRadius: 2, backgroundColor: "rgba(255,255,255,0.08)" },
  progressFill:   { height: "100%", borderRadius: 2 },
  progressLabel:  { fontSize: 10, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular" },

  statsRow:       { flexDirection: "row", marginHorizontal: 16, marginBottom: 12, gap: 8 },
  statCard:       { flex: 1, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, padding: 12, alignItems: "center", gap: 4 },
  statValue:      { fontSize: 20, fontWeight: "700", fontFamily: "SpaceGrotesk-Bold" },
  statLabel:      { fontSize: 10, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular" },

  section:        { paddingHorizontal: 16, marginBottom: 12 },
  sectionTitle:   { fontSize: 10, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1.5, marginBottom: 8 },

  menuRow:        { flexDirection: "row", alignItems: "center", gap: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, padding: 14, marginBottom: 8 },
  menuIcon:       { width: 32, height: 32, borderRadius: 10, alignItems: "center", justifyContent: "center" },
  menuLabel:      { fontSize: 13, color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold" },
  menuSub:        { fontSize: 10, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular", marginTop: 1 },

  infoBlock:      { backgroundColor: "rgba(0,229,255,0.04)", borderWidth: 1, borderColor: "rgba(0,229,255,0.12)", borderRadius: 12, padding: 14, marginBottom: 8, gap: 10 },
  infoRow:        { flexDirection: "row", justifyContent: "space-between", alignItems: "center" },
  infoLabel:      { fontSize: 10, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 0.8 },
  infoValue:      { fontSize: 12, color: "#FFF", fontFamily: "JetBrainsMono-Regular" },

  demoPill:       { backgroundColor: "rgba(255,171,0,0.12)", borderWidth: 1, borderColor: "rgba(255,171,0,0.3)", borderRadius: 6, paddingHorizontal: 7, paddingVertical: 3 },
  demoPillText:   { fontSize: 9, color: AMBER, fontFamily: "JetBrainsMono-Regular" },

  toggleRow:      { flexDirection: "row", alignItems: "center", backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingHorizontal: 14, paddingVertical: 12, marginBottom: 8 },
  toggleLabel:    { fontSize: 13, color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold" },
  toggleSub:      { fontSize: 10, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular", marginTop: 2 },

  signOutBtn:     { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 8, backgroundColor: "rgba(255,59,92,0.08)", borderWidth: 1, borderColor: "rgba(255,59,92,0.2)", borderRadius: 12, padding: 14 },
  signOutText:    { fontSize: 14, fontWeight: "600", color: RED },
});
