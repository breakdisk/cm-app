import React from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { Shipment } from '../store/slices/shipments';
import StatusBadge from './StatusBadge';
import { COLORS } from '../utils/colors';
import { formatDate, formatCurrency, formatRouteString } from '../utils/formatting';

interface ShipmentCardProps {
  shipment: Shipment;
  onPress: () => void;
}

export default function ShipmentCard({ shipment, onPress }: ShipmentCardProps) {
  return (
    <TouchableOpacity onPress={onPress} activeOpacity={0.8}>
      <LinearGradient colors={[COLORS.GLASS, COLORS.GLASS_HOVER]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}>
        <View style={{ padding: 16, borderRadius: 12, borderWidth: 1, borderColor: COLORS.BORDER }}>
          {/* Header: AWB + Status */}
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 12 }}>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 16, fontWeight: '700' }}>{shipment.awb}</Text>
            <StatusBadge status={shipment.status} size="sm" />
          </View>

          {/* Route */}
          <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13, marginBottom: 8 }}>
            {formatRouteString(shipment.origin, shipment.destination)}
          </Text>

          {/* Date + Fee */}
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' }}>
            <Text style={{ color: COLORS.TEXT_TERTIARY, fontSize: 12 }}>{formatDate(new Date(shipment.date))}</Text>
            <Text style={{ color: COLORS.CYAN, fontSize: 13, fontWeight: '600' }}>
              {formatCurrency(shipment.fee, shipment.currency)}
            </Text>
          </View>
        </View>
      </LinearGradient>
    </TouchableOpacity>
  );
}
