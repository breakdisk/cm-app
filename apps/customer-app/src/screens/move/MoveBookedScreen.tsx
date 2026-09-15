/**
 * Move app — Booked. Confirmation, then the delivery PIN, grouped.
 *
 * The PIN is issued here and kept until delivery; the driver cannot complete
 * the delivery without it (pod requires it, PR on claude/delivery-pin-required).
 */
import React from 'react';
import { ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useSelector } from 'react-redux';
import { Ionicons } from '@expo/vector-icons';
import type { RootState } from '../../store';
import { DeliveryPinCard } from '../../components/DeliveryPinCard';
import { HEADING, HEADING_LIGHT, M } from './theme';
import { Ambient, GhostButton, PrimaryButton } from './ui';

export function MoveBookedScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const phone = useSelector((s: RootState) => s.auth.phone);
  const { awb, id, quotedText, when } = route.params ?? {};

  return (
    <View style={s.root}>
      <Ambient />
      <ScrollView contentContainerStyle={[s.body, { paddingTop: insets.top + 40 }]}>
        <View style={s.check}>
          <Ionicons name="checkmark" size={30} color={M.accent} />
        </View>
        <Text style={s.h1Light}>Booked.</Text>
        {!!quotedText && <Text style={s.h1Strong}>Quoted {quotedText}</Text>}
        <Text style={s.note}>
          Reference {awb}{when ? ` · you asked for ${when}` : ''}. The driver confirms the pickup time, and the price is
          confirmed on your invoice.
        </Text>
        {!!id && (
          <View style={{ marginTop: 22 }}>
            <DeliveryPinCard shipmentId={id} recipientPhone={phone ?? ''} autoIssue />
          </View>
        )}
      </ScrollView>
      <View style={[s.actions, { paddingBottom: insets.bottom + 24 }]}>
        <PrimaryButton label="TRACK IT" onPress={() => navigation.replace('Track', { awb, id })} />
        <GhostButton label="Back to the agent" onPress={() => navigation.popToTop()} />
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  root:     { flex: 1, backgroundColor: M.ground },
  body:     { flexGrow: 1, justifyContent: 'center', paddingHorizontal: 24, paddingBottom: 24 },
  check:    { width: 64, height: 64, borderRadius: 32, alignItems: 'center', justifyContent: 'center', backgroundColor: M.accentTintStrong, borderWidth: 1, borderColor: 'rgba(0,229,255,0.4)', marginBottom: 28 },
  h1Light:  { fontFamily: HEADING_LIGHT, fontWeight: '300', fontSize: 40, lineHeight: 43, color: M.ink },
  h1Strong: { fontFamily: HEADING, fontWeight: '700', fontSize: 40, lineHeight: 43, color: M.ink },
  note:     { marginTop: 14, fontSize: 14, lineHeight: 21, color: M.muted },
  actions:  { paddingHorizontal: 24, gap: 12 },
});
