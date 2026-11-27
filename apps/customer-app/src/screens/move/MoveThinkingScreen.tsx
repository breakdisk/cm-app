/**
 * Move app — Thinking. The agent's steps, revealed in sequence, then the plan.
 *
 * The first step waits on the server's reading of the prompt (a booking, a
 * whole-home move, or a question about the running job), with the on-device
 * parse as the fallback. A question goes to Support instead of a plan.
 *
 * Step four is "priced it off the rate card". The handoff is explicit: do not
 * reintroduce the spot-market auction copy — there is no auction.
 */
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { Animated, Easing, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { classifyIntent, describeParse, parsePrompt } from './parsePrompt';
import { classifyPrompt, route as routePrompt, type Classification, type Routed } from '../../services/api/classify';
import { isHomeMove, readChips, readHome, sizeFromRead, SIZES_BY_TYPE, type HomeRead } from '../../services/api/homeMove';
import { resetDraft, emptyDraft } from './home/homeDraft';

/** The design's floor for trusting the server's reading of a home sentence. */
const HOME_CONFIDENCE = 0.6;

/** What a home sentence stated: the server's reading when it is confident,
 *  else the on-device one. Only stated fields — never a guess. */
function homeRead(ai: Classification | null, prompt: string): { read: HomeRead; confident: boolean } {
  const p = ai?.extracted.property;
  if (ai && ai.intent === 'home_move' && ai.confidence >= HOME_CONFIDENCE && p) {
    const read: HomeRead = {};
    if (p.property_type) read.property_type = p.property_type;
    if (p.bedrooms != null) read.bedrooms = p.bedrooms;
    if (p.desks != null) read.desks = p.desks;
    if (p.pickup_floor != null) read.pickup_floor = p.pickup_floor;
    if (p.pickup_has_lift != null) read.pickup_has_lift = p.pickup_has_lift;
    return { read, confident: true };
  }
  return { read: readHome(prompt), confident: false };
}
import { HEADING, M } from './theme';
import { Ambient } from './ui';

const STEP_MS = 640;
const LAND_MS = 700;

function StepRow({ title, meta, done, active, visible }: { title: string; meta: string; done: boolean; active: boolean; visible: boolean }) {
  const anim = useRef(new Animated.Value(0)).current;
  useEffect(() => {
    Animated.timing(anim, {
      toValue: visible ? 1 : 0,
      duration: 500,
      easing: Easing.bezier(0.16, 1, 0.3, 1),
      useNativeDriver: true,
    }).start();
  }, [anim, visible]);

  return (
    <Animated.View
      style={[
        s.row,
        {
          opacity: anim.interpolate({ inputRange: [0, 1], outputRange: [0.3, 1] }),
          transform: [{ translateY: anim.interpolate({ inputRange: [0, 1], outputRange: [8, 0] }) }],
        },
      ]}
    >
      <View style={[s.mark, done && s.markDone, active && s.markActive]}>
        {done && <Ionicons name="checkmark" size={12} color={M.accentInk} />}
      </View>
      <View style={{ flex: 1 }}>
        <Text style={s.title}>{title}</Text>
        <Text style={s.meta}>{meta}</Text>
      </View>
    </Animated.View>
  );
}

export function MoveThinkingScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const prompt: string = route.params?.prompt ?? '';
  const mode: 'prompt' | 'voice' = route.params?.mode === 'voice' ? 'voice' : 'prompt';
  const hasActiveJob = !!route.params?.hasActiveJob;
  const regex = useMemo(() => parsePrompt(prompt), [prompt]);

  const [routed, setRouted] = useState<Routed | null>(null);
  const [ai, setAi] = useState<Classification | null>(null);
  useEffect(() => {
    let live = true;
    // Offline, the handoff's home rules come first: a whole home is never
    // priced as a single load.
    const offline = isHomeMove(prompt) ? 'home_move' : classifyIntent(prompt, hasActiveJob);
    classifyPrompt(prompt, hasActiveJob).then((answer) => {
      if (!live) return;
      setAi(answer);
      setRouted(routePrompt(answer, regex, offline, mode));
    });
    return () => { live = false; };
  }, [prompt, hasActiveJob, regex, mode]);

  const home = useMemo(() => {
    if (routed?.intent !== 'home_move') return null;
    const { read, confident } = homeRead(ai, prompt);
    const type = read.property_type ?? null;
    const size = type ? sizeFromRead(type, SIZES_BY_TYPE[type], read) : null;
    return { read, confident, type, size };
  }, [routed, ai, prompt]);

  // A question about the running job: the answer is in Support, not a plan.
  useEffect(() => {
    if (routed?.intent === 'support') navigation.replace('Support', { initialMessage: prompt });
  }, [routed, navigation, prompt]);

  const parsed = routed?.parsed ?? regex;

  const steps = useMemo(() => home ? [
    { title: 'Read your message', meta: 'Whole-home relocation — not a single load' },
    { title: 'Matched the property', meta: readChips(home.read, home.size).join(' · ') || 'Property type still needed' },
    { title: 'Loaded the room list', meta: 'Rooms with a preset inventory each' },
    { title: 'Sized the fleet class', meta: 'Box trucks · trucks and trips, not a choice of vehicle' },
    {
      title: home.type === 'apartment' && home.size && SIZES_BY_TYPE.apartment.indexOf(home.size) < 2 ? 'Self-declared inventory' : 'Flagged an attended survey',
      meta: home.type === 'apartment' && home.size && SIZES_BY_TYPE.apartment.indexOf(home.size) < 2
        ? 'Below two bedrooms you list it yourself'
        : 'A surveyor checks the inventory before the move',
    },
  ] : [
    { title: 'Parsed your request', meta: describeParse(parsed) },
    {
      title: 'Matched item profiles',
      meta: parsed.items.map((i) => (i.qty > 1 ? `${i.qty} ${i.name}` : i.name)).join(', ') || 'Add the items on the next screen',
    },
    { title: 'Sized the vehicle', meta: 'From the weight and size of the load' },
    { title: 'Priced it off the rate card', meta: 'Base + per km + per kg, on billable weight' },
    { title: 'Ready for your check', meta: 'Nothing is booked until you lock it in' },
  ], [parsed, home]);

  const [step, setStep] = useState(0);
  useEffect(() => {
    // The first step is the reading itself: it holds until the server has
    // answered or the fallback has taken over.
    if (step === 0 && !routed) return;
    if (routed?.intent === 'support') return;
    if (step < steps.length) {
      const t = setTimeout(() => setStep((n) => n + 1), STEP_MS);
      return () => clearTimeout(t);
    }
    const t = setTimeout(() => {
      if (home) {
        // Type and size both read, confidently: straight to the rooms.
        // Anything less: the property step, with what was read filled in.
        const base = emptyDraft().property;
        resetDraft({
          property: {
            ...base,
            ...(home.type ? { type: home.type } : {}),
            ...(home.size ? { size: home.size } : {}),
            ...(home.read.pickup_floor != null ? { pickup_floor: home.read.pickup_floor } : {}),
            ...(home.read.pickup_has_lift != null ? { pickup_has_lift: home.read.pickup_has_lift } : {}),
          },
          origin: { line1: parsed.from, city: '' },
          destination: { line1: parsed.to, city: '' },
          read: { sentence: prompt, chips: readChips(home.read, home.size), confident: home.confident },
          intake: routed?.intake ? { ...routed.intake, extracted: { ...routed.intake.extracted, property: home.read } } : undefined,
        });
        navigation.replace(home.confident && home.type && home.size ? 'MoveHomeRooms' : 'MoveHomeSet');
      } else {
        navigation.replace('MovePlan', { parsed, intake: routed?.intake, intent: routed?.intent });
      }
    }, LAND_MS);
    return () => clearTimeout(t);
  }, [step, steps.length, navigation, parsed, routed, home, prompt]);

  return (
    <View style={[s.root, { paddingTop: insets.top + 16, paddingBottom: insets.bottom + 40 }]}>
      <Ambient />
      <View style={s.working}>
        <View style={s.spinner} />
        <Text style={s.workingText}>Working</Text>
      </View>
      <View style={s.list}>
        {steps.map((st, i) => (
          <StepRow key={st.title} title={st.title} meta={st.meta} done={i < step} active={i === step} visible={i <= step} />
        ))}
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  root:        { flex: 1, backgroundColor: M.ground, paddingHorizontal: 20 },
  working:     { flexDirection: 'row', alignItems: 'center', gap: 10 },
  spinner:     { width: 16, height: 16, borderRadius: 8, borderWidth: 1.5, borderColor: 'rgba(0,229,255,0.3)', borderTopColor: M.accent },
  workingText: { fontSize: 11, letterSpacing: 2.2, textTransform: 'uppercase', color: M.label },
  list:        { flex: 1, justifyContent: 'center' },
  row:         { flexDirection: 'row', gap: 15, paddingBottom: 22 },
  mark:        { width: 20, height: 20, marginTop: 2, borderRadius: 10, borderWidth: 1, borderColor: M.hairlineStrong, alignItems: 'center', justifyContent: 'center' },
  markActive:  { borderColor: M.accent },
  markDone:    { backgroundColor: M.accent, borderColor: M.accent },
  title:       { fontFamily: HEADING, fontWeight: '700', fontSize: 19, lineHeight: 23, color: M.ink },
  meta:        { fontSize: 12, lineHeight: 18, color: M.faint, marginTop: 3 },
});
