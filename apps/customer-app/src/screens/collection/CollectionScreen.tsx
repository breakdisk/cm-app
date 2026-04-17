/**
 * CollectionScreen — pickup tracking
 * Shown after booking. Customer sees their scheduled pickup status,
 * QR code for driver to scan at the door, and driver ETA once assigned.
 *
 * Navigation params: { awb: string; type: "local" | "international" }
 * Navigation: navigate("Collection", { awb, type })
 */
import React, { useState, useEffect, useCallback, useRef } from "react";
import {
  View, Text, StyleSheet, ScrollView, Pressable,
  ActivityIndicator, RefreshControl, Linking, Alert, Animated,
  Share, TextInput, Keyboard,
} from "react-native";
import { useSafeAreaInsets } from "react-native-safe-area-context";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useSelector } from "react-redux";
import { useNetInfo } from "@react-native-community/netinfo";
import { AwbQRCode } from "../../components/AwbQRCode";
import { FadeInView } from "../../components/FadeInView";
import { trackingApi, type PublicTrackingData } from "../../services/api/tracking";
import type { RootState } from "../../store";
import { formatDate } from "../../utils/formatting";

// ── Design tokens ─────────────────────────────────────────────────────────────
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

// ── Pickup status progression ──────────────────────────────────────────────────
type PickupStatus =
  | "confirmed"        // just booked
  | "driver_assigned"  // driver was dispatched
  | "driver_en_route"  // driver is moving toward pickup
  | "driver_arrived"   // driver is at the door
  | "picked_up";       // driver scanned QR / confirmed pickup

const STATUS_STEPS: { key: PickupStatus; label: string; icon: string }[] = [
  { key: "confirmed",       label: "Confirmed",     icon: "checkmark-circle-outline" },
  { key: "driver_assigned", label: "Driver Ready",  icon: "person-outline"           },
  { key: "driver_en_route", label: "Driver Coming", icon: "car-outline"              },
  { key: "driver_arrived",  label: "At Your Door",  icon: "location-outline"         },
  { key: "picked_up",       label: "Collected",     icon: "archive-outline"          },
];

/** Map server-side status strings → PickupStatus */
function toPickupStatus(serverStatus: string): PickupStatus {
  const s = serverStatus.toLowerCase();
  if (s === "picked_up" || s === "in_transit") return "picked_up";
  if (s === "driver_arrived")                  return "driver_arrived";
  if (s === "driver_en_route")                 return "driver_en_route";
  if (s === "driver_assigned")                 return "driver_assigned";
  return "confirmed";
}

function stepIndex(status: PickupStatus): number {
  return STATUS_STEPS.findIndex(s => s.key === status);
}

// ── Navigation props ─────────────────────────────────────────────────────────
interface CollectionScreenProps {
  route: {
    params: {
      awb: string;
      type?: "local" | "international";
    };
  };
  navigation: any;
}

// ── Pulse dot for "active" step ───────────────────────────────────────────────
function PulseDot({ color }: { color: string }) {
  const scale = useRef(new Animated.Value(1)).current;
  useEffect(() => {
    Animated.loop(
      Animated.sequence([
        Animated.timing(scale, { toValue: 1.35, duration: 800, useNativeDriver: true }),
        Animated.timing(scale, { toValue: 1,    duration: 800, useNativeDriver: true }),
      ])
    ).start();
  }, [scale]);
  return (
    <Animated.View style={{ transform: [{ scale }] }}>
      <View style={{ width: 10, height: 10, borderRadius: 5, backgroundColor: color }} />
    </Animated.View>
  );
}

