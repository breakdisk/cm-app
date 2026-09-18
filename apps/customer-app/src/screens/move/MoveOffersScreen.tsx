/**
 * Move app — Offers. The redemption window as a month grid, a code field that
 * asks the server, and the offers this account can see.
 *
 * Nothing here decides anything: which offers show, which days are lit, and
 * why a code is refused all come from promotions. Opened from the plan screen,
 * an available offer can be put straight onto the quote; opened from home, the
 * codes are shown to use on the next move.
 */
import React, { useCallback, useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, RefreshControl, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import {
  getOffers, monthTitle, monthWeeks, offerHeadline, validateCode,
  type CodeCheck, type Offer, type OffersFeed,
} from '../../services/api/promotions';
import { getMyTenant } from '../../services/api/tenant';
import { formatMoney } from './format';
import { HEADING, M } from './theme';
import { Ambient, Field, GhostButton, Label, Panel, TopBar } from './ui';

const WEEKDAYS = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];

function ordinal(n: number): string {
  const tens = n % 100;
  const suffix = tens >= 11 && tens <= 13 ? 'th' : ({ 1: 'st', 2: 'nd', 3: 'rd' } as Record<number, string>)[n % 10] ?? 'th';
  return `${n}${suffix}`;
}

function apiMessage(err: any): string {
  return err?.data?.error?.message ?? err?.message ?? 'Something went wrong.';
}

export function MoveOffersScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  // Codes are in the move's own currency; the tenant bills in it.
  const [currency, setCurrency] = useState<string | undefined>(undefined);
  useEffect(() => {
    getMyTenant().then((t) => setCurrency(t.currency ?? undefined)).catch(() => {});
  }, []);
  const returnTo: string | undefined = route.params?.returnTo;

  const [feed, setFeed] = useState<OffersFeed | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const [draft, setDraft] = useState('');
  const [checking, setChecking] = useState(false);
  const [check, setCheck] = useState<CodeCheck | null>(null);

  const money = useCallback((cents: number) => formatMoney(cents, currency, { whole: true }), [currency]);

  const load = useCallback(async () => {
    try {
      setFeed(await getOffers());
      setError(null);
    } catch (err) {
      setError(apiMessage(err));
    }
  }, []);

  useEffect(() => { load(); }, [load]);

  async function runCheck() {
    const code = draft.trim();
    if (!code || checking) return;
    setChecking(true);
    try {
      setCheck(await validateCode(code));
    } catch (err) {
      setCheck({ ok: false, code, message: apiMessage(err) });
    } finally {
      setChecking(false);
    }
  }

  function use(code: string) {
    if (returnTo) navigation.navigate({ name: returnTo, params: { promoCode: code }, merge: true });
  }

  const window = feed?.window;

  return (
    <View style={s.root}>
      <Ambient />
      <TopBar label="Offers" onBack={() => navigation.goBack()} />
      <ScrollView
        contentContainerStyle={[s.content, { paddingBottom: insets.bottom + 32 }]}
        refreshControl={<RefreshControl refreshing={refreshing} tintColor={M.accent} onRefresh={async () => { setRefreshing(true); await load(); setRefreshing(false); }} />}
      >
        {feed === undefined && !error && <ActivityIndicator color={M.accent} style={{ marginTop: 48 }} />}
        {error && <Panel tone="amber"><Text style={s.body}>{error}</Text></Panel>}
        {feed === null && (
          <Panel><Text style={s.body}>Offers aren't running in your area yet.</Text></Panel>
        )}

        {window && (
          <>
            <Label>Promo code</Label>
            <Panel tone={window.open_today ? 'accent' : 'default'}>
              <View style={s.windowHead}>
                <View style={s.windowState}>
                  <View style={[s.dot, { backgroundColor: window.open_today ? M.accent : M.faint }]} />
                  <Text style={[s.kicker, { color: window.open_today ? M.accent : M.label }]}>
                    {window.open_today ? 'REDEMPTION WINDOW OPEN' : 'REDEMPTION WINDOW CLOSED'}
                  </Text>
                </View>
                <Text style={s.kickerMuted}>{monthTitle(window.month).toUpperCase()}</Text>
              </View>

              <View style={s.grid} accessibilityLabel={`Codes can be used on weekdays from the ${ordinal(window.from_day)} to the ${ordinal(window.to_day)}`}>
                <View style={s.week}>
                  {WEEKDAYS.map((d, i) => <Text key={i} style={s.weekday}>{d}</Text>)}
                </View>
                {monthWeeks(window.days).map((week, w) => (
                  <View key={w} style={s.week}>
                    {week.map((d, i) => (
                      <View
                        key={i}
                        style={[
                          s.cell,
                          d?.eligible && s.cellLit,
                          d?.day === window.today && s.cellToday,
                        ]}
                      >
                        {d && <Text style={[s.cellText, d.eligible && s.cellTextLit]}>{d.day}</Text>}
                      </View>
                    ))}
                  </View>
                ))}
              </View>

              <Text style={[s.body, { marginTop: 12 }]}>
                {window.message ?? `One code a month, on weekdays from the ${ordinal(window.from_day)} to the ${ordinal(window.to_day)}. Weekends are out — vans are scarcest then.`}
              </Text>

              <View style={s.codeRow}>
                <Field
                  label="Have a code?"
                  value={draft}
                  onChangeText={(t) => { setDraft(t.toUpperCase()); setCheck(null); }}
                  autoCapitalize="characters"
                  autoCorrect={false}
                  placeholder="MOVE20"
                  returnKeyType="done"
                  onSubmitEditing={runCheck}
                />
                <GhostButton label={checking ? 'Checking…' : 'Check'} onPress={runCheck} color={M.accent} />
              </View>
              {check && (
                <View style={{ marginTop: 10, gap: 8 }}>
                  <Text style={[s.body, { color: check.ok ? M.accent : M.amberText }]}>
                    {check.ok ? `${check.code} can be used today.` : check.message ?? "That code can't be used."}
                  </Text>
                  {check.ok && returnTo && <GhostButton label={`Put ${check.code} on my quote`} onPress={() => use(check.code)} color={M.accent} />}
                </View>
              )}
            </Panel>
          </>
        )}

        {feed && feed.offers.length > 0 && <Label>Offers</Label>}
        {feed?.offers.map((o) => <OfferCard key={o.code} offer={o} money={money} onUse={returnTo ? () => use(o.code) : undefined} />)}
        {feed && feed.offers.length === 0 && (
          <Panel><Text style={s.body}>No offers running right now.</Text></Panel>
        )}
      </ScrollView>
    </View>
  );
}

