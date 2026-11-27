/**
 * Move app — Tracking. Where is my stuff, and who has it.
 *
 * Refreshes every 5 s while on screen (the handoff: at 30 s a moving vehicle
 * reads as broken) and stops at a terminal status. The PIN is repeated here so
 * the customer never has to remember it. The tracking response carries no
 * heading, so the driver marker does not rotate.
 */
import React, { useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Alert, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useIsFocused } from '@react-navigation/native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useSelector } from 'react-redux';
import { Ionicons } from '@expo/vector-icons';
import type { RootState } from '../../store';
import { trackingApi } from '../../services/api/tracking';
import { callMessage, startCall, unreadCount } from '../../services/api/chat';
import { LiveDriverMap } from '../../components/LiveDriverMap';
import { DeliveryPinCard } from '../../components/DeliveryPinCard';
import { isTerminalStatus, LIVE_TRACKING_POLL_MS, mapPublicTracking, type TrackingResult } from '../tracking/mapTracking';
import { ACTIVE_STATUSES, STATUS_WORDS } from './MoveHomeScreen';
import { initials } from './format';
import { getAddendum, getHomeMove, teamLine } from '../../services/api/homeMove';
import { HEADING, M } from './theme';
import { Ambient, GhostButton, Label, Panel, TopBar } from './ui';

const LEGS = ['pending', 'confirmed', 'picked_up', 'in_transit', 'out_for_delivery', 'delivered'];
const CELLS = 18;
const CANCELLABLE = ['pending', 'confirmed'];

