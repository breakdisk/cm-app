import { Text, View } from "react-native";
import { theme } from "@/theme";

export type CardState = "working" | "done" | "degraded";

export function AgentCard({
  label,
  vertical,
  state,
  note,
}: {
  label: string;
  vertical: string;
  state: CardState;
  note?: string | null;
}) {
  const accent =
    state === "done" ? theme.green : state === "degraded" ? theme.amber : theme.cyan;

  const status =
    state === "done" ? "DONE" : state === "degraded" ? "UNAVAILABLE" : "CHECKING";

  return (
    <View
      accessibilityRole="summary"
      accessibilityLabel={`${label}, ${status.toLowerCase()}`}
      style={{
        flexDirection: "row",
        gap: 10,
        paddingVertical: 10,
        borderBottomWidth: 1,
        borderBottomColor: "rgba(255,255,255,0.06)",
      }}
    >
      <View
        style={{
          width: 29, height: 29, borderRadius: 9,
          borderWidth: 1, borderColor: `${accent}66`,
          backgroundColor: `${accent}22`,
        }}
      />
      <View style={{ flex: 1 }}>
        <View style={{ flexDirection: "row", justifyContent: "space-between" }}>
          <Text style={{ color: theme.text, fontSize: 12, fontWeight: "600" }}>{label}</Text>
          <Text style={{ color: accent, fontSize: 9.5, fontWeight: "700", letterSpacing: 0.5 }}>
            {status}
          </Text>
        </View>
        <Text style={{ color: theme.muted, fontSize: 10.5, marginTop: 2 }}>
          {/* A degraded card says what the customer can still do, not what broke. */}
          {state === "degraded"
            ? `Couldn't check ${vertical} — you can browse it yourself`
            : note ?? vertical}
        </Text>
      </View>
    </View>
  );
}
