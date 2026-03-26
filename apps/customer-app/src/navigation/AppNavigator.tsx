/**
 * Customer App — Navigation
 * Auth gate: guests → onboarding stack. Authenticated → tab navigator.
 */
import { NavigationContainer } from "@react-navigation/native";
import { createBottomTabNavigator } from "@react-navigation/bottom-tabs";
import { createNativeStackNavigator } from "@react-navigation/native-stack";
import { Ionicons } from "@expo/vector-icons";
import { useSelector } from "react-redux";
import type { RootState } from "../store";

import { HomeScreen }             from "../screens/home/HomeScreen";
import { TrackingScreen }         from "../screens/tracking/TrackingScreen";
import { BookingScreen }          from "../screens/booking/BookingScreen";
import { ProfileScreen }          from "../screens/profile/ProfileScreen";
import { NotificationsScreen }    from "../screens/notifications/NotificationsScreen";
import { HistoryScreen }          from "../screens/history/HistoryScreen";
import { SupportScreen }          from "../screens/support/SupportScreen";
import { PhoneScreen }            from "../screens/auth/PhoneScreen";
import { OnboardingProfileScreen }from "../screens/auth/OnboardingProfileScreen";
import { KYCScreen }              from "../screens/auth/KYCScreen";

// ── Design tokens ───────────────────────────────────────────────────────────────
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const BORDER = "rgba(255,255,255,0.08)";

// ── Navigators ──────────────────────────────────────────────────────────────────
const Tab   = createBottomTabNavigator();
const Stack = createNativeStackNavigator();

// ── Tab navigator (authenticated) ──────────────────────────────────────────────

function TabNavigator() {
  return (
    <Tab.Navigator
      screenOptions={({ route }) => ({
        headerShown: false,
        tabBarStyle: {
          backgroundColor: CANVAS,
          borderTopColor:  BORDER,
          borderTopWidth:  1,
          paddingBottom:   6,
          height:          62,
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
    <Stack.Navigator screenOptions={{ headerShown: false, contentStyle: { backgroundColor: CANVAS }, animation: "slide_from_right" }}>
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

// ── Root navigator ──────────────────────────────────────────────────────────────

export function AppNavigator() {
  const { isGuest, onboardingStep } = useSelector((s: RootState) => s.auth);

  // Show onboarding until complete
  const showOnboarding = isGuest || onboardingStep !== "complete";

  return (
    <NavigationContainer>
      <Stack.Navigator screenOptions={{ headerShown: false, contentStyle: { backgroundColor: CANVAS }, animation: "fade" }}>
        {showOnboarding ? (
          <Stack.Screen name="Onboarding" component={OnboardingNavigator} />
        ) : (
          <Stack.Screen name="Main" component={TabNavigator} />
        )}
      </Stack.Navigator>
    </NavigationContainer>
  );
}
