/**
 * Driver App — Task List (My Deliveries)
 * Shows today's assigned deliveries sorted by route sequence.
 * Supports swipe-to-navigate gesture on each task card.
 */
import { useEffect, useCallback } from "react";
import {
  View, Text, StyleSheet, RefreshControl, Pressable, Alert,
} from "react-native";
import { FlashList } from "@shopify/flash-list";
import { useDispatch, useSelector } from "react-redux";
import { router } from "expo-router";
import Animated, { FadeInDown } from "react-native-reanimated";
import * as Haptics from "expo-haptics";

import type { RootState, AppDispatch } from "../../store";
import { taskActions, earningsActions, type DeliveryTask } from "../../store";

// ── Design tokens ─────────────────────────────────────────────────────────────
const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const AMBER   = "#FFAB00";
const RED     = "#FF3B5C";
const PURPLE  = "#A855F7";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

// ── Mock data (replace with RTK Query / SWR fetch) ────────────────────────────
const MOCK_TASKS: DeliveryTask[] = [
  // Pickup tasks (first-mile collections)
  {
    id: "p1", shipment_id: "sp1", tracking_number: "LS-Q7R8S9T0",
    sequence: 0, status: "awaiting_pickup", task_type: "pickup",
    recipient_name: "", recipient_phone: "",
    address_line1: "88 Ayala Ave", address_city: "Makati City",
    lat: 14.5553, lng: 121.0177, attempt_count: 0,
    sender_name: "Ahmad Al Rashid", sender_phone: "+971501234567",
    package_desc: "Balikbayan Box — Clothes & food items",
    package_weight: "22 kg",
    eta_minutes: 8,
  },
  // Delivery tasks
  {
    id: "t1", shipment_id: "s1", tracking_number: "LS-A1B2C3D4",
    sequence: 1, status: "assigned", task_type: "delivery",
    recipient_name: "Maria Santos", recipient_phone: "09171234567",
    address_line1: "123 Ayala Ave", address_city: "Makati City",
    lat: 14.5547, lng: 121.0244, cod_amount: 1500, attempt_count: 0,
    eta_minutes: 12,
  },
  {
    id: "t2", shipment_id: "s2", tracking_number: "LS-E5F6G7H8",
    sequence: 2, status: "assigned", task_type: "delivery",
    recipient_name: "Juan Dela Cruz", recipient_phone: "09281234567",
    address_line1: "456 Gil Puyat Ave", address_city: "Makati City",
    lat: 14.5595, lng: 120.9842, attempt_count: 0,
    eta_minutes: 28,
  },
  {
    id: "t3", shipment_id: "s3", tracking_number: "LS-I9J0K1L2",
    sequence: 3, status: "completed", task_type: "delivery",
    recipient_name: "Ana Reyes", recipient_phone: "09091234567",
    address_line1: "789 EDSA", address_city: "Pasig City",
    lat: 14.5875, lng: 121.0607, cod_amount: 3200, attempt_count: 1,
  },
  {
    id: "t4", shipment_id: "s4", tracking_number: "LS-M3N4O5P6",
    sequence: 4, status: "failed", task_type: "delivery",
    recipient_name: "Pedro Garcia", recipient_phone: "09551234567",
    address_line1: "22 Taguig St", address_city: "Taguig City",
    lat: 14.5247, lng: 121.0775, attempt_count: 2,
    special_notes: "Call before delivery. Gate is locked.",
  },
];

// ── Task status config ────────────────────────────────────────────────────────

const STATUS_CONFIG: Record<string, { label: string; color: string }> = {
  awaiting_pickup:  { label: "Collect Now",  color: PURPLE },
  pickup_confirmed: { label: "Picked Up",    color: GREEN  },
  assigned:         { label: "To Deliver",   color: CYAN   },
  navigating:       { label: "On the Way",   color: AMBER  },
  arrived:          { label: "Arrived",      color: PURPLE },
  pod_pending:      { label: "POD Needed",   color: AMBER  },
  completed:        { label: "Delivered",    color: GREEN  },
  failed:           { label: "Failed",       color: RED    },
};

// ── Task card component ───────────────────────────────────────────────────────

interface TaskCardProps {
  task:  DeliveryTask;
  index: number;
}

