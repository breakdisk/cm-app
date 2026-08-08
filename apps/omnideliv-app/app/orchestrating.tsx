import { useEffect, useMemo } from "react";
import { ScrollView, Text, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useLocalSearchParams, useRouter } from "expo-router";

import { AgentCard, type CardState } from "@/components/AgentCard";
import { currentDeliveryPoint } from "@/deliveryPoint";
import { useMeshRun } from "@/hooks/useMeshRun";
import { theme } from "@/theme";

export default function Orchestrating() {
  const { utterance } = useLocalSearchParams<{ utterance: string }>();
  const { events, running, error, run, cancel } = useMeshRun();
  const router = useRouter();

  useEffect(() => {
    // The same point browsing and checkout use, so the shops the agent
    // finds are the shops the customer was just looking at.
    if (utterance) void run(utterance, currentDeliveryPoint());
    return cancel;
  }, [utterance, run, cancel]);

  // Fold the event stream into one card per specialist.
  const cards = useMemo(() => {
    const byId = new Map<string, { label: string; vertical: string; state: CardState; note?: string | null }>();
    for (const e of events) {
      if (e.event === "specialist_started") {
        byId.set(e.sub_intent_id, { label: e.label, vertical: e.vertical, state: "working" });
      } else if (e.event === "specialist_progress") {
        const c = byId.get(e.sub_intent_id);
        if (c) byId.set(e.sub_intent_id, { ...c, note: e.note });
      } else if (e.event === "specialist_finished") {
        const c = byId.get(e.sub_intent_id);
        if (c) byId.set(e.sub_intent_id, { ...c, state: e.degraded ? "degraded" : "done", note: e.note });
      }
    }
    return [...byId.entries()];
  }, [events]);

  const constraint = events.find((e) => e.event === "constraint_detected");
  const completed  = events.find((e) => e.event === "completed");
  const failed     = events.find((e) => e.event === "failed");

  // A failed run means the mesh produced nothing usable. Back to the canvas,
  // where the intent pills are a working deterministic path — rather than
  // forward to a checkout screen showing an empty basket.
  useEffect(() => {
    if (failed) router.replace("/");
  }, [failed, router]);

  useEffect(() => {
    if (completed && completed.event === "completed") {
      router.replace({ pathname: "/review", params: { basketId: completed.basket_id } });
    }
  }, [completed, router]);

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <ScrollView contentContainerStyle={{ padding: 20, gap: 14 }}>
        <Text style={{ color: theme.text, fontSize: 18, fontWeight: "600", lineHeight: 24 }}>
          {cards.length > 1 ? `Got it — working on ${cards.length} things at once.` : "Got it — working on it."}
        </Text>

        <View
          style={{
            backgroundColor: theme.surface,
            borderColor: theme.border,
            borderWidth: 1,
            borderRadius: theme.radius.md,
            paddingHorizontal: 13,
          }}
        >
          {cards.map(([id, c]) => (
            <AgentCard key={id} label={c.label} vertical={c.vertical} state={c.state} note={c.note} />
          ))}
          {cards.length === 0 && running && (
            <Text style={{ color: theme.faint, fontSize: 12, paddingVertical: 14 }}>
              Reading your message…
            </Text>
          )}
        </View>

        {constraint?.event === "constraint_detected" && (
          <View
            style={{
              borderLeftWidth: 2,
              borderLeftColor: theme.amber,
              backgroundColor: "rgba(255,171,0,0.08)",
              borderRadius: theme.radius.sm,
              padding: 11,
            }}
          >
            <Text style={{ color: theme.amber, fontSize: 9.5, letterSpacing: 1, marginBottom: 4 }}>
              WORTH KNOWING
            </Text>
            <Text style={{ color: "rgba(255,255,255,0.8)", fontSize: 12 }}>
              {constraint.description}
            </Text>
          </View>
        )}

        {error && (
          <Text accessibilityRole="alert" style={{ color: theme.amber, fontSize: 12 }}>
            Lost the connection. Your basket is saved — pull back to reopen it.
          </Text>
        )}
      </ScrollView>
    </SafeAreaView>
  );
}
