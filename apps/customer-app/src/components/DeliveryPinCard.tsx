/**
 * The recipient's delivery PIN, from booking until delivery.
 *
 * The driver cannot complete the delivery without it (pod refuses the proof of
 * delivery). `autoIssue` is set only on the booking confirmation, where the
 * shipment is new and cannot have a PIN yet. Everywhere else the card shows the
 * stored PIN, or asks before requesting one — see services/api/pod.ts for why a
 * request can replace a PIN the recipient already holds.
 */
import React, { useEffect, useRef, useState } from "react";
import { View, Text, Pressable, ActivityIndicator, StyleSheet } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import {
  getStoredDeliveryPin,
  issueDeliveryPin,
  onDeliveryPinIssued,
  type DeliveryPin,
} from "../services/api/pod";

const CYAN = "#00E5FF";
const RED  = "#FF3B5C";

interface DeliveryPinCardProps {
  shipmentId:      string;
  recipientPhone?: string;
  compact?:        boolean;
  /** Request a PIN on mount when none is stored. Booking confirmation only. */
  autoIssue?:      boolean;
}

function pinErrorMessage(err: any): string {
  if (err?.status === 403) return "This account can't request a PIN for this shipment.";
  if (err?.status === 404) return "This shipment wasn't found.";
  return "Couldn't get a PIN right now. Try again.";
}

export function DeliveryPinCard({ shipmentId, recipientPhone, compact = false, autoIssue = false }: DeliveryPinCardProps) {
  const [pin,     setPin]     = useState<DeliveryPin | null>(null);
  const [loaded,  setLoaded]  = useState(false);
  const [issuing, setIssuing] = useState(false);
  const [error,   setError]   = useState<string | null>(null);
  const autoIssued = useRef<string | null>(null);

  async function request(reissue: boolean) {
    setIssuing(true);
    setError(null);
    try {
      setPin(await issueDeliveryPin(shipmentId, recipientPhone ?? "", { reissue }));
    } catch (err) {
      setError(pinErrorMessage(err));
    } finally {
      setIssuing(false);
    }
  }

  useEffect(() => {
    let alive = true;
    setPin(null);
    setLoaded(false);
    setError(null);
    getStoredDeliveryPin(shipmentId).then(stored => {
      if (!alive) return;
      setPin(stored);
      setLoaded(true);
      if (!stored && autoIssue && autoIssued.current !== shipmentId) {
        autoIssued.current = shipmentId;
        request(false);
      }
    });
    const off = onDeliveryPinIssued((id, issued) => {
      if (!alive || id !== shipmentId) return;
      setPin(issued);
      setError(null);
    });
    return () => {
      alive = false;
      off();
    };
    // request is recreated each render; the effect is keyed on the shipment only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shipmentId, autoIssue]);

  if (!loaded) return null;

  const half = pin?.kind === "code" ? Math.ceil(pin.code.length / 2) : 0;

  return (
    <View style={[st.panel, compact && st.panelCompact]}>
      <View style={st.headerRow}>
        <Ionicons name="keypad-outline" size={14} color={CYAN} />
        <Text style={st.label}>Delivery PIN</Text>
      </View>

      {issuing && !pin && <ActivityIndicator size="small" color={CYAN} />}

      {pin?.kind === "code" && (
        <>
          <View
            style={st.digits}
            accessible
            accessibilityLabel={`Delivery PIN ${pin.code.split("").join(" ")}`}
          >
            {pin.code.split("").map((digit, i) => (
              <View key={i} style={[st.box, compact && st.boxCompact, i === half && st.groupGap]}>
                <Text style={[st.digit, compact && st.digitCompact]}>{digit}</Text>
              </View>
            ))}
          </View>
          <Text style={st.note}>
            Give it to your driver when your delivery arrives. They can't complete the delivery without it.
          </Text>
          <Pressable
            onPress={() => request(true)}
            disabled={issuing}
            accessibilityRole="button"
            style={({ pressed }) => [st.linkButton, { opacity: pressed || issuing ? 0.6 : 1 }]}
          >
            <Text style={st.linkText}>{issuing ? "Getting a new PIN…" : "PIN not working? Get a new one"}</Text>
          </Pressable>
        </>
      )}

      {pin?.kind === "sent" && (
        <Text style={st.note}>A delivery PIN was texted to the recipient's phone.</Text>
      )}

      {(pin === null || pin.kind === "active") && !(issuing && !pin) && (
        <>
          <Text style={st.note}>
            {pin?.kind === "active"
              ? "A PIN was already issued for this shipment, but it isn't saved on this device. A new PIN replaces the old one."
              : "The driver can't complete the delivery without it."}
          </Text>
          <Pressable
            onPress={() => request(pin?.kind === "active")}
            disabled={issuing}
            accessibilityRole="button"
            style={({ pressed }) => [st.button, { opacity: pressed || issuing ? 0.7 : 1 }]}
          >
            {issuing
              ? <ActivityIndicator size="small" color={CYAN} />
              : <Text style={st.buttonText}>{pin?.kind === "active" ? "Get a new PIN" : "Get delivery PIN"}</Text>}
          </Pressable>
        </>
      )}

      {error && <Text style={st.error}>{error}</Text>}
    </View>
  );
}

const st = StyleSheet.create({
  panel:        { backgroundColor: "rgba(0,229,255,0.06)", borderWidth: 1, borderColor: "rgba(0,229,255,0.25)", borderRadius: 16, padding: 14, gap: 10, alignSelf: "stretch" },
  panelCompact: { padding: 12, gap: 8, marginTop: 4 },
  headerRow:    { flexDirection: "row", alignItems: "center", gap: 6 },
  label:        { flex: 1, fontSize: 11, letterSpacing: 1.6, textTransform: "uppercase", color: "rgba(255,255,255,0.66)", fontFamily: "JetBrainsMono-Regular" },
  digits:       { flexDirection: "row", justifyContent: "center", gap: 6 },
  box:          { flex: 1, maxWidth: 46, height: 56, borderRadius: 12, borderWidth: 1, borderColor: "rgba(0,229,255,0.34)", backgroundColor: "rgba(0,229,255,0.08)", alignItems: "center", justifyContent: "center" },
  boxCompact:   { height: 44, borderRadius: 10 },
  groupGap:     { marginLeft: 10 },
  digit:        { fontSize: 26, color: "#FFF", fontFamily: "JetBrainsMono-Regular", fontVariant: ["tabular-nums"] },
  digitCompact: { fontSize: 20 },
  note:         { fontSize: 13, lineHeight: 18, color: "rgba(255,255,255,0.6)" },
  button:       { minHeight: 44, borderRadius: 12, borderWidth: 1, borderColor: "rgba(0,229,255,0.4)", backgroundColor: "rgba(0,229,255,0.08)", alignItems: "center", justifyContent: "center", paddingHorizontal: 14 },
  buttonText:   { fontSize: 14, color: CYAN, fontFamily: "SpaceGrotesk-SemiBold" },
  linkButton:   { minHeight: 44, alignItems: "center", justifyContent: "center" },
  linkText:     { fontSize: 13, color: CYAN, fontFamily: "SpaceGrotesk-SemiBold" },
  error:        { fontSize: 12, color: RED },
});
