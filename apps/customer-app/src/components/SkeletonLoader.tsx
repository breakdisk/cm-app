import React, { useRef, useEffect } from 'react';
import { View, Animated, type DimensionValue } from 'react-native';
import { COLORS } from '../utils/colors';

interface SkeletonLoaderProps {
  width?: DimensionValue;
  height?: number;
  borderRadius?: number;
  testID?: string;
}

/**
 * SkeletonLoader component for displaying placeholder loading states.
 * Renders a pulsing shimmer effect using opacity animation.
 *
 * @param width - Width of skeleton (default: '100%')
 * @param height - Height of skeleton in pixels (default: 20)
 * @param borderRadius - Border radius in pixels (default: 8)
 * @param testID - Optional test identifier
 * @returns Loading skeleton placeholder component
 */
export default function SkeletonLoader({
  width = '100%',
  height = 20,
  borderRadius = 8,
  testID,
}: SkeletonLoaderProps) {
  const shimmerAnim = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    Animated.loop(
      Animated.sequence([
        Animated.timing(shimmerAnim, {
          toValue: 1,
          duration: 1000,
          useNativeDriver: true,
        }),
        Animated.timing(shimmerAnim, {
          toValue: 0,
          duration: 1000,
          useNativeDriver: true,
        }),
      ])
    ).start();
  }, [shimmerAnim]);

  return (
    <View
      testID={testID}
      style={[
        {
          width,
          height,
          backgroundColor: COLORS.SURFACE,
          borderRadius,
          overflow: 'hidden',
          marginBottom: 8,
        },
      ]}
    >
      <Animated.View
        style={{
          flex: 1,
          backgroundColor: COLORS.GLASS,
          opacity: shimmerAnim,
        }}
      />
    </View>
  );
}