function TaskCard({ task, index }: TaskCardProps) {
  const cfg       = STATUS_CONFIG[task.status] ?? { label: task.status, color: CYAN };
  const isPickup  = task.task_type === "pickup";
  const accentClr = isPickup ? PURPLE : CYAN;
  const isActive  = task.status === "assigned" || task.status === "navigating" || task.status === "awaiting_pickup";

  function handlePress() {
    Haptics.impactAsync(Haptics.ImpactFeedbackStyle.Light);
    router.push(`/task/${task.id}`);
  }

  return (
    <Animated.View entering={FadeInDown.delay(index * 60).springify()}>
      <Pressable
        onPress={handlePress}
        style={({ pressed }) => [
          styles.card,
          isPickup && { borderColor: `${PURPLE}30`, backgroundColor: `${PURPLE}06` },
          pressed && styles.cardPressed,
        ]}
      >
        {/* Sequence + Status */}
        <View style={styles.cardHeader}>
          <View style={[styles.sequenceBadge, { backgroundColor: `${accentClr}12`, borderColor: `${accentClr}20` }]}>
            {isPickup
              ? <Text style={[styles.sequenceText, { color: PURPLE }]}>↑</Text>
              : <Text style={[styles.sequenceText, { color: CYAN }]}>{String(task.sequence).padStart(2, "0")}</Text>
            }
          </View>

          <View style={[styles.statusBadge, { borderColor: `${cfg.color}40`, backgroundColor: `${cfg.color}12` }]}>
            <View style={[styles.statusDot, { backgroundColor: cfg.color }]} />
            <Text style={[styles.statusText, { color: cfg.color }]}>{cfg.label}</Text>
          </View>

          {task.eta_minutes != null && isActive && (
            <Text style={styles.etaText}>{task.eta_minutes}m away</Text>
          )}
        </View>

        {/* Name + address */}
        <Text style={styles.recipientName}>
          {isPickup ? (task.sender_name ?? "Unknown Sender") : task.recipient_name}
        </Text>
        <Text style={styles.address}>{task.address_line1} · {task.address_city}</Text>
        <Text style={styles.tracking}>{task.tracking_number}</Text>

        {/* Pickup: package description */}
        {isPickup && task.package_desc && (
          <View style={styles.packageRow}>
            <Text style={styles.packageText}>{task.package_desc}</Text>
            {task.package_weight && (
              <Text style={styles.packageWeight}>{task.package_weight}</Text>
            )}
          </View>
        )}

        {/* Delivery: COD + attempt */}
        {!isPickup && (
          <View style={styles.cardFooter}>
            {task.cod_amount ? (
              <View style={styles.codBadge}>
                <Text style={styles.codText}>COD ₱{task.cod_amount.toLocaleString("en-PH")}</Text>
              </View>
            ) : (
              <View style={styles.prepaidBadge}>
                <Text style={styles.prepaidText}>Prepaid</Text>
              </View>
            )}
            {task.attempt_count > 0 && (
              <Text style={styles.attemptText}>Attempt #{task.attempt_count + 1}</Text>
            )}
            {task.special_notes && <Text style={styles.noteIcon}>📝</Text>}
          </View>
        )}
      </Pressable>
    </Animated.View>
  );
}

// ── Summary header ────────────────────────────────────────────────────────────

function SummaryHeader({ tasks }: { tasks: DeliveryTask[] }) {
  const deliveries = tasks.filter((t) => t.task_type === "delivery");
  const pickups    = tasks.filter((t) => t.task_type === "pickup" && t.status === "awaiting_pickup");
  const total      = deliveries.length;
  const done       = deliveries.filter((t) => t.status === "completed").length;
  const remaining  = deliveries.filter((t) => t.status === "assigned" || t.status === "navigating").length;
  const failed     = deliveries.filter((t) => t.status === "failed").length;
  const pct        = total > 0 ? Math.round((done / total) * 100) : 0;

  return (
    <View style={styles.summaryCard}>
      <View style={styles.summaryRow}>
        <View style={styles.summaryItem}>
          <Text style={[styles.summaryValue, { color: CYAN }]}>{remaining}</Text>
          <Text style={styles.summaryLabel}>Remaining</Text>
        </View>
        <View style={styles.summaryDivider} />
        <View style={styles.summaryItem}>
          <Text style={[styles.summaryValue, { color: GREEN }]}>{done}</Text>
          <Text style={styles.summaryLabel}>Delivered</Text>
        </View>
        <View style={styles.summaryDivider} />
        <View style={styles.summaryItem}>
          <Text style={[styles.summaryValue, { color: RED }]}>{failed}</Text>
          <Text style={styles.summaryLabel}>Failed</Text>
        </View>
        {pickups.length > 0 && (
          <>
            <View style={styles.summaryDivider} />
            <View style={styles.summaryItem}>
              <Text style={[styles.summaryValue, { color: PURPLE }]}>{pickups.length}</Text>
              <Text style={styles.summaryLabel}>Pickups</Text>
            </View>
          </>
        )}
      </View>

      {/* Progress bar */}
      <View style={styles.progressBar}>
        <View style={[styles.progressFill, { width: `${pct}%` }]} />
      </View>
      <Text style={styles.progressLabel}>{pct}% deliveries complete</Text>
    </View>
  );
}

