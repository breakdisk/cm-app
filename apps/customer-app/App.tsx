/**
 * Customer App — Entry Point
 * LogisticOS customer mobile app for shipment tracking, booking, loyalty.
 */
import { useEffect } from "react";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { SafeAreaProvider } from "react-native-safe-area-context";
import { Provider } from "react-redux";
import { useNetInfo } from "@react-native-community/netinfo";
import * as TaskManager from "expo-task-manager";
import * as BackgroundFetch from "expo-background-fetch";
import { store } from "./src/store";
import { AppNavigator } from "./src/navigation/AppNavigator";
import { initializeDatabase } from "./src/db/sqlite";
import { syncShipments } from "./src/db/sync";

const BACKGROUND_SYNC_TASK = "background-sync-task";

/**
 * Define background sync task
 * This task runs periodically (every 15 minutes) to sync pending shipments
 */
TaskManager.defineTask(BACKGROUND_SYNC_TASK, async () => {
  try {
    await syncShipments();
    return BackgroundFetch.BackgroundFetchResult.NewData;
  } catch (error) {
    console.error("Background sync failed:", error);
    return BackgroundFetch.BackgroundFetchResult.Failed;
  }
});

/**
 * Main App Component
 * Initializes database and background sync on startup
 */
function AppContent() {
  const { isConnected } = useNetInfo();

  useEffect(() => {
    // Initialize database on app startup
    const initDb = async () => {
      try {
        await initializeDatabase();
        console.log("Database initialized successfully");
      } catch (err) {
        console.error("Failed to initialize database:", err);
      }
    };

    initDb();

    // Register background fetch task for periodic syncing
    const registerBackgroundFetch = async () => {
      try {
        await BackgroundFetch.registerTaskAsync(BACKGROUND_SYNC_TASK, {
          minimumInterval: 15 * 60, // 15 minutes
          stopOnTerminate: false,
          startOnBoot: true,
        });
        console.log("Background fetch task registered successfully");
      } catch (err) {
        console.warn("Background fetch registration failed:", err);
      }
    };

    registerBackgroundFetch();
  }, []);

  // Sync when network connection is restored
  useEffect(() => {
    if (isConnected) {
      const syncOnReconnect = async () => {
        try {
          await syncShipments();
          console.log("Sync triggered on network restore");
        } catch (err) {
          console.error("Manual sync failed:", err);
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
