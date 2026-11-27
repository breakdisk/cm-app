/**
 * Whole-home move, A1 — the property and the route. The route is part of the
 * intake because a home move is priced door to door and every load is a
 * round trip over it; the server geocodes both ends and measures it.
 */
import React, { useEffect, useState } from 'react';
import { ActivityIndicator, KeyboardAvoidingView, Platform, ScrollView, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FLOORS, getCatalogue, surveyRequired, type HomeCatalogue, type PropertyType } from '../../../services/api/homeMove';
import { M } from '../theme';
import { Ambient, Field, Label, Panel, PrimaryButton, Toggle, TopBar } from '../ui';
import { setDraft, useHomeDraft } from './homeDraft';
import { apiMessage, h, Pills, ReadBackPanel } from './homeUi';
import { WaitlistPanel } from './WaitlistPanel';

const TYPES: { value: PropertyType; label: string }[] = [
  { value: 'apartment', label: 'Apartment' },
  { value: 'villa', label: 'Villa' },
  { value: 'offices', label: 'Offices' },
];
const FLOOR_OPTIONS = FLOORS.map((label, value) => ({ value, label }));

export function MoveHomeSetScreen({ navigation }: { navigation: any }) {
  const insets = useSafeAreaInsets();
  const d = useHomeDraft();
  const p = d.property;
  const [catalogue, setCatalogue] = useState<HomeCatalogue | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setCatalogue(undefined);
    getCatalogue(p.type)
      .then((c) => {
        setCatalogue(c);
        // A size from another type's list cannot be priced; keep the choice
        // only while it still exists.
        if (c && !c.sizes.includes(p.size)) setDraft({ property: { ...p, size: c.sizes[Math.min(2, c.sizes.length - 1)] } });
      })
      .catch((e) => setError(apiMessage(e)));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [p.type]);

  const setP = (patch: Partial<typeof p>) => setDraft({ property: { ...p, ...patch } });
  const sizes = catalogue?.sizes ?? [];
  const surveyed = surveyRequired(p, sizes);
  const ready =
    !!catalogue && sizes.includes(p.size) &&
    d.origin.line1.trim().length >= 5 && d.origin.city.trim().length >= 2 &&
    d.destination.line1.trim().length >= 5 && d.destination.city.trim().length >= 2;

  return (
    <KeyboardAvoidingView style={h.root} behavior={Platform.OS === 'ios' ? 'padding' : undefined}>
      <Ambient />
      <TopBar label="Whole-home move" onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={[h.content, { paddingBottom: 24 }]} keyboardShouldPersistTaps="handled">
        <Text style={h.heading}>The home, then the inventory.</Text>
        <WaitlistPanel onClaim={(id) => navigation.navigate('MoveHomeWaitlist', { id })} />
        <ReadBackPanel read={d.read} />

        {catalogue === null && (
          <Panel tone="amber"><Text style={h.body}>Whole-home moves aren't offered in your area yet.</Text></Panel>
        )}
        {error && <Panel tone="amber"><Text style={h.body}>{error}</Text></Panel>}

        <Label>Route</Label>
        <Panel>
          <View style={{ gap: 10 }}>
            <View style={{ flexDirection: 'row', gap: 10 }}>
              <Field label="From" value={d.origin.line1} onChangeText={(v) => setDraft({ origin: { ...d.origin, line1: v } })} placeholder="Street and number" style={{ flex: 2 }} />
              <Field label="City" value={d.origin.city} onChangeText={(v) => setDraft({ origin: { ...d.origin, city: v } })} placeholder="City" />
            </View>
            <View style={{ flexDirection: 'row', gap: 10 }}>
              <Field label="To" value={d.destination.line1} onChangeText={(v) => setDraft({ destination: { ...d.destination, line1: v } })} placeholder="Street and number" style={{ flex: 2 }} />
              <Field label="City" value={d.destination.city} onChangeText={(v) => setDraft({ destination: { ...d.destination, city: v } })} placeholder="City" />
            </View>
            <Text style={h.note}>Priced door to door: each truckload is a round trip over this route.</Text>
          </View>
        </Panel>

        <Label>Property</Label>
        <Panel>
          <View style={{ gap: 14 }}>
            <Pills label="Property type" options={TYPES} value={p.type} onChange={(type) => setP({ type })} />
            {catalogue === undefined ? (
              <ActivityIndicator color={M.accent} />
            ) : (
              <Pills label="Property size" options={sizes.map((s) => ({ value: s, label: s }))} value={p.size} onChange={(size) => setP({ size })} />
            )}
          </View>
        </Panel>

        <Label>Floors</Label>
        <Panel>
          <View style={{ gap: 12 }}>
            <Text style={h.note}>AT PICK-UP</Text>
            <Pills label="Floor at pick-up" options={FLOOR_OPTIONS} value={p.pickup_floor} onChange={(pickup_floor) => setP({ pickup_floor })} />
            <View style={h.row}>
              <Text style={h.rowLabel}>Working lift</Text>
              <Toggle label="Working lift at pick-up" value={p.pickup_has_lift} onChange={(pickup_has_lift) => setP({ pickup_has_lift })} />
            </View>
            <Text style={[h.note, { marginTop: 6 }]}>AT DROP-OFF</Text>
            <Pills label="Floor at drop-off" options={FLOOR_OPTIONS} value={p.dropoff_floor} onChange={(dropoff_floor) => setP({ dropoff_floor })} />
            <View style={h.row}>
              <Text style={h.rowLabel}>Working lift</Text>
              <Toggle label="Working lift at drop-off" value={p.dropoff_has_lift} onChange={(dropoff_has_lift) => setP({ dropoff_has_lift })} />
            </View>
            <Text style={h.note}>No lift above the second floor adds a helper.</Text>
            <View style={[h.row, { marginTop: 4 }]}>
              <View style={{ flexShrink: 1 }}>
                <Text style={h.rowLabel}>Long carry to the truck</Text>
                <Text style={h.note}>Over 40 m from the door to the loading point</Text>
              </View>
              <Toggle label="Long carry" value={p.long_carry} onChange={(long_carry) => setP({ long_carry })} />
            </View>
          </View>
        </Panel>

        {!!catalogue && (
          <Panel tone={surveyed ? 'accent' : 'default'}>
            <Text style={[h.note, { color: surveyed ? M.accent : M.label, letterSpacing: 1.4 }]}>
              {surveyed ? 'ATTENDED SURVEY' : 'SELF-DECLARED INVENTORY'}
            </Text>
            <Text style={[h.body, { marginTop: 6 }]}>
              {surveyed
                ? 'A surveyor visits before the move to check the inventory — at least two days ahead, so anything missed can be corrected. Any survey fee is credited back on the quote.'
                : 'For a studio or one-bedroom apartment you list the inventory yourself; no visit is needed.'}
            </Text>
          </Panel>
        )}
      </ScrollView>
      <View style={[h.footer, { paddingBottom: insets.bottom + 12 }]}>
        <PrimaryButton label="LIST THE ROOMS" disabled={!ready} onPress={() => navigation.navigate('MoveHomeRooms')} />
      </View>
    </KeyboardAvoidingView>
  );
}
