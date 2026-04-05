import React from 'react';
import { View, Text, TouchableOpacity, ScrollView } from 'react-native';
import Input from '../../components/Input';
import { COLORS } from '../../utils/colors';

interface PackageDetailsFormProps {
  description: string;
  onDescriptionChange: (value: string) => void;
  weight: string;
  onWeightChange: (value: string) => void;
  cargoType: string;
  onCargoTypeChange: (value: string) => void;
  codEnabled: boolean;
  onCodEnabledChange: (value: boolean) => void;
  codAmount: string;
  onCodAmountChange: (value: string) => void;
  errors: Record<string, string>;
}

export default function PackageDetailsForm({
  description,
  onDescriptionChange,
  weight,
  onWeightChange,
  cargoType,
  onCargoTypeChange,
  codEnabled,
  onCodEnabledChange,
  codAmount,
  onCodAmountChange,
  errors,
}: PackageDetailsFormProps) {
  const cargoTypes = ['documents', 'goods', 'fragile', 'electronics'];

  return (
    <ScrollView showsVerticalScrollIndicator={false}>
      <Input
        label="Package Description"
        placeholder="What are you shipping?"
        value={description}
        onChangeText={onDescriptionChange}
        error={errors.description}
        multiline
      />

      <Input
        label="Weight (kg)"
        placeholder="0.5"
        value={weight}
        onChangeText={onWeightChange}
        error={errors.weight}
        keyboardType="decimal-pad"
      />

      <View style={{ marginBottom: 16 }}>
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 8 }}>
          Cargo Type
        </Text>
        <View style={{ flexDirection: 'row', flexWrap: 'wrap', gap: 8 }}>
          {cargoTypes.map(type => (
            <TouchableOpacity
              key={type}
              onPress={() => onCargoTypeChange(type)}
              style={{
                paddingVertical: 8,
                paddingHorizontal: 12,
                borderRadius: 8,
                backgroundColor: cargoType === type ? COLORS.CYAN : COLORS.SURFACE,
                borderWidth: 1,
                borderColor: cargoType === type ? COLORS.CYAN : COLORS.BORDER,
              }}
            >
              <Text
                style={{
                  color: cargoType === type ? COLORS.CANVAS : COLORS.TEXT_PRIMARY,
                  fontSize: 12,
                  fontWeight: '600',
                }}
              >
                {type.charAt(0).toUpperCase() + type.slice(1)}
              </Text>
            </TouchableOpacity>
          ))}
        </View>
      </View>

      {/* COD Toggle */}
      <View
        style={{
          marginBottom: 16,
          flexDirection: 'row',
          justifyContent: 'space-between',
          alignItems: 'center',
          paddingVertical: 12,
          paddingHorizontal: 12,
          backgroundColor: COLORS.SURFACE,
          borderRadius: 8,
          borderWidth: 1,
          borderColor: COLORS.BORDER,
        }}
      >
        <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600' }}>
          Cash on Delivery (COD)
        </Text>
        <TouchableOpacity
          onPress={() => onCodEnabledChange(!codEnabled)}
          style={{
            width: 50,
            height: 30,
            backgroundColor: codEnabled ? COLORS.CYAN : COLORS.SURFACE,
            borderRadius: 15,
            justifyContent: codEnabled ? 'flex-end' : 'flex-start',
            paddingHorizontal: 2,
            borderWidth: 1,
            borderColor: COLORS.BORDER,
          }}
        >
          <View
            style={{
              width: 26,
              height: 26,
              backgroundColor: COLORS.TEXT_PRIMARY,
              borderRadius: 13,
            }}
          />
        </TouchableOpacity>
      </View>

      {codEnabled && (
        <Input
          label="COD Amount (PHP)"
          placeholder="1000"
          value={codAmount}
          onChangeText={onCodAmountChange}
          error={errors.codAmount}
          keyboardType="numeric"
        />
      )}
    </ScrollView>
  );
}
