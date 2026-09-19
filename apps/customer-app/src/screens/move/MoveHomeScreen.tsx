/**
 * Move app — Home. "Tell me what needs moving."
 *
 * The only entry point: the prompt either routes to support (the handoff's
 * SUPPORT_HINTS, and only with an active job) or into the planner.
 */
import React, { useCallback, useState } from 'react';
import { KeyboardAvoidingView, Platform, Pressable, ScrollView, StyleSheet, Text, TextInput, View, Alert } from 'react-native';
import { useFocusEffect } from '@react-navigation/native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useSelector } from 'react-redux';
import { Ionicons } from '@expo/vector-icons';
import type { RootState } from '../../store';
import { useShipments } from '../../hooks/useShipments';
import { speechAvailable } from './voice';
import { unreadCount } from '../../services/api/inbox';
import { resetDraft } from './home/homeDraft';
import { HEADING, HEADING_LIGHT, M } from './theme';
import { Ambient, IconButton, Label, PrimaryButton } from './ui';

const SUGGESTIONS = [
  'Sofa and 2 boxes to my new place Saturday',
  'Fridge swap, haul the old one',
  'Two pallets to the warehouse Thursday',
];

export const ACTIVE_STATUSES = ['pending', 'confirmed', 'picked_up', 'in_transit', 'out_for_delivery', 'delivery_attempted'];

export const STATUS_WORDS: Record<string, string> = {
  pending: 'Booked',
  confirmed: 'Booked',
  picked_up: 'Picked up',
  in_transit: 'On the move',
  out_for_delivery: 'Almost there',
  delivery_attempted: 'Delivery attempted',
  delivered: 'Delivered',
  cancelled: 'Cancelled',
  returned: 'Returned',
};

