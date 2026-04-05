import React from 'react';
import { View, Text, ScrollView } from 'react-native';
import { MaterialIcons } from '@expo/vector-icons';
import { COLORS } from '../../utils/colors';
import Button from '../../components/Button';

interface BookingConfirmationProps {
  awb: string;
  onTrackPress: () => void;
  onHomePress: () => void;
}

export default function BookingConfirmation({
  awb,
  onTrackPress,
  onHomePress,
}: BookingConfirmationProps) {
  return (
    <ScrollView
      style={{ flex: 1, backgroundColor: COLORS.CANVAS }}
      contentContainerStyle={{
        padding: 20,
        justifyContent: 'center',
        minHeight: '100%',
      }}
    >
      <View style={{ alignItems: 'center', marginBottom: 32 }}>
        <MaterialIcons name="check-circle" size={80} color={COLORS.GREEN} />
        <Text
          style={{
            color: COLORS.TEXT_PRIMARY,
            fontSize: 24,
            fontWeight: '700',
            marginTop: 16,
            textAlign: 'center',
          }}
        >
          Booking Confirmed!
        </Text>
      </View>

      <View
        style={{
          backgroundColor: COLORS.SURFACE,
          borderRadius: 12,
          padding: 20,
          marginBottom: 20,
          borderWidth: 1,
          borderColor: COLORS.BORDER,
        }}
      >
        <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 12, marginBottom: 8 }}>
          Your Tracking Number
        </Text>
        <Text
          style={{
            color: COLORS.CYAN,
            fontSize: 20,
            fontWeight: '700',
            letterSpacing: 2,
          }}
        >
          {awb}
        </Text>
        <Text
          style={{
            color: COLORS.TEXT_TERTIARY,
            fontSize: 12,
            marginTop: 12,
          }}
        >
          Save this number to track your shipment
        </Text>
      </View>

      <View style={{ gap: 12 }}>
        <Button
          label="Track Shipment"
          onPress={onTrackPress}
          size="lg"
        />
        <Button
          label="Back to Home"
          onPress={onHomePress}
          variant="secondary"
          size="lg"
        />
      </View>
    </ScrollView>
  );
}
