import React, { useEffect, useState } from 'react';
import { ScrollView, View, Text, Animated } from 'react-native';
import { useAppSelector } from '../../store/hooks';
import { COLORS } from '../../utils/colors';
import { useFadeInUp } from '../../hooks/useAnimation';
import QuickActionButton from './QuickActionButton';
import RecentShipmentCard from './RecentShipmentCard';
import LoyaltyBanner from './LoyaltyBanner';

export function HomeScreen({ navigation }: any) {
  const auth = useAppSelector(state => state.auth);
  const shipments = useAppSelector(state => state.shipments.list);
  const recentShipments = shipments.slice(0, 3);

  const headerAnim = useFadeInUp(0);
  const actionsAnim = useFadeInUp(100);
  const shipmentsAnim = useFadeInUp(200);

  return (
    <ScrollView
      style={{ flex: 1, backgroundColor: COLORS.CANVAS }}
      contentContainerStyle={{ padding: 16, paddingBottom: 40 }}
      showsVerticalScrollIndicator={false}
    >
      {/* Header with fade-in */}
      <Animated.View style={[headerAnim, { marginBottom: 24 }]}>
        <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 14 }}>Welcome back</Text>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 24, fontWeight: '700', marginTop: 4 }}>
          {auth.name || 'Customer'}
        </Text>
      </Animated.View>

      {/* Loyalty Banner */}
      <LoyaltyBanner points={auth.loyaltyPoints} onPress={() => console.log('Loyalty tapped')} />

      {/* Quick Actions with animation */}
      <Animated.View style={[actionsAnim, { marginTop: 24, marginBottom: 24 }]}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Quick Actions</Text>
        <View style={{ display: 'flex', flexDirection: 'row', flexWrap: 'wrap', gap: 12 }}>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="add-box" label="Book New" onPress={() => navigation.navigate('Book')} />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="location-on" label="Track" onPress={() => navigation.navigate('Track')} />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="history" label="History" onPress={() => navigation.navigate('History')} />
          </View>
          <View style={{ width: '48%' }}>
            <QuickActionButton icon="support-agent" label="Support" onPress={() => navigation.navigate('Support')} />
          </View>
        </View>
      </Animated.View>

      {/* Recent Shipments with animation */}
      {recentShipments.length > 0 && (
        <Animated.View style={shipmentsAnim}>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>Recent Shipments</Text>
          {recentShipments.map(shipment => (
            <View key={shipment.awb} style={{ marginBottom: 12 }}>
              <RecentShipmentCard shipment={shipment} onPress={() => navigation.navigate('Track')} />
            </View>
          ))}
        </Animated.View>
      )}
    </ScrollView>
  );
}

export default HomeScreen;
