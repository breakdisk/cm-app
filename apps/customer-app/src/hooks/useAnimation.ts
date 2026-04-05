import { useRef, useEffect } from 'react';
import { Animated, Easing } from 'react-native';

/**
 * Hook for fade-in and translate-up animation with optional staggered delay.
 * Animates opacity from 0 to 1 and translateY from 20px to 0.
 * Uses cubic-out easing for spring-like effect.
 *
 * @param delay - Optional delay in milliseconds before animation starts (default: 0)
 * @returns Animated style object with opacity and transform
 */
export function useFadeInUp(delay = 0) {
  const animValue = useRef(new Animated.Value(0)).current;

  useEffect(() => {
    Animated.sequence([
      Animated.delay(delay),
      Animated.timing(animValue, {
        toValue: 1,
        duration: 500,
        easing: Easing.out(Easing.cubic),
        useNativeDriver: true,
      }),
    ]).start();
  }, [delay, animValue]);

  return {
    opacity: animValue,
    transform: [
      {
        translateY: animValue.interpolate({
          inputRange: [0, 1],
          outputRange: [20, 0],
        }),
      },
    ],
  };
}

/**
 * Hook for scale-down animation on press (press feedback).
 * Scales from 1 to 0.95 and back to 1 on press.
 *
 * @returns Object containing scale animated value and onPress callback
 */
export function useScale() {
  const animValue = useRef(new Animated.Value(1)).current;

  const press = () => {
    Animated.timing(animValue, {
      toValue: 0.95,
      duration: 100,
      useNativeDriver: true,
    }).start(() => {
      Animated.timing(animValue, {
        toValue: 1,
        duration: 100,
        useNativeDriver: true,
      }).start();
    });
  };

  return {
    scale: animValue,
    onPress: press,
  };
}

/**
 * Hook for continuous pulse/breathing animation.
 * Scales from 1 to 1.1 and back, looping infinitely.
 *
 * @returns Object containing scale animated value
 */
export function usePulse() {
  const animValue = useRef(new Animated.Value(1)).current;

  useEffect(() => {
    Animated.loop(
      Animated.sequence([
        Animated.timing(animValue, {
          toValue: 1.1,
          duration: 1000,
          useNativeDriver: true,
        }),
        Animated.timing(animValue, {
          toValue: 1,
          duration: 1000,
          useNativeDriver: true,
        }),
      ])
    ).start();
  }, [animValue]);

  return {
    scale: animValue,
  };
}

/**
 * Hook for shake animation on demand.
 * Oscillates translateX left and right with rapid timing.
 *
 * @returns Object containing translateX animated value and shake callback
 */
export function useShake() {
  const animValue = useRef(new Animated.Value(0)).current;

  const shake = () => {
    Animated.sequence([
      Animated.timing(animValue, {
        toValue: -10,
        duration: 50,
        useNativeDriver: true,
      }),
      Animated.timing(animValue, {
        toValue: 10,
        duration: 50,
        useNativeDriver: true,
      }),
      Animated.timing(animValue, {
        toValue: -10,
        duration: 50,
        useNativeDriver: true,
      }),
      Animated.timing(animValue, {
        toValue: 0,
        duration: 50,
        useNativeDriver: true,
      }),
    ]).start();
  };

  return {
    translateX: animValue,
    shake,
  };
}
