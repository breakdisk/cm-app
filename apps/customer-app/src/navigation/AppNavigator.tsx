/**
 * Customer App — Navigation
 * Auth gate: guests → onboarding stack. Authenticated → tab navigator.
 */
import { NavigationContainer } from "@react-navigation/native";
import { createBottomTabNavigator } from "@react-navigation/bottom-tabs";
import { createNativeStackNavigator } from "@react-navigation/native-stack";
import { Ionicons } from "@expo/vector-icons";
import { useSelector } from "react-redux";
import { SafeAreaView, useSafeAreaInsets } from "react-native-safe-area-context";
import { StatusBar } from "expo-status-bar";
import type { RootState } from "../store";
import { navigationRef } from "./navigationRef";

import { HomeScreen }             from "../screens/home/HomeScreen";
import { TrackingScreen }         from "../screens/tracking/TrackingScreen";
import { BookingScreen }          from "../screens/booking/BookingScreen";
import { ProfileScreen }          from "../screens/profile/ProfileScreen";
import { NotificationsScreen }    from "../screens/notifications/NotificationsScreen";
import { HistoryScreen }          from "../screens/history/HistoryScreen";
import { ReceiptScreen }          from "../screens/history/ReceiptScreen";
import { SupportScreen }          from "../screens/support/SupportScreen";
import { PhoneScreen }            from "../screens/auth/PhoneScreen";
import { OnboardingProfileScreen }from "../screens/auth/OnboardingProfileScreen";
import { KYCScreen }              from "../screens/auth/KYCScreen";
import { InvoicesScreen }         from "../screens/invoices/InvoicesScreen";
import { InvoiceDetailScreen }    from "../screens/invoices/InvoiceDetailScreen";
import { CollectionScreen }       from "../screens/collection/CollectionScreen";

// ── Design tokens ───────────────────────────────────────────────────────────────
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const BORDER = "rgba(255,255,255,0.08)";

// ── Navigators ──────────────────────────────────────────────────────────────────
const Tab   = createBottomTabNavigator();
const Stack = createNativeStackNavigator();

// ── Tab navigator (authenticated) ──────────────────────────────────────────────

function TabNavigator() {
  const insets = useSafeAreaInsets();
  const tabBarHeight = 56 + insets.bottom;

  return (
    <Tab.Navigator
      id="TabNavigator"
      screenOptions={({ route }) => ({
        headerShown: false,
        tabBarStyle: {
          backgroundColor: CANVAS,
          borderTopColor:  BORDER,
          borderTopWidth:  1,
          height:          tabBarHeight,
          paddingBottom:   insets.bottom,
        },
        tabBarActiveTintColor:   CYAN,
        tabBarInactiveTintColor: "rgba(255,255,255,0.35)",
        tabBarLabelStyle: { fontSize: 10, fontFamily: "JetBrainsMono-Regular", marginTop: 2 },
        tabBarIcon: ({ focused, color, size }) => {
          const icons: Record<string, [string, string]> = {
            Home:    ["home",               "home-outline"              ],
            Track:   ["locate",             "locate-outline"            ],
            Book:    ["add-circle",         "add-circle-outline"        ],
            History: ["time",               "time-outline"              ],
            Support: ["chatbubble-ellipses","chatbubble-ellipses-outline"],
            Profile: ["person-circle",      "person-circle-outline"     ],
          };
          const [active, inactive] = icons[route.name] ?? ["help", "help-outline"];
          return <Ionicons name={(focused ? active : inactive) as any} size={size} color={color} />;
        },
      })}
    >
      <Tab.Screen name="Home"    component={HomeScreen}     />
      <Tab.Screen name="Track"   component={TrackingScreen} />
      <Tab.Screen name="Book"    component={BookingScreen}  />
      <Tab.Screen name="History" component={HistoryScreen}  />
      <Tab.Screen name="Support" component={SupportScreen}  />
      <Tab.Screen name="Profile" component={ProfileScreen}  />
    </Tab.Navigator>
  );
}

// ── Onboarding stack (unauthenticated) ─────────────────────────────────────────

function OnboardingNavigator() {
  const onboardingStep = useSelector((s: RootState) => s.auth.onboardingStep);

  return (
    <Stack.Navigator id="OnboardingStack" screenOptions={{ headerShown: false, contentStyle: { backgroundColor: CANVAS }, animation: "slide_from_right" }}>
      {onboardingStep === "phone" && (
        <Stack.Screen name="Phone"   component={PhoneScreen} />
      )}
      {onboardingStep === "profile" && (
        <Stack.Screen name="Profile" component={OnboardingProfileScreen} />
      )}
      {onboardingStep === "kyc" && (
        <Stack.Screen name="KYC"     component={KYCScreen} />
      )}
    </Stack.Navigator>
  );
}

// ── Authenticated app shell — tabs + modal-style screens ────────────────────────

function AuthenticatedNavigator() {
  return (
    <Stack.Navigator id="AuthenticatedStack" screenOptions={{ headerShown: false, contentStyle: { backgroundColor: CANVAS }, animation: "slide_from_right" }}>
      <Stack.Screen name="Tabs"          component={TabNavigator}        />
      <Stack.Screen name="Receipt"       component={ReceiptScreen}        />
      <Stack.Screen name="Collection"    component={CollectionScreen}     />
      <Stack.Screen name="Invoices"      component={InvoicesScreen}       />
      <Stack.Screen name="InvoiceDetail" component={InvoiceDetailScreen}  />
    </Stack.Navigator>
  );
}

// ── Root navigator ──────────────────────────────────────────────────────────────

export function AppNavigator() {
  const { isGuest, onboardingStep } = useSelector((s: RootState) => s.auth);

  // Show onboarding until complete
  const showOnboarding = isGuest || onboardingStep !== "complete";

  const linking = {
    prefixes: ["logisticos://"],
    config: {
      screens: {
        Main: {
          screens: {
            // logisticos://invoices/:id → InvoiceDetailScreen
            InvoiceDetail: "invoices/:id",
          },
        },
      },
    },
  };

  return (
    <SafeAreaView style={{ flex: 1, backgroundColor: CANVAS }} edges={["left", "right"]}>
      <StatusBar style="light" backgroundColor={CANVAS} />
      <NavigationContainer ref={navigationRef} linking={linking}>
        <Stack.Navigator id="RootStack" screenOptions={{ headerShown: false, contentStyle: { backgroundColor: CANVAS }, animation: "fade" }}>
          {showOnboarding ? (
            <Stack.Screen name="Onboarding" component={OnboardingNavigator} />
          ) : (
            <Stack.Screen name="Main" component={AuthenticatedNavigator} />
          )}
        </Stack.Navigator>
      </NavigationContainer>
    </SafeAreaView>
  );
}
