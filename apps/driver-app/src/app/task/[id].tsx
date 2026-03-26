/**
 * Driver App — Task Detail / Delivery Action Screen
 * Shows recipient info, address, COD amount, and action buttons.
 * Transitions driver through: Assigned → Navigating → Arrived → POD → Completed.
 */
import { useState } from "react";
import {
  View, Text, StyleSheet, ScrollView, Pressable, Linking, Alert,
} from "react-native";
import { useLocalSearchParams, router } from "expo-router";
import { useDispatch, useSelector } from "react-redux";
import Animated, { FadeInUp, useAnimatedStyle, withSpring } from "react-native-reanimated";
import * as Haptics from "expo-haptics";
import { Ionicons } from "@expo/vector-icons";

import type { RootState, AppDispatch } from "../../store";
import { taskActions } from "../../store";
import { deliveryQueue } from "../../services/storage/delivery_queue";

// ── Design tokens ─────────────────────────────────────────────────────────────
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

// ── Action configuration per status ──────────────────────────────────────────

interface ActionConfig {
  primary:   { label: string; color: string; icon: string; nextStatus?: string };
  secondary: { label: string; color: string; icon: string }[];
}

const ACTIONS: Record<string, ActionConfig> = {
  assigned: {
    primary:   { label: "Start Navigation",  color: CYAN,  icon: "navigate",       nextStatus: "navigating" },
    secondary: [{ label: "Call Recipient",   color: AMBER, icon: "call-outline" }],
  },
  navigating: {
    primary:   { label: "Arrived at Stop",   color: AMBER,  icon: "location",      nextStatus: "arrived" },
    secondary: [{ label: "Call Recipient",   color: CYAN,   icon: "call-outline" }],
  },
  arrived: {
    primary:   { label: "Capture POD",       color: GREEN,  icon: "camera",        nextStatus: "pod_pending" },
    secondary: [
      { label: "Call Recipient",  color: CYAN, icon: "call-outline" },
      { label: "Mark Failed",     color: RED,  icon: "close-circle-outline" },
    ],
  },
  pod_pending: {
    primary:   { label: "Submit Delivery",   color: GREEN,  icon: "checkmark-circle", nextStatus: "completed" },
    secondary: [{ label: "Retake POD",       color: AMBER,  icon: "camera-reverse"  }],
  },
};

// ── Screen ────────────────────────────────────────────────────────────────────

