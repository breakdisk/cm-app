/**
 * The recipient's delivery PIN, requested at handover.
 *
 * Never issues on its own. A PIN lives 15 minutes and each request replaces the
 * last, so an automatic issue would text a code that dies before the driver
 * arrives, or overwrite one the recipient is already holding. See
 * services/api/pod.ts.
 */
import React, { useEffect, useState } from "react";
import { View, Text, Pressable, ActivityIndicator, StyleSheet } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import {
  DELIVERY_PIN_TTL_MS,
  getStoredDeliveryPin,
  issueDeliveryPin,
  isPinLive,
  onDeliveryPinIssued,
  type DeliveryPin,
} from "../services/api/pod";

const CYAN  = "#00E5FF";
const AMBER = "#FFAB00";
const RED   = "#FF3B5C";

interface DeliveryPinCardProps {
  shipmentId:      string;
  recipientPhone?: string;
  compact?:        boolean;
}

function pinErrorMessage(err: any): string {
  if (err?.status === 403) return "This account can't request a PIN for this shipment.";
  if (err?.status === 404) return "This shipment wasn't found.";
  return "Couldn't get a PIN right now. Try again.";
}

function timeLeft(pin: DeliveryPin, now: number): string {
  const ms = Math.max(0, pin.issuedAt + DELIVERY_PIN_TTL_MS - now);
  const minutes = Math.floor(ms / 60_000);
  const seconds = Math.floor((ms % 60_000) / 1000);
  return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export function DeliveryPinCard({ shipmentId, recipientPhone, compact = false }: DeliveryPinCardProps) {
  const [pin,     setPin]     = useState<DeliveryPin | null>(null);
  const [loaded,  setLoaded]  = useState(false);
  const [issuing, setIssuing] = useState(false);
  const [error,   setError]   = useState<string | null>(null);
  const [now,     setNow]     = useState(() => Date.now());

  useEffect(() => {
    let alive = true;
    setPin(null);
    setLoaded(false);
    setError(null);
    getStoredDeliveryPin(shipmentId).then(stored => {
      if (!alive) return;
      setPin(stored);
      setNow(Date.now());
      setLoaded(true);
    });
    const off = onDeliveryPinIssued((id, issued) => {
      if (!alive || id !== shipmentId) return;
      setPin(issued);
      setNow(Date.now());
      setError(null);
    });
    return () => {
      alive = false;
      off();
    };
  }, [shipmentId]);

  // Counts down while a PIN is showing, and stops once it has expired.
  useEffect(() => {
    if (!pin) return;
    const timer = setInterval(() => {
      const t = Date.now();
      setNow(t);
      if (!isPinLive(pin, t)) clearInterval(timer);
    }, 1000);
    return () => clearInterval(timer);
  }, [pin]);

  async function handleIssue() {
    setIssuing(true);
    setError(null);
    try {
      setPin(await issueDeliveryPin(shipmentId, recipientPhone ?? ""));
      setNow(Date.now());
    } catch (err) {
      setError(pinErrorMessage(err));
    } finally {
      setIssuing(false);
    }
  }

  if (!loaded) return null;

  const live = isPinLive(pin, now) ? pin : null;
  const half = live?.kind === "code" ? Math.ceil(live.code.length / 2) : 0;

  return (
    <View style={[st.panel, compact && st.panelCompact]}>
      <View style={st.headerRow}>
        <Ionicons name="keypad-outline" size={14} color={CYAN} />
        <Text style={st.label}>Delivery PIN</Text>
        {live && <Text style={st.expiry}>Expires in {timeLeft(live, now)}</Text>}
      </View>

      {live?.kind === "code" && (
        <>
          <View
            style={st.digits}
            accessible
            accessibilityLabel={`Delivery PIN ${live.code.split("").join(" ")}`}
          >
            {live.code.split("").map((digit, i) => (
              <View key={i} style={[st.box, compact && st.boxCompact, i === half && st.groupGap]}>
                <Text style={[st.digit, compact && st.digitCompact]}>{digit}</Text>
              </View>
            ))}
          </View>
          <Text style={st.note}>Read it to your driver at handover.</Text>
        </>
      )}

      {live?.kind === "sent" && (
        <Text style={st.note}>A PIN was texted to the recipient's phone.</Text>
      )}

      {!live && (
        <>
          <Text style={st.note}>
            Get it when your driver is close. A PIN lasts 15 minutes, and a new one replaces the last.
          </Text>
          <Pressable
            onPress={handleIssue}
            disabled={issuing}
            accessibilityRole="button"
            style={({ pressed }) => [st.button, { opacity: pressed || issuing ? 0.7 : 1 }]}
          >
            {issuing
              ? <ActivityIndicator size="small" color={CYAN} />
              : <Text style={st.buttonText}>{pin ? "Get a new PIN" : "Get delivery PIN"}</Text>}
          </Pressable>
        </>
      )}

      {error && <Text style={st.error}>{error}</Text>}
    </View>
  );
}

const st = StyleSheet.create({
  panel:        { backgroundColor: "rgba(0,229,255,0.06)", borderWidth: 1, borderColor: "rgba(0,229,255,0.25)", borderRadius: 16, padding: 14, gap: 10 },
  panelCompact: { padding: 12, gap: 8, marginTop: 4 },
  headerRow:    { flexDirection: "row", alignItems: "center", gap: 6 },
  label:        { flex: 1, fontSize: 11, letterSpacing: 1.6, textTransform: "uppercase", color: "rgba(255,255,255,0.66)", fontFamily: "JetBrainsMono-Regular" },
  expiry:       { fontSize: 11, color: AMBER, fontFamily: "JetBrainsMono-Regular", fontVariant: ["tabular-nums"] },
  digits:       { flexDirection: "row", justifyContent: "center", gap: 6 },
  box:          { flex: 1, maxWidth: 46, height: 56, borderRadius: 12, borderWidth: 1, borderColor: "rgba(0,229,255,0.34)", backgroundColor: "rgba(0,229,255,0.08)", alignItems: "center", justifyContent: "center" },
  boxCompact:   { height: 44, borderRadius: 10 },
  groupGap:     { marginLeft: 10 },
  digit:        { fontSize: 26, color: "#FFF", fontFamily: "JetBrainsMono-Regular", fontVariant: ["tabular-nums"] },
  digitCompact: { fontSize: 20 },
  note:         { fontSize: 13, lineHeight: 18, color: "rgba(255,255,255,0.6)" },
  button:       { minHeight: 44, borderRadius: 12, borderWidth: 1, borderColor: "rgba(0,229,255,0.4)", backgroundColor: "rgba(0,229,255,0.08)", alignItems: "center", justifyContent: "center", paddingHorizontal: 14 },
  buttonText:   { fontSize: 14, color: CYAN, fontFamily: "SpaceGrotesk-SemiBold" },
  error:        { fontSize: 12, color: RED },
});