function when(iso: string): string {
  const d = new Date(iso);
  return Number.isNaN(d.getTime())
    ? ''
    : d.toLocaleString(undefined, { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' });
}

export function MoveTrackScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const isFocused = useIsFocused();
  const awb: string | undefined = route.params?.awb;
  const shipments = useSelector((s: RootState) => s.shipments.list);
  const authPhone = useSelector((s: RootState) => s.auth.phone);
  const booked = awb ? shipments.find((s) => s.awb === awb) : undefined;
  const id: string | undefined = route.params?.id ?? booked?.id;

  const [result, setResult] = useState<TrackingResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [showMap, setShowMap] = useState(false);
  const [unread, setUnread] = useState(0);
  const [calling, setCalling] = useState(false);

  const load = useCallback(async () => {
    if (!awb) return;
    try {
      const res = await trackingApi.getByTrackingNumber(awb);
      setResult(mapPublicTracking(res.data, awb));
      setError(null);
    } catch (err: any) {
      setError(err?.status === 404 ? 'Live tracking starts once your move is in the system.' : err?.message ?? "Couldn't load tracking.");
    }
  }, [awb]);

  useEffect(() => { void load(); }, [load]);

  const status = result?.status ?? booked?.status;
  useEffect(() => {
    if (!isFocused || !awb || (status && isTerminalStatus(status))) return;
    let busy = false;
    const timer = setInterval(async () => {
      if (busy) return;
      busy = true;
      try { await load(); } finally { busy = false; }
    }, LIVE_TRACKING_POLL_MS);
    return () => clearInterval(timer);
  }, [isFocused, awb, status, load]);

  // The badge on the chat button, on the same rhythm as tracking. A failure
  // here is silent: an unread count is not worth an error on this screen.
  useEffect(() => {
    if (!isFocused || !id || (status && isTerminalStatus(status))) return;
    let cancelled = false;
    const tick = () => {
      unreadCount(id).then((n) => { if (!cancelled) setUnread(n); }).catch(() => {});
    };
    tick();
    const timer = setInterval(tick, LIVE_TRACKING_POLL_MS);
    return () => { cancelled = true; clearInterval(timer); };
  }, [isFocused, id, status]);

  // A masked call: the platform rings this phone, then the driver. The
  // driver's number is never sent to this app, so there is nothing to dial.
  async function callDriver() {
    if (!id || calling) return;
    setCalling(true);
    try {
      const attempt = await startCall(id);
      Alert.alert(attempt.bridged ? 'Connecting you' : 'Call not available', callMessage(attempt));
    } catch (err: any) {
      Alert.alert("Couldn't call", err?.message ?? 'Try a message instead.');
    } finally {
      setCalling(false);
    }
  }

  if (!awb) {
    const moves = shipments.filter((s) => ACTIVE_STATUSES.includes(s.status));
    return (
      <View style={s.root}>
        <Ambient />
        <TopBar label="Your moves" onBack={() => navigation.goBack()} />
        <ScrollView contentContainerStyle={{ paddingHorizontal: 20, paddingBottom: insets.bottom + 30, gap: 10 }}>
          {moves.length === 0 ? (
            <Text style={s.note}>Nothing on the way. Book a move from home.</Text>
          ) : moves.map((m) => (
            <Pressable
              key={m.awb}
              onPress={() => navigation.setParams({ awb: m.awb, id: m.id })}
              accessibilityRole="button"
              style={({ pressed }) => [s.moveRow, pressed && { transform: [{ scale: 0.98 }] }]}
            >
              <View style={{ flex: 1 }}>
                <Text style={s.moveRef}>{m.awb}</Text>
                <Text style={s.moveTitle} numberOfLines={1}>{m.description || m.destination}</Text>
              </View>
              <Text style={s.moveStatus}>{STATUS_WORDS[m.status] ?? m.status}</Text>
            </Pressable>
          ))}
        </ScrollView>
      </View>
    );
  }

  const legIndex = Math.max(0, LEGS.indexOf(status ?? 'pending'));
  const filled = status === 'delivered' ? CELLS : Math.max(1, Math.round(((legIndex + 1) / LEGS.length) * CELLS));
  const headline = status ? STATUS_WORDS[status] ?? status : 'Loading…';
  const note = result?.eta
    ? `Arrives ${result.eta}`
    : status && CANCELLABLE.includes(status)
      ? 'Waiting for a driver to be assigned'
      : result?.destination_city
        ? `To ${result.destination_city}`
        : '';
  const feed = [...(result?.timeline ?? [])].reverse();

  return (
    <View style={s.root}>
      <Ambient />
      <TopBar label={awb} onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={{ paddingHorizontal: 20, paddingTop: 10, paddingBottom: insets.bottom + 40 }}>
        <Text style={s.kicker}>Status</Text>
        <Text style={s.headline}>{headline}</Text>
        {!!note && <Text style={s.note}>{note}</Text>}
        {!!error && <Text style={[s.note, { color: M.amberText }]}>{error}</Text>}

        {!!id && !!status && !isTerminalStatus(status) && (
          <View style={{ marginTop: 22 }}>
            <DeliveryPinCard shipmentId={id} recipientPhone={booked?.recipientPhone ?? authPhone ?? ''} compact />
          </View>
        )}

        <View style={s.legs} accessibilityLabel={`Progress: ${headline}`}>
          {Array.from({ length: CELLS }, (_, i) => (
            <View key={i} style={[s.leg, i < filled && s.legOn]} />
          ))}
        </View>

        {feed.length > 0 && (
          <View style={{ marginTop: 24, gap: 14 }}>
            {feed.map((e, i) => (
              <View key={`${e.occurred_at}-${i}`} style={[s.feedRow, { opacity: i === 0 ? 1 : 0.7 }]}>
                <View style={[s.feedDot, i === 0 && s.feedDotOn]} />
                <View style={{ flex: 1 }}>
                  <Text style={s.feedTitle}>{e.description || STATUS_WORDS[e.status] || e.status}</Text>
                  <Text style={s.feedTime}>{[when(e.occurred_at), e.location].filter(Boolean).join(' · ')}</Text>
                </View>
              </View>
            ))}
          </View>
        )}

        {!!result?.driver_location && (
          <>
            <Pressable
              onPress={() => setShowMap((v) => !v)}
              accessibilityRole="button"
              style={({ pressed }) => [s.mapRow, pressed && { transform: [{ scale: 0.97 }] }]}
            >
              <Ionicons name="location-outline" size={18} color={M.accent} />
              <Text style={s.mapText}>{showMap ? 'Hide the map' : `See ${result.driver_name ?? 'the driver'}'s location`}</Text>
              <Ionicons name={showMap ? 'chevron-up' : 'chevron-forward'} size={16} color="rgba(214,251,255,0.6)" />
            </Pressable>
            {showMap && (
              <View style={{ marginTop: 12, borderRadius: 18, overflow: 'hidden' }}>
                <LiveDriverMap driverLocation={result.driver_location} driverName={result.driver_name} />
              </View>
            )}
          </>
        )}

        {!!result?.driver_name && (
          <Panel style={{ marginTop: 16 }}>
            <View style={s.driverTop}>
              <View style={s.avatar}><Text style={s.avatarText}>{initials(result.driver_name)}</Text></View>
              <View style={{ flex: 1 }}>
                <Text style={s.driverName}>{result.driver_name}</Text>
                <Text style={s.feedTime}>Your driver</Text>
              </View>
            </View>
            <View style={s.contactRow}>
              <Pressable
                onPress={callDriver}
                disabled={!id || calling}
                accessibilityRole="button"
                accessibilityLabel="Call your driver on a masked line"
                style={({ pressed }) => [s.contactBtn, pressed && { transform: [{ scale: 0.96 }] }]}
              >
                {calling
                  ? <ActivityIndicator color={M.accent} size="small" />
                  : <Ionicons name="call-outline" size={17} color={M.accent} />}
                <Text style={s.contactText}>CALL</Text>
              </Pressable>
              <Pressable
                onPress={() => id && navigation.navigate('MoveChat', { id, driverName: result.driver_name, driverPhone: result.driver_phone })}
                disabled={!id}
                accessibilityRole="button"
                accessibilityLabel={unread > 0 ? `Chat with your driver, ${unread} unread` : 'Chat with your driver'}
                style={({ pressed }) => [s.contactBtn, s.contactBtnPrimary, pressed && { transform: [{ scale: 0.96 }] }]}
              >
                <Ionicons name="chatbubble-outline" size={17} color={M.accentInk} />
                <Text style={[s.contactText, { color: M.accentInk }]}>CHAT</Text>
                {unread > 0 && (
                  <View style={s.badge}>
                    <Text style={s.badgeText}>{unread > 9 ? '9+' : unread}</Text>
                  </View>
                )}
              </Pressable>
            </View>
            <Text style={[s.feedTime, { marginTop: 12 }]}>
              Messages and calls stay on this move. Neither of you sees the other’s number.
            </Text>
          </Panel>
        )}

        {!!id && <HomeMoveLink id={id} onOpen={() => navigation.navigate('MoveHomeJob', { id })} />}

        <Pressable
          onPress={() => navigation.navigate('MoveRules')}
          accessibilityRole="button"
          style={({ pressed }) => [s.coverRow, pressed && { transform: [{ scale: 0.97 }] }]}
        >
          <Text style={s.coverText}>Coverage and rules for this move</Text>
          <Ionicons name="chevron-forward" size={16} color={M.faint} />
        </Pressable>

        {!!id && !!status && CANCELLABLE.includes(status) && (
          <>
            <Label>Change of plan</Label>
            <GhostButton label="Cancel this move" color={M.penalty} onPress={() => navigation.navigate('MoveCancel', { id, awb })} />
          </>
        )}
      </ScrollView>
    </View>
  );
}

const s = StyleSheet.create({
  root:             { flex: 1, backgroundColor: M.ground },
  kicker:           { fontSize: 13, letterSpacing: 2.6, textTransform: 'uppercase', color: M.label },
  headline:         { fontFamily: HEADING, fontWeight: '700', fontSize: 52, lineHeight: 56, letterSpacing: -1, color: M.ink, marginTop: 4 },
  note:             { marginTop: 10, fontSize: 13, lineHeight: 19, color: M.muted },
  legs:             { flexDirection: 'row', gap: 3, marginTop: 26 },
  leg:              { flex: 1, height: 4, borderRadius: 2, backgroundColor: 'rgba(255,255,255,0.1)' },
  legOn:            { backgroundColor: M.accent },
  feedRow:          { flexDirection: 'row', gap: 14, alignItems: 'flex-start' },
  feedDot:          { width: 8, height: 8, borderRadius: 4, marginTop: 6, backgroundColor: 'rgba(242,246,250,0.3)' },
  feedDotOn:        { backgroundColor: M.accent },
  feedTitle:        { fontSize: 14, color: M.ink },
  feedTime:         { fontSize: 11, color: M.faint, marginTop: 2 },
  mapRow:           { flexDirection: 'row', alignItems: 'center', gap: 12, height: 56, marginTop: 22, paddingHorizontal: 16, borderRadius: 16, backgroundColor: 'rgba(0,229,255,0.1)', borderWidth: 1, borderColor: M.accentBorder },
  mapText:          { flex: 1, fontSize: 14, color: M.accentText },
  driverTop:        { flexDirection: 'row', alignItems: 'center', gap: 14 },
  avatar:           { width: 48, height: 48, borderRadius: 24, alignItems: 'center', justifyContent: 'center', backgroundColor: M.accentTint, borderWidth: 1, borderColor: M.accentBorder },
  avatarText:       { fontFamily: HEADING, fontWeight: '700', fontSize: 18, color: M.accent },
  driverName:       { fontSize: 15, color: M.ink },
  contactRow:       { flexDirection: 'row', gap: 10, marginTop: 14 },
  contactBtn:       { flex: 1, height: 52, borderRadius: 15, flexDirection: 'row', alignItems: 'center', justifyContent: 'center', gap: 9, backgroundColor: 'rgba(255,255,255,0.05)', borderWidth: 1, borderColor: M.hairlineStrong },
  contactBtnPrimary:{ backgroundColor: M.accent, borderColor: M.accent },
  contactText:      { fontSize: 13, fontWeight: '700', letterSpacing: 0.8, color: M.ink },
  badge:            { minWidth: 20, height: 20, borderRadius: 10, paddingHorizontal: 6, alignItems: 'center', justifyContent: 'center', backgroundColor: M.accentInk },
  badgeText:        { fontSize: 11, fontWeight: '700', color: M.accent },
  coverRow:         { flexDirection: 'row', alignItems: 'center', justifyContent: 'space-between', height: 56, marginTop: 16, paddingHorizontal: 18, borderRadius: 16, backgroundColor: 'rgba(255,255,255,0.04)', borderWidth: 1, borderColor: M.hairline },
  coverText:        { fontSize: 14, color: M.ink },
  moveRow:          { flexDirection: 'row', alignItems: 'center', gap: 12, padding: 16, borderRadius: 18, backgroundColor: M.panel, borderWidth: 1, borderColor: M.hairline },
  moveRef:          { fontSize: 11, letterSpacing: 2, color: M.label },
  moveTitle:        { fontFamily: HEADING, fontWeight: '700', fontSize: 18, color: M.ink, marginTop: 4 },
  moveStatus:       { fontSize: 11, letterSpacing: 1.4, textTransform: 'uppercase', color: M.accent },
});

/** On a whole-home move: the team, and a way to the survey and its addendum.
 *  Renders nothing for any other shipment. */
function HomeMoveLink({ id, onOpen }: { id: string; onOpen: () => void }) {
  const [line, setLine] = useState<string | null>(null);
  const [pending, setPending] = useState(false);
  useEffect(() => {
    let live = true;
    getHomeMove(id)
      .then(async (res) => {
        if (!live || !res) return;
        setLine(teamLine(res.lead, res.move.trucks, res.move.crew_total));
        const a = await getAddendum(id).catch(() => null);
        if (live) setPending(!!a && (a.status === 'pending' || a.status === 'approved'));
      })
      .catch(() => {});
    return () => { live = false; };
  }, [id]);
  if (!line) return null;
  return (
    <Pressable onPress={onOpen} accessibilityRole="button" style={({ pressed }) => [pressed && { opacity: 0.8 }]}>
      <Panel tone={pending ? 'amber' : 'accent'}>
        <Text style={{ fontSize: 11, letterSpacing: 1.4, color: pending ? M.amber : M.label }}>
          {pending ? 'THE SURVEY FOUND MORE — YOUR ANSWER NEEDED' : 'WHOLE-HOME MOVE'}
        </Text>
        <Text style={{ fontSize: 14, color: M.ink, marginTop: 4 }}>{line}</Text>
      </Panel>
    </Pressable>
  );
}

