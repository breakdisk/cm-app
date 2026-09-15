/**
 * "LogisticOS Move" — the consumer moving app from the mobile design handoff.
 *
 * One stack with Home as the only entry point (the design has no tab bar).
 * Shared screens — support, profile, invoices, the card-payment flow — are the
 * existing ones. Some of them navigate to "Tabs"; that name is kept here and
 * lands on Home.
 */
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { MoveHomeScreen } from '../screens/move/MoveHomeScreen';
import { MoveThinkingScreen } from '../screens/move/MoveThinkingScreen';
import { MovePlanScreen } from '../screens/move/MovePlanScreen';
import { MoveBookedScreen } from '../screens/move/MoveBookedScreen';
import { MoveTrackScreen } from '../screens/move/MoveTrackScreen';
import { MoveCancelScreen } from '../screens/move/MoveCancelScreen';
import { MoveCancelledScreen } from '../screens/move/MoveCancelledScreen';
import { MovePaymentsScreen } from '../screens/move/MovePaymentsScreen';
import { MoveRulesScreen } from '../screens/move/MoveRulesScreen';
import { M } from '../screens/move/theme';
import { SupportScreen } from '../screens/support/SupportScreen';
import { ProfileScreen } from '../screens/profile/ProfileScreen';
import { KYCScreen } from '../screens/auth/KYCScreen';
import { InvoiceDetailScreen } from '../screens/invoices/InvoiceDetailScreen';
import { NotificationsScreen } from '../screens/notifications/NotificationsScreen';
import { PaymentWebViewScreen } from '../screens/booking/PaymentWebView';
import { BookingConfirmationPendingScreen } from '../screens/booking/BookingConfirmationPending';
import { CollectionScreen } from '../screens/collection/CollectionScreen';
import { ReceiptScreen } from '../screens/history/ReceiptScreen';

const Stack = createNativeStackNavigator();

export function MoveNavigator() {
  return (
    <Stack.Navigator
      id="MoveStack"
      screenOptions={{ headerShown: false, contentStyle: { backgroundColor: M.ground }, animation: 'fade_from_bottom' }}
    >
      <Stack.Screen name="MoveHome" component={MoveHomeScreen} />
      <Stack.Screen name="MoveThinking" component={MoveThinkingScreen} options={{ animation: 'fade' }} />
      <Stack.Screen name="MovePlan" component={MovePlanScreen} />
      <Stack.Screen name="MoveBooked" component={MoveBookedScreen} options={{ gestureEnabled: false }} />
      <Stack.Screen name="Track" component={MoveTrackScreen} />
      <Stack.Screen name="MoveCancel" component={MoveCancelScreen} />
      <Stack.Screen name="MoveCancelled" component={MoveCancelledScreen} options={{ gestureEnabled: false }} />
      <Stack.Screen name="MovePayments" component={MovePaymentsScreen} />
      <Stack.Screen name="Invoices" component={MovePaymentsScreen} />
      <Stack.Screen name="MoveRules" component={MoveRulesScreen} />
      <Stack.Screen name="Support" component={SupportScreen} />
      <Stack.Screen name="Profile" component={ProfileScreen} />
      <Stack.Screen name="KYC" component={KYCScreen} />
      <Stack.Screen name="InvoiceDetail" component={InvoiceDetailScreen} />
      <Stack.Screen name="Notifications" component={NotificationsScreen} />
      <Stack.Screen name="PaymentWebView" component={PaymentWebViewScreen} />
      <Stack.Screen name="BookingConfirmationPending" component={BookingConfirmationPendingScreen} />
      <Stack.Screen name="Collection" component={CollectionScreen} />
      <Stack.Screen name="Receipt" component={ReceiptScreen} />
      <Stack.Screen name="Tabs" component={MoveHomeScreen} />
    </Stack.Navigator>
  );
}