export default function TaskDetailScreen() {
  const { id } = useLocalSearchParams<{ id: string }>();
  const dispatch = useDispatch<AppDispatch>();
  const task = useSelector((s: RootState) => s.tasks.tasks.find((t) => t.id === id));

  const [loading, setLoading] = useState(false);

  if (!task) {
    return (
      <View style={styles.empty}>
        <Text style={styles.emptyText}>Task not found</Text>
      </View>
    );
  }

  const actionCfg = ACTIONS[task.status];

  async function advanceStatus(nextStatus: string) {
    if (!task) return;
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Medium);
    setLoading(true);
    try {
      // Persist action offline-first
      await deliveryQueue.enqueue({
        action_type: "status_update",
        payload:     JSON.stringify({ task_id: task.id, shipment_id: task.shipment_id, status: nextStatus }),
        created_at:  Date.now(),
        last_error:  undefined,
      });
      dispatch(taskActions.updateTaskStatus({ id: task.id, status: nextStatus as never }));
      dispatch(taskActions.incrementSyncPending());

      // If capturing POD, navigate to POD screen
      if (nextStatus === "pod_pending") {
        router.push(`/pod/${task.id}`);
      }
    } finally {
      setLoading(false);
    }
  }

  async function markFailed() {
    Alert.alert(
      "Mark as Failed",
      "Select a reason for failed delivery:",
      [
        { text: "Not Home",         onPress: () => advanceStatus("failed") },
        { text: "Wrong Address",    onPress: () => advanceStatus("failed") },
        { text: "Customer Refused", onPress: () => advanceStatus("failed") },
        { text: "Cancel", style: "cancel" },
      ]
    );
  }

  function callRecipient() {
    Linking.openURL(`tel:${task!.recipient_phone}`);
  }

  function openMaps() {
    const url = `https://maps.apple.com/?q=${task!.lat},${task!.lng}`;
    Linking.openURL(url);
  }

  return (
    <ScrollView style={styles.container} contentContainerStyle={{ paddingBottom: 40 }}>

      {/* Status banner */}
      <Animated.View entering={FadeInUp.springify()} style={styles.statusBanner}>
        <Text style={styles.stopLabel}>Stop #{task.sequence}</Text>
        <Text style={styles.trackingNumber}>{task.tracking_number}</Text>
        {task.status === "completed" && (
          <View style={styles.completedBadge}>
            <Ionicons name="checkmark-circle" size={16} color={GREEN} />
            <Text style={[styles.badgeText, { color: GREEN }]}>Delivered</Text>
          </View>
        )}
      </Animated.View>

      {/* Recipient card */}
      <Animated.View entering={FadeInUp.delay(80).springify()} style={styles.card}>
        <Text style={styles.cardLabel}>Recipient</Text>
        <Text style={styles.recipientName}>{task.recipient_name}</Text>
        <Pressable onPress={callRecipient} style={styles.phoneRow}>
          <Ionicons name="call-outline" size={14} color={CYAN} />
          <Text style={styles.phone}>{task.recipient_phone}</Text>
        </Pressable>
      </Animated.View>

      {/* Address card */}
      <Animated.View entering={FadeInUp.delay(120).springify()} style={styles.card}>
        <Text style={styles.cardLabel}>Delivery Address</Text>
        <Text style={styles.address}>{task.address_line1}</Text>
        <Text style={styles.addressCity}>{task.address_city}</Text>
        <Pressable onPress={openMaps} style={styles.mapsButton}>
          <Ionicons name="navigate" size={14} color={CYAN} />
          <Text style={styles.mapsText}>Open in Maps</Text>
        </Pressable>
      </Animated.View>

      {/* Payment card */}
      <Animated.View entering={FadeInUp.delay(160).springify()} style={styles.card}>
        <Text style={styles.cardLabel}>Payment</Text>
        {task.cod_amount ? (
          <View style={styles.codRow}>
            <Text style={styles.codLabel}>Collect COD</Text>
            <Text style={styles.codAmount}>₱{task.cod_amount.toLocaleString("en-PH")}</Text>
          </View>
        ) : (
          <View style={styles.prepaidRow}>
            <Ionicons name="checkmark-circle" size={16} color={GREEN} />
            <Text style={styles.prepaidText}>Prepaid — No collection required</Text>
          </View>
        )}
      </Animated.View>

      {/* Special notes */}
      {task.special_notes && (
        <Animated.View entering={FadeInUp.delay(200).springify()} style={[styles.card, styles.noteCard]}>
          <View style={styles.noteHeader}>
            <Ionicons name="information-circle" size={14} color={AMBER} />
            <Text style={[styles.cardLabel, { color: AMBER }]}>Special Instructions</Text>
          </View>
          <Text style={styles.noteText}>{task.special_notes}</Text>
        </Animated.View>
      )}

      {/* Attempt history */}
      {task.attempt_count > 0 && (
        <Animated.View entering={FadeInUp.delay(220).springify()} style={styles.card}>
          <Text style={[styles.cardLabel, { color: RED }]}>
            Previous Attempt #{task.attempt_count}
          </Text>
          <Text style={styles.attemptNote}>This is a re-delivery attempt. Confirm address before proceeding.</Text>
        </Animated.View>
      )}

      {/* Primary action */}
      {actionCfg && (
        <Animated.View entering={FadeInUp.delay(240).springify()} style={styles.actionsContainer}>
          <Pressable
            onPress={() => actionCfg.primary.nextStatus
              ? advanceStatus(actionCfg.primary.nextStatus)
              : undefined
            }
            disabled={loading}
            style={({ pressed }) => [
              styles.primaryButton,
              { backgroundColor: actionCfg.primary.color, opacity: pressed || loading ? 0.75 : 1 },
            ]}
          >
            <Ionicons name={actionCfg.primary.icon as never} size={18} color={CANVAS} />
            <Text style={styles.primaryButtonText}>{actionCfg.primary.label}</Text>
          </Pressable>

          {/* Secondary actions */}
          <View style={styles.secondaryRow}>
            {actionCfg.secondary.map((sec) => (
              <Pressable
                key={sec.label}
                onPress={sec.label === "Mark Failed" ? markFailed : callRecipient}
                style={({ pressed }) => [
                  styles.secondaryButton,
                  { borderColor: `${sec.color}40`, opacity: pressed ? 0.7 : 1 },
                ]}
              >
                <Ionicons name={sec.icon as never} size={15} color={sec.color} />
                <Text style={[styles.secondaryButtonText, { color: sec.color }]}>{sec.label}</Text>
              </Pressable>
            ))}
          </View>
        </Animated.View>
      )}
    </ScrollView>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container:        { flex: 1, backgroundColor: CANVAS },
  empty:            { flex: 1, alignItems: "center", justifyContent: "center", backgroundColor: CANVAS },
  emptyText:        { color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular" },
  statusBanner:     { margin: 12, padding: 16, borderRadius: 12, backgroundColor: "rgba(0,229,255,0.06)", borderWidth: 1, borderColor: "rgba(0,229,255,0.15)" },
  stopLabel:        { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.35)", textTransform: "uppercase", letterSpacing: 1.5, marginBottom: 2 },
  trackingNumber:   { fontSize: 22, fontFamily: "JetBrainsMono-Bold", color: CYAN, letterSpacing: 1 },
  completedBadge:   { flexDirection: "row", alignItems: "center", gap: 4, marginTop: 6 },
  badgeText:        { fontSize: 12, fontFamily: "JetBrainsMono-Regular" },
  card:             { marginHorizontal: 12, marginBottom: 10, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 14 },
  cardLabel:        { fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, color: "rgba(255,255,255,0.3)", marginBottom: 6 },
  recipientName:    { fontSize: 18, fontFamily: "SpaceGrotesk-SemiBold", color: "#FFFFFF", marginBottom: 6 },
  phoneRow:         { flexDirection: "row", alignItems: "center", gap: 6 },
  phone:            { fontSize: 14, fontFamily: "JetBrainsMono-Regular", color: CYAN },
  address:          { fontSize: 15, fontFamily: "SpaceGrotesk-Regular", color: "#FFFFFF", marginBottom: 2 },
  addressCity:      { fontSize: 13, color: "rgba(255,255,255,0.45)", marginBottom: 8 },
  mapsButton:       { flexDirection: "row", alignItems: "center", gap: 6, marginTop: 4 },
  mapsText:         { fontSize: 13, color: CYAN, fontFamily: "JetBrainsMono-Regular" },
  codRow:           { flexDirection: "row", alignItems: "center", justifyContent: "space-between" },
  codLabel:         { fontSize: 13, color: "rgba(255,255,255,0.6)" },
  codAmount:        { fontSize: 22, fontFamily: "SpaceGrotesk-Bold", color: AMBER },
  prepaidRow:       { flexDirection: "row", alignItems: "center", gap: 8 },
  prepaidText:      { fontSize: 13, color: GREEN },
  noteCard:         { borderColor: `${AMBER}30` },
  noteHeader:       { flexDirection: "row", alignItems: "center", gap: 6, marginBottom: 6 },
  noteText:         { fontSize: 13, color: "rgba(255,255,255,0.6)", lineHeight: 20 },
  attemptNote:      { fontSize: 12, color: "rgba(255,59,92,0.7)", lineHeight: 18 },
  actionsContainer: { marginHorizontal: 12, marginTop: 8, gap: 10 },
  primaryButton:    { flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 8, borderRadius: 14, paddingVertical: 16 },
  primaryButtonText:{ fontSize: 15, fontFamily: "SpaceGrotesk-SemiBold", color: CANVAS },
  secondaryRow:     { flexDirection: "row", gap: 10 },
  secondaryButton:  { flex: 1, flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6, borderRadius: 12, borderWidth: 1, paddingVertical: 12, backgroundColor: GLASS },
  secondaryButtonText: { fontSize: 12, fontFamily: "JetBrainsMono-Regular" },
});
