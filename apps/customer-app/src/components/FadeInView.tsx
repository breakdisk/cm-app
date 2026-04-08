/**
 * FadeInView — drop-in replacement for Reanimated's Animated.View with `entering` prop.
 * Uses React Native's built-in Animated API — works in Expo Go without native modules.
 */
import React, { useEffect, useRef } from 'react';
import { Animated, ViewProps } from 'react-native';

interface FadeInViewProps extends ViewProps {
  children?: React.ReactNode;
  delay?: number;
  duration?: number;
  fromY?: number; // translateY start offset (positive = from below, negative = from above)
}

export function FadeInView({ children, delay = 0, duration = 350, fromY = 16, style, ...props }: FadeInViewProps) {
  const opacity = useRef(new Animated.Value(0)).current;
  const translateY = useRef(new Animated.Value(fromY)).current;

  useEffect(() => {
    Animated.sequence([
      Animated.delay(delay),
      Animated.parallel([
        Animated.timing(opacity, { toValue: 1, duration, useNativeDriver: true }),
        Animated.timing(translateY, { toValue: 0, duration, useNativeDriver: true }),
      ]),
    ]).start();
  }, []);

  return (
    <Animated.View style={[{ opacity, transform: [{ translateY }] }, style]} {...props}>
      {children}
    </Animated.View>
  );
}
