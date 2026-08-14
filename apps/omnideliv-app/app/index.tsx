import { useState } from "react";
import { Pressable, Text, TextInput, View } from "react-native";
import { SafeAreaView } from "react-native-safe-area-context";
import { useRouter } from "expo-router";

import { IntentPills } from "@/components/IntentPills";
import { theme } from "@/theme";

function greeting(now = new Date()): string {
  const h = now.getHours();
  if (h < 12) return "Good morning";
  if (h < 18) return "Good afternoon";
  return "Good evening";
}

export default function OmniIntentCanvas() {
  const [utterance, setUtterance] = useState("");
  const router = useRouter();

  const submit = () => {
    const text = utterance.trim();
    if (!text) return;
    router.push({ pathname: "/orchestrating", params: { utterance: text } });
  };

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: theme.canvas }}>
      <View style={{ flex: 1, padding: 20, gap: 16 }}>
        <Text style={{ color: theme.muted, fontSize: 14 }}>{greeting()}.</Text>

        <View
          style={{
            backgroundColor: "rgba(255,255,255,0.06)",
            borderColor: "rgba(0,229,255,0.38)",
            borderWidth: 1,
            borderRadius: theme.radius.lg,
            padding: 14,
            shadowColor: theme.cyan,
            shadowOpacity: 0.16,
            shadowRadius: 24,
          }}
        >
          <Text style={{ color: "rgba(0,229,255,0.6)", fontSize: 10, letterSpacing: 1.2, marginBottom: 6 }}>
            TELL ME WHAT YOU NEED
          </Text>
          <TextInput
            value={utterance}
            onChangeText={setUtterance}
            onSubmitEditing={submit}
            multiline
            placeholder="Dinner for two from Kuya's, and we're out of milk and eggs"
            placeholderTextColor={theme.faint}
            accessibilityLabel="What do you need?"
            style={{ color: theme.text, fontSize: 15, lineHeight: 21, minHeight: 56 }}
          />
          <Pressable
            accessibilityRole="button"
            accessibilityLabel="Send"
            disabled={!utterance.trim()}
            onPress={submit}
            style={{
              alignSelf: "flex-end",
              marginTop: 10,
              backgroundColor: utterance.trim() ? theme.cyan : "rgba(255,255,255,0.08)",
              borderRadius: 999,
              paddingHorizontal: 16,
              paddingVertical: 8,
            }}
          >
            <Text style={{ color: utterance.trim() ? theme.canvas : theme.faint, fontWeight: "700" }}>
              Go
            </Text>
          </Pressable>
        </View>

        <View>
          <Text style={{ color: theme.faint, fontSize: 10, letterSpacing: 1.2, marginBottom: 6 }}>
            OR JUMP STRAIGHT IN
          </Text>
          <IntentPills />
        </View>

        {/* The way back to an order in flight. Checkout used to be a one-way
            door: it handed you a tracking screen and nothing linked back, so
            closing the app lost the order until it turned up at the door. */}
        <Pressable
          onPress={() => router.push("/orders")}
          style={{ paddingVertical: 12 }}
          accessibilityRole="button"
        >
          <Text style={{ color: theme.cyan, fontSize: 14, fontWeight: "600" }}>
            Your orders →
          </Text>
        </Pressable>
      </View>
    </SafeAreaView>
  );
}
