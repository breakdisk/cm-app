/**
 * Pieces the whole-home screens share: the read-back panel from the design
 * (what the prompt box read), a pill row, and common styles.
 */
import React, { useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import { Ionicons } from '@expo/vector-icons';
import { HEADING, M } from '../theme';
import type { ReadBack } from './homeDraft';

export function apiMessage(err: any): string {
  return err?.data?.error?.message ?? err?.data?.message ?? err?.message ?? 'Something went wrong.';
}

export function makeKey(): string {
  return `hm_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 10)}`;
}

/** "From your message": the sentence and what was read from it. Dismissing
 *  hides it for this screen; it never un-fills the intake. */
export function ReadBackPanel({ read, onChangeProperty }: { read: ReadBack | null; onChangeProperty?: () => void }) {
  const [hidden, setHidden] = useState(false);
  if (!read || hidden) return null;
  return (
    <View style={h.read}>
      <View style={h.readHead}>
        <Ionicons name="sparkles-outline" size={14} color={M.accent} />
        <Text style={h.readLabel}>FROM YOUR MESSAGE</Text>
        <Pressable onPress={() => setHidden(true)} accessibilityRole="button" hitSlop={8}>
          <Text style={h.dismiss}>DISMISS</Text>
        </Pressable>
      </View>
      <Text style={h.sentence}>“{read.sentence}”</Text>
      {read.chips.length > 0 && (
        <View style={h.chips}>
          {read.chips.map((c) => <View key={c} style={h.chip}><Text style={h.chipText}>{c}</Text></View>)}
        </View>
      )}
      <Text style={h.readNote}>
        {read.confident
          ? 'Read from your message. Correct anything that is wrong before you price it.'
          : 'Read from your message. Fill in what is missing below.'}
      </Text>
      {onChangeProperty && (
        <Pressable onPress={onChangeProperty} accessibilityRole="button" style={({ pressed }) => [h.outline, pressed && { opacity: 0.7 }]}>
          <Text style={h.outlineText}>CHANGE THE PROPERTY</Text>
        </Pressable>
      )}
    </View>
  );
}

export function Pills<T extends string | number>({ options, value, onChange, label }: {
  options: { value: T; label: string }[];
  value: T;
  onChange: (v: T) => void;
  label: string;
}) {
  return (
    <View style={h.pills} accessibilityRole="radiogroup" accessibilityLabel={label}>
      {options.map((o) => {
        const on = o.value === value;
        return (
          <Pressable
            key={String(o.value)}
            onPress={() => onChange(o.value)}
            accessibilityRole="radio"
            accessibilityState={{ selected: on }}
            style={({ pressed }) => [h.pill, on && h.pillOn, pressed && { opacity: 0.75 }]}
          >
            <Text style={[h.pillText, on && { color: M.accent }]}>{o.label}</Text>
          </Pressable>
        );
      })}
    </View>
  );
}

export const h = StyleSheet.create({
  root:      { flex: 1, backgroundColor: M.ground },
  content:   { paddingHorizontal: 16, gap: 12 },
  heading:   { fontFamily: HEADING, fontWeight: '700', fontSize: 26, lineHeight: 30, color: M.ink, marginTop: 4, marginBottom: 6 },
  body:      { fontSize: 13, lineHeight: 19, color: M.muted },
  note:      { fontSize: 11, lineHeight: 16, color: M.faint },
  row:       { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12 },
  rowLabel:  { fontSize: 14, color: M.ink, flexShrink: 1 },
  big:       { fontFamily: HEADING, fontWeight: '700', fontSize: 30, color: M.ink },
  footer:    { paddingHorizontal: 16, paddingTop: 10, gap: 10, borderTopWidth: 1, borderTopColor: M.hairline, backgroundColor: 'rgba(5,8,16,0.92)' },
  read:      { borderRadius: 18, paddingVertical: 15, paddingHorizontal: 16, backgroundColor: 'rgba(45,212,232,0.07)', borderWidth: 1, borderColor: 'rgba(45,212,232,0.26)', marginBottom: 6 },
  readHead:  { flexDirection: 'row', alignItems: 'center', gap: 7 },
  readLabel: { flex: 1, fontSize: 10, letterSpacing: 1.8, color: '#2dd4e8' },
  dismiss:   { fontSize: 10, letterSpacing: 1.4, color: 'rgba(242,246,250,0.45)' },
  sentence:  { marginTop: 10, fontSize: 13, lineHeight: 19.5, color: 'rgba(242,246,250,0.72)' },
  chips:     { flexDirection: 'row', flexWrap: 'wrap', gap: 7, marginTop: 10 },
  chip:      { paddingVertical: 6, paddingHorizontal: 11, borderRadius: 999, backgroundColor: 'rgba(45,212,232,0.14)', borderWidth: 1, borderColor: 'rgba(45,212,232,0.3)' },
  chipText:  { fontSize: 12, color: '#bdf3fb' },
  readNote:  { marginTop: 10, fontSize: 11, lineHeight: 16, color: 'rgba(242,246,250,0.45)' },
  outline:   { marginTop: 12, minHeight: 44, borderRadius: 12, borderWidth: 1, borderColor: M.accentBorder, alignItems: 'center', justifyContent: 'center' },
  outlineText: { color: M.accent, fontWeight: '700', letterSpacing: 1.2, fontSize: 12 },
  pills:     { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  pill:      { minHeight: 40, paddingHorizontal: 14, borderRadius: 999, borderWidth: 1, borderColor: M.hairlineStrong, alignItems: 'center', justifyContent: 'center' },
  pillOn:    { borderColor: M.accentBorder, backgroundColor: M.accentTintStrong },
  pillText:  { fontSize: 13, color: M.muted },
  bar:       { height: 5, borderRadius: 3, backgroundColor: M.hairline, overflow: 'hidden', marginTop: 8 },
  barFill:   { height: 5, borderRadius: 3, backgroundColor: M.accent },
});
