/**
 * Driver App — Expo Router root layout.
 * Sets up navigation stack, bootstraps SQLite, and starts the offline sync service.
 */
import { useEffect } from "react";
import { Stack } from "expo-router";
import { StatusBar } from "expo-status-bar";
import { Provider } from "react-redux";
import { store } from "../store";
import { deliveryQueue } from "../services/storage/delivery_queue";
import { offlineSync } from "../services/sync/offline-sync";
import { tokenStore } from "../services/auth/token-store";

const CANVAS = "#050810";

// Module-level ref so the sync service's synchronous getter can access it.
// Auth screens must call: import { tokenRef } from "@/app/_layout"; tokenRef.current = token;
export const tokenRef = { current: null as string | null };

export default function RootLayout() {
  useEffect(() => {
    let started = false;

    async function bootstrap() {
      try {
        await deliveryQueue.open();
        offlineSync.start(() => tokenRef.current);
        started = true;
        tokenStore.getAccessToken().then((t) => { tokenRef.current = t; });
      } catch {
        // Native-only services (SQLite, SecureStore) are unavailable on web — skip.
      }
    }

    bootstrap();

    return () => {
      if (started) offlineSync.stop();
    };
  }, []);

  return (
    <Provider store={store}>
      <StatusBar style="light" backgroundColor={CANVAS} />
      <Stack
        screenOptions={{
          headerStyle:          { backgroundColor: CANVAS },
          headerTintColor:      "#00E5FF",
          headerTitleStyle:     { fontFamily: "SpaceGrotesk-SemiBold", color: "#FFFFFF" },
          headerShadowVisible:  false,
          contentStyle:         { backgroundColor: CANVAS },
          animation:            "slide_from_right",
        }}
      >
        <Stack.Screen name="(tabs)"   options={{ headerShown: false }} />
        <Stack.Screen name="task/[id]" options={{ title: "Delivery Task", presentation: "modal" }} />
        <Stack.Screen name="pod/[id]"  options={{ title: "Proof of Delivery", presentation: "fullScreenModal" }} />
      </Stack>
    </Provider>
  );
}
