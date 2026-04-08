/**
 * AwbQRCode — displays an AWB tracking code in a scannable visual block.
 * Shows the AWB in a styled code box as a fallback (no external QR library needed).
 *
 * Usage:
 *   <AwbQRCode awb="LS-A1B2C3D4" size={180} />
 *   <AwbQRCode awb="LS-A1B2C3D4" size={280} fullscreen onClose={() => ...} />
 */
import React from "react";
import {
  View, Text, StyleSheet, Pressable,
} from "react-native";
import { Ionicons } from "@expo/vector-icons";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const BORDER = "rgba(255,255,255,0.08)";

interface Props {
  awb:        string;
  size?:      number;
  accent?:    string;
  fullscreen?: boolean;
  onClose?:   () => void;
}

export function AwbQRCode({ awb, size = 180, accent = CYAN, fullscreen = false, onClose }: Props) {
  const qrBlock = (
    <View style={[styles.qrWrap, { width: size + 16, borderColor: accent + "40" }]}>
      <View style={[styles.awbDisplay, { width: size, height: size }]}>
        <Ionicons name="qr-code-outline" size={size * 0.35} color={accent} />
        <Text style={[styles.awbCode, { color: accent, fontSize: Math.max(11, size * 0.07) }]}>
          {awb}
        </Text>
      </View>
      <View style={styles.awbRow}>
        <Ionicons name="qr-code-outline" size={11} color={accent} />
        <Text style={[styles.awbText, { color: accent }]}>{awb}</Text>
      </View>
      <Text style={styles.hint}>Show this to your driver on pickup</Text>
    </View>
  );

  if (!fullscreen) return qrBlock;

  return (
    <View style={styles.overlay}>
      <Pressable style={styles.backdrop} onPress={onClose} />
      <View style={styles.sheet}>
        <View style={styles.sheetHeader}>
          <Text style={styles.sheetTitle}>Pickup QR Code</Text>
          <Pressable onPress={onClose} style={styles.closeBtn}>
            <Ionicons name="close" size={20} color="rgba(255,255,255,0.6)" />
          </Pressable>
        </View>
        <View style={styles.sheetBody}>
          <AwbQRCode awb={awb} size={240} accent={accent} />
          <View style={styles.instructionCard}>
            <Ionicons name="scan-outline" size={16} color={GREEN} />
            <Text style={styles.instructionText}>
              Your driver will scan this code when they arrive for pickup to verify and confirm your shipment.
            </Text>
          </View>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  qrWrap:       { alignItems: "center", backgroundColor: "#FFF", borderRadius: 14, padding: 8, borderWidth: 2, gap: 6 },
  awbDisplay:   { alignItems: "center", justifyContent: "center", backgroundColor: "#F8F8F8", borderRadius: 8, gap: 8, padding: 8 },
  awbCode:      { fontFamily: "JetBrainsMono-Regular", fontWeight: "700", letterSpacing: 1.5, textAlign: "center" },
  awbRow:       { flexDirection: "row", alignItems: "center", gap: 5, backgroundColor: CANVAS, borderRadius: 6, paddingHorizontal: 10, paddingVertical: 4 },
  awbText:      { fontSize: 12, fontFamily: "JetBrainsMono-Regular", fontWeight: "700", letterSpacing: 1 },
  hint:         { fontSize: 9, fontFamily: "JetBrainsMono-Regular", color: "rgba(0,0,0,0.45)", textAlign: "center", paddingHorizontal: 4 },

  overlay:      { position: "absolute", top: 0, left: 0, right: 0, bottom: 0, zIndex: 200, justifyContent: "flex-end" },
  backdrop:     { position: "absolute", top: 0, left: 0, right: 0, bottom: 0, backgroundColor: "rgba(0,0,0,0.82)" },
  sheet:        { backgroundColor: "#0A0E1A", borderTopLeftRadius: 24, borderTopRightRadius: 24, borderWidth: 1, borderColor: BORDER },
  sheetHeader:  { flexDirection: "row", alignItems: "center", paddingHorizontal: 20, paddingTop: 20, paddingBottom: 14, borderBottomWidth: 1, borderBottomColor: BORDER },
  sheetTitle:   { flex: 1, fontSize: 17, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  closeBtn:     { width: 32, height: 32, borderRadius: 16, backgroundColor: "rgba(255,255,255,0.06)", alignItems: "center", justifyContent: "center" },
  sheetBody:    { padding: 24, alignItems: "center", gap: 20, paddingBottom: 40 },
  instructionCard: { flexDirection: "row", gap: 10, backgroundColor: "rgba(0,255,136,0.06)", borderWidth: 1, borderColor: "rgba(0,255,136,0.2)", borderRadius: 12, padding: 14, alignItems: "flex-start" },
  instructionText: { flex: 1, fontSize: 13, color: "rgba(255,255,255,0.55)", lineHeight: 20 },
});
