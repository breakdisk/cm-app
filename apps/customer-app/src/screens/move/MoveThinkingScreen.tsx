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
import { classifyPrompt, route as routePrompt, type Routed } from '../../services/api/classify';
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
  useEffect(() => {
    let live = true;
    classifyPrompt(prompt, hasActiveJob).then((ai) => {
      if (live) setRouted(routePrompt(ai, regex, classifyIntent(prompt, hasActiveJob), mode));
    });
    return () => { live = false; };
  }, [prompt, hasActiveJob, regex, mode]);

  // A question about the running job: the answer is in Support, not a plan.
  useEffect(() => {
    if (routed?.intent === 'support') navigation.replace('Support', { initialMessage: prompt });
  }, [routed, navigation, prompt]);

  const parsed = routed?.parsed ?? regex;

  const steps = useMemo(() => [
    { title: 'Parsed your request', meta: describeParse(parsed) },
    {
      title: 'Matched item profiles',
      meta: parsed.items.map((i) => (i.qty > 1 ? `${i.qty} ${i.name}` : i.name)).join(', ') || 'Add the items on the next screen',
    },
    { title: 'Sized the vehicle', meta: 'From the weight and size of the load' },
    { title: 'Priced it off the rate card', meta: 'Base + per km + per kg, on billable weight' },
    { title: 'Ready for your check', meta: 'Nothing is booked until you lock it in' },
  ], [parsed]);

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
    const t = setTimeout(
      () => navigation.replace('MovePlan', { parsed, intake: routed?.intake, intent: routed?.intent }),
      LAND_MS,
    );
    return () => clearTimeout(t);
  }, [step, steps.length, navigation, parsed, routed]);

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