export function CollectionScreen({ route, navigation }: CollectionScreenProps) {
  const insets = useSafeAreaInsets();
  const { isConnected } = useNetInfo();
  const { awb, type = "local" } = route.params;
  const accent = type === "international" ? PURPLE : CYAN;

  const [trackData, setTrackData]     = useState<PublicTrackingData | null>(null);
  const [loading, setLoading]         = useState(true);
  const [refreshing, setRefreshing]   = useState(false);
  const [error, setError]             = useState<string | null>(null);
  const [pickupStatus, setPickupStatus] = useState<PickupStatus>("confirmed");
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Pickup receipt — Email + Share state
  const profileEmail = useSelector((st: RootState) => st.auth.email ?? "");
  const [emailExpanded, setEmailExpanded] = useState(false);
  const [emailValue, setEmailValue]       = useState("");
  const [emailSending, setEmailSending]   = useState(false);
  const [emailSent, setEmailSent]         = useState(false);
  const emailFadeAnim = useRef(new Animated.Value(0)).current;

  useEffect(() => { if (profileEmail && !emailValue) setEmailValue(profileEmail); }, [profileEmail]);
  useEffect(() => {
    Animated.timing(emailFadeAnim, {
      toValue: emailExpanded ? 1 : 0,
      duration: 220,
      useNativeDriver: true,
    }).start();
  }, [emailExpanded, emailFadeAnim]);

  const fetchTracking = useCallback(async (quiet = false) => {
    if (!quiet) setLoading(true);
    setError(null);
    try {
      const res = await trackingApi.getByTrackingNumber(awb);
      const data = (res.data as any)?.data ?? res.data;
      setTrackData(data);
      setPickupStatus(toPickupStatus(data.status ?? "confirmed"));
    } catch (err: any) {
      if (!quiet) setError(err?.message ?? "Could not load tracking data.");
    } finally {
      setLoading(false);
      setRefreshing(false);
    }
  }, [awb]);

  // Initial load + 20s poll
  useEffect(() => {
    fetchTracking();
    pollRef.current = setInterval(() => fetchTracking(true), 20_000);
    return () => { if (pollRef.current) clearInterval(pollRef.current); };
  }, [fetchTracking]);

  // Stop polling once picked up
  useEffect(() => {
    if (pickupStatus === "picked_up" && pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, [pickupStatus]);

  const onRefresh = useCallback(() => {
    setRefreshing(true);
    fetchTracking(true);
  }, [fetchTracking]);

  const currentStep = stepIndex(pickupStatus);
  const isComplete  = pickupStatus === "picked_up";
  const driver      = (trackData as any)?.driver;

  // Pickup receipt — derive timestamp from history, fall back to "just now"
  const pickupEvent  = trackData?.history?.find(e => e.status === "picked_up");
  const pickedUpAt   = pickupEvent?.occurred_at ?? null;
  const pickupOrigin = trackData?.origin ?? trackData?.origin_city ?? "";

  async function handleShareReceipt() {
    const lines = [
      `CargoMarket — Pickup Receipt`,
      `AWB: ${awb}`,
      `Status: Collected`,
      pickedUpAt ? `Picked up: ${formatDate(pickedUpAt, { time: true })}` : "",
      pickupOrigin ? `From: ${pickupOrigin}` : "",
      trackData?.destination ? `To: ${trackData.destination}` : "",
    ].filter(Boolean).join("\n");
    try {
      await Share.share({ message: lines, title: `Pickup Receipt — ${awb}` });
    } catch {}
  }

  async function handleEmailReceipt() {
    const email = emailValue.trim();
    if (!email || !email.includes("@")) {
      Alert.alert("Invalid Email", "Please enter a valid email address.");
      return;
    }
    Keyboard.dismiss();
    setEmailSending(true);
    try {
      await trackingApi.sendReceiptByEmail(awb, email);
      setEmailSent(true);
      setEmailExpanded(false);
    } catch (err: any) {
      const msg = err?.data?.error ?? err?.response?.data?.error ?? err?.message ?? "Could not send receipt. Please try again.";
      Alert.alert("Send Failed", msg);
    } finally {
      setEmailSending(false);
    }
  }

  return (
    <View style={{ flex: 1, backgroundColor: CANVAS }}>
      {/* ── Header ─────────────────────────────────────────────────────────── */}
      <LinearGradient
        colors={isComplete
          ? ["rgba(0,255,136,0.12)", CANVAS]
          : ["rgba(0,229,255,0.08)", CANVAS]}
        style={[s.header, { paddingTop: insets.top + 8 }]}
      >
        <Pressable onPress={() => navigation.goBack()} hitSlop={12} style={s.backBtn}>
          <Ionicons name="chevron-back" size={22} color="#FFF" />
        </Pressable>
        <View style={{ flex: 1 }}>
          <Text style={s.headerTitle}>Pickup Tracking</Text>
          <Text style={s.headerSub} numberOfLines={1}>{awb}</Text>
        </View>
        {!isConnected && (
          <View style={s.offlinePill}>
            <Ionicons name="cloud-offline-outline" size={12} color={AMBER} />
            <Text style={s.offlinePillText}>Offline</Text>
          </View>
        )}
      </LinearGradient>

      {loading ? (
        <View style={s.centered}>
          <ActivityIndicator color={accent} size="large" />
          <Text style={s.loadingText}>Loading pickup status…</Text>
        </View>
      ) : error ? (
        <View style={s.centered}>
          <Ionicons name="cloud-offline-outline" size={40} color="rgba(255,255,255,0.2)" />
          <Text style={s.errorText}>{error}</Text>
          <Pressable onPress={() => fetchTracking()} style={s.retryBtn}>
            <Text style={s.retryText}>Retry</Text>
          </Pressable>
        </View>
      ) : (
        <ScrollView
          contentContainerStyle={{ paddingBottom: insets.bottom + 32 }}
          refreshControl={<RefreshControl refreshing={refreshing} onRefresh={onRefresh} tintColor={accent} />}
        >
          {/* ── Status banner ─────────────────────────────────────────────── */}
          <FadeInView fromY={-8} style={s.statusBanner}>
            {isComplete ? (
              <View style={s.statusBannerInner}>
                <Ionicons name="checkmark-done-circle" size={20} color={GREEN} />
                <Text style={[s.statusBannerText, { color: GREEN }]}>Package Collected!</Text>
              </View>
            ) : (
              <View style={s.statusBannerInner}>
                <PulseDot color={accent} />
                <Text style={[s.statusBannerText, { color: accent }]}>
                  {STATUS_STEPS[currentStep]?.label ?? "Processing"}
                </Text>
                <Text style={s.statusBannerEta}>
                  {driver?.eta ?? trackData?.estimated_delivery ?? "ETA updating…"}
                </Text>
              </View>
            )}
          </FadeInView>

          {/* ── Progress stepper ──────────────────────────────────────────── */}
          <FadeInView delay={60} fromY={12} style={s.card}>
            <Text style={s.cardTitle}>Collection Progress</Text>
            <View style={s.stepperWrap}>
              {STATUS_STEPS.map((step, idx) => {
                const isDone    = idx < currentStep;
                const isActive  = idx === currentStep;
                const stepColor = isDone || isActive
                  ? (isComplete ? GREEN : accent)
                  : "rgba(255,255,255,0.15)";

                return (
                  <View key={step.key} style={s.stepRow}>
                    {/* Connector line above */}
                    {idx > 0 && (
                      <View style={[
                        s.stepLine,
                        { backgroundColor: idx <= currentStep ? stepColor : "rgba(255,255,255,0.1)" },
                      ]} />
                    )}
                    <View style={s.stepDotRow}>
                      {/* Circle */}
                      <View style={[s.stepCircle, { borderColor: stepColor, backgroundColor: isDone || isActive ? stepColor + "22" : "transparent" }]}>
                        {isDone ? (
                          <Ionicons name="checkmark" size={12} color={isComplete ? GREEN : accent} />
                        ) : isActive ? (
                          <PulseDot color={isComplete ? GREEN : accent} />
                        ) : (
                          <View style={{ width: 6, height: 6, borderRadius: 3, backgroundColor: "rgba(255,255,255,0.15)" }} />
                        )}
                      </View>
                      {/* Label */}
                      <View style={s.stepLabelWrap}>
                        <Text style={[s.stepLabel, { color: isDone || isActive ? "#FFF" : "rgba(255,255,255,0.3)" }]}>
                          {step.label}
                        </Text>
                        {isActive && !isComplete && (
                          <Text style={[s.stepSubLabel, { color: accent }]}>In progress</Text>
                        )}
                        {isDone && (
                          <Text style={s.stepSubLabel}>Done</Text>
                        )}
                      </View>
                      {/* Icon */}
                      <Ionicons
                        name={step.icon as any}
                        size={16}
                        color={isDone || isActive ? stepColor : "rgba(255,255,255,0.15)"}
                      />
                    </View>
                  </View>
                );
              })}
            </View>
          </FadeInView>

          {/* ── QR Code card ──────────────────────────────────────────────── */}
          {!isComplete && (
            <FadeInView delay={120} fromY={12} style={s.card}>
              <Text style={s.cardTitle}>Pickup QR Code</Text>
              <Text style={s.cardSub}>Show this to your driver when they arrive</Text>
              <View style={s.qrCenter}>
                <AwbQRCode awb={awb} size={200} accent={accent} />
              </View>
            </FadeInView>
          )}

          {/* ── Driver card ───────────────────────────────────────────────── */}
          {driver ? (
            <FadeInView delay={180} fromY={12} style={s.card}>
              <Text style={s.cardTitle}>Your Driver</Text>
              <View style={s.driverRow}>
                <View style={[s.driverAvatar, { backgroundColor: accent + "22", borderColor: accent + "44" }]}>
                  <Ionicons name="person" size={22} color={accent} />
                </View>
                <View style={{ flex: 1, gap: 2 }}>
                  <Text style={s.driverName}>{driver.name ?? "Driver assigned"}</Text>
                  {driver.vehicle && (
                    <Text style={s.driverVehicle}>{driver.vehicle}</Text>
                  )}
                  {driver.eta && (
                    <View style={s.etaRow}>
                      <Ionicons name="time-outline" size={12} color={AMBER} />
                      <Text style={s.etaText}>ETA: {driver.eta}</Text>
                    </View>
                  )}
                </View>
                {driver.phone && (
                  <Pressable
                    onPress={() => Linking.openURL(`tel:${driver.phone}`)}
                    style={s.callBtn}
                    hitSlop={8}
                  >
                    <Ionicons name="call" size={18} color={GREEN} />
                  </Pressable>
                )}
              </View>
            </FadeInView>
          ) : !isComplete && (
            <FadeInView delay={180} fromY={12} style={s.card}>
              <View style={s.driverPending}>
                <Ionicons name="person-circle-outline" size={32} color="rgba(255,255,255,0.2)" />
                <Text style={s.driverPendingText}>Driver will be assigned shortly</Text>
              </View>
            </FadeInView>
          )}

          {/* ── Collection instructions ───────────────────────────────────── */}
          {!isComplete && (
            <FadeInView delay={240} fromY={12} style={s.card}>
              <Text style={s.cardTitle}>Preparing for Pickup</Text>
              {[
                { icon: "checkmark-circle-outline", color: GREEN,  text: "Have your package sealed and labeled" },
                { icon: "qr-code-outline",          color: CYAN,   text: "Show your QR code to the driver" },
                { icon: "camera-outline",           color: PURPLE, text: "Driver will take a photo as proof of collection" },
                { icon: "document-text-outline",    color: AMBER,  text: "You'll receive a collection confirmation SMS" },
              ].map((item, i) => (
                <View key={i} style={s.instructionRow}>
                  <Ionicons name={item.icon as any} size={16} color={item.color} />
                  <Text style={s.instructionText}>{item.text}</Text>
                </View>
              ))}
            </FadeInView>
          )}

          {/* ── Collection complete celebration + receipt ─────────────────── */}
          {isComplete && (
            <>
              <FadeInView delay={60} fromY={12} style={[s.card, s.successCard]}>
                <LinearGradient
                  colors={["rgba(0,255,136,0.12)", "rgba(0,255,136,0.03)"]}
                  style={s.successGrad}
                >
                  <Ionicons name="checkmark-done-circle" size={48} color={GREEN} />
                  <Text style={s.successTitle}>Package Collected!</Text>
                  {pickedUpAt && (
                    <Text style={s.successTime}>{formatDate(pickedUpAt, { time: true })}</Text>
                  )}
                  <Text style={s.successSub}>
                    Your package is now with our courier and will be processed at the hub.
                  </Text>
                </LinearGradient>
              </FadeInView>

              {/* Pickup Receipt */}
              <FadeInView delay={120} fromY={12} style={s.card}>
                <Text style={s.cardTitle}>Pickup Receipt</Text>
                <View style={s.receiptDivider} />
                <View style={s.receiptRow}>
                  <Text style={s.receiptLabel}>AWB</Text>
                  <Text style={[s.receiptValue, s.receiptMono]}>{awb}</Text>
                </View>
                <View style={s.receiptRow}>
                  <Text style={s.receiptLabel}>Status</Text>
                  <Text style={[s.receiptValue, { color: GREEN }]}>Collected</Text>
                </View>
                {pickedUpAt && (
                  <View style={s.receiptRow}>
                    <Text style={s.receiptLabel}>Picked up</Text>
                    <Text style={s.receiptValue}>{formatDate(pickedUpAt, { time: true })}</Text>
                  </View>
                )}
                {pickupOrigin ? (
                  <View style={s.receiptRow}>
                    <Text style={s.receiptLabel}>From</Text>
                    <Text style={s.receiptValue} numberOfLines={2}>{pickupOrigin}</Text>
                  </View>
                ) : null}
                {trackData?.destination ? (
                  <View style={s.receiptRow}>
                    <Text style={s.receiptLabel}>To</Text>
                    <Text style={s.receiptValue} numberOfLines={2}>{trackData.destination}</Text>
                  </View>
                ) : null}

                {/* Action buttons */}
                <View style={s.receiptActionsRow}>
                  <Pressable onPress={handleShareReceipt} style={s.receiptActionBtn}>
                    <Ionicons name="share-outline" size={16} color={CYAN} />
                    <Text style={[s.receiptActionText, { color: CYAN }]}>Share</Text>
                  </Pressable>
                  <Pressable
                    onPress={() => { setEmailExpanded(v => !v); setEmailSent(false); }}
                    style={s.receiptActionBtn}
                  >
                    <Ionicons name="mail-outline" size={16} color={PURPLE} />
                    <Text style={[s.receiptActionText, { color: PURPLE }]}>Email</Text>
                    <Ionicons
                      name={emailExpanded ? "chevron-up" : "chevron-down"}
                      size={12} color="rgba(255,255,255,0.35)"
                    />
                  </Pressable>
                </View>

                {/* Email confirmation banner */}
                {emailSent && (
                  <View style={s.emailSentBanner}>
                    <Ionicons name="checkmark-circle" size={16} color={GREEN} />
                    <Text style={[s.emailSentText, { color: GREEN }]}>Sent to {emailValue}</Text>
                  </View>
                )}

                {/* Email input — collapsible */}
                <Animated.View style={{ opacity: emailFadeAnim, overflow: "hidden", maxHeight: emailExpanded ? 60 : 0 }}>
                  <View style={s.emailInputRow}>
                    <TextInput
                      style={s.emailInput}
                      value={emailValue}
                      onChangeText={setEmailValue}
                      placeholder="your@email.com"
                      placeholderTextColor="rgba(255,255,255,0.25)"
                      keyboardType="email-address"
                      autoCapitalize="none"
                      autoCorrect={false}
                    />
                    <Pressable
                      onPress={handleEmailReceipt}
                      disabled={emailSending}
                      style={[s.emailSendBtn, { opacity: emailSending ? 0.6 : 1 }]}
                    >
                      {emailSending ? (
                        <ActivityIndicator size="small" color={CANVAS} />
                      ) : (
                        <Ionicons name="send" size={14} color={CANVAS} />
                      )}
                    </Pressable>
                  </View>
                </Animated.View>

                {/* Track shipment CTA */}
                <Pressable
                  onPress={() => navigation.navigate("Tabs", { screen: "Track" })}
                  style={s.trackNowBtn}
                >
                  <Ionicons name="locate-outline" size={16} color={CANVAS} />
                  <Text style={s.trackNowText}>Track Shipment</Text>
                </Pressable>
              </FadeInView>
            </>
          )}

          {/* ── Shipment events timeline ───────────────────────────────────── */}
          {trackData?.history && trackData.history.length > 0 && (
            <FadeInView delay={300} fromY={12} style={s.card}>
              <Text style={s.cardTitle}>Event History</Text>
              {[...trackData.history].reverse().map((event, i) => (
                <View key={i} style={s.eventRow}>
                  <View style={s.eventDot} />
                  <View style={{ flex: 1, gap: 2 }}>
                    <Text style={s.eventStatus}>{event.status.replace(/_/g, " ")}</Text>
                    {event.description && (
                      <Text style={s.eventDesc}>{event.description}</Text>
                    )}
                    <Text style={s.eventTime}>{event.occurred_at}</Text>
                  </View>
                </View>
              ))}
            </FadeInView>
          )}
        </ScrollView>
      )}
    </View>
  );
}

const s = StyleSheet.create({
  header:       { paddingHorizontal: 16, paddingBottom: 16, flexDirection: "row", alignItems: "center", gap: 12 },
  backBtn:      { width: 36, height: 36, alignItems: "center", justifyContent: "center" },
  headerTitle:  { fontSize: 16, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  headerSub:    { fontSize: 11, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular", marginTop: 1 },
  offlinePill:  { flexDirection: "row", alignItems: "center", gap: 4, backgroundColor: "rgba(255,171,0,0.12)", borderRadius: 10, paddingHorizontal: 8, paddingVertical: 4, borderWidth: 1, borderColor: "rgba(255,171,0,0.25)" },
  offlinePillText: { fontSize: 10, color: AMBER, fontFamily: "JetBrainsMono-Regular" },

  centered:    { flex: 1, alignItems: "center", justifyContent: "center", gap: 12, padding: 24 },
  loadingText: { fontSize: 13, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular" },
  errorText:   { fontSize: 14, color: "rgba(255,255,255,0.5)", textAlign: "center", lineHeight: 20 },
  retryBtn:    { marginTop: 8, paddingHorizontal: 20, paddingVertical: 10, borderRadius: 10, borderWidth: 1, borderColor: CYAN + "50" },
  retryText:   { color: CYAN, fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold" },

  statusBanner:       { marginHorizontal: 16, marginBottom: 12, borderRadius: 14, overflow: "hidden", borderWidth: 1, borderColor: BORDER },
  statusBannerInner:  { flexDirection: "row", alignItems: "center", gap: 10, padding: 14 },
  statusBannerText:   { flex: 1, fontSize: 15, fontWeight: "700", fontFamily: "SpaceGrotesk-Bold" },
  statusBannerEta:    { fontSize: 11, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular" },

  card:         { marginHorizontal: 16, marginBottom: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 20, gap: 12 },
  cardTitle:    { fontSize: 11, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1 },
  cardSub:      { fontSize: 12, color: "rgba(255,255,255,0.4)", marginTop: -6 },

  stepperWrap:  { gap: 0 },
  stepRow:      { gap: 0 },
  stepLine:     { width: 2, height: 16, marginLeft: 12, borderRadius: 1 },
  stepDotRow:   { flexDirection: "row", alignItems: "center", gap: 12, paddingVertical: 4 },
  stepCircle:   { width: 26, height: 26, borderRadius: 13, borderWidth: 1.5, alignItems: "center", justifyContent: "center" },
  stepLabelWrap:{ flex: 1 },
  stepLabel:    { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold", fontWeight: "600" },
  stepSubLabel: { fontSize: 10, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular", marginTop: 1 },

  qrCenter:     { alignItems: "center", paddingVertical: 8 },

  driverRow:    { flexDirection: "row", alignItems: "center", gap: 12 },
  driverAvatar: { width: 48, height: 48, borderRadius: 24, borderWidth: 1.5, alignItems: "center", justifyContent: "center" },
  driverName:   { fontSize: 15, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  driverVehicle:{ fontSize: 12, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular" },
  etaRow:       { flexDirection: "row", alignItems: "center", gap: 4, marginTop: 2 },
  etaText:      { fontSize: 11, color: AMBER, fontFamily: "JetBrainsMono-Regular" },
  callBtn:      { width: 40, height: 40, borderRadius: 20, backgroundColor: "rgba(0,255,136,0.12)", borderWidth: 1, borderColor: "rgba(0,255,136,0.3)", alignItems: "center", justifyContent: "center" },

  driverPending:      { flexDirection: "row", alignItems: "center", gap: 12, paddingVertical: 4 },
  driverPendingText:  { fontSize: 13, color: "rgba(255,255,255,0.4)", flex: 1 },

  instructionRow:   { flexDirection: "row", alignItems: "flex-start", gap: 10 },
  instructionText:  { flex: 1, fontSize: 13, color: "rgba(255,255,255,0.55)", lineHeight: 19 },

  successCard:  { padding: 0, overflow: "hidden" },
  successGrad:  { alignItems: "center", gap: 8, padding: 28 },
  successTitle: { fontSize: 20, fontWeight: "700", color: GREEN, fontFamily: "SpaceGrotesk-Bold" },
  successTime:  { fontSize: 11, color: "rgba(255,255,255,0.5)", fontFamily: "JetBrainsMono-Regular" },
  successSub:   { fontSize: 13, color: "rgba(255,255,255,0.55)", textAlign: "center", lineHeight: 20, marginTop: 4 },
  trackNowBtn:  { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 8, marginTop: 8, backgroundColor: GREEN, borderRadius: 12, paddingHorizontal: 20, paddingVertical: 12 },
  trackNowText: { fontSize: 14, fontWeight: "700", color: CANVAS, fontFamily: "SpaceGrotesk-Bold" },

  receiptDivider:    { height: 1, backgroundColor: BORDER, marginVertical: 2 },
  receiptRow:        { flexDirection: "row", alignItems: "flex-start", gap: 8, paddingVertical: 2 },
  receiptLabel:      { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.45)", fontFamily: "JetBrainsMono-Regular" },
  receiptValue:      { fontSize: 13, color: "#FFF", fontFamily: "SpaceGrotesk-Regular", flexShrink: 1, textAlign: "right", maxWidth: "60%" },
  receiptMono:       { fontFamily: "JetBrainsMono-Regular", letterSpacing: 0.5 },
  receiptActionsRow: { flexDirection: "row", gap: 10, marginTop: 8 },
  receiptActionBtn:  { flex: 1, flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6, paddingVertical: 11, borderRadius: 10, borderWidth: 1, borderColor: "rgba(255,255,255,0.08)", backgroundColor: "rgba(255,255,255,0.03)" },
  receiptActionText: { fontSize: 13, fontWeight: "600", fontFamily: "SpaceGrotesk-SemiBold" },
  emailInputRow:     { flexDirection: "row", alignItems: "center", gap: 8, marginTop: 4 },
  emailInput:        { flex: 1, height: 40, backgroundColor: "rgba(255,255,255,0.05)", borderRadius: 10, paddingHorizontal: 12, color: "#FFF", fontSize: 14, borderWidth: 1, borderColor: BORDER },
  emailSendBtn:      { width: 40, height: 40, borderRadius: 10, backgroundColor: PURPLE, alignItems: "center", justifyContent: "center" },
  emailSentBanner:   { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6, paddingVertical: 10, borderRadius: 10, borderWidth: 1, borderColor: GREEN + "40", backgroundColor: GREEN + "10", marginTop: 4 },
  emailSentText:     { fontSize: 12, fontWeight: "600", fontFamily: "SpaceGrotesk-SemiBold" },

  eventRow:   { flexDirection: "row", alignItems: "flex-start", gap: 12 },
  eventDot:   { width: 8, height: 8, borderRadius: 4, backgroundColor: CYAN, marginTop: 5 },
  eventStatus:{ fontSize: 13, color: "#FFF", fontWeight: "600", textTransform: "capitalize" },
  eventDesc:  { fontSize: 12, color: "rgba(255,255,255,0.45)" },
  eventTime:  { fontSize: 10, color: "rgba(255,255,255,0.25)", fontFamily: "JetBrainsMono-Regular" },
});
