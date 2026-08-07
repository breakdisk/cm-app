/**
 * The non-AI fallback, per the platform rule that every operation has one.
 *
 * These route to deterministic category browse with no model in the path. If
 * Claude is down, the mesh times out, or the tenant is on a non-AI plan, this
 * is still a working app. That is their job — they are not decoration.
 */
import { Pressable, ScrollView, Text } from "react-native";
import { useRouter } from "expo-router";

import { theme } from "@/theme";

const PILLS = [
  { vertical: "restaurant", emoji: "🍔", label: "Order Food" },
  { vertical: "grocery",    emoji: "🛒", label: "Restock" },
  { vertical: "pharmacy",   emoji: "💊", label: "Refill Rx" },
  { vertical: "florist",    emoji: "💐", label: "Flowers" },
  { vertical: "retail",     emoji: "📦", label: "Shop" },
] as const;

export function IntentPills() {
  const router = useRouter();

  return (
    <ScrollView
      horizontal
      showsHorizontalScrollIndicator={false}
      contentContainerStyle={{ gap: 6, paddingVertical: 4 }}
    >
      {PILLS.map((p) => (
        <Pressable
          key={p.vertical}
          accessibilityRole="button"
          accessibilityLabel={p.label}
          onPress={() => router.push(`/browse/${p.vertical}`)}
          style={{
            backgroundColor: theme.surface,
            borderColor: theme.border,
            borderWidth: 1,
            borderRadius: 999,
            paddingHorizontal: 12,
            paddingVertical: 7,
          }}
        >
          <Text style={{ color: theme.muted, fontSize: 13 }}>
            {p.emoji}  {p.label}
          </Text>
        </Pressable>
      ))}
    </ScrollView>
  );
}