// ── Screen ────────────────────────────────────────────────────────────────────

export default function TaskListScreen() {
  const dispatch = useDispatch<AppDispatch>();
  const tasks    = useSelector((s: RootState) => s.tasks.tasks);

  // Initial load — seed tasks + driver earnings config + demo history
  useEffect(() => {
    dispatch(taskActions.setTasks(MOCK_TASKS));

    // Set driver commission config (part-time: ₱85/delivery + 2% COD bonus)
    dispatch(earningsActions.setDriverConfig({
      driverType:        "part_time",
      commissionRate:    85,
      codCommissionRate: 0.02,
    }));

    // Seed some past delivery earnings for demo
    const today = new Date().toISOString().slice(0, 10);
    dispatch(earningsActions.recordDeliveryEarning({
      taskId: "t3", shipmentId: "s3",
      completedAt: `${today}T08:45:00.000Z`,
      baseAmount: 85, codBonus: 64, total: 149,
    }));
    dispatch(earningsActions.recordDeliveryEarning({
      taskId: "demo-a", shipmentId: "s-demo-a",
      completedAt: `${today}T07:22:00.000Z`,
      baseAmount: 85, codBonus: 0, total: 85,
    }));
    dispatch(earningsActions.recordDeliveryEarning({
      taskId: "demo-b", shipmentId: "s-demo-b",
      completedAt: `${today}T06:10:00.000Z`,
      baseAmount: 85, codBonus: 0, total: 85,
    }));
  }, []);

  const onRefresh = useCallback(() => {
    // In production: trigger re-fetch from API
    dispatch(taskActions.setTasks(MOCK_TASKS));
  }, [dispatch]);

  const pickupTasks    = tasks.filter((t) => t.task_type === "pickup");
  const activeTasks    = tasks.filter((t) => t.task_type === "delivery" && t.status !== "completed" && t.status !== "failed");
  const completedTasks = tasks.filter((t) => t.task_type === "delivery" && (t.status === "completed" || t.status === "failed"));

  type ListItem =
    | { type: "header"; label: string; color?: string }
    | { type: "task"; task: DeliveryTask; index: number };

  const allItems: ListItem[] = [
    ...(pickupTasks.length > 0
      ? [
          { type: "header" as const, label: `First-Mile Pickups · ${pickupTasks.length}`, color: PURPLE },
          ...pickupTasks.map((task, i) => ({ type: "task" as const, task, index: i })),
        ]
      : []),
    { type: "header" as const, label: `Deliveries · ${activeTasks.length}` },
    ...activeTasks.map((task, i) => ({ type: "task" as const, task, index: pickupTasks.length + i })),
    ...(completedTasks.length > 0
      ? [
          { type: "header" as const, label: `Completed · ${completedTasks.length}` },
          ...completedTasks.map((task, i) => ({ type: "task" as const, task, index: pickupTasks.length + activeTasks.length + i })),
        ]
      : []),
  ];

  return (
    <View style={styles.container}>
      <FlashList
        data={allItems}
        estimatedItemSize={120}
        ListHeaderComponent={<SummaryHeader tasks={tasks} />}
        renderItem={({ item }) => {
          if (item.type === "header") {
            return (
              <Text style={[styles.sectionHeader, item.color ? { color: item.color } : {}]}>
                {item.label}
              </Text>
            );
          }
          return <TaskCard task={item.task} index={item.index} />;
        }}
        keyExtractor={(item) =>
          item.type === "header" ? `header-${item.label}` : `task-${item.task.id}`
        }
        refreshControl={
          <RefreshControl
            refreshing={false}
            onRefresh={onRefresh}
            tintColor={CYAN}
            colors={[CYAN]}
          />
        }
        contentContainerStyle={{ paddingBottom: 32 }}
      />
    </View>
  );
}

