import React from 'react';
import { TouchableOpacity, Text, ViewStyle } from 'react-native';
import { COLORS } from '../utils/colors';

interface ButtonProps {
  onPress: () => void;
  label: string;
  variant?: 'primary' | 'secondary' | 'ghost';
  size?: 'sm' | 'md' | 'lg';
  disabled?: boolean;
  style?: ViewStyle;
}

export default function Button({
  onPress,
  label,
  variant = 'primary',
  size = 'md',
  disabled = false,
  style,
}: ButtonProps) {
  const config = {
    primary: { bgColor: COLORS.CYAN, textColor: COLORS.CANVAS },
    secondary: { bgColor: COLORS.SURFACE, textColor: COLORS.CYAN },
    ghost: { bgColor: 'transparent', textColor: COLORS.CYAN },
  };

  const { bgColor, textColor } = config[variant];

  const sizes = {
    sm: { paddingVertical: 8, paddingHorizontal: 16, fontSize: 12 },
    md: { paddingVertical: 12, paddingHorizontal: 20, fontSize: 14 },
    lg: { paddingVertical: 16, paddingHorizontal: 24, fontSize: 16 },
  };

  const { paddingVertical, paddingHorizontal, fontSize } = sizes[size];

  return (
    <TouchableOpacity
      onPress={onPress}
      disabled={disabled}
      activeOpacity={0.7}
      style={[
        {
          backgroundColor: bgColor,
          paddingVertical,
          paddingHorizontal,
          borderRadius: 12,
          alignItems: 'center',
          justifyContent: 'center',
          opacity: disabled ? 0.5 : 1,
          borderWidth: variant === 'secondary' ? 1 : 0,
          borderColor: COLORS.BORDER,
        },
        style,
      ]}
    >
      <Text style={{ color: textColor, fontSize, fontWeight: '600' }}>{label}</Text>
    </TouchableOpacity>
  );
}
