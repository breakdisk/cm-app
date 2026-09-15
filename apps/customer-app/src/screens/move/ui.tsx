/**
 * The Move app's building blocks: glass panels, the gradient primary button,
 * 44px icon buttons and form fields, per the design handoff.
 */
import React from 'react';
import {
  ActivityIndicator, Pressable, StyleProp, StyleSheet, Switch, Text, TextInput,
  TextInputProps, View, ViewStyle,
} from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { Ionicons } from '@expo/vector-icons';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { HEADING, M } from './theme';

/** The soft cyan wash behind every screen. */
export function Ambient() {
  return (
    <LinearGradient
      pointerEvents="none"
      colors={['rgba(0,229,255,0.10)', 'rgba(89,128,166,0.04)', 'rgba(5,8,16,0)']}
      locations={[0, 0.35, 0.7]}
      style={st.ambient}
    />
  );
}

export function TopBar({ label, onBack, right }: { label: string; onBack?: () => void; right?: React.ReactNode }) {
  const insets = useSafeAreaInsets();
  return (
    <View style={[st.topBar, { paddingTop: insets.top + 10 }]}>
      {onBack && (
        <Pressable
          onPress={onBack}
          hitSlop={6}
          accessibilityRole="button"
          accessibilityLabel="Back"
          style={({ pressed }) => [st.backBtn, pressed && st.pressed]}
        >
          <Ionicons name="chevron-back" size={22} color="rgba(242,246,250,0.8)" />
        </Pressable>
      )}
      <Text style={st.topLabel} numberOfLines={1}>{label}</Text>
      {right}
    </View>
  );
}

export function Label({ children, style, right }: { children: React.ReactNode; style?: StyleProp<ViewStyle>; right?: React.ReactNode }) {
  return (
    <View style={[st.labelRow, style]}>
      <Text style={st.label}>{children}</Text>
      {right}
    </View>
  );
}

type Tone = 'default' | 'accent' | 'amber' | 'penalty';

const TONES: Record<Tone, { bg: string; border: string }> = {
  default: { bg: M.panel, border: M.hairline },
  accent:  { bg: M.accentTint, border: M.accentBorder },
  amber:   { bg: M.amberTint, border: M.amberBorder },
  penalty: { bg: M.penaltyTint, border: M.penaltyBorder },
};

export function Panel({ children, tone = 'default', style }: { children: React.ReactNode; tone?: Tone; style?: StyleProp<ViewStyle> }) {
  return <View style={[st.panel, { backgroundColor: TONES[tone].bg, borderColor: TONES[tone].border }, style]}>{children}</View>;
}

export function PrimaryButton({
  label, onPress, disabled, loading, danger, style,
}: { label: string; onPress: () => void; disabled?: boolean; loading?: boolean; danger?: boolean; style?: StyleProp<ViewStyle> }) {
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled || loading}
      accessibilityRole="button"
      accessibilityState={{ disabled: !!disabled, busy: !!loading }}
      style={({ pressed }) => [st.primaryWrap, style, disabled && st.disabled, pressed && st.pressed]}
    >
      <LinearGradient colors={danger ? M.penaltyGrad : M.accentGrad} style={st.primary}>
        {loading
          ? <ActivityIndicator color={danger ? M.penaltyInk : M.accentInk} />
          : <Text style={[st.primaryText, danger && { color: M.penaltyInk }]}>{label}</Text>}
      </LinearGradient>
    </Pressable>
  );
}

export function GhostButton({ label, onPress, color, style }: { label: string; onPress: () => void; color?: string; style?: StyleProp<ViewStyle> }) {
  return (
    <Pressable
      onPress={onPress}
      accessibilityRole="button"
      style={({ pressed }) => [st.ghost, style, pressed && st.pressed]}
    >
      <Text style={[st.ghostText, color ? { color } : null]}>{label}</Text>
    </Pressable>
  );
}

export function IconButton({ name, label, onPress, tone = 'default', badge }: {
  name: React.ComponentProps<typeof Ionicons>['name'];
  label: string;
  onPress: () => void;
  tone?: 'default' | 'amber';
  badge?: boolean;
}) {
  const amber = tone === 'amber';
  return (
    <Pressable
      onPress={onPress}
      hitSlop={4}
      accessibilityRole="button"
      accessibilityLabel={label}
      style={({ pressed }) => [st.iconBtn, amber && st.iconBtnAmber, pressed && st.pressed]}
    >
      <Ionicons name={name} size={18} color={amber ? M.amber : 'rgba(242,246,250,0.82)'} />
      {badge && <View style={st.badge} />}
    </Pressable>
  );
}

