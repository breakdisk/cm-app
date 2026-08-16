/**
 * A coordinate plot in pure React Native — no native module, no API key.
 *
 * `Animated` rather than Reanimated: Reanimated was deliberately dropped from
 * this app's dependencies (it needs a separate worklets package on this Expo
 * generation) and one pulsing dot is not a reason to bring it back.
 *
 * The width is measured at layout rather than assumed, so the plot is correct
 * on a 360 dp phone and on a tablet without a breakpoint.
 */
import { useEffect, useRef, useState } from "react";
import { Animated, Easing, Text, View } from "react-native";

import type { CourierFix, StopView } from "@/api/tracking";
import { theme } from "@/theme";
import { markerIndices, projectAll } from "./project";
import type { MapSurfaceProps } from "./types";

const HEIGHT = 220;
const MIN_HEIGHT = 160;
const PADDING = 28;

export function CanvasPlot({ courier, stops, destination }: MapSurfaceProps) {
  const pulse = useRef(new Animated.Value(0)).current;
  const [width, setWidth] = useState(0);

  useEffect(() => {
    if (!courier) return;
    const loop = Animated.loop(
      Animated.sequence([
        Animated.timing(pulse, {
          toValue: 1,
          duration: 1100,
          easing: Easing.out(Easing.ease),
          useNativeDriver: true,
        }),
        Animated.timing(pulse, { toValue: 0, duration: 0, useNativeDriver: true }),
      ]),
    );
    loop.start();
    return () => loop.stop();
  }, [courier, pulse]);

  // Nothing to anchor the picture to. A lone courier dot in an empty frame
  // tells a customer less than the screen saying so in words.
  if (!destination) return null;

  const points = [
    ...stops.map((s) => ({ lat: s.lat, lng: s.lng })),
    { lat: destination.lat, lng: destination.lng },
    ...(courier ? [{ lat: courier.lat, lng: courier.lng }] : []),
  ];

  return (
    <View
      accessibilityLabel="Delivery map"
      onLayout={(e) => setWidth(e.nativeEvent.layout.width)}
      style={{
        height: HEIGHT,
        minHeight: MIN_HEIGHT,
        borderRadius: theme.radius.md,
        borderColor: theme.border,
        borderWidth: 1,
        backgroundColor: "rgba(255,255,255,0.03)",
        overflow: "hidden",
      }}
    >
      {/* Nothing is drawn until the container has been measured — projecting
          into a zero-width box would put every marker in the same place. */}
      {width > 0 && (
        <PlotBody
          points={points}
          stops={stops}
          courier={courier}
          pulse={pulse}
          box={{ width, height: HEIGHT, padding: PADDING }}
        />
      )}
    </View>
  );
}

function PlotBody({
  points,
  stops,
  courier,
  pulse,
  box,
}: {
  points: { lat: number; lng: number }[];
  stops: StopView[];
  courier: CourierFix | null;
  pulse: Animated.Value;
  box: { width: number; height: number; padding: number };
}) {
  // Same order the points array was built in: stops, destination, courier.
  const xy = projectAll(points, box);
  const { destIndex, courierIndex } = markerIndices(stops.length, courier != null);
  const stopPts = xy.slice(0, stops.length);
  const destPt = xy[destIndex];
  const courierPt = courierIndex != null ? xy[courierIndex] : null;

  return (
    <>
      {stopPts.map((p, i) => (
        <View
          key={i}
          style={{ position: "absolute", left: p.x - 6, top: p.y - 6, alignItems: "center" }}
        >
          <View
            style={{
              width: 12,
              height: 12,
              borderRadius: 6,
              borderWidth: 2,
              borderColor: stops[i].picked_up ? theme.green : theme.muted,
              backgroundColor: stops[i].picked_up ? theme.green : "transparent",
            }}
          />
          {/* Truncated, not wrapped: a long shop name must not reflow the plot. */}
          <Text
            numberOfLines={1}
            style={{ color: theme.faint, fontSize: 9, marginTop: 2, maxWidth: 70 }}
          >
            {stops[i].vendor_name}
          </Text>
        </View>
      ))}

      {destPt && (
        <View
          style={{ position: "absolute", left: destPt.x - 7, top: destPt.y - 7, alignItems: "center" }}
        >
          <View style={{ width: 14, height: 14, borderRadius: 3, backgroundColor: theme.cyan }} />
          <Text style={{ color: theme.faint, fontSize: 9, marginTop: 2 }}>You</Text>
        </View>
      )}

      {courierPt && (
        <Animated.View
          accessibilityLabel="Courier location"
          style={{
            position: "absolute",
            left: courierPt.x - 8,
            top: courierPt.y - 8,
            width: 16,
            height: 16,
            borderRadius: 8,
            backgroundColor: theme.amber,
            transform: [
              { scale: pulse.interpolate({ inputRange: [0, 1], outputRange: [1, 1.6] }) },
            ],
            opacity: pulse.interpolate({ inputRange: [0, 1], outputRange: [1, 0.35] }),
          }}
        />
      )}
    </>
  );
}
