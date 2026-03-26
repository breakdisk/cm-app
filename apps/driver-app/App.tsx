/**
 * Driver App — Entry point.
 * Expo Router handles navigation; this file bootstraps global providers.
 */
import "react-native-gesture-handler";
import { ExpoRoot } from "expo-router";
import { Provider } from "react-redux";
import { store } from "./src/store";
import { GestureHandlerRootView } from "react-native-gesture-handler";
import { StyleSheet } from "react-native";

export default function App() {
  return (
    <GestureHandlerRootView style={styles.container}>
      <Provider store={store}>
        <ExpoRoot context={require.context("./src/app")} />
      </Provider>
    </GestureHandlerRootView>
  );
}

const styles = StyleSheet.create({
  container: { flex: 1 },
});
