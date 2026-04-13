/**
 * LiveDriverMap — shows the driver's current position on a dark-themed map.
 * Renders when delivery-experience returns driver_location (out_for_delivery / assigned).
 */
import React, { useMemo } from 'react';
import { View, Text, StyleSheet } from 'react-native';
import MapView, { Marker } from 'react-native-maps';
import { Ionicons } from '@expo/vector-icons';

const CYAN = '#00E5FF';
const GREEN = '#00FF88';
const CANVAS = '#050810';
const BORDER = 'rgba(255,255,255,0.08)';

interface Props {
  driverLocation: { lat: number; lng: number };
  driverName?: string;
  height?: number;
}

const DARK_MAP_STYLE = [
  { elementType: 'geometry', stylers: [{ color: '#0a0e1a' }] },
  { elementType: 'labels.text.fill', stylers: [{ color: '#6b7280' }] },
  { elementType: 'labels.text.stroke', stylers: [{ color: '#050810' }] },
  { featureType: 'road', elementType: 'geometry', stylers: [{ color: '#1a2030' }] },
  { featureType: 'road.highway', elementType: 'geometry', stylers: [{ color: '#2a3445' }] },
  { featureType: 'water', elementType: 'geometry', stylers: [{ color: '#050a15' }] },
  { featureType: 'poi', elementType: 'labels', stylers: [{ visibility: 'off' }] },
  { featureType: 'transit', elementType: 'labels', stylers: [{ visibility: 'off' }] },
  { featureType: 'administrative', elementType: 'geometry', stylers: [{ color: '#1a2030' }] },
  { featureType: 'landscape', elementType: 'geometry', stylers: [{ color: '#0f1420' }] },
];

export function LiveDriverMap({ driverLocation, driverName, height = 220 }: Props) {
  const region = useMemo(
    () => ({
      latitude: driverLocation.lat,
      longitude: driverLocation.lng,
      latitudeDelta: 0.02,
      longitudeDelta: 0.02,
    }),
    [driverLocation.lat, driverLocation.lng]
  );

  return (
    <View style={[styles.container, { height }]}>
      <View style={styles.header}>
        <View style={styles.pulseDot} />
        <Text style={styles.headerText}>Live Driver Location</Text>
      </View>
      <MapView
        style={StyleSheet.absoluteFill}
        region={region}
        customMapStyle={DARK_MAP_STYLE}
        pointerEvents="none"
        toolbarEnabled={false}
        showsMyLocationButton={false}
        showsCompass={false}
        zoomEnabled={false}
        scrollEnabled={false}
        rotateEnabled={false}
        pitchEnabled={false}
      >
        <Marker
          coordinate={{ latitude: driverLocation.lat, longitude: driverLocation.lng }}
          title={driverName ?? 'Driver'}
        >
          <View style={styles.markerWrap}>
            <View style={styles.markerRing} />
            <View style={styles.markerDot}>
              <Ionicons name="bicycle" size={14} color={CANVAS} />
            </View>
          </View>
        </Marker>
      </MapView>
      <View style={styles.coordsBadge}>
        <Ionicons name="navigate" size={10} color={CYAN} />
        <Text style={styles.coordsText}>
          {driverLocation.lat.toFixed(4)}, {driverLocation.lng.toFixed(4)}
        </Text>
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    marginTop: 16,
    borderRadius: 14,
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: BORDER,
    backgroundColor: CANVAS,
  },
  header: {
    position: 'absolute',
    top: 10,
    left: 10,
    zIndex: 10,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    backgroundColor: 'rgba(5,8,16,0.85)',
    borderWidth: 1,
    borderColor: GREEN + '40',
    borderRadius: 20,
    paddingHorizontal: 10,
    paddingVertical: 5,
  },
  pulseDot: {
    width: 6,
    height: 6,
    borderRadius: 3,
    backgroundColor: GREEN,
  },
  headerText: {
    fontSize: 10,
    color: GREEN,
    fontWeight: '700',
    letterSpacing: 0.5,
  },
  markerWrap: {
    alignItems: 'center',
    justifyContent: 'center',
  },
  markerRing: {
    position: 'absolute',
    width: 44,
    height: 44,
    borderRadius: 22,
    backgroundColor: CYAN + '30',
    borderWidth: 2,
    borderColor: CYAN,
  },
  markerDot: {
    width: 28,
    height: 28,
    borderRadius: 14,
    backgroundColor: CYAN,
    alignItems: 'center',
    justifyContent: 'center',
  },
  coordsBadge: {
    position: 'absolute',
    bottom: 10,
    right: 10,
    zIndex: 10,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 5,
    backgroundColor: 'rgba(5,8,16,0.85)',
    borderWidth: 1,
    borderColor: BORDER,
    borderRadius: 8,
    paddingHorizontal: 8,
    paddingVertical: 4,
  },
  coordsText: {
    fontSize: 9,
    color: CYAN,
    fontFamily: 'JetBrainsMono-Regular',
    fontWeight: '600',
  },
});