export function Field({ label, style, ...input }: TextInputProps & { label: string; style?: StyleProp<ViewStyle> }) {
  return (
    <View style={[{ flex: 1 }, style]}>
      <Text style={st.fieldLabel}>{label}</Text>
      <TextInput placeholderTextColor={M.faint} style={st.field} {...input} />
    </View>
  );
}

export function Toggle({ value, onChange, label }: { value: boolean; onChange: (v: boolean) => void; label: string }) {
  return (
    <Switch
      value={value}
      onValueChange={onChange}
      accessibilityLabel={label}
      trackColor={{ false: 'rgba(255,255,255,0.12)', true: 'rgba(0,229,255,0.45)' }}
      thumbColor={value ? M.accent : 'rgba(242,246,250,0.7)'}
    />
  );
}

export function Stepper({ value, min = 0, max, onChange, label }: { value: number; min?: number; max?: number; onChange: (v: number) => void; label: string }) {
  return (
    <View style={st.stepper}>
      <Pressable
        onPress={() => onChange(Math.max(min, value - 1))}
        disabled={value <= min}
        hitSlop={4}
        accessibilityRole="button"
        accessibilityLabel={`Fewer ${label}`}
        style={({ pressed }) => [st.stepBtn, value <= min && st.disabled, pressed && st.pressed]}
      >
        <Ionicons name="remove" size={16} color="rgba(242,246,250,0.85)" />
      </Pressable>
      <Text style={st.stepValue}>{value}</Text>
      <Pressable
        onPress={() => onChange(max === undefined ? value + 1 : Math.min(max, value + 1))}
        disabled={max !== undefined && value >= max}
        hitSlop={4}
        accessibilityRole="button"
        accessibilityLabel={`More ${label}`}
        style={({ pressed }) => [st.stepBtn, max !== undefined && value >= max && st.disabled, pressed && st.pressed]}
      >
        <Ionicons name="add" size={16} color="rgba(242,246,250,0.85)" />
      </Pressable>
    </View>
  );
}

export const st = StyleSheet.create({
  ambient:     { position: 'absolute', top: 0, left: 0, right: 0, height: 520 },
  topBar:      { flexDirection: 'row', alignItems: 'center', gap: 8, paddingHorizontal: 14, paddingBottom: 10 },
  backBtn:     { width: 44, height: 44, borderRadius: 14, alignItems: 'center', justifyContent: 'center' },
  topLabel:    { flex: 1, fontSize: 11, letterSpacing: 2.2, textTransform: 'uppercase', color: M.label },
  pressed:     { transform: [{ scale: 0.96 }] },
  disabled:    { opacity: 0.45 },
  labelRow:    { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', marginTop: 26, marginBottom: 12 },
  label:       { fontSize: 11, letterSpacing: 2.2, textTransform: 'uppercase', color: M.label },
  panel:       { borderRadius: 20, padding: 16, borderWidth: 1 },
  primaryWrap: { borderRadius: 18, overflow: 'hidden' },
  primary:     { height: 60, alignItems: 'center', justifyContent: 'center', borderRadius: 18, paddingHorizontal: 16 },
  primaryText: { fontSize: 13, fontWeight: '700', letterSpacing: 1.3, color: M.accentInk },
  ghost:       { height: 56, borderRadius: 18, borderWidth: 1, borderColor: M.hairlineStrong, alignItems: 'center', justifyContent: 'center', paddingHorizontal: 16 },
  ghostText:   { fontSize: 13, color: M.muted },
  iconBtn:     { width: 40, height: 40, borderRadius: 12, alignItems: 'center', justifyContent: 'center', backgroundColor: 'rgba(255,255,255,0.05)', borderWidth: 1, borderColor: M.hairline },
  iconBtnAmber:{ backgroundColor: M.amberTint, borderColor: M.amberBorder },
  badge:       { position: 'absolute', top: -2, right: -2, width: 11, height: 11, borderRadius: 6, backgroundColor: M.amber, borderWidth: 2, borderColor: M.ground },
  fieldLabel:  { fontSize: 11, color: M.faint, marginBottom: 6 },
  field:       { minHeight: 48, borderRadius: 13, paddingHorizontal: 13, paddingVertical: 10, fontSize: 15, color: M.ink, backgroundColor: 'rgba(255,255,255,0.05)', borderWidth: 1, borderColor: M.hairlineStrong },
  stepper:     { flexDirection: 'row', alignItems: 'center', gap: 6 },
  stepBtn:     { width: 36, height: 36, borderRadius: 11, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderColor: M.hairlineStrong },
  stepValue:   { minWidth: 22, textAlign: 'center', fontFamily: HEADING, fontWeight: '700', fontSize: 17, color: M.ink, fontVariant: ['tabular-nums'] },
});
