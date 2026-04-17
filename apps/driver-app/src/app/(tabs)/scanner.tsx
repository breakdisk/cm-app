/**
 * Driver App — Smart Barcode Scanner
 * 4 scan modes: Pickup / Delivery / Hub Induction / Return
 * Falls back to manual AWB entry when camera is unavailable.
 */
import { useState, useCallback } from "react";
import {
  View, Text, StyleSheet, Pressable, TextInput,
  Keyboard, ScrollView, ActivityIndicator, Alert,
} from "react-native";
import { BarCodeScanner, type BarCodeScannerResult } from "expo-barcode-scanner";
import { useCameraPermissions } from "expo-camera";
import Animated, { FadeIn, FadeInDown, FadeInUp } from "react-native-reanimated";
import { router } from "expo-router";
import { useDispatch, useSelector } from "react-redux";
import { Ionicons } from "@expo/vector-icons";
import * as Haptics from "expo-haptics";

import type { RootState, AppDispatch } from "../../store";
import { taskActions } from "../../store";
import { tasksApi } from "../../services/api/tasks";

// ── Design tokens ─────────────────────────────────────────────────────────────
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

// ── Scan modes ────────────────────────────────────────────────────────────────

type ScanMode = "pickup" | "delivery" | "hub" | "return";

interface ModeConfig {
  label:   string;
  icon:    string;
  color:   string;
  hint:    string;
  action:  string;  // CTA label in result card
}

const MODES: Record<ScanMode, ModeConfig> = {
  pickup: {
    label:  "Pickup",
    icon:   "archive-outline",
    color:  PURPLE,
    hint:   "Scan customer QR to confirm parcel collection",
    action: "Confirm Pickup",
  },
  delivery: {
    label:  "Delivery",
    icon:   "bicycle-outline",
    color:  CYAN,
    hint:   "Scan AWB before handing parcel to recipient",
    action: "Start Delivery",
  },
  hub: {
    label:  "Hub Induction",
    icon:   "business-outline",
    color:  AMBER,
    hint:   "Scan parcel on arrival at sorting hub",
    action: "Record Induction",
  },
  return: {
    label:  "Return",
    icon:   "return-down-back-outline",
    color:  RED,
    hint:   "Scan failed delivery parcel for return processing",
    action: "Process Return",
  },
};

const MODE_ORDER: ScanMode[] = ["pickup", "delivery", "hub", "return"];

// ── Scan result type ──────────────────────────────────────────────────────────

interface ScanResult {
  awb:     string;
  mode:    ScanMode;
  found:   boolean;
  taskId?: string;
  label?:  string;
}

// ── Scanner Screen ────────────────────────────────────────────────────────────

