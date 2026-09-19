/**
 * Whole-home move, A6 — booked. The reference, the survey and the move day.
 */
import React from 'react';
import { ScrollView, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { slotDay, slotHours } from '../../../services/api/homeMove';
import { HEADING_LIGHT, M } from '../theme';
import { Ambient, GhostButton, Panel, PrimaryButton } from '../ui';
import { h } from './homeUi';

export function MoveHomeBookedScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const { awb, id, surveyAt, moveAt, offset = 480 } = route.params ?? {};
  const at = (iso: string, hours: number) => ({ starts_at: iso, ends_at: new Date(new Date(iso).getTime() + hours * 3_600_000).toISOString() });

  return (
    <View style={h.root}>
      <Ambient />
      <ScrollView contentContainerStyle={[h.content, { paddingTop: insets.top + 40, paddingBottom: 24 }]}>
        <View style={{ width: 64, height: 64, borderRadius: 32, alignItems: 'center', justifyContent: 'center', backgroundColor: M.accentTintStrong, borderWidth: 1, borderColor: M.accentBorder, marginBottom: 20 }}>
          <Ionicons name="home-outline" size={28} color={M.accent} />
        </View>
        <Text style={{ fontFamily: HEADING_LIGHT, fontWeight: '300', fontSize: 38, color: M.ink }}>Your move is booked.</Text>
        <Text style={[h.body, { marginTop: 8 }]}>Reference {awb}</Text>

        {!!surveyAt && (
          <Panel style={{ marginTop: 16 }}>
            <Text style={[h.note, { letterSpacing: 1.4 }]}>SURVEY</Text>
            <Text style={[h.rowLabel, { marginTop: 4 }]}>{slotDay(surveyAt, offset)}</Text>
            <Text style={h.note}>{slotHours(at(surveyAt, 2), offset)} · your crew lead checks the inventory</Text>
          </Panel>
        )}
        {!!moveAt && (
          <Panel tone="accent">
            <Text style={[h.note, { letterSpacing: 1.4 }]}>MOVE DAY</Text>
            <Text style={[h.rowLabel, { marginTop: 4 }]}>{slotDay(moveAt, offset)}</Text>
            <Text style={h.note}>Crew arrives {slotHours(at(moveAt, 4), offset).split(' – ')[0]}</Text>
          </Panel>
        )}
        <Text style={h.note}>We'll message you when your crew lead is named.</Text>
      </ScrollView>
      <View style={[h.footer, { paddingBottom: insets.bottom + 12 }]}>
        {!!id && <PrimaryButton label="TRACK IT" onPress={() => navigation.replace('Track', { awb, id })} />}
        <GhostButton label="Back home" onPress={() => navigation.popToTop()} />
      </View>
    </View>
  );
}
