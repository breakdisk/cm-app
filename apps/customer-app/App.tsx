/**
 * Customer App — Entry Point
 * LogisticOS customer mobile app for shipment tracking, booking, loyalty.
 */
import { LogBox } from "react-native";
import { useEffect } from "react";
import Constants, { ExecutionEnvironment } from "expo-constants";

LogBox.ignoreLogs([
  "expo-notifications: Android Push notifications",
  "expo-notifications: iOS Push notifications",
  "`expo-notifications` functionality is not fully supported in Expo Go",
]);
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { Provider } from "react-redux";
import { useNetInfo } from "@react-native-community/netinfo";
import * as TaskManager from "expo-task-manager";
import * as BackgroundFetch from "expo-background-fetch";
import { store } from "./src/store";
import { setCredentials } from "./src/store/slices/auth";
import { hydrateBrandingFromCache, fetchBranding } from "./src/store/slices/branding";
import { getStoredToken, getStoredCustomerId } from "./src/services/api/auth";
import * as SecureStore from "expo-secure-store";
import { AppNavigator } from "./src/navigation/AppNavigator";
import { initializeDatabase } from "./src/db/sqlite";
import { syncShipments } from "./src/db/sync";

const BACKGROUND_SYNC_TASK = "background-sync-task";

const isExpoGo = Constants.executionEnvironment === ExecutionEnvironment.StoreClient;

if (!isExpoGo) {
  TaskManager.defineTask(BACKGROUND_SYNC_TASK, async () => {
    try {
      await syncShipments();
      return BackgroundFetch.BackgroundFetchResult.NewData;
    } catch (error) {
      console.error("Background sync failed:", error);
      return BackgroundFetch.BackgroundFetchResult.Failed;
    }
  });
}

/**
 * Main App Component
 * Initializes database and background sync on startup
 */
function AppContent() {
  const { isConnected } = useNetInfo();

  useEffect(() => {
    // ── Session restore ────────────────────────────────────────────────────
    // On every cold start, read the persisted JWT from SecureStore. If a
    // token exists the user was previously signed in — restore the minimum
    // Redux state needed for the navigator to skip the onboarding stack.
    // kycStatus is intentionally left at the default 'none'; ProfileScreen
    // syncs the live value from the backend lazily on mount.
    const restoreSession = async () => {
      try {
        const token      = await getStoredToken();
        const customerId = await getStoredCustomerId();
        const phone      = await SecureStore.getItemAsync("customer_phone");
        const name       = await SecureStore.getItemAsync("customer_name").catch(() => null);
        if (token && customerId) {
          store.dispatch(setCredentials({
            token,
            customerId,
            phone:           phone ?? null,
            name:            name  ?? null,
            onboardingStep:  "complete",
            isGuest:         false,
          }));
        }
      } catch {
        // SecureStore unavailable — stay on onboarding screen
      }
    };

    restoreSession();

    // ── White-label branding ───────────────────────────────────────────────
    // Emit the cached brand immediately for instant theming, then refresh from
    // the network (authed if signed in, else public by tenant slug).
    store.dispatch(hydrateBrandingFromCache());
    store.dispatch(fetchBranding());

    // ── Database init ──────────────────────────────────────────────────────
    const initDb = async () => {
      try {
        await initializeDatabase();
        console.log("Database initialized successfully");
      } catch (err) {
        console.error("Failed to initialize database:", err);
      }
    };

    initDb();

    if (!isExpoGo) {
      const registerBackgroundFetch = async () => {
        try {
          await BackgroundFetch.registerTaskAsync(BACKGROUND_SYNC_TASK, {
            minimumInterval: 15 * 60,
            stopOnTerminate: false,
            startOnBoot: true,
          });
          console.log("Background fetch task registered successfully");
        } catch (err) {
          console.warn("Background fetch registration failed:", err);
        }
      };

      registerBackgroundFetch();
    }
  }, []);

  useEffect(() => {
    if (isConnected) {
      const syncOnReconnect = async () => {
        try {
          await syncShipments();
        } catch (err: any) {
          const status = err?.status ?? err?.response?.status;
          if (status !== 401 && status !== 403) {
            console.error("Manual sync failed:", err);
          }
        }
      };

      syncOnReconnect();
    }
  }, [isConnected]);

  return (
    <GestureHandlerRootView style={{ flex: 1 }}>
      <SafeAreaProvider>
        <Provider store={store}>
          <AppNavigator />
        </Provider>
      </SafeAreaProvider>
    </GestureHandlerRootView>
  );
}

export default function App() {
  return <AppContent />;
}
