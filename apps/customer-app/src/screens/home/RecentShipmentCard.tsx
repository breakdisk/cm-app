import React from 'react';
import { TouchableOpacity } from 'react-native';
import ShipmentCard from '../../components/ShipmentCard';
import { Shipment } from '../../store/slices/shipments';

interface RecentShipmentCardProps {
  shipment: Shipment;
  onPress: () => void;
}

export default function RecentShipmentCard({ shipment, onPress }: RecentShipmentCardProps) {
  return <ShipmentCard shipment={shipment} onPress={onPress} />;
}