// ── Styles ────────────────────────────────────────────────────────────────────

const styles = StyleSheet.create({
  container:     { flex: 1, backgroundColor: CANVAS },
  // Summary card
  summaryCard:   { margin: 12, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 16 },
  summaryRow:    { flexDirection: "row", justifyContent: "space-around", marginBottom: 14 },
  summaryItem:   { alignItems: "center", gap: 2 },
  summaryValue:  { fontSize: 28, fontWeight: "700", fontFamily: "SpaceGrotesk-Bold" },
  summaryLabel:  { fontSize: 10, color: "rgba(255,255,255,0.35)", fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1 },
  summaryDivider:{ width: 1, backgroundColor: BORDER, alignSelf: "stretch" },
  progressBar:   { height: 3, backgroundColor: "rgba(255,255,255,0.08)", borderRadius: 999, overflow: "hidden" },
  progressFill:  { height: "100%", backgroundColor: GREEN, borderRadius: 999 },
  progressLabel: { marginTop: 6, fontSize: 10, color: "rgba(255,255,255,0.25)", fontFamily: "JetBrainsMono-Regular", textAlign: "right" },
  // Section header
  sectionHeader: { paddingHorizontal: 12, paddingTop: 12, paddingBottom: 4, fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1.5, color: "rgba(255,255,255,0.25)" },
  // Task card
  card:          { marginHorizontal: 12, marginVertical: 4, borderRadius: 12, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, padding: 14 },
  cardPressed:   { opacity: 0.75 },
  cardHeader:    { flexDirection: "row", alignItems: "center", gap: 8, marginBottom: 8 },
  sequenceBadge: { width: 28, height: 28, borderRadius: 8, backgroundColor: "rgba(0,229,255,0.12)", alignItems: "center", justifyContent: "center", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)" },
  sequenceText:  { fontSize: 11, fontFamily: "JetBrainsMono-Bold", color: CYAN },
  statusBadge:   { flexDirection: "row", alignItems: "center", gap: 5, borderRadius: 999, borderWidth: 1, paddingHorizontal: 8, paddingVertical: 3 },
  statusDot:     { width: 6, height: 6, borderRadius: 999 },
  statusText:    { fontSize: 10, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 0.5 },
  etaText:       { marginLeft: "auto", fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: AMBER },
  recipientName: { fontSize: 15, fontWeight: "600", color: "#FFFFFF", fontFamily: "SpaceGrotesk-SemiBold", marginBottom: 2 },
  address:       { fontSize: 12, color: "rgba(255,255,255,0.45)", marginBottom: 2 },
  tracking:      { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)", marginBottom: 10 },
  cardFooter:    { flexDirection: "row", alignItems: "center", gap: 8, marginTop: 4 },
  packageRow:    { flexDirection: "row", alignItems: "center", gap: 8, marginTop: 2 },
  packageText:   { flex: 1, fontSize: 11, color: "rgba(168,85,247,0.7)", fontFamily: "JetBrainsMono-Regular" },
  packageWeight: { fontSize: 10, color: "rgba(168,85,247,0.5)", fontFamily: "JetBrainsMono-Regular", borderWidth: 1, borderColor: "rgba(168,85,247,0.2)", borderRadius: 4, paddingHorizontal: 5, paddingVertical: 1 },
  codBadge:      { paddingHorizontal: 8, paddingVertical: 3, borderRadius: 6, backgroundColor: "rgba(255,171,0,0.12)", borderWidth: 1, borderColor: "rgba(255,171,0,0.25)" },
  codText:       { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: AMBER },
  prepaidBadge:  { paddingHorizontal: 8, paddingVertical: 3, borderRadius: 6, backgroundColor: "rgba(0,255,136,0.08)", borderWidth: 1, borderColor: "rgba(0,255,136,0.2)" },
  prepaidText:   { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: GREEN },
  attemptText:   { fontSize: 10, color: RED, fontFamily: "JetBrainsMono-Regular" },
  noteIcon:      { marginLeft: "auto", fontSize: 14 },
});
