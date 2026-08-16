/**
 * Push registration.
 *
 * Order notifications currently fail with `no push tokens registered for user …`
 * — engagement builds and records the message correctly and there is simply
 * nowhere to send it. This is the missing half.
 *
 * Registration happens after sign-in, not at launch: the token is stored
 * against a user, so asking before there is one would either fail or attach the
 * device to nobody.
 */
import * as Device from "expo-device";
import * as Notifications from "expo-notifications";
import { Platform } from "react-native";

import { currentToken } from "./auth";

const AUTH_BASE =
  process.env.EXPO_PUBLIC_GATEWAY_API ?? "http://localhost:8000";

/**
 * Ask for permission, get a token, and hand it to identity.
 *
 * Never throws. A customer who declines notifications, or a simulator with no
 * push support, must still be able to order — this is an enhancement to the
 * order flow, not a step in it. Returns whether a token was registered so a
 * caller can decide whether to explain what they will miss.
 */
export async function registerForPush(): Promise<boolean> {
  try {
    // Simulators cannot receive push. Bail before prompting, so nobody sees a
    // permission dialog that could never lead anywhere.
    if (!Device.isDevice) return false;

    const existing = await Notifications.getPermissionsAsync();
    let status = existing.status;
    if (status !== "granted") {
      status = (await Notifications.requestPermissionsAsync()).status;
    }
    if (status !== "granted") return false;

    const { data: token } = await Notifications.getExpoPushTokenAsync();
    if (!token) return false;

    const jwt = await currentToken();
    if (!jwt) return false;

    const res = await fetch(`${AUTH_BASE}/push-tokens`, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${jwt}`,
      },
      body: JSON.stringify({
        token,
        platform: Platform.OS,
        // "customer", matching the app column identity keys tokens on. A driver
        // value here would deliver a customer's order updates to the wrong app.
        app: "customer",
        device_id: Device.osInternalBuildId ?? undefined,
      }),
    });

    return res.ok;
  } catch {
    // Swallowed on purpose, with the same reasoning: nothing here is worth
    // blocking an order over.
    return false;
  }
}
