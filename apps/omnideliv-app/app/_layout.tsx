/**
 * Root layout, and the gate.
 *
 * Two things must be true before the app can do anything useful: the customer
 * is signed in, and we know where to deliver. Checking here rather than in each
 * screen means a new screen cannot forget — and the previous version of this
 * app had neither check, so it read a token nothing wrote and delivered every
 * order to a hardcoded point in Manila.
 */
import { useEffect, useState } from "react";
import { ActivityIndicator, View } from "react-native";
import { Stack, useRouter, useSegments } from "expo-router";

import { isSignedIn, loadSession } from "@/api/auth";
import { registerForPush } from "@/api/push";
import { hasDeliveryPoint, loadDeliveryPoint } from "@/deliveryPoint";
import { theme } from "@/theme";

export default function RootLayout() {
  const router = useRouter();
  const segments = useSegments();
  const [ready, setReady] = useState(false);

  useEffect(() => {
    void (async () => {
      await loadSession();
      await loadDeliveryPoint();
      setReady(true);

      // After sign-in, not before: the token is stored against a user.
      if (isSignedIn()) void registerForPush();
    })();
  }, []);

  useEffect(() => {
    if (!ready) return;

    const route = segments[0];
    const onSignIn = route === "sign-in";
    const onAddress = route === "address";

    // Read on every navigation, never from React state. Signing in writes the
    // token and then routes here; a `signedIn` captured once at mount is still
    // false at that moment, and the gate bounces a freshly signed-in customer
    // straight back to the code screen — which reads as "that OTP didn't work"
    // and only clears on the next cold start.
    const signedIn = isSignedIn();

    if (!signedIn && !onSignIn) {
      router.replace("/sign-in");
      return;
    }
    // Address is asked for once, after sign-in. Deliberately a gate rather than
    // a default: a wrong address is worse than an absent one, because it looks
    // like it worked and a courier goes to the wrong door.
    if (signedIn && !hasDeliveryPoint() && !onAddress) {
      router.replace("/address");
    }
  }, [ready, segments, router]);

  if (!ready) {
    return (
      <View style={{ flex: 1, backgroundColor: theme.canvas, justifyContent: "center" }}>
        <ActivityIndicator color={theme.cyan} />
      </View>
    );
  }

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
