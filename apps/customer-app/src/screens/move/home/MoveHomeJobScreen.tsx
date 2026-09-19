/**
 * Whole-home move — the booked job. The team ("Team {lead} · 2 trucks &
 * 7-person crew" once a lead takes it), the survey and the move day, and the
 * survey's addendum when the lead found more than was booked: the customer
 * approves it (and pays the difference) or declines it (and the move stands
 * as booked). The price never goes down; the addendum only adds.
 */
import React, { useCallback, useState } from 'react';
import { ActivityIndicator, Alert, RefreshControl, ScrollView, Text, View } from 'react-native';
import { useFocusEffect } from '@react-navigation/native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import {
  approveAddendum, declineAddendum, getAddendum, getHomeMove, m3, slotDay, slotHours, teamLine,
  type Addendum, type BookedHomeMove,
} from '../../../services/api/homeMove';
import { formatMoney } from '../format';
import { M } from '../theme';
import { Ambient, GhostButton, Label, Panel, PrimaryButton, TopBar } from '../ui';
import { apiMessage, h } from './homeUi';

const OFFSET = 480;

export function MoveHomeJobScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const id: string = route.params?.id;
  const [move, setMove] = useState<BookedHomeMove | null | undefined>(undefined);
  const [lead, setLead] = useState<string | null>(null);
  const [addendum, setAddendum] = useState<Addendum | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const res = await getHomeMove(id);
      setMove(res?.move ?? null);
      setLead(res?.lead ?? null);
      setAddendum(res ? await getAddendum(id) : null);
      setError(null);
    } catch (e) {
      setError(apiMessage(e));
    }
  }, [id]);

  useFocusEffect(useCallback(() => { void load(); }, [load]));

  async function approve(a: Addendum) {
    setBusy(true);
    try {
      const checkoutUrl = a.status === 'approved' && a.checkout_url ? a.checkout_url : await approveAddendum(id, a.id);
      navigation.navigate('PaymentWebView', { checkoutUrl, shipmentId: id });
    } catch (e) {
      Alert.alert("Couldn't approve", apiMessage(e));
      void load();
    } finally {
      setBusy(false);
    }
  }

  function decline(a: Addendum) {
    Alert.alert(
      'Decline the additions?',
      'The move goes ahead as booked, with only the inventory you declared. Items the survey found beyond it may be left behind.',
      [
        { text: 'Keep it open', style: 'cancel' },
        {
          text: 'Decline',
          style: 'destructive',
          onPress: async () => {
            try {
              await declineAddendum(id, a.id);
              await load();
            } catch (e) {
              Alert.alert("Couldn't decline", apiMessage(e));
            }
          },
        },
      ],
    );
  }

  const money = (c: number) => formatMoney(c, move?.currency ?? addendum?.currency);
  const waiting = addendum && (addendum.status === 'pending' || addendum.status === 'approved');

  return (
    <View style={h.root}>
      <Ambient />
      <TopBar label="Your move" onBack={() => navigation.goBack()} />
      <ScrollView
        contentContainerStyle={[h.content, { paddingBottom: insets.bottom + 24 }]}
        refreshControl={<RefreshControl refreshing={refreshing} tintColor={M.accent} onRefresh={async () => { setRefreshing(true); await load(); setRefreshing(false); }} />}
      >
        {move === undefined && !error && <ActivityIndicator color={M.accent} style={{ marginTop: 32 }} />}
        {error && <Panel tone="amber"><Text style={h.body}>{error}</Text></Panel>}
        {move === null && <Panel><Text style={h.body}>This isn't a whole-home move.</Text></Panel>}

        {move && (
          <>
            <Panel tone="accent">
              <Text style={[h.note, { letterSpacing: 1.4 }]}>{lead ? 'YOUR TEAM' : 'YOUR CREW'}</Text>
              <Text style={[h.rowLabel, { marginTop: 4, fontWeight: '700' }]}>{teamLine(lead, move.trucks, move.crew_total)}</Text>
              <Text style={[h.note, { marginTop: 6 }]}>
                {lead ? 'Your lead runs the survey and the move, and coordinates every truck.' : 'A verified lead takes the job and brings the crew. You’ll see their name here.'}
              </Text>
            </Panel>

            {!!move.survey_at && (
              <Panel>
                <Text style={[h.note, { letterSpacing: 1.4 }]}>SURVEY{move.survey_submitted_at ? ' · DONE' : ''}</Text>
                <Text style={[h.rowLabel, { marginTop: 4 }]}>{slotDay(move.survey_at, OFFSET)}</Text>
                <Text style={h.note}>{slotHours({ starts_at: move.survey_at, ends_at: new Date(new Date(move.survey_at).getTime() + 7_200_000).toISOString() }, OFFSET)}</Text>
              </Panel>
            )}
            <Panel>
              <Text style={[h.note, { letterSpacing: 1.4 }]}>MOVE DAY</Text>
              <Text style={[h.rowLabel, { marginTop: 4 }]}>{slotDay(move.move_at, OFFSET)}</Text>
              <Text style={h.note}>Crew arrives {slotHours({ starts_at: move.move_at, ends_at: move.move_at }, OFFSET).split(' – ')[0]} · {money(move.total_cents)} paid</Text>
            </Panel>

            {addendum && (
              <>
                <Label>What the survey found</Label>
                <Panel tone={waiting ? 'amber' : 'default'}>
                  <View style={{ gap: 8 }}>
                    {addendum.items.map((i, n) => (
                      <View key={`i${n}`} style={h.row}>
                        <Text style={[h.body, { flex: 1 }]}>{i.qty} × {i.name}</Text>
                        <Text style={h.note}>{m3(i.volume_l * i.qty)}</Text>
                      </View>
                    ))}
                    {addendum.extras.map((e, n) => (
                      <View key={`e${n}`} style={h.row}>
                        <Text style={[h.body, { flex: 1 }]}>{e.qty} × {e.name} <Text style={h.note}>({e.kind})</Text></Text>
                        <Text style={h.note}>{money(e.qty * e.unit_cents)}</Text>
                      </View>
                    ))}
                    {!!addendum.note && <Text style={[h.note, { marginTop: 4 }]}>“{addendum.note}”</Text>}
                  </View>
                  <View style={[h.row, { marginTop: 12, borderTopWidth: 1, borderTopColor: M.hairline, paddingTop: 10 }]}>
                    <Text style={h.rowLabel}>To add</Text>
                    <Text style={[h.rowLabel, { fontWeight: '700' }]}>{money(addendum.total_cents)}</Text>
                  </View>
                  <Text style={[h.note, { marginTop: 4 }]}>
                    The move becomes {addendum.trucks} truck{addendum.trucks === 1 ? '' : 's'} and a {addendum.crew_total}-person crew.
                  </Text>
                  {addendum.status === 'declined' && <Text style={[h.note, { marginTop: 8 }]}>You declined this. The move goes ahead as booked.</Text>}
                  {addendum.status === 'paid' && <Text style={[h.note, { marginTop: 8, color: M.accent }]}>Approved and paid — it's part of your move.</Text>}
                </Panel>
                {waiting && (
                  <View style={{ gap: 10 }}>
                    <PrimaryButton label={`APPROVE AND PAY ${money(addendum.total_cents)}`} loading={busy} onPress={() => approve(addendum)} />
                    {addendum.status === 'pending' && <GhostButton label="Decline" onPress={() => decline(addendum)} />}
                  </View>
                )}
              </>
            )}

            <Label>Booked inventory</Label>
            <Panel>
              <Text style={h.body}>
                {move.items.reduce((n, i) => n + i.qty, 0)} items · {m3(move.items.reduce((n, i) => n + i.qty * i.volume_l, 0))}
              </Text>
            </Panel>
          </>
        )}
      </ScrollView>
    </View>
  );
}
