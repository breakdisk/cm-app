/**
 * Driver App — Tab navigator layout.
 */
import { Tabs } from "expo-router";
import { Platform } from "react-native";
import { Ionicons } from "@expo/vector-icons";
import { useSelector } from "react-redux";
import type { RootState } from "../../store";

const CANVAS       = "#050810";
const CYAN         = "#00E5FF";
const GLASS_BORDER = "rgba(255,255,255,0.08)";
const MUTED        = "rgba(255,255,255,0.35)";

export default function TabsLayout() {
  const syncPending = useSelector((s: RootState) => s.tasks.syncPending);

  return (
    <Tabs
      screenOptions={{
        tabBarStyle: {
          backgroundColor:   CANVAS,
          borderTopColor:    GLASS_BORDER,
          borderTopWidth:    1,
          height:            Platform.OS === "ios" ? 88 : 64,
          paddingBottom:     Platform.OS === "ios" ? 28 : 8,
          paddingTop:        8,
        },
        tabBarActiveTintColor:   CYAN,
        tabBarInactiveTintColor: MUTED,
        tabBarLabelStyle: { fontFamily: "JetBrainsMono-Regular", fontSize: 10, marginTop: 2 },
        headerStyle:          { backgroundColor: CANVAS },
        headerTintColor:      CYAN,
        headerTitleStyle:     { fontFamily: "SpaceGrotesk-SemiBold", color: "#FFFFFF" },
        headerShadowVisible:  false,
      }}
    >
      <Tabs.Screen
        name="index"
        options={{
          title:    "My Tasks",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="list" size={size} color={color} />
          ),
          tabBarBadge: syncPending > 0 ? syncPending : undefined,
          tabBarBadgeStyle: { backgroundColor: "#FF3B5C", color: "white", fontSize: 10 },
        }}
      />
      <Tabs.Screen
        name="map"
        options={{
          title:    "Route Map",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="map" size={size} color={color} />
          ),
        }}
      />
      <Tabs.Screen
        name="scanner"
        options={{
          title:    "Scanner",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="barcode" size={size} color={color} />
          ),
        }}
      />
      <Tabs.Screen
        name="earnings"
        options={{
          title:    "Earnings",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="cash-outline" size={size} color={color} />
          ),
        }}
      />
      <Tabs.Screen
        name="profile"
        options={{
          title:    "Profile",
          tabBarIcon: ({ color, size }) => (
            <Ionicons name="person" size={size} color={color} />
          ),
        }}
      />
    </Tabs>
  );
}