function OfferCard({ offer, money, onUse }: { offer: Offer; money: (c: number) => string; onUse?: () => void }) {
  const used = offer.state === 'used';
  const available = offer.state === 'available';
  return (
    <Panel tone={available ? 'accent' : 'default'} style={used ? { opacity: 0.6 } : undefined}>
      <View style={s.offerTop}>
        <View style={s.codeChip}><Text style={s.codeText}>{offer.code}</Text></View>
        <Text style={[s.kickerMuted, used && { color: M.faint }]}>
          {used ? 'USED' : available ? (offer.windowed ? 'WEEKDAY WINDOW' : 'ANY DAY') : 'NOT TODAY'}
        </Text>
      </View>
      <Text style={s.offerTitle}>{offer.title}</Text>
      <Text style={s.offerWhat}>{offerHeadline(offer, money)}</Text>
      {!!offer.body && <Text style={s.body}>{offer.body}</Text>}
      {!!offer.reason && !used && <Text style={[s.body, { color: M.amberText, marginTop: 6 }]}>{offer.reason}</Text>}
      {available && onUse && (
        <Pressable onPress={onUse} accessibilityRole="button" style={({ pressed }) => [s.useBtn, pressed && { opacity: 0.7 }]}>
          <Text style={s.useText}>USE ON THIS MOVE</Text>
        </Pressable>
      )}
    </Panel>
  );
}

const s = StyleSheet.create({
  root:        { flex: 1, backgroundColor: M.ground },
  content:     { paddingHorizontal: 16, gap: 12 },
  body:        { fontSize: 13, lineHeight: 19, color: M.muted },
  kicker:      { fontSize: 11, letterSpacing: 1.6, fontWeight: '700' },
  kickerMuted: { fontSize: 11, letterSpacing: 1.6, color: M.label },
  windowHead:  { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center', gap: 8, flexWrap: 'wrap' },
  windowState: { flexDirection: 'row', alignItems: 'center', gap: 6 },
  dot:         { width: 7, height: 7, borderRadius: 4 },
  grid:        { marginTop: 14, gap: 4 },
  week:        { flexDirection: 'row', gap: 4 },
  weekday:     { flex: 1, textAlign: 'center', fontSize: 10, color: M.faint },
  cell:        { flex: 1, aspectRatio: 1, maxHeight: 40, borderRadius: 8, alignItems: 'center', justifyContent: 'center', borderWidth: 1, borderColor: 'transparent' },
  cellLit:     { backgroundColor: M.accentTintStrong, borderColor: M.accentBorder },
  cellToday:   { borderColor: M.ink },
  cellText:    { fontSize: 12, color: M.faint, fontVariant: ['tabular-nums'] },
  cellTextLit: { color: M.accentText, fontWeight: '700' },
  codeRow:     { flexDirection: 'row', alignItems: 'flex-end', gap: 10, marginTop: 14 },
  offerTop:    { flexDirection: 'row', justifyContent: 'space-between', alignItems: 'center' },
  codeChip:    { borderWidth: 1, borderStyle: 'dashed', borderColor: M.accentBorder, borderRadius: 8, paddingHorizontal: 10, paddingVertical: 4 },
  codeText:    { color: M.accent, fontWeight: '700', letterSpacing: 1.2, fontVariant: ['tabular-nums'] },
  offerTitle:  { fontFamily: HEADING, fontWeight: '700', fontSize: 18, color: M.ink, marginTop: 10 },
  offerWhat:   { fontSize: 14, color: M.accentText, marginTop: 2, marginBottom: 4 },
  useBtn:      { marginTop: 12, minHeight: 44, borderRadius: 12, borderWidth: 1, borderColor: M.accentBorder, alignItems: 'center', justifyContent: 'center' },
  useText:     { color: M.accent, fontWeight: '700', letterSpacing: 1.2, fontSize: 13 },
});
