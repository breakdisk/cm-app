import React from 'react';
import { View } from 'react-native';
import Input from '../../components/Input';

interface AddressInputProps {
  label: string;
  value: string;
  onChange: (value: string) => void;
  error?: string;
  placeholder?: string;
  multiline?: boolean;
  keyboardType?: 'default' | 'email-address' | 'numeric' | 'phone-pad' | 'decimal-pad';
}

export default function AddressInput({
  label,
  value,
  onChange,
  error,
  placeholder,
  multiline = false,
  keyboardType = 'default',
}: AddressInputProps) {
  return (
    <View style={{ marginBottom: 16 }}>
      <Input
        label={label}
        placeholder={placeholder || 'Enter ' + label.toLowerCase()}
        value={value}
        onChangeText={onChange}
        error={error}
        multiline={multiline}
        keyboardType={keyboardType}
      />
    </View>
  );
}
