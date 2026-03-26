/**
 * AwbQRCode — generates a QR code from an AWB string using the `qrcode` package.
 * Renders as a base64 PNG via <Image>. Works on Expo web export.
 *
 * Usage:
 *   <AwbQRCode awb="LS-A1B2C3D4" size={180} />
 *   <AwbQRCode awb="LS-A1B2C3D4" size={280} fullscreen onClose={() => ...} />
 */
import React, { useEffect, useState } from "react";
import {
  View, Image, Text, StyleSheet, Pressable, ActivityIndicator,
} from "react-native";
import QRCode from "qrcode";
import { Ionicons } from "@expo/vector-icons";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const BORDER = "rgba(255,255,255,0.08)";

interface Props {
  awb:        string;
  size?:      number;
  accent?:    string;   // border/label colour
  fullscreen?: boolean; // overlay mode
  onClose?:   () => void;
}

export function AwbQRCode({ awb, size = 180, accent = CYAN, fullscreen = false, onClose }: Props) {
  const [uri, setUri] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    QRCode.toDataURL(awb, {
      width:  size * 2,      // 2× for retina
      margin: 2,
      color:  { dark: "#000000", light: "#FFFFFF" },
      errorCorrectionLevel: "M",
    }).then((url) => {
      if (!cancelled) setUri(url);
    }).catch(() => {});
    return () => { cancelled = true; };
  }, [awb, size]);

  const qrBlock = (
    <View style={[styles.qrWrap, { width: size + 16, borderColor: accent + "40" }]}>
      {uri ? (
        <Image
          source={{ uri }}
          style={{ width: size, height: size, borderRadius: 8 }}
          resizeMode="contain"
        />
      ) : (
        <View style={{ width: size, height: size, alignItems: "center", justifyContent: "center" }}>
          <ActivityIndicator color={accent} />
        </View>
      )}
      <View style={styles.awbRow}>
        <Ionicons name="qr-code-outline" size={11} color={accent} />
        <Text style={[styles.awbText, { color: accent }]}>{awb}</Text>
      </View>
      <Text style={styles.hint}>Show this to your driver on pickup</Text>
    </View>
  );

  if (!fullscreen) return qrBlock;

  // Fullscreen overlay
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
              Your driver will scan this QR code when they arrive for pickup to verify and confirm your shipment.
            </Text>
          </View>
        </View>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  qrWrap:       { alignItems: "center", backgroundColor: "#FFF", borderRadius: 14, padding: 8, borderWidth: 2, gap: 6 },
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