export function MoveHomeScreen({ navigation }: { navigation: any }) {
  const insets = useSafeAreaInsets();
  const brand = useSelector((s: RootState) => s.branding.displayName);
  const { list, refetch } = useShipments();
  const [prompt, setPrompt] = useState('');
  // Offered only where this build and this phone can actually listen.
  const [canSpeak] = useState(speechAvailable);

  // The inbox dot: refreshed each time home comes back into view.
  const [unread, setUnread] = useState(0);
  useFocusEffect(useCallback(() => {
    refetch();
    unreadCount().then(setUnread);
  }, [refetch]));

  const active = list.find((s) => ACTIVE_STATUSES.includes(s.status));

  function send() {
    const text = prompt.trim();
    if (!text) return;
    // Thinking asks the server where this goes (a booking, a whole-home
    // move, or a question about the running job) and falls back to the
    // on-device reading. With no job running, a question is still a booking.
    navigation.navigate('MoveThinking', { prompt: text, mode: 'prompt', hasActiveJob: !!active });
    setPrompt('');
  }

  return (
    <KeyboardAvoidingView style={s.root} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
      <Ambient />

      <View style={[s.header, { paddingTop: insets.top + 12 }]}>
        <View style={s.brandRow}>
          <View style={s.brandRing}><View style={s.brandDot} /></View>
          <Text style={s.brand} numberOfLines={1}>{(brand || 'LogisticOS').toUpperCase()}</Text>
        </View>
        <View style={s.headerIcons}>
          <IconButton name="chatbubble-ellipses-outline" label="Support" onPress={() => navigation.navigate('Support')} />
          <IconButton name="shield-checkmark-outline" label="Coverage and rules" onPress={() => navigation.navigate('MoveRules')} />
          <IconButton name="pricetags-outline" label="Offers" onPress={() => navigation.navigate('MoveOffers')} />
          <IconButton
            name="mail-outline"
            label={unread > 0 ? `Inbox, ${unread} unread` : 'Inbox'}
            onPress={() => navigation.navigate('MoveInbox')}
            badge={unread > 0}
          />
          <IconButton name="wallet-outline" label="Payments" onPress={() => navigation.navigate('MovePayments')} />
          <IconButton name="person-circle-outline" label="Account" onPress={() => navigation.navigate('Profile')} />
        </View>
      </View>

      <ScrollView
        contentContainerStyle={{ paddingHorizontal: 20, paddingTop: 14, paddingBottom: 250 + insets.bottom }}
        keyboardShouldPersistTaps="handled"
      >
        <Text style={s.h1Light}>Tell me what</Text>
        <Text style={s.h1Strong}>needs moving.</Text>
        <Text style={s.sub}>Type it. I size the vehicle, price it all-in, and find the driver.</Text>

        <View style={s.chips}>
          {SUGGESTIONS.map((label) => (
            <Pressable
              key={label}
              onPress={() => setPrompt(label)}
              accessibilityRole="button"
              style={({ pressed }) => [s.chip, pressed && { transform: [{ scale: 0.95 }] }]}
            >
              <Text style={s.chipText}>{label}</Text>
            </Pressable>
          ))}
        </View>

        <Pressable
          onPress={() => { resetDraft(); navigation.navigate('MoveHomeSet'); }}
          accessibilityRole="button"
          style={({ pressed }) => [s.homeCard, pressed && { transform: [{ scale: 0.98 }] }]}
        >
          <Ionicons name="home-outline" size={20} color={M.accent} />
          <View style={{ flex: 1 }}>
            <Text style={s.homeTitle}>Moving a whole home</Text>
            <Text style={s.homeMeta}>Room-by-room inventory · trucks · helpers</Text>
          </View>
          <Ionicons name="chevron-forward" size={18} color={M.faint} />
        </Pressable>

        {active && (
          <>
            <Label>Live now</Label>
            <Pressable
              onPress={() => navigation.navigate('Track', { awb: active.awb, id: active.id })}
              accessibilityRole="button"
              style={({ pressed }) => [s.live, pressed && { transform: [{ scale: 0.975 }] }]}
            >
              <View style={s.liveTop}>
                <Text style={s.liveRef}>{active.awb}</Text>
                <View style={s.liveStatus}>
                  <View style={s.liveDot} />
                  <Text style={s.liveStatusText}>{STATUS_WORDS[active.status] ?? active.status}</Text>
                </View>
              </View>
              <Text style={s.liveTitleLight} numberOfLines={1}>{active.description || 'Your move'}</Text>
              <Text style={s.liveTitleStrong} numberOfLines={1}>{active.destination}</Text>
              <Text style={s.liveMeta}>Tap to track</Text>
            </Pressable>
          </>
        )}
      </ScrollView>

      <View style={[s.composer, { bottom: insets.bottom + 14 }]}>
        <TextInput
          value={prompt}
          onChangeText={setPrompt}
          placeholder="Move my sofa and 2 boxes to 240 N Michigan tomorrow at 2 PM"
          placeholderTextColor={M.faint}
          multiline
          style={s.composerInput}
          accessibilityLabel="What needs moving"
        />
        <View style={s.composerRow}>
          {canSpeak && (
            <Pressable
              onPress={() => navigation.navigate('MoveVoice')}
              accessibilityRole="button"
              accessibilityLabel="Say what needs moving"
              style={({ pressed }) => [s.mic, pressed && { transform: [{ scale: 0.92 }] }]}
            >
              <Ionicons name="mic-outline" size={22} color={M.accent} />
            </Pressable>
          )}
          <PrimaryButton label="PLAN IT  →" onPress={send} disabled={!prompt.trim()} style={{ flex: 1 }} />
        </View>
      </View>
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  root:           { flex: 1, backgroundColor: M.ground },
  header:         { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', paddingHorizontal: 20, paddingBottom: 8, gap: 10 },
  brandRow:       { flexDirection: 'row', alignItems: 'center', gap: 10, flexShrink: 1 },
  brandRing:      { width: 26, height: 26, borderRadius: 13, borderWidth: 1, borderColor: 'rgba(0,229,255,0.5)', alignItems: 'center', justifyContent: 'center' },
  brandDot:       { width: 9, height: 9, borderRadius: 5, backgroundColor: M.accent },
  brand:          { flexShrink: 1, fontFamily: HEADING, fontWeight: '700', fontSize: 15, letterSpacing: 3.3, color: M.ink },
  // Six 40px buttons: 6px gaps keep them and the brand ring on a 320px phone.
  headerIcons:    { flexDirection: 'row', gap: 6, flexShrink: 0 },
  h1Light:        { fontFamily: HEADING_LIGHT, fontWeight: '300', fontSize: 44, lineHeight: 46, letterSpacing: -0.6, color: M.ink },
  h1Strong:       { fontFamily: HEADING, fontWeight: '700', fontSize: 44, lineHeight: 46, letterSpacing: -0.6, color: M.ink },
  sub:            { marginTop: 14, fontSize: 14, lineHeight: 21, color: M.muted, maxWidth: 300 },
  chips:          { flexDirection: 'row', flexWrap: 'wrap', gap: 8, marginTop: 22 },
  chip:           { paddingHorizontal: 14, paddingVertical: 11, borderRadius: 999, backgroundColor: 'rgba(255,255,255,0.04)', borderWidth: 1, borderColor: M.hairline },
  chipText:       { fontSize: 13, color: 'rgba(242,246,250,0.75)' },
  homeCard:       { flexDirection: 'row', alignItems: 'center', gap: 13, marginTop: 20, padding: 16, borderRadius: 18, backgroundColor: 'rgba(255,255,255,0.04)', borderWidth: 1, borderColor: M.hairlineStrong },
  homeTitle:      { fontSize: 14, color: M.ink },
  homeMeta:       { fontSize: 11, color: M.faint, marginTop: 3 },
  soon:           { fontSize: 10, letterSpacing: 1.6, color: M.label, paddingHorizontal: 8, paddingVertical: 4, borderRadius: 7, borderWidth: 1, borderColor: M.hairlineStrong },
  live:           { borderRadius: 24, padding: 20, backgroundColor: 'rgba(255,255,255,0.06)', borderWidth: 1, borderColor: 'rgba(255,255,255,0.11)' },
  liveTop:        { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between' },
  liveRef:        { fontSize: 11, letterSpacing: 2.2, color: M.label },
  liveStatus:     { flexDirection: 'row', alignItems: 'center', gap: 7 },
  liveDot:        { width: 6, height: 6, borderRadius: 3, backgroundColor: M.accent },
  liveStatusText: { fontSize: 11, letterSpacing: 1.5, textTransform: 'uppercase', color: M.accent },
  liveTitleLight: { fontFamily: HEADING_LIGHT, fontWeight: '300', fontSize: 26, lineHeight: 30, color: M.ink, marginTop: 12 },
  liveTitleStrong:{ fontFamily: HEADING, fontWeight: '700', fontSize: 26, lineHeight: 30, color: M.ink },
  liveMeta:       { fontSize: 11, color: M.faint, marginTop: 12 },
  composer:       { position: 'absolute', left: 14, right: 14, borderRadius: 26, padding: 14, backgroundColor: M.sheet, borderWidth: 1, borderColor: 'rgba(255,255,255,0.12)' },
  composerInput:  { minHeight: 48, maxHeight: 110, fontSize: 16, lineHeight: 23, color: M.ink, padding: 0, textAlignVertical: 'top' },
  composerRow:    { flexDirection: 'row', alignItems: 'center', gap: 10, marginTop: 12 },
  mic:            { width: 52, height: 52, borderRadius: 16, alignItems: 'center', justifyContent: 'center', backgroundColor: M.accentTint, borderWidth: 1, borderColor: M.accentBorder },
});
