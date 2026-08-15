/**
 * Choose an item's options before it goes in the basket.
 *
 * Only appears for items that actually offer choices, which is a minority — a
 * picker that opened for every tap would put a sheet between the customer and
 * the thing they already decided to buy.
 *
 * Deliberately not a react-native `Modal`: this app also ships as an Expo web
 * export, where Modal does not render. An absolutely-positioned overlay behaves
 * the same on all three targets.
 *
 * The running total shown here is a preview, not the price. The server
 * recomputes it from the catalog when the line is added — a total this screen
 * could set would be a total the customer could set.
 */
import { useMemo, useState } from "react";
import { Pressable, ScrollView, Text, View } from "react-native";

import type { ModifierGroup, SearchHit } from "@/api/catalog";
import { theme } from "@/theme";

const peso = (cents: number) => `₱${(cents / 100).toFixed(2)}`;

/** How a group reads to a customer, rather than as min/max numbers. */
function groupHint(g: ModifierGroup): string {
  if (g.min_select === 0 && g.max_select === 1) return "Optional · pick one";
  if (g.min_select === 0) return `Optional · up to ${g.max_select}`;
  if (g.min_select === g.max_select) {
    return g.min_select === 1 ? "Required · pick one" : `Required · pick ${g.min_select}`;
  }
  return `Required · ${g.min_select}–${g.max_select}`;
}

export function ModifierPicker({
  item,
  onCancel,
  onConfirm,
}: {
  item: SearchHit;
  onCancel: () => void;
  onConfirm: (optionIds: string[]) => void;
}) {
  const groups = item.modifiers ?? [];
  const [chosen, setChosen] = useState<string[]>([]);

  const toggle = (g: ModifierGroup, optionId: string) => {
    setChosen((prev) => {
      if (prev.includes(optionId)) return prev.filter((x) => x !== optionId);

      const inThisGroup = prev.filter((id) => g.options.some((o) => o.id === id));
      // At the cap, the newest choice replaces the oldest in this group rather
      // than being silently dropped. For the common pick-one group that makes
      // the options behave like radio buttons without any extra wiring.
      if (inThisGroup.length >= g.max_select) {
        const drop = inThisGroup[0];
        return [...prev.filter((x) => x !== drop), optionId];
      }
      return [...prev, optionId];
    });
  };

  const { total, unmet } = useMemo(() => {
    let delta = 0;
    const missing: string[] = [];
    for (const g of groups) {
      const n = chosen.filter((id) => g.options.some((o) => o.id === id)).length;
      if (n < g.min_select) missing.push(g.name);
      for (const o of g.options) {
        if (chosen.includes(o.id)) delta += o.price_delta_cents;
      }
    }
    return { total: item.price_cents + delta, unmet: missing };
  }, [chosen, groups, item.price_cents]);

  return (
    <View
      style={{
        position: "absolute",
        top: 0,
        left: 0,
        right: 0,
        bottom: 0,
        backgroundColor: "rgba(0,0,0,0.65)",
        justifyContent: "flex-end",
      }}
    >
      {/* Tapping the dimmed area backs out — the expected gesture, and the only
          one available to someone who cannot reach the Cancel button. */}
      <Pressable style={{ flex: 1 }} onPress={onCancel} accessibilityLabel="Close options" />

      <View
        style={{
          backgroundColor: theme.canvas,
          borderTopLeftRadius: 20,
          borderTopRightRadius: 20,
          borderTopWidth: 1,
          borderColor: "rgba(255,255,255,0.1)",
          padding: 20,
          gap: 14,
          maxHeight: "80%",
        }}
      >
        <View>
          <Text style={{ color: theme.text, fontSize: 16, fontWeight: "600" }}>{item.name}</Text>
          <Text style={{ color: theme.muted, fontSize: 12 }}>{peso(item.price_cents)} base</Text>
        </View>

        <ScrollView style={{ flexGrow: 0 }} contentContainerStyle={{ gap: 16 }}>
          {groups.map((g) => (
            <View key={g.id} style={{ gap: 8 }}>
              <View>
                <Text style={{ color: theme.text, fontSize: 13, fontWeight: "600" }}>{g.name}</Text>
                <Text style={{ color: theme.muted, fontSize: 11 }}>{groupHint(g)}</Text>
              </View>

              {g.options.map((o) => {
                const on = chosen.includes(o.id);
                return (
                  <Pressable
                    key={o.id}
                    onPress={() => toggle(g, o.id)}
                    accessibilityRole="button"
                    accessibilityState={{ selected: on }}
                    accessibilityLabel={`${o.name}${
                      o.price_delta_cents !== 0 ? `, ${peso(o.price_delta_cents)}` : ""
                    }`}
                    style={{
                      flexDirection: "row",
                      alignItems: "center",
                      justifyContent: "space-between",
                      paddingVertical: 10,
                      paddingHorizontal: 12,
                      borderRadius: 12,
                      borderWidth: 1,
                      borderColor: on ? theme.cyan : "rgba(255,255,255,0.1)",
                      backgroundColor: on ? "rgba(0,229,255,0.08)" : "rgba(255,255,255,0.03)",
                    }}
                  >
                    <Text style={{ color: on ? theme.cyan : theme.text, fontSize: 13 }}>
                      {o.name}
                    </Text>
                    {o.price_delta_cents !== 0 && (
                      <Text style={{ color: theme.muted, fontSize: 12 }}>
                        {o.price_delta_cents > 0 ? "+" : "−"}
                        {peso(Math.abs(o.price_delta_cents))}
                      </Text>
                    )}
                  </Pressable>
                );
              })}
            </View>
          ))}
        </ScrollView>

        {unmet.length > 0 && (
          <Text style={{ color: theme.muted, fontSize: 11 }}>
            Still to choose: {unmet.join(", ")}
          </Text>
        )}

        <View style={{ flexDirection: "row", gap: 10 }}>
          <Pressable
            onPress={onCancel}
            accessibilityRole="button"
            accessibilityLabel="Cancel"
            style={{
              paddingVertical: 12,
              paddingHorizontal: 18,
              borderRadius: 12,
              borderWidth: 1,
              borderColor: "rgba(255,255,255,0.12)",
            }}
          >
            <Text style={{ color: theme.muted, fontSize: 13 }}>Cancel</Text>
          </Pressable>

          <Pressable
            onPress={() => onConfirm(chosen)}
            disabled={unmet.length > 0}
            accessibilityRole="button"
            accessibilityState={{ disabled: unmet.length > 0 }}
            accessibilityLabel={`Add for ${peso(total)}`}
            style={{
              flex: 1,
              paddingVertical: 12,
              borderRadius: 12,
              alignItems: "center",
              backgroundColor: unmet.length > 0 ? "rgba(255,255,255,0.06)" : "rgba(0,229,255,0.15)",
              borderWidth: 1,
              borderColor: unmet.length > 0 ? "rgba(255,255,255,0.1)" : theme.cyan,
            }}
          >
            <Text
              style={{
                color: unmet.length > 0 ? theme.muted : theme.cyan,
                fontSize: 13,
                fontWeight: "600",
              }}
            >
              Add · {peso(total)}
            </Text>
          </Pressable>
        </View>
      </View>
    </View>
  );
}
