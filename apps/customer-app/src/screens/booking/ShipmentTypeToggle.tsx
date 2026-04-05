import React from 'react';
import { View, TouchableOpacity, Text } from 'react-native';
import { COLORS } from '../../utils/colors';

interface ShipmentTypeToggleProps {
  value: 'local' | 'international';
  onChange: (value: 'local' | 'international') => void;
}

export default function ShipmentTypeToggle({ value, onChange }: ShipmentTypeToggleProps) {
  return (
    <View
      style={{
        flexDirection: 'row',
        backgroundColor: COLORS.SURFACE,
        borderRadius: 12,
        padding: 4,
        marginBottom: 20,
        borderWidth: 1,
        borderColor: COLORS.BORDER,
      }}
    >
      <TouchableOpacity
        testID="shipment-type-local"
        onPress={() => onChange('local')}
        style={{
          flex: 1,
          paddingVertical: 12,
          paddingHorizontal: 16,
          borderRadius: 10,
          backgroundColor: value === 'local' ? COLORS.CYAN : 'transparent',
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <Text
          style={{
            color: value === 'local' ? COLORS.CANVAS : COLORS.TEXT_PRIMARY,
            fontWeight: '600',
            fontSize: 14,
          }}
        >
          Local
        </Text>
      </TouchableOpacity>
      <TouchableOpacity
        testID="shipment-type-international"
        onPress={() => onChange('international')}
        style={{
          flex: 1,
          paddingVertical: 12,
          paddingHorizontal: 16,
          borderRadius: 10,
          backgroundColor: value === 'international' ? COLORS.CYAN : 'transparent',
          alignItems: 'center',
          justifyContent: 'center',
        }}
      >
        <Text
          style={{
            color: value === 'international' ? COLORS.CANVAS : COLORS.TEXT_PRIMARY,
            fontWeight: '600',
            fontSize: 14,
          }}
        >
          International
        </Text>
      </TouchableOpacity>
    </View>
  );
}
