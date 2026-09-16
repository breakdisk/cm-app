/**
 * Move app — Voice. Say what needs moving.
 *
 * The phone listens, the words fill the same prompt the home screen types, and
 * the same intent check routes it: support for a question about a live job,
 * the planner for a move.
 */
import React, { useCallback, useEffect, useRef, useState } from 'react';
import { Animated, Easing, Pressable, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { useShipments } from '../../hooks/useShipments';
import { ACTIVE_STATUSES } from './MoveHomeScreen';
import { classifyIntent } from './parsePrompt';
import { HEADING_LIGHT, M } from './theme';
import { Ambient, PrimaryButton } from './ui';
import {
  addVoiceListener, mergeTranscript, requestVoicePermission, startListening, stopListening, voiceErrorMessage,
} from './voice';

const BARS = [0, 1, 2, 3, 4, 5, 6];

export function MoveVoiceScreen({ navigation }: { navigation: any }) {
  const insets = useSafeAreaInsets();
  const { list } = useShipments();
  const [heard, setHeard] = useState('');
  const [listening, setListening] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const active = list.find((s) => ACTIVE_STATUSES.includes(s.status));
  const bars = useRef(BARS.map(() => new Animated.Value(0.35))).current;

  useEffect(() => {
    const subs = [
      addVoiceListener('start', () => { setListening(true); setError(null); }),
      addVoiceListener('end', () => setListening(false)),
      addVoiceListener('result', (e) => setHeard((prev) => mergeTranscript(prev, e?.results?.[0]?.transcript))),
      addVoiceListener('error', (e) => {
        const message = voiceErrorMessage(e?.error, e?.message);
        if (message) setError(message);
        setListening(false);
      }),
    ];
    return () => {
      subs.forEach((sub) => sub?.remove());
      stopListening();
    };
  }, []);

  useEffect(() => {
    let cancelled = false;
    requestVoicePermission().then((granted) => {
      if (cancelled) return;
      if (granted) startListening();
      else setError(voiceErrorMessage('not-allowed'));
    });
    return () => { cancelled = true; };
  }, []);

  // The bars move only while the phone is actually listening — a lively
  // animation over a dead microphone is a lie about what the app is doing.
  useEffect(() => {
    const loops = bars.map((value, i) =>
      Animated.loop(
        Animated.sequence([
          Animated.timing(value, {
            toValue: 1, duration: 380 + i * 55, delay: i * 60,
            easing: Easing.inOut(Easing.ease), useNativeDriver: true,
          }),
          Animated.timing(value, {
            toValue: 0.35, duration: 380 + i * 55,
            easing: Easing.inOut(Easing.ease), useNativeDriver: true,
          }),
        ]),
      ),
    );
    if (listening) {
      loops.forEach((loop) => loop.start());
    } else {
      bars.forEach((value) => value.setValue(0.35));
    }
    return () => loops.forEach((loop) => loop.stop());
  }, [listening, bars]);

  const toggle = useCallback(() => {
    if (listening) stopListening();
    else { setError(null); startListening(); }
  }, [listening]);

  const planIt = useCallback(() => {
    const text = heard.trim();
    if (!text) return;
    stopListening();
    if (classifyIntent(text, !!active) === 'support') navigation.replace('Support', { initialMessage: text });
    else navigation.replace('MoveThinking', { prompt: text });
  }, [heard, active, navigation]);

  return (
    <View style={s.root}>
      <Ambient />

      <View style={[s.top, { paddingTop: insets.top + 10 }]}>
        <Pressable
          onPress={() => { stopListening(); navigation.goBack(); }}
          hitSlop={8}
          accessibilityRole="button"
          accessibilityLabel="Close"
          style={({ pressed }) => [s.close, pressed && { transform: [{ scale: 0.92 }] }]}
        >
          <Ionicons name="close" size={20} color="rgba(242,246,250,0.8)" />
        </Pressable>
      </View>

      <Pressable
        onPress={toggle}
        accessibilityRole="button"
        accessibilityLabel={listening ? 'Stop listening' : 'Start listening'}
        style={s.stage}
      >
        <View style={s.halo}>
          <View style={s.ring} />
          <View style={s.bars}>
            {bars.map((value, i) => (
              <Animated.View key={i} style={[s.bar, { transform: [{ scaleY: value }] }]} />
            ))}
          </View>
        </View>

        <Text style={s.heard}>
          {heard ? `“${heard}”` : 'Say what needs moving'}
        </Text>
        <Text style={s.state}>
          {listening ? 'Listening · tap to stop' : error ? 'Stopped' : 'Tap to speak'}
        </Text>
        {!!error && <Text style={s.error}>{error}</Text>}
      </Pressable>

      <View style={[s.footer, { paddingBottom: insets.bottom + 24 }]}>
        <PrimaryButton label="THAT'S IT — PLAN IT" onPress={planIt} disabled={!heard.trim()} />
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  root:   { flex: 1, backgroundColor: M.ground },
  top:    { paddingHorizontal: 20, paddingBottom: 6, alignItems: 'flex-end' },
  close:  { width: 44, height: 44, borderRadius: 14, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderColor: M.hairlineStrong },
  stage:  { flex: 1, alignItems: 'center', justifyContent: 'center', paddingHorizontal: 26 },
  halo:   { width: 132, height: 132, borderRadius: 66, alignItems: 'center', justifyContent: 'center', backgroundColor: M.accentTint },
  ring:   { position: 'absolute', top: 14, left: 14, right: 14, bottom: 14, borderRadius: 52, borderWidth: 1, borderColor: M.accentBorder },
  bars:   { flexDirection: 'row', alignItems: 'center', gap: 5, height: 44 },
  bar:    { width: 4, height: 44, borderRadius: 2, backgroundColor: M.accent },
  heard:  { marginTop: 36, fontFamily: HEADING_LIGHT, fontWeight: '300', fontSize: 30, lineHeight: 35, color: M.ink, textAlign: 'center' },
  state:  { marginTop: 16, fontSize: 11, letterSpacing: 2.2, textTransform: 'uppercase', color: M.label, textAlign: 'center' },
  error:  { marginTop: 14, fontSize: 13, lineHeight: 19, color: M.amberText, textAlign: 'center' },
  footer: { paddingHorizontal: 20, paddingTop: 10 },
});
