import React from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { COLORS } from '../../utils/colors';

interface LoyaltyBannerProps {
  points: number;
  onPress?: () => void;
}

export default function LoyaltyBanner({ points, onPress }: LoyaltyBannerProps) {
  return (
    <TouchableOpacity onPress={onPress} activeOpacity={0.8}>
      <LinearGradient colors={[COLORS.PURPLE, COLORS.CYAN]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}>
        <View
          style={{
            padding: 16,
            borderRadius: 12,
            flexDirection: 'row',
            justifyContent: 'space-between',
            alignItems: 'center',
          }}
        >
          <View>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '500', opacity: 0.9 }}>Loyalty Points</Text>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 28, fontWeight: '700', marginTop: 4 }}>{points}</Text>
          </View>
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '500' }}>10% off next order →</Text>
        </View>
      </LinearGradient>
    </TouchableOpacity>
  );
}
