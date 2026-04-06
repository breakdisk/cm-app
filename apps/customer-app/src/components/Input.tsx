import React from 'react';
import { TextInput as RNTextInput, View, Text, TextInputProps as RNTextInputProps } from 'react-native';
import { COLORS } from '../utils/colors';

interface InputProps extends RNTextInputProps {
  label?: string;
  error?: string;
  multiline?: boolean;
}

export default function Input({ label, error, style, multiline, ...props }: InputProps) {
  return (
    <View style={{ marginBottom: 12 }}>
      {label && <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 14, fontWeight: '600', marginBottom: 6 }}>{label}</Text>}
      <RNTextInput
        {...props}
        multiline={multiline}
        placeholderTextColor={COLORS.TEXT_TERTIARY}
        style={[
          {
            backgroundColor: COLORS.SURFACE,
            borderWidth: 1,
            borderColor: error ? COLORS.RED : COLORS.BORDER,
            borderRadius: 8,
            paddingHorizontal: 12,
            paddingVertical: 10,
            fontSize: 14,
            color: COLORS.TEXT_PRIMARY,
            minHeight: multiline ? 100 : 44,
          },
          style,
        ]}
      />
      {error && <Text style={{ color: COLORS.RED, fontSize: 12, marginTop: 4 }}>{error}</Text>}
    </View>
  );
}
