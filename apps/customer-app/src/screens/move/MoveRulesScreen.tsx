/**
 * Move app — Coverage and rules.
 *
 * Only what the platform actually does. The design's cover amounts, insurer
 * re-checks and protection-tier prices are placeholders with no backend behind
 * them, so they are not shown.
 */
import React from 'react';
import { ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { HEADING, HEADING_LIGHT, M } from './theme';
import { Ambient, Label, Panel, PrimaryButton, TopBar } from './ui';

const WONT_MOVE = ['Hazardous materials', 'Firearms and ammunition', 'Live animals', 'Cash and valuables', 'Perishables without cooling', 'Prescription drugs'];

const AUTHORITY = [
  { label: 'Price your move off the rate card', state: 'Always', on: true },
  { label: 'Book it', state: 'Only when you lock it in', on: true },
  { label: 'Change the price after booking', state: 'Never without you', on: false },
  { label: 'Share your number with the driver', state: 'For this job only', on: true },
];

export function MoveRulesScreen({ navigation }: { navigation: any }) {
  const insets = useSafeAreaInsets();
  return (
    <View style={s.root}>
      <Ambient />
      <TopBar label="Coverage & rules" onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 20, paddingTop: 8, paddingBottom: insets.bottom + 44 }}>
        <Panel tone="accent">
          <Text style={s.kicker}>How every move works</Text>
          <Text style={s.h1Light}>Priced before you commit,</Text>
          <Text style={s.h1Strong}>closed with your PIN.</Text>
          <Text style={s.body}>
            You see the full price, row by row, before anything is booked. The driver can't complete the delivery without
            your PIN — share it only when your things are in front of you.
          </Text>
        </Panel>

        <Label>What I do without asking</Label>
        <View style={s.list}>
          {AUTHORITY.map((a, i) => (
            <View key={a.label} style={[s.listRow, i === AUTHORITY.length - 1 && { borderBottomWidth: 0 }]}>
              <Text style={s.listLabel}>{a.label}</Text>
              <Text style={[s.listState, { color: a.on ? M.accent : M.amberText }]}>{a.state}</Text>
            </View>
          ))}
        </View>

        <Label>What I won't move</Label>
        <View style={s.chips}>
          {WONT_MOVE.map((b) => <Text key={b} style={s.chip}>{b}</Text>)}
        </View>
        <Text style={s.note}>Ask about anything else before you book.</Text>

        <Panel tone="amber" style={{ marginTop: 24 }}>
          <Text style={s.claimTitle}>Something arrive damaged?</Text>
          <Text style={s.body}>Tell support what happened. Your PIN record and the driver's proof of delivery are attached to the case.</Text>
          <PrimaryButton
            label="START A CLAIM"
            style={{ marginTop: 14 }}
            onPress={() => navigation.navigate('Support', { initialMessage: 'My items arrived damaged and I want to start a claim.' })}
          />
        </Panel>

        <Label>Cancelling</Label>
        <Text style={s.note}>
          Cancel from the move's tracking screen. The cost of cancelling, if any, is shown before you confirm.
        </Text>
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  root:       { flex: 1, backgroundColor: M.ground },
  kicker:     { fontSize: 10, letterSpacing: 2, textTransform: 'uppercase', color: M.accent },
  h1Light:    { fontFamily: HEADING_LIGHT, fontWeight: '300', fontSize: 30, lineHeight: 33, color: M.ink, marginTop: 8 },
  h1Strong:   { fontFamily: HEADING, fontWeight: '700', fontSize: 30, lineHeight: 33, color: M.ink },
  body:       { fontSize: 13, lineHeight: 20, color: M.muted, marginTop: 10 },
  list:       { borderRadius: 20, overflow: 'hidden', backgroundColor: 'rgba(255,255,255,0.035)', borderWidth: 1, borderColor: M.hairline },
  listRow:    { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', gap: 12, paddingHorizontal: 16, paddingVertical: 14, borderBottomWidth: 1, borderBottomColor: 'rgba(255,255,255,0.05)' },
  listLabel:  { flex: 1, fontSize: 13, color: M.ink },
  listState:  { fontSize: 10, letterSpacing: 1.4, textTransform: 'uppercase', textAlign: 'right', maxWidth: 140 },
  chips:      { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  chip:       { fontSize: 12, color: M.muted, paddingHorizontal: 12, paddingVertical: 9, borderRadius: 999, backgroundColor: 'rgba(255,255,255,0.03)', borderWidth: 1, borderColor: M.hairline, overflow: 'hidden' },
  note:       { fontSize: 12, lineHeight: 18, color: M.faint, marginTop: 12 },
  claimTitle: { fontFamily: HEADING, fontWeight: '700', fontSize: 19, color: M.ink },
});
