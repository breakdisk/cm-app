/**
 * AwbQRCode — renders a real, scannable QR code for an AWB tracking number.
 * Pure React Native implementation — no native modules required.
 * Uses the `qrcode` package to generate the QR matrix and renders cells as Views.
 *
 * Usage:
 *   <AwbQRCode awb="CM-PH1-S0000001A" size={180} />
 *   <AwbQRCode awb="CM-PH1-S0000001A" size={280} fullscreen onClose={() => ...} />
 */
import React, { useMemo } from "react";
import {
  View, Text, StyleSheet, Pressable,
} from "react-native";
// Metro resolves "qrcode" → "qrcode/lib/core/qrcode" on native (see metro.config.js),
// bypassing the Canvas/pngjs/yargs renderers. Only `create` is used.
import { create as qrCreate } from "qrcode";
import { Ionicons } from "@expo/vector-icons";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const BORDER = "rgba(255,255,255,0.08)";
const QUIET  = 4; // QR spec requires ≥4 modules of quiet zone on each side

interface Props {
  awb:         string;
  size?:       number;
  accent?:     string;
  fullscreen?: boolean;
  onClose?:    () => void;
}

function QRMatrix({ awb, size }: { awb: string; size: number }) {
  const matrix = useMemo(() => {
    try {
      return qrCreate(awb, { errorCorrectionLevel: "M" });
    } catch {
      return null;
    }
  }, [awb]);

  if (!matrix) return null;

  const dim      = matrix.modules.size;
  const totalDim = dim + QUIET * 2;
  const cell     = Math.floor(size / totalDim);
  const rendered = cell * totalDim;
  const offset   = Math.floor((size - rendered) / 2);

  return (
    <View style={{ width: size, height: size, backgroundColor: "#FFFFFF", alignItems: "center", justifyContent: "center" }}>
      <View style={{ width: rendered, height: rendered, marginLeft: offset, marginTop: offset }}>
        {Array.from({ length: totalDim }, (_, row) => (
          <View key={row} style={{ flexDirection: "row", height: cell }}>
            {Array.from({ length: totalDim }, (_, col) => {
              const dataRow = row - QUIET;
              const dataCol = col - QUIET;
              const isQuiet = dataRow < 0 || dataRow >= dim || dataCol < 0 || dataCol >= dim;
              const isBlack = !isQuiet && matrix.modules.data[dataRow * dim + dataCol] === 1;
              return (
                <View
                  key={col}
                  style={{ width: cell, height: cell, backgroundColor: isBlack ? "#000000" : "#FFFFFF" }}
                />
              );
            })}
          </View>
        ))}
      </View>
    </View>
  );
}

export function AwbQRCode({ awb, size = 180, accent = CYAN, fullscreen = false, onClose }: Props) {
  const qrBlock = (
    <View style={[styles.qrWrap, { width: size + 32, borderColor: accent + "40" }]}>
      <QRMatrix awb={awb} size={size} />
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
  qrWrap:       { alignItems: "center", backgroundColor: "#FFF", borderRadius: 14, padding: 16, borderWidth: 2, gap: 10 },
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
