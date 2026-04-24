import React from 'react';
import { ScrollView, View, Text, Animated, ActivityIndicator, Pressable } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useAppSelector } from '../../store/hooks';
import { useShipments } from '../../hooks/useShipments';
import { COLORS } from '../../utils/colors';
import { useFadeInUp } from '../../hooks/useAnimation';
import QuickActionButton from './QuickActionButton';
import RecentShipmentCard from './RecentShipmentCard';
import LoyaltyBanner from './LoyaltyBanner';

export function HomeScreen({ navigation }: any) {
  const insets = useSafeAreaInsets();
  const auth = useAppSelector(state => state.auth);

  // Triggers the /v1/shipments fetch and populates Redux. Previously the
  // HomeScreen read state.shipments.list directly without ever driving the
  // fetch, so first-time visitors saw an empty "Recent Shipments" section
  // even if they had live bookings. Hook is idempotent via Redux.
  const { list: shipments, loading } = useShipments({ limit: 5 });
  const recentShipments = shipments.slice(0, 3);

  const headerAnim    = useFadeInUp(0);
  const actionsAnim   = useFadeInUp(100);
  const shipmentsAnim = useFadeInUp(200);

  return (
    <ScrollView
      style={{ flex: 1, backgroundColor: COLORS.CANVAS }}
      contentContainerStyle={{ padding: 16, paddingBottom: 40 }}
      showsVerticalScrollIndicator={false}
    >
      {/* Header */}
      <Animated.View style={[headerAnim, { marginBottom: 24 }]}>
        <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 14 }}>Welcome back</Text>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 24, fontWeight: '700', marginTop: 4 }}>
          {auth.name || 'Customer'}
        </Text>
      </Animated.View>

      {/* Loyalty Banner — taps jump to Profile where the full loyalty view lives */}
      <LoyaltyBanner
        points={auth.loyaltyPoints}
        onPress={() => navigation.navigate('Profile')}
      />

      {/* Quick Actions */}
      <Animated.View style={[actionsAnim, { marginTop: 24, marginBottom: 24 }]}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Quick Actions</Text>
        <View style={{ display: 'flex', flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="add-box"       label="Book New" onPress={() => navigation.navigate('Book')}    />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="location-on"   label="Track"    onPress={() => navigation.navigate('Track')}   />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="history"       label="History"  onPress={() => navigation.navigate('History')} />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="support-agent" label="Support"  onPress={() => navigation.navigate('Support')} />
          </View>
        </View>
      </Animated.View>

      {/* Recent Shipments */}
      <Animated.View style={shipmentsAnim}>
        <View style={{ flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginBottom: 12 }}>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600' }}>Recent Shipments</Text>
          {shipments.length > recentShipments.length && (
            <Pressable onPress={() => navigation.navigate('History')}>
              <Text style={{ color: COLORS.CYAN_NEON ?? '#00E5FF', fontSize: 12 }}>See all →</Text>
            </Pressable>
          )}
        </View>

        {loading && shipments.length === 0 ? (
          <View style={{ paddingVertical: 24, alignItems: 'center' }}>
            <ActivityIndicator color={COLORS.CYAN_NEON ?? '#00E5FF'} />
          </View>
        ) : recentShipments.length === 0 ? (
          <View
            style={{
              paddingVertical: 28,
              paddingHorizontal: 16,
              borderRadius: 16,
              borderWidth: 1,
              borderColor: 'rgba(255,255,255,0.08)',
              backgroundColor: 'rgba(255,255,255,0.03)',
              alignItems: 'center',
            }}
          >
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600' }}>No shipments yet</Text>
            <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 12, marginTop: 4, textAlign: 'center' }}>
              Book your first shipment to get started.
            </Text>
            <Pressable
              onPress={() => navigation.navigate('Book')}
              style={({ pressed }) => ({
                marginTop: 12,
                paddingHorizontal: 16,
                paddingVertical: 8,
                borderRadius: 10,
                backgroundColor: 'rgba(0,229,255,0.12)',
                opacity: pressed ? 0.7 : 1,
              })}
            >
              <Text style={{ color: COLORS.CYAN_NEON ?? '#00E5FF', fontSize: 13, fontWeight: '600' }}>Book Now</Text>
            </Pressable>
          </View>
        ) : (
          recentShipments.map((shipment) => (
            <View key={shipment.awb} style={{ marginBottom: 12 }}>
              <RecentShipmentCard
                shipment={shipment}
                onPress={() => navigation.navigate('Track', { awb: shipment.awb })}
              />
            </View>
          ))
        )}
      </Animated.View>
    </ScrollView>
  );
}

export default HomeScreen;