export default function ScannerScreen() {
  const [permission, requestPermission] = useCameraPermissions();
  const [mode,       setMode]           = useState<ScanMode>("delivery");
  const [scanned,    setScanned]        = useState(false);
  const [result,     setResult]         = useState<ScanResult | null>(null);
  const [manualAwb,  setManualAwb]      = useState("");
  const [showManual, setShowManual]     = useState(false);
  const [confirmed,  setConfirmed]      = useState(false);
  const [submitting, setSubmitting]     = useState(false);

  const dispatch = useDispatch<AppDispatch>();
  const tasks    = useSelector((s: RootState) => s.tasks.tasks);
  const token    = useSelector((s: RootState) => s.auth.token);

  const cfg = MODES[mode];

  // ── Process scanned/entered AWB ───────────────────────────────────────────

  const processAwb = useCallback((rawAwb: string, scanMode: ScanMode) => {
    Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
    const awb  = rawAwb.trim().toUpperCase();

    // Match against tasks depending on mode
    let task;
    if (scanMode === "pickup") {
      task = tasks.find((t) => t.tracking_number === awb && t.task_type === "pickup");
    } else if (scanMode === "delivery") {
      task = tasks.find((t) => t.tracking_number === awb && t.task_type === "delivery");
    } else {
      task = tasks.find((t) => t.tracking_number === awb);
    }

    const label = task
      ? (scanMode === "pickup"
          ? `${task.sender_name ?? "Unknown"} · ${task.address_city}`
          : `${task.recipient_name} · ${task.address_city}`)
      : undefined;

    setResult({ awb, mode: scanMode, found: !!task, taskId: task?.id, label });
    setConfirmed(false);
  }, [tasks]);

  // ── Camera scan handler ───────────────────────────────────────────────────

  const handleBarCodeScanned = useCallback(({ data }: BarCodeScannerResult) => {
    if (scanned) return;
    setScanned(true);
    processAwb(data, mode);
  }, [scanned, mode, processAwb]);

  // ── Manual entry submit ───────────────────────────────────────────────────

  function handleManualSubmit() {
    if (!manualAwb.trim()) return;
    Keyboard.dismiss();
    setScanned(true);
    processAwb(manualAwb, mode);
    setManualAwb("");
  }

  // ── Action button handler ─────────────────────────────────────────────────

  async function handleAction() {
    if (!result || submitting) return;
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);

    if (result.found && result.taskId) {
      if (result.mode === "pickup") {
        if (!token) {
          Alert.alert("Not signed in", "Sign in again to confirm the pickup.");
          return;
        }
        setSubmitting(true);
        try {
          // Start the task (no-op error if already started)
          try { await tasksApi.start(result.taskId, token); } catch {}
          // Complete the task — pickup task type emits pickup.completed event
          await tasksApi.complete(result.taskId, {}, token);
          dispatch(taskActions.updateTaskStatus({ id: result.taskId, status: "pickup_confirmed" }));
          Haptics.notificationAsync(Haptics.NotificationFeedbackType.Success);
          setConfirmed(true);
          setTimeout(() => router.push("/(tabs)"), 1600);
        } catch (err: any) {
          Haptics.notificationAsync(Haptics.NotificationFeedbackType.Error);
          const msg = err?.message ?? "Could not confirm pickup. Try again.";
          Alert.alert("Pickup failed", msg);
        } finally {
          setSubmitting(false);
        }
      } else if (result.mode === "delivery") {
        router.push(`/task/${result.taskId}`);
      }
    } else if (result.mode === "hub" || result.mode === "return") {
      // Hub/Return just show confirmation — no task routing needed
      setConfirmed(true);
    }
  }

  // ── Reset ─────────────────────────────────────────────────────────────────

  function reset() {
    setScanned(false);
    setResult(null);
    setConfirmed(false);
  }

  // ── Render: permission request ────────────────────────────────────────────

  const cameraAvailable = permission?.granted;

  // ── Main render ───────────────────────────────────────────────────────────

  return (
    <View style={styles.container}>
      {/* Camera or placeholder background */}
      {cameraAvailable && !showManual ? (
        <BarCodeScanner
          onBarCodeScanned={scanned ? undefined : handleBarCodeScanned}
          style={StyleSheet.absoluteFillObject}
        />
      ) : (
        <View style={[StyleSheet.absoluteFillObject, styles.noCameraBackground]} />
      )}

      {/* Dark vignette overlay */}
      <View style={styles.vignette} />

      {/* ── Mode selector strip ────────────────────────────────────────── */}
      <Animated.View entering={FadeInDown.springify()} style={styles.modeStrip}>
        <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={styles.modeScroll}>
          {MODE_ORDER.map((m) => {
            const mc      = MODES[m];
            const isActive = mode === m;
            return (
              <Pressable
                key={m}
                onPress={() => { setMode(m); reset(); }}
                style={[
                  styles.modeTab,
                  isActive && { backgroundColor: `${mc.color}20`, borderColor: `${mc.color}60` },
                ]}
              >
                <Ionicons name={mc.icon as never} size={14} color={isActive ? mc.color : "rgba(255,255,255,0.35)"} />
                <Text style={[styles.modeTabText, isActive && { color: mc.color }]}>{mc.label}</Text>
              </Pressable>
            );
          })}
        </ScrollView>
      </Animated.View>

      {/* ── Scan frame (hidden in manual mode) ────────────────────────── */}
      {!showManual && (
        <View style={styles.frameArea}>
          <View style={[styles.scanFrame, { borderColor: `${cfg.color}40` }]}>
            {/* Corner accents */}
            <View style={[styles.corner, styles.cornerTL, { borderColor: cfg.color }]} />
            <View style={[styles.corner, styles.cornerTR, { borderColor: cfg.color }]} />
            <View style={[styles.corner, styles.cornerBL, { borderColor: cfg.color }]} />
            <View style={[styles.corner, styles.cornerBR, { borderColor: cfg.color }]} />
            {/* Scan line */}
            {!scanned && (
              <View style={[styles.scanLine, { backgroundColor: `${cfg.color}60` }]} />
            )}
          </View>
          <Text style={[styles.hintText, { color: `${cfg.color}99` }]}>{cfg.hint}</Text>

          {/* Camera permission / unavailable notice */}
          {!cameraAvailable && (
            <Animated.View entering={FadeIn} style={styles.noCameraNote}>
              <Ionicons name="camera-off-outline" size={18} color="rgba(255,255,255,0.3)" />
              <Text style={styles.noCameraText}>
                {permission ? "Camera unavailable on web — use manual entry below" : "Camera permission needed"}
              </Text>
              {!permission?.granted && (
                <Pressable onPress={requestPermission} style={styles.grantBtn}>
                  <Text style={styles.grantText}>Grant Access</Text>
                </Pressable>
              )}
            </Animated.View>
          )}
        </View>
      )}

      {/* ── Manual entry toggle + input ────────────────────────────────── */}
      <Animated.View entering={FadeInUp.springify()} style={styles.manualSection}>
        <Pressable
          onPress={() => { setShowManual((v) => !v); reset(); }}
          style={styles.manualToggle}
        >
          <Ionicons name={showManual ? "barcode-outline" : "create-outline"} size={14} color="rgba(255,255,255,0.4)" />
          <Text style={styles.manualToggleText}>
            {showManual ? "Switch to Camera Scan" : "Enter AWB Manually"}
          </Text>
        </Pressable>

        {showManual && (
          <Animated.View entering={FadeInDown.springify()} style={styles.manualInputRow}>
            <TextInput
              value={manualAwb}
              onChangeText={setManualAwb}
              placeholder="e.g. LS-A1B2C3D4"
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={[styles.manualInput, { borderColor: `${cfg.color}40` }]}
              autoCapitalize="characters"
              autoCorrect={false}
              returnKeyType="search"
              onSubmitEditing={handleManualSubmit}
            />
            <Pressable
              onPress={handleManualSubmit}
              style={[styles.manualSubmitBtn, { backgroundColor: cfg.color }]}
            >
              <Ionicons name="search" size={16} color={CANVAS} />
            </Pressable>
          </Animated.View>
        )}
      </Animated.View>

      {/* ── Result card ────────────────────────────────────────────────── */}
      {result && (
        <Animated.View
          entering={FadeInUp.springify()}
          style={[
            styles.resultCard,
            confirmed
              ? { borderColor: `${GREEN}40`, backgroundColor: `${GREEN}0A` }
              : result.found
                ? { borderColor: `${cfg.color}40`, backgroundColor: `${cfg.color}0A` }
                : { borderColor: `${AMBER}40`, backgroundColor: `${AMBER}0A` },
          ]}
        >
          {confirmed ? (
            /* Confirmed state */
            <View style={styles.confirmedBlock}>
              <Ionicons name="checkmark-circle" size={28} color={GREEN} />
              <Text style={styles.confirmedTitle}>
                {result.mode === "pickup"   ? "Pickup Confirmed!" :
                 result.mode === "hub"      ? "Hub Induction Recorded" :
                 result.mode === "return"   ? "Return Logged" :
                 "Done"}
              </Text>
              <Text style={styles.confirmedSub}>{result.awb}</Text>
            </View>
          ) : (
            /* Scan result */
            <>
              <View style={styles.resultHeader}>
                <View style={styles.resultHeaderLeft}>
                  <Ionicons
                    name={result.found ? "barcode" : "alert-circle-outline"}
                    size={16}
                    color={result.found ? cfg.color : AMBER}
                  />
                  <Text style={[styles.resultAwb, { color: result.found ? "#FFF" : AMBER }]}>
                    {result.awb}
                  </Text>
                </View>
                <View style={[styles.modeChip, { borderColor: `${cfg.color}40`, backgroundColor: `${cfg.color}12` }]}>
                  <Text style={[styles.modeChipText, { color: cfg.color }]}>{cfg.label}</Text>
                </View>
              </View>

              {result.found ? (
                <>
                  {result.label && (
                    <Text style={styles.resultLabel}>{result.label}</Text>
                  )}
                  <Pressable
                    onPress={handleAction}
                    disabled={submitting}
                    style={[styles.actionBtn, { backgroundColor: cfg.color, opacity: submitting ? 0.6 : 1 }]}
                  >
                    {submitting ? (
                      <ActivityIndicator size="small" color={CANVAS} />
                    ) : (
                      <Ionicons name={MODES[result.mode].icon as never} size={15} color={CANVAS} />
                    )}
                    <Text style={styles.actionBtnText}>{submitting ? "Confirming…" : cfg.action}</Text>
                  </Pressable>
                </>
              ) : (
                <Text style={styles.notFoundText}>
                  AWB not found in your assigned tasks.
                  {result.mode === "hub" || result.mode === "return"
                    ? " Record manually or contact dispatch."
                    : " Contact dispatch for assistance."}
                </Text>
              )}

              <Pressable onPress={reset} style={styles.rescanBtn}>
                <Ionicons name="scan-outline" size={12} color="rgba(255,255,255,0.4)" />
                <Text style={styles.rescanText}>Scan Another</Text>
              </Pressable>
            </>
          )}
        </Animated.View>
      )}
    </View>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const C_SIZE  = 20;
const C_THICK = 3;

const styles = StyleSheet.create({
  container:          { flex: 1, backgroundColor: CANVAS },
  noCameraBackground: { backgroundColor: "#06091A" },
  vignette:           { ...StyleSheet.absoluteFillObject, backgroundColor: "rgba(5,8,16,0.45)" },

  // Mode strip
  modeStrip:    { position: "absolute", top: 0, left: 0, right: 0, paddingTop: 52, paddingBottom: 8 },
  modeScroll:   { paddingHorizontal: 16, gap: 8 },
  modeTab:      { flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: 14, paddingVertical: 8, borderRadius: 20, borderWidth: 1, borderColor: "rgba(255,255,255,0.1)", backgroundColor: "rgba(255,255,255,0.04)" },
  modeTabText:  { fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.35)" },

  // Scan frame
  frameArea:    { flex: 1, alignItems: "center", justifyContent: "center", paddingTop: 120 },
  scanFrame:    { width: 260, height: 260, borderRadius: 12, borderWidth: 1, position: "relative", alignItems: "center", justifyContent: "center" },
  corner:       { position: "absolute", width: C_SIZE, height: C_SIZE },
  cornerTL:     { top: -1, left: -1, borderTopWidth: C_THICK, borderLeftWidth: C_THICK, borderTopLeftRadius: 4 },
  cornerTR:     { top: -1, right: -1, borderTopWidth: C_THICK, borderRightWidth: C_THICK, borderTopRightRadius: 4 },
  cornerBL:     { bottom: -1, left: -1, borderBottomWidth: C_THICK, borderLeftWidth: C_THICK, borderBottomLeftRadius: 4 },
  cornerBR:     { bottom: -1, right: -1, borderBottomWidth: C_THICK, borderRightWidth: C_THICK, borderBottomRightRadius: 4 },
  scanLine:     { position: "absolute", top: "50%", left: 16, right: 16, height: 1.5, borderRadius: 1 },
  hintText:     { marginTop: 16, fontSize: 12, fontFamily: "JetBrainsMono-Regular", textAlign: "center", paddingHorizontal: 32 },
  noCameraNote: { marginTop: 24, alignItems: "center", gap: 8, paddingHorizontal: 32 },
  noCameraText: { fontSize: 12, color: "rgba(255,255,255,0.3)", textAlign: "center", fontFamily: "JetBrainsMono-Regular" },
  grantBtn:     { borderRadius: 8, borderWidth: 1, borderColor: "rgba(0,229,255,0.3)", paddingHorizontal: 16, paddingVertical: 8, backgroundColor: "rgba(0,229,255,0.08)", marginTop: 4 },
  grantText:    { color: CYAN, fontFamily: "JetBrainsMono-Regular", fontSize: 12 },

  // Manual entry
  manualSection:    { position: "absolute", bottom: 180, left: 16, right: 16, alignItems: "center" },
  manualToggle:     { flexDirection: "row", alignItems: "center", gap: 6, paddingHorizontal: 14, paddingVertical: 8, borderRadius: 20, backgroundColor: "rgba(255,255,255,0.05)", borderWidth: 1, borderColor: "rgba(255,255,255,0.08)" },
  manualToggleText: { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.4)" },
  manualInputRow:   { flexDirection: "row", alignItems: "center", gap: 8, marginTop: 10, width: "100%" },
  manualInput:      { flex: 1, height: 44, borderRadius: 10, borderWidth: 1, backgroundColor: "rgba(255,255,255,0.04)", paddingHorizontal: 14, fontSize: 14, fontFamily: "JetBrainsMono-Regular", color: "#FFFFFF" },
  manualSubmitBtn:  { width: 44, height: 44, borderRadius: 10, alignItems: "center", justifyContent: "center" },

  // Result card
  resultCard:       { position: "absolute", bottom: 24, left: 16, right: 16, borderRadius: 16, padding: 16, borderWidth: 1, gap: 12 },
  resultHeader:     { flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  resultHeaderLeft: { flexDirection: "row", alignItems: "center", gap: 8, flex: 1 },
  resultAwb:        { fontSize: 16, fontFamily: "JetBrainsMono-Bold", letterSpacing: 0.5, flex: 1 },
  modeChip:         { borderRadius: 999, borderWidth: 1, paddingHorizontal: 10, paddingVertical: 3 },
  modeChipText:     { fontSize: 10, fontFamily: "JetBrainsMono-Regular" },
  resultLabel:      { fontSize: 12, color: "rgba(255,255,255,0.5)", fontFamily: "JetBrainsMono-Regular" },
  notFoundText:     { fontSize: 12, color: AMBER, fontFamily: "JetBrainsMono-Regular", lineHeight: 18 },
  actionBtn:        { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 8, borderRadius: 12, paddingVertical: 13 },
  actionBtnText:    { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: CANVAS },
  rescanBtn:        { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6, paddingVertical: 4 },
  rescanText:       { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.35)" },

  // Confirmed
  confirmedBlock:   { alignItems: "center", gap: 6, paddingVertical: 8 },
  confirmedTitle:   { fontSize: 16, fontFamily: "SpaceGrotesk-SemiBold", color: GREEN },
  confirmedSub:     { fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.4)" },
});
