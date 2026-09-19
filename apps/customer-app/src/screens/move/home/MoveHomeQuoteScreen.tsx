/**
 * Whole-home move, A4 — the quote, line by line as the server priced it.
 * The plan (one truck per load, or fewer trucks doing two loads) re-prices;
 * nothing here adds anything up — the total is the server's, and it is
 * shown only when the lines sum to it.
 */
import React, { useEffect, useState } from 'react';
import { ActivityIndicator, ScrollView, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { COUNTRY_FOR_CURRENCY, distanceLine } from '../../../services/api/move';
import { crewLine, declared, quoteHome, type HomeQuote, type TruckPlan } from '../../../services/api/homeMove';
import { getMyTenant } from '../../../services/api/tenant';
import { formatMoney } from '../format';
import { M } from '../theme';
import { Ambient, Label, Panel, PrimaryButton, TopBar } from '../ui';
import { setDraft, useHomeDraft } from './homeDraft';
import { apiMessage, h, Pills } from './homeUi';

const PLANS: { value: TruckPlan; label: string }[] = [
  { value: 'trucks', label: 'Done in a day' },
  { value: 'trips', label: 'Fewer trucks, more trips' },
];

export function MoveHomeQuoteScreen({ navigation }: { navigation: any }) {
  const insets = useSafeAreaInsets();
  const d = useHomeDraft();
  const [country, setCountry] = useState<string | null>(null);
  const [quote, setQuote] = useState<HomeQuote | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pricing, setPricing] = useState(false);

  useEffect(() => {
    getMyTenant()
      .then((t) => setCountry((t.currency && COUNTRY_FOR_CURRENCY[t.currency]) || 'PH'))
      .catch(() => setCountry('PH'));
  }, []);

  useEffect(() => {
    if (!country) return;
    let live = true;
    setPricing(true);
    setError(null);
    quoteHome({
      origin: { ...d.origin, country_code: country },
      destination: { ...d.destination, country_code: country },
      property: d.property,
      items: declared(d.inventory),
      plan: d.plan,
    })
      .then((q) => { if (live) setQuote(q); })
      .catch((e) => { if (live) { setQuote(null); setError(apiMessage(e)); } })
      .finally(() => { if (live) setPricing(false); });
    return () => { live = false; };
    // The inventory cannot change on this screen; the plan can.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [country, d.plan]);

  const reconciles = !!quote && quote.lines.reduce((n, l) => n + l.amount_cents, 0) === quote.total_cents;
  const money = (c: number) => formatMoney(c, quote?.currency);

  return (
    <View style={h.root}>
      <Ambient />
      <TopBar label="The quote" onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={[h.content, { paddingBottom: 24 }]}>
        {pricing && !quote && <ActivityIndicator color={M.accent} style={{ marginTop: 32 }} />}
        {error && <Panel tone="amber"><Text style={h.body}>{error}</Text></Panel>}

        {quote && (
          <>
            <Panel tone="accent">
              <Text style={[h.note, { letterSpacing: 1.4 }]}>ALL-IN, DOOR TO DOOR</Text>
              <Text style={h.big}>{reconciles ? formatMoney(quote.total_cents, quote.currency, { whole: true }) : '—'}</Text>
              <Text style={h.body}>
                {crewLine(quote.trucks, quote.crew_total ?? quote.trucks + quote.helpers)} · {quote.loads} load{quote.loads === 1 ? '' : 's'} · {quote.helper_hours} h
              </Text>
              <Text style={[h.note, { marginTop: 6 }]}>
                {(quote.volume_l / 1000).toFixed(1)} m³ · {quote.weight_kg.toLocaleString()} kg · {distanceLine(quote.distance_km, quote.distance_basis, quote.drive_minutes)}, {quote.origin_text} → {quote.destination_text}
              </Text>
            </Panel>

            <Label>How the trucks run</Label>
            <Pills label="Truck plan" options={PLANS} value={d.plan} onChange={(plan) => setDraft({ plan })} />
            <Text style={h.note}>
              {quote.large_estate
                ? 'A Large Estate: over 75 m³ always runs on at least two trucks, with a multi-truck team.'
                : 'The same loads either way. One truck per load finishes in a day; fewer trucks doing two loads each costs less and takes longer.'}
            </Text>

            <Label>How it's priced</Label>
            <Panel>
              <View style={{ gap: 12 }}>
                {quote.lines.map((l) => (
                  <View key={l.key} style={h.row}>
                    <View style={{ flex: 1 }}>
                      <Text style={[h.rowLabel, l.amount_cents < 0 && { color: M.accent }]}>{l.label}</Text>
                      {!!l.note && <Text style={h.note}>{l.note}</Text>}
                    </View>
                    <Text style={[h.rowLabel, { fontVariant: ['tabular-nums'] }, l.amount_cents < 0 && { color: M.accent }]}>
                      {l.amount_cents < 0 ? '−' : ''}{money(Math.abs(l.amount_cents))}
                    </Text>
                  </View>
                ))}
              </View>
              {!reconciles && (
                <Text style={[h.note, { color: M.amberText, marginTop: 10 }]}>These lines don't add up to the total, so no price is shown. Go back and try again.</Text>
              )}
            </Panel>
            {quote.survey_required && (
              <Text style={h.note}>A surveyor checks the inventory before the move. If it differs, the quote is corrected before anything is dispatched.</Text>
            )}
          </>
        )}
      </ScrollView>
      <View style={[h.footer, { paddingBottom: insets.bottom + 12 }]}>
        <PrimaryButton
          label="PICK THE DATES"
          disabled={!quote || !reconciles || pricing}
          onPress={() => quote && navigation.navigate('MoveHomeSchedule', { quote })}
        />
      </View>
    </View>
  );
}
