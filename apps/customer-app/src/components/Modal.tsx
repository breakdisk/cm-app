import React from 'react';
import { Modal as RNModal, View, TouchableOpacity, Text } from 'react-native';
import { COLORS } from '../utils/colors';
import Button from './Button';

interface ModalProps {
  visible: boolean;
  onClose: () => void;
  title?: string;
  children: React.ReactNode;
  actions?: Array<{ label: string; onPress: () => void; variant?: 'primary' | 'secondary' }>;
}

export default function Modal({ visible, onClose, title, children, actions }: ModalProps) {
  return (
    <RNModal visible={visible} transparent animationType="slide">
      <View style={{ flex: 1, backgroundColor: 'rgba(0,0,0,0.5)', justifyContent: 'flex-end' }}>
        <View
          style={{
            backgroundColor: COLORS.SURFACE,
            borderTopLeftRadius: 20,
            borderTopRightRadius: 20,
            paddingHorizontal: 20,
            paddingVertical: 20,
            paddingBottom: 40,
            maxHeight: '80%',
          }}
        >
          {/* Header */}
          <View style={{ flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', marginBottom: 16 }}>
            <Text style={{ color: COLORS.TEXT_PRIMARY, fontSize: 18, fontWeight: '700' }}>{title}</Text>
            <TouchableOpacity onPress={onClose}>
              <Text style={{ color: COLORS.TEXT_SECONDARY, fontSize: 28 }}>×</Text>
            </TouchableOpacity>
          </View>

          {/* Content */}
          {children}

          {/* Actions */}
          {actions && (
            <View style={{ marginTop: 20, gap: 10 }}>
              {actions.map((action, i) => (
                <Button key={i} label={action.label} onPress={action.onPress} variant={action.variant || 'primary'} size="md" />
              ))}
            </View>
          )}
        </View>
      </View>
    </RNModal>
  );
}
