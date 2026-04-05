import React from 'react';
import { TouchableOpacity, View, Text } from 'react-native';
import { MaterialIcons } from '@expo/vector-icons';
import { LinearGradient } from 'expo-linear-gradient';
import { COLORS } from '../../utils/colors';

interface QuickActionButtonProps {
  icon: string;
  label: string;
  onPress: () => void;
}

export default function QuickActionButton({ icon, label, onPress }: QuickActionButtonProps) {
  return (
    <TouchableOpacity onPress={onPress} activeOpacity={0.8} testID="quick-action">
      <LinearGradient colors={[COLORS.GLASS, COLORS.GLASS_HOVER]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}>
        <View
          style={{
            padding: 16,
            borderRadius: 12,
            borderWidth: 1,
            borderColor: COLORS.BORDER,
            alignItems: 'center',
            gap: 8,
          }}
        >
          <MaterialIcons name={icon as any} size={32} color={COLORS.CYAN} />
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 12, fontWeight: '600', textAlign: 'center' }}>{label}</Text>
        </View>
      </LinearGradient>
    </TouchableOpacity>
  );
}
