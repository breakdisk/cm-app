import React, { useMemo } from 'react';
import { View, Text } from 'react-native';
import { COLORS } from '../utils/colors';

type Status = 'pending' | 'processing' | 'picked' | 'in_transit' | 'delivered' | 'failed' | 'cancelled';

interface StatusBadgeProps {
  status: Status;
  size?: 'sm' | 'md';
}

export default function StatusBadge({ status, size = 'md' }: StatusBadgeProps) {
  const { label, bgColor, textColor } = useMemo(() => {
    const config: Record<Status, { label: string; bgColor: string; textColor: string }> = {
      pending: { label: 'Pending', bgColor: COLORS.AMBER, textColor: COLORS.CANVAS },
      processing: { label: 'Processing', bgColor: COLORS.AMBER, textColor: COLORS.CANVAS },
      picked: { label: 'Picked Up', bgColor: COLORS.CYAN, textColor: COLORS.CANVAS },
      in_transit: { label: 'In Transit', bgColor: COLORS.PURPLE, textColor: COLORS.TEXT_PRIMARY },
      delivered: { label: 'Delivered', bgColor: COLORS.GREEN, textColor: COLORS.CANVAS },
      failed: { label: 'Failed', bgColor: COLORS.RED, textColor: COLORS.TEXT_PRIMARY },
      cancelled: { label: 'Cancelled', bgColor: COLORS.TEXT_TERTIARY, textColor: COLORS.TEXT_PRIMARY },
    };
    return config[status] || config.pending;
  }, [status]);

  const padding = size === 'sm' ? { paddingVertical: 4, paddingHorizontal: 8 } : { paddingVertical: 6, paddingHorizontal: 12 };
  const fontSize = size === 'sm' ? 12 : 14;

  return (
    <View
      testID="status-badge"
      style={[
        {
          backgroundColor: bgColor,
          borderRadius: 12,
          alignSelf: 'flex-start',
        },
        padding,
      ]}
    >
      <Text style={{ color: textColor, fontSize, fontWeight: '600' }}>{label}</Text>
    </View>
  );
}
