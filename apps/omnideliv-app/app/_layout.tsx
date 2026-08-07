import { Stack } from "expo-router";
import { theme } from "@/theme";

export default function RootLayout() {
  return (
    <Stack
      screenOptions={{
        headerShown: false,
        contentStyle: { backgroundColor: theme.canvas },
        animation: "fade",
      }}
    />
  );
}
