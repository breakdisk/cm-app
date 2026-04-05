import React from 'react';
import { View, Text } from 'react-native';
import { MaterialCommunityIcons } from '@expo/vector-icons';
import { COLORS } from '../utils/colors';
import { formatDate } from '../utils/formatting';

interface OfflineIndicatorProps {
  isOffline: boolean;
  lastUpdated?: number; // timestamp in ms
}

export default function OfflineIndicator({ isOffline, lastUpdated }: OfflineIndicatorProps) {
  if (!isOffline) return null;

  const lastUpdatedText = lastUpdated
    ? formatDate(new Date(lastUpdated), { relative: true })
    : 'Never';

  return (
    <View style={{
      backgroundColor: COLORS.AMBER,
      paddingVertical: 10,
      paddingHorizontal: 14,
      flexDirection: 'row',
      alignItems: 'center',
      gap: 10,
    }}>
      <MaterialCommunityIcons name="cloud-off-outline" size={16} color={COLORS.CANVAS} />
      <View style={{ flex: 1 }}>
        <Text style={{ color: COLORS.CANVAS, fontSize: 12, fontWeight: '600' }}>
          Offline Mode
        </Text>
        <Text style={{ color: COLORS.CANVAS, fontSize: 11, opacity: 0.8 }}>
          Last updated {lastUpdatedText}
        </Text>
      </View>
    </View>
  );
}
