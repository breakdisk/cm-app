import React from 'react';
import { View, Text } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { COLORS } from '../../utils/colors';

interface FeeBreakdownProps {
  baseFee: number;
  codFee: number;
  tax: number;
  total: number;
}

export default function FeeBreakdown({ baseFee, codFee, tax, total }: FeeBreakdownProps) {
  return (
    <LinearGradient
      colors={[COLORS.GLASS, COLORS.GLASS_HOVER]}
      start={{ x: 0, y: 0 }}
      end={{ x: 1, y: 1 }}
      style={{ borderRadius: 12, overflow: 'hidden' }}
    >
      <View
        style={{
          padding: 16,
          borderRadius: 12,
          borderWidth: 1,
          borderColor: COLORS.BORDER,
        }}
      >
        <View style={{ marginBottom: 12 }}>
          <View
            style={{
              flexDirection: 'row',
              justifyContent: 'space-between',
              marginBottom: 8,
            }}
          >
            <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13 }}>Base Fee</Text>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, fontWeight: '600' }}>
              ₱{baseFee}
            </Text>
          </View>
          {codFee > 0 && (
            <View
              style={{
                flexDirection: 'row',
                justifyContent: 'space-between',
                marginBottom: 8,
              }}
            >
              <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13 }}>COD Fee</Text>
              <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, fontWeight: '600' }}>
                ₱{codFee}
              </Text>
            </View>
          )}
          <View style={{ flexDirection: 'row', justifyContent: 'space-between' }}>
            <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 13 }}>Tax</Text>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 13, fontWeight: '600' }}>
              ₱{tax}
            </Text>
          </View>
        </View>
        <View
          style={{
            borderTopWidth: 1,
            borderTopColor: COLORS.BORDER,
            paddingTop: 12,
            flexDirection: 'row',
            justifyContent: 'space-between',
          }}
        >
          <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 16, fontWeight: '700' }}>
            Total
          </Text>
          <Text style={{ color: COLORS.CYAN, fontSize: 16, fontWeight: '700' }}>
            ₱{total}
          </Text>
        </View>
      </View>
    </LinearGradient>
  );
}
