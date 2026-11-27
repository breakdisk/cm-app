/**
 * Whole-home move, A5 — the survey and the move day. The survey must land at
 * least two days before the move so the inventory can be corrected before
 * dispatch; moving the survey later re-resolves the move to the first legal
 * day rather than trusting the earlier pick. The server holds the same rule.
 *
 * Windows fill as verified teams are booked: a full one is greyed out, and
 * when every window shown is full the next open one is named. The lead is
 * named once one takes the job, and runs both the survey and the move.
 *
 * A window whose teams are nearly all booked is in high demand: it shows its
 * multiplier and the total with it, and the server holds the booking to what
 * was shown. A fully booked date can be queued for on the priority waitlist;
 * a window held off it arrives here as `waitlist`, chosen and counting down.
 */
import React, { useEffect, useState } from 'react';
import { ActivityIndicator, Alert, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { useSelector } from 'react-redux';
import type { RootState } from '../../../store';
import {
  bookHome, crewLine, firstOpenSurvey, fullDates, getSlots, holdLeft, joinWaitlist, moveOk, slotDay, slotHours, surgedTotal,
  surgeFromError, surgeLabel, validMovePick, NO_SURGE_BPS, type HomeQuote, type HomeSlots,
} from '../../../services/api/homeMove';
import { formatMoney } from '../format';
import { M } from '../theme';
import { Ambient, Field, Label, Panel, PrimaryButton, TopBar } from '../ui';
import { resetDraft, useHomeDraft } from './homeDraft';
import { apiMessage, h, makeKey } from './homeUi';

export function MoveHomeScheduleScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const d = useHomeDraft();
  const quote: HomeQuote = route.params?.quote;
  const waitlist: { id: string; move_at: string; hold_expires_at: string } | undefined = route.params?.waitlist;
  const auth = useSelector((s: RootState) => s.auth);
  const [joining, setJoining] = useState(false);
  const [, tick] = useState(0);
  // A held window counts down.
  useEffect(() => {
    if (!waitlist) return;
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [waitlist]);
  const [slots, setSlots] = useState<HomeSlots | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [surveyPick, setSurveyPick] = useState(0);
  const [movePick, setMovePick] = useState(0);
  const [phone, setPhone] = useState(auth.phone ?? '');
  const [booking, setBooking] = useState(false);
  // One key per booking attempt on this screen, so a retry is not a second move.
  const [key] = useState(makeKey);

  const loadSlots = React.useCallback(() => {
    getSlots({ large_estate: quote?.large_estate, international: quote?.international, waitlist_id: waitlist?.id })
      .then((s) => {
        setSlots(s);
        setSurveyPick(firstOpenSurvey(s));
        // The held window is the move day.
        if (waitlist) {
          const i = s.move.findIndex((m) => m.starts_at === waitlist.move_at);
          if (i >= 0) setMovePick(i);
        }
      })
      .catch((e) => setError(apiMessage(e)));
  }, [quote?.large_estate, quote?.international, waitlist]);
  useEffect(() => { loadSlots(); }, [loadSlots]);

  const required = quote?.survey_required ?? false;
  const move = slots ? validMovePick(slots, surveyPick, movePick, required) : 0;
  const survey = slots && required ? slots.survey[surveyPick] ?? null : null;
  const moveSlot = slots?.move[move];
  const surge = moveSlot?.surge_bps ?? NO_SURGE_BPS;
  const total = quote ? surgedTotal(quote, surge) : 0;
  const held = waitlist ? holdLeft(waitlist.hold_expires_at) : null;
  const queueable = slots ? fullDates(slots) : [];

  async function queueFor(date: string) {
    if (!quote) return;
    setJoining(true);
    try {
      await joinWaitlist(quote.quote_token, date);
      Alert.alert(
        "You're on the waitlist",
        `We'll push you the moment a team opens on ${slotDay(`${date}T12:00:00Z`, 0)}. You'll have 5 minutes to book it.`,
        [{ text: 'OK', onPress: () => navigation.popToTop() }],
      );
    } catch (e) {
      const msg = apiMessage(e);
      Alert.alert(/OPEN_WINDOW/.test(msg) ? 'A window just opened' : "Couldn't join", /OPEN_WINDOW/.test(msg) ? 'That date has an open window now — pick it above.' : msg);
      if (/OPEN_WINDOW/.test(msg)) loadSlots();
    } finally {
      setJoining(false);
    }
  }

  async function book() {
    if (!slots || !quote) return;
    setBooking(true);
    try {
      const res = await bookHome({
        quote_token: quote.quote_token,
        survey_at: survey?.starts_at ?? null,
        move_at: slots.move[move].starts_at,
        accepted_surge_bps: surge,
        waitlist_id: waitlist?.id,
        contact_name: auth.name ?? undefined,
        contact_phone: phone.trim() || undefined,
        intake: d.intake,
        idempotency_key: key,
      });
      const awb = res.shipment.awb ?? res.shipment.tracking_number ?? '';
      resetDraft();
      if (res.checkout_url) {
        navigation.navigate('PaymentWebView', { checkoutUrl: res.checkout_url, shipmentId: res.shipment.id });
      } else {
        navigation.replace('MoveHomeBooked', {
          awb, id: res.shipment.id, surveyAt: res.home.survey_at ?? null, moveAt: res.home.move_at,
          offset: slots.utc_offset_minutes,
        });
      }
    } catch (e) {
      const msg = apiMessage(e);
      if (/SLOT_FULL/.test(msg)) {
        Alert.alert('That window just filled', 'Every verified moving team is booked for it now. Pick another — the next open one is highlighted.');
        loadSlots();
      } else if (surgeFromError(msg) != null) {
        Alert.alert('Demand went up', `That window is now ${surgeLabel(surgeFromError(msg)) ?? 'in high demand'}. Here is the new price — book again if it suits you.`);
        loadSlots();
      } else if (/HOLD_LAPSED/.test(msg)) {
        Alert.alert('The hold ended', 'That window went to the next in line. Pick an open one, or rejoin the waitlist.');
        navigation.setParams({ waitlist: undefined });
        loadSlots();
      } else if (/expired|price the move again/i.test(msg)) {
        Alert.alert('The quote expired', 'Prices hold for 30 minutes. Here it is again.', [{ text: 'OK', onPress: () => navigation.goBack() }]);
      } else {
        Alert.alert("Couldn't book", msg);
      }
    } finally {
      setBooking(false);
    }
  }

  return (
    <View style={h.root}>
      <Ambient />
      <TopBar label="Dates" onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={[h.content, { paddingBottom: 24 }]} keyboardShouldPersistTaps="handled">
        {error && <Panel tone="amber"><Text style={h.body}>{error}</Text></Panel>}
        {!slots && !error && <ActivityIndicator color={M.accent} style={{ marginTop: 32 }} />}

        {slots && (
          <>
            {waitlist && (
              <Panel tone={held ? 'accent' : 'amber'}>
                <Text style={[h.note, { letterSpacing: 1.4 }]}>{held ? `HELD FOR YOU · ${held}` : 'THE HOLD HAS ENDED'}</Text>
                <Text style={[h.body, { marginTop: 4 }]}>
                  {held
                    ? `A team opened up on ${slotDay(waitlist.move_at, slots.utc_offset_minutes)}. Book before the hold ends and it's yours.`
                    : 'The window has gone to the next in line.'}
                </Text>
              </Panel>
            )}
            <Panel>
              <Text style={[h.note, { letterSpacing: 1.4 }]}>YOUR CREW</Text>
              <Text style={[h.rowLabel, { marginTop: 4 }]}>{crewLine(quote.trucks, quote.crew_total ?? quote.trucks + quote.helpers)}</Text>
              <Text style={[h.body, { marginTop: 6 }]}>
                {required
                  ? 'Your crew lead is named before the survey. The same lead surveys the home and runs the move, so you meet the same person twice.'
                  : 'Your crew is named before the move day.'}
              </Text>
            </Panel>

            {required && (
              <>
                <Label>Survey</Label>
                <View style={s.slots}>
                  {slots.survey.slice(0, 12).map((sl, i) => (
                    <SlotPill key={sl.starts_at} on={i === surveyPick} disabled={sl.open === false} top={slotDay(sl.starts_at, slots.utc_offset_minutes)} bottom={sl.open === false ? 'Fully booked' : slotHours(sl, slots.utc_offset_minutes)} onPress={() => { setSurveyPick(i); setMovePick(validMovePick(slots, i, movePick, required)); }} />
                  ))}
                </View>
              </>
            )}

            <Label>Move day</Label>
            <View style={s.slots}>
              {slots.move.slice(0, 16).map((sl, i) => {
                const ok = moveOk(sl, survey, required, slots);
                return (
                  <SlotPill
                    key={sl.starts_at}
                    on={i === move}
                    disabled={!ok || (!!waitlist && !!held && sl.starts_at !== waitlist.move_at)}
                    top={slotDay(sl.starts_at, slots.utc_offset_minutes)}
                    bottom={sl.open === false ? 'Fully booked' : surgeLabel(sl.surge_bps) ?? `Crew arrives ${slotHours(sl, slots.utc_offset_minutes).split(' – ')[0]}`}
                    hot={!!surgeLabel(sl.surge_bps) && sl.open !== false}
                    onPress={() => setMovePick(i)}
                  />
                );
              })}
            </View>
            {slots.move.slice(0, 16).every((m) => m.open === false) ? (
              <Panel tone="amber">
                <Text style={h.body}>
                  All our verified moving teams are fully booked for these dates.
                  {slots.next_open_move ? ` The next available slot is ${slotDay(slots.next_open_move, slots.utc_offset_minutes)}.` : ' Check back soon — teams open new days every week.'}
                </Text>
              </Panel>
            ) : (
              <Text style={h.note}>
                {required ? 'Days before the survey plus two, and fully booked windows, are greyed out.' : 'Fully booked windows are greyed out.'}
              </Text>
            )}

            {queueable.length > 0 && !waitlist && (
              <>
                <Label>Priority waitlist</Label>
                <Panel>
                  <Text style={h.body}>Want a fully booked day? Queue for it — first come, first served. When a team opens we push you, and it's held for you for 5 minutes.</Text>
                  <View style={[s.slots, { marginTop: 10 }]}>
                    {queueable.slice(0, 8).map((day) => (
                      <SlotPill key={day} on={false} disabled={joining} top={slotDay(`${day}T12:00:00Z`, 0)} bottom="Join the waitlist" onPress={() => queueFor(day)} />
                    ))}
                  </View>
                </Panel>
              </>
            )}

            <Label>Contact</Label>
            <Panel>
              <Field label="Phone for the crew" value={phone} onChangeText={setPhone} keyboardType="phone-pad" placeholder="+63 917 555 0123" />
            </Panel>

            <Panel tone="accent">
              <View style={h.row}>
                <Text style={h.rowLabel}>Total</Text>
                <Text style={[h.rowLabel, { fontWeight: '700' }]}>{formatMoney(total, quote.currency, { whole: true })}</Text>
              </View>
              {surge > NO_SURGE_BPS && (
                <Text style={[h.note, { marginTop: 6, color: M.amberText }]}>
                  {surgeLabel(surge)}: {formatMoney(total - quote.total_cents, quote.currency, { whole: true })} added for this window. It all goes to your crew.
                </Text>
              )}
              <Text style={[h.note, { marginTop: 6 }]}>Paid now, online. Cancelling follows the policy shown before you pay.</Text>
            </Panel>
          </>
        )}
      </ScrollView>
      <View style={[h.footer, { paddingBottom: insets.bottom + 12 }]}>
        <PrimaryButton label="BOOK THE MOVE" loading={booking} disabled={!slots || phone.trim().length < 7 || (!!waitlist && !held && slots.move[move]?.starts_at === waitlist.move_at)} onPress={book} />
      </View>
    </View>
  );
}

function SlotPill({ top, bottom, on, disabled, hot, onPress }: { top: string; bottom: string; on: boolean; disabled?: boolean; hot?: boolean; onPress: () => void }) {
  return (
    <Pressable
      onPress={onPress}
      disabled={disabled}
      accessibilityRole="radio"
      accessibilityState={{ selected: on, disabled: !!disabled }}
      style={({ pressed }) => [s.slot, on && s.slotOn, disabled && { opacity: 0.35 }, pressed && { opacity: 0.75 }]}
    >
      <Text style={[s.slotTop, on && { color: M.accent }]}>{top}</Text>
      <Text style={[s.slotBottom, hot && { color: M.amberText }]}>{bottom}</Text>
    </Pressable>
  );
}

const s = StyleSheet.create({
  slots:      { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  slot:       { width: '48%', flexGrow: 1, minHeight: 56, padding: 10, borderRadius: 14, borderWidth: 1, borderColor: M.hairlineStrong },
  slotOn:     { borderColor: M.accentBorder, backgroundColor: M.accentTintStrong },
  slotTop:    { fontSize: 13, fontWeight: '600', color: M.ink },
  slotBottom: { fontSize: 11, color: M.faint, marginTop: 3 },
});
