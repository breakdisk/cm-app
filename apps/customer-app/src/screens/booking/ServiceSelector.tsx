import React from 'react';
import { View, Text, TouchableOpacity } from 'react-native';
import { COLORS } from '../../utils/colors';

interface ServiceOption {
  id: string;
  name: string;
  description: string;
  estimatedDays: number;
  price: number;
}

interface ServiceSelectorProps {
  type: 'local' | 'international';
  selected: string;
  onSelect: (id: string) => void;
}

export default function ServiceSelector({ type, selected, onSelect }: ServiceSelectorProps) {
  const services: ServiceOption[] =
    type === 'local'
      ? [
          { id: 'standard', name: 'Standard', description: '3-5 days', estimatedDays: 5, price: 150 },
          { id: 'express', name: 'Express', description: '1-2 days', estimatedDays: 2, price: 350 },
          {
            id: 'nextday',
            name: 'Next Day',
            description: 'Next business day',
            estimatedDays: 1,
            price: 500,
          },
        ]
      : [
          { id: 'air', name: 'Air Freight', description: '5-7 days', estimatedDays: 7, price: 800 },
          {
            id: 'sea',
            name: 'Sea Freight',
            description: '14-21 days',
            estimatedDays: 21,
            price: 300,
          },
        ];

  return (
    <View style={{ marginBottom: 20 }}>
      <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 12 }}>
        Delivery Service
      </Text>
      {services.map(service => (
        <TouchableOpacity
          key={service.id}
          onPress={() => onSelect(service.id)}
          style={{
            paddingHorizontal: 12,
            paddingVertical: 12,
            borderRadius: 8,
            backgroundColor: selected === service.id ? COLORS.CYAN : COLORS.SURFACE,
            marginBottom: 8,
            borderWidth: 1,
            borderColor: selected === service.id ? COLORS.CYAN : COLORS.BORDER,
          }}
        >
          <View
            style={{
              flexDirection: 'row',
              justifyContent: 'space-between',
              alignItems: 'center',
            }}
          >
            <View>
              <Text
                style={{
                  color: selected === service.id ? COLORS.CANVAS : COLORS.TEXT_PRIMARY,
                  fontWeight: '600',
                  fontSize: 14,
                }}
              >
                {service.name}
              </Text>
              <Text
                style={{
                  color: selected === service.id ? COLORS.CANVAS : COLORS.TEXT_SECONDARY,
                  fontSize: 12,
                  marginTop: 2,
                }}
              >
                {service.description}
              </Text>
            </View>
            <Text
              style={{
                color: selected === service.id ? COLORS.CANVAS : COLORS.CYAN,
                fontWeight: '700',
                fontSize: 14,
              }}
            >
              ₱{service.price}
            </Text>
          </View>
        </TouchableOpacity>
      ))}
    </View>
  );
}
