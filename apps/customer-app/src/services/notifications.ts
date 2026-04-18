/**
 * Push notifications service — request permissions, fetch Expo push token,
 * and register it with the identity service.
 *
 * Call `registerForPushNotifications()` after successful login.
 * Call `unregisterPushToken()` on logout.
 */
import * as Notifications from 'expo-notifications';
import * as Device from 'expo-device';
import Constants, { ExecutionEnvironment } from 'expo-constants';
import { Platform } from 'react-native';
import { getIdentityClient } from './api/client';

const STORED_PUSH_TOKEN_KEY = 'push_token';
const isExpoGo = Constants.executionEnvironment === ExecutionEnvironment.StoreClient;

if (!isExpoGo) {
  Notifications.setNotificationHandler({
    handleNotification: async () => ({
      shouldShowAlert: true,
      shouldPlaySound: true,
      shouldSetBadge: true,
      shouldShowBanner: true,
      shouldShowList: true,
    }),
  });
}

async function getExpoPushToken(): Promise<string | null> {
  if (isExpoGo) {
    console.log('Push notifications unavailable in Expo Go (SDK 53+). Use a development build.');
    return null;
  }
  if (!Device.isDevice) {
    console.log('Push notifications require a physical device');
    return null;
  }

  const { status: existing } = await Notifications.getPermissionsAsync();
  let finalStatus = existing;

  if (existing !== 'granted') {
    const { status } = await Notifications.requestPermissionsAsync();
    finalStatus = status;
  }

  if (finalStatus !== 'granted') {
    console.log('Push notification permission denied');
    return null;
  }

  if (Platform.OS === 'android') {
    await Notifications.setNotificationChannelAsync('default', {
      name: 'default',
      importance: Notifications.AndroidImportance.MAX,
      vibrationPattern: [0, 250, 250, 250],
      lightColor: '#00E5FF',
    });
  }

  try {
    const projectId = process.env.EXPO_PUBLIC_EAS_PROJECT_ID;
    const tokenResult = projectId
      ? await Notifications.getExpoPushTokenAsync({ projectId })
      : await Notifications.getExpoPushTokenAsync();
    return tokenResult.data;
  } catch (err) {
    console.error('Failed to get Expo push token:', err);
    return null;
  }
}

/**
 * Request permissions, obtain the Expo push token, and POST it to the identity service.
 * Safe to call multiple times — the backend upserts on `(tenant_id, token)`.
 */
export async function registerForPushNotifications(): Promise<string | null> {
  const token = await getExpoPushToken();
  if (!token) return null;

  try {
    const client = getIdentityClient();
    await client.post('/v1/push-tokens', {
      token,
      platform: Platform.OS === 'ios' ? 'ios' : 'android',
      app: 'customer',
      device_id: Device.osInternalBuildId ?? Device.modelId ?? null,
    });
    console.log('Push token registered with backend');

    try {
      const SecureStore = await import('expo-secure-store');
      await SecureStore.setItemAsync(STORED_PUSH_TOKEN_KEY, token);
    } catch {}

    return token;
  } catch (err) {
    console.error('Failed to register push token with backend:', err);
    return null;
  }
}

/**
 * Delete the push token from the backend. Call on logout.
 */
export async function unregisterPushToken(): Promise<void> {
  try {
    const SecureStore = await import('expo-secure-store');
    const token = await SecureStore.getItemAsync(STORED_PUSH_TOKEN_KEY);
    if (!token) return;

    const client = getIdentityClient();
    await client.delete('/v1/push-tokens', { data: { token } });
    await SecureStore.deleteItemAsync(STORED_PUSH_TOKEN_KEY);
    console.log('Push token deleted from backend');
  } catch (err) {
    console.error('Failed to unregister push token:', err);
  }
}
