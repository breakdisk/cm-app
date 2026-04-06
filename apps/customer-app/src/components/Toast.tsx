import React, { useEffect, useState } from 'react';
import { Animated, View, Text } from 'react-native';
import { COLORS } from '../utils/colors';

interface ToastProps {
  message: string;
  type: 'success' | 'error' | 'info';
  visible: boolean;
  onHide: () => void;
  duration?: number;
}

export default function Toast({ message, type, visible, onHide, duration = 3000 }: ToastProps) {
  const fadeAnim = new Animated.Value(0);

  const bgColor = {
    success: COLORS.GREEN,
    error: COLORS.RED,
    info: COLORS.CYAN,
  }[type];

  useEffect(() => {
    if (visible) {
      Animated.sequence([
        Animated.timing(fadeAnim, { toValue: 1, duration: 200, useNativeDriver: true }),
        Animated.delay(duration),
        Animated.timing(fadeAnim, { toValue: 0, duration: 200, useNativeDriver: true }),
      ]).start(() => onHide());
    }
  }, [visible]);

  if (!visible) return null;

  return (
    <Animated.View
      style={{
        opacity: fadeAnim,
        position: 'absolute',
        bottom: 40,
        left: 20,
        right: 20,
        backgroundColor: bgColor,
        borderRadius: 12,
        padding: 16,
        zIndex: 999,
      }}
    >
      <Text style={{ color: COLORS.CANVAS, fontSize: 14, fontWeight: '500' }}>{message}</Text>
    </Animated.View>
  );
}
