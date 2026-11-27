/**
 * Whole-home move, A2 — the rooms. A running total, one row per room with
 * its items and volume, and a declaration before pricing: the customer
 * confirms the list they are pricing, and any later change un-declares it.
 */
import React, { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { Ionicons } from '@expo/vector-icons';
import { getCatalogue, m3, roomTotals, totals, type HomeCatalogue } from '../../../services/api/homeMove';
import { M } from '../theme';
import { Ambient, Label, Panel, PrimaryButton, Toggle, TopBar } from '../ui';
import { setDraft, useHomeDraft } from './homeDraft';
import { apiMessage, h, ReadBackPanel } from './homeUi';

export function MoveHomeRoomsScreen({ navigation }: { navigation: any }) {
  const insets = useSafeAreaInsets();
  const d = useHomeDraft();
  const [catalogue, setCatalogue] = useState<HomeCatalogue | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getCatalogue(d.property.type).then(setCatalogue).catch((e) => setError(apiMessage(e)));
  }, [d.property.type]);

  const all = totals(d.inventory, catalogue ?? null);
  const rooms = catalogue?.rooms ?? [];
  const biggest = Math.max(1, ...rooms.map((r) => roomTotals(d.inventory, r).volumeL));
  const typeLabel = d.property.type === 'offices' ? 'Offices' : d.property.type === 'villa' ? 'Villa' : 'Apartment';

  return (
    <View style={h.root}>
      <Ambient />
      <TopBar label="Rooms" onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={[h.content, { paddingBottom: 24 }]}>
        <ReadBackPanel read={d.read} onChangeProperty={() => navigation.navigate('MoveHomeSet')} />
        {error && <Panel tone="amber"><Text style={h.body}>{error}</Text></Panel>}

        <Panel tone="accent">
          <Text style={[h.note, { letterSpacing: 1.4 }]}>SO FAR</Text>
          <Text style={h.big}>{m3(all.volumeL)}</Text>
          <Text style={h.body}>
            {all.count} item{all.count === 1 ? '' : 's'} · {all.weightKg.toLocaleString()} kg
          </Text>
          <Text style={[h.note, { marginTop: 6 }]}>
            {typeLabel} · {d.property.size} · {d.origin.city.trim() || 'from'} → {d.destination.city.trim() || 'to'}
          </Text>
        </Panel>

        <Label>{d.property.type === 'offices' ? 'Areas' : 'Rooms'}</Label>
        {catalogue === undefined && !error && <ActivityIndicator color={M.accent} />}
        {rooms.map((r) => {
          const t = roomTotals(d.inventory, r);
          return (
            <Pressable
              key={r.key}
              onPress={() => navigation.navigate('MoveHomeRoom', { room: r.key })}
              accessibilityRole="button"
              accessibilityLabel={`${r.name}, ${t.count} items`}
              style={({ pressed }) => [pressed && { opacity: 0.8 }]}
            >
              <Panel>
                <View style={h.row}>
                  <View style={{ flex: 1 }}>
                    <Text style={h.rowLabel}>{r.name}</Text>
                    <Text style={h.note}>{t.count === 0 ? 'Nothing listed' : `${t.count} item${t.count === 1 ? '' : 's'} · ${m3(t.volumeL)}`}</Text>
                  </View>
                  <Ionicons name="chevron-forward" size={18} color={M.faint} />
                </View>
                <View style={h.bar}>
                  <View style={[h.barFill, { width: `${Math.round((t.volumeL / biggest) * 100)}%` }]} />
                </View>
              </Panel>
            </Pressable>
          );
        })}

        {all.count > 0 && (
          <Panel>
            <View style={h.row}>
              <Text style={[h.rowLabel, { flex: 1 }]}>I've listed everything I'm moving</Text>
              <Toggle label="Declare the inventory" value={d.declared} onChange={(declared) => setDraft({ declared })} />
            </View>
            <Text style={[h.note, { marginTop: 6 }]}>
              The quote is for this list. {d.property.type === 'apartment' ? 'A surveyor checks it for larger homes.' : 'A surveyor checks it before the move.'}
            </Text>
          </Panel>
        )}
      </ScrollView>
      <View style={[h.footer, { paddingBottom: insets.bottom + 12 }]}>
        <PrimaryButton
          label={d.declared ? 'PRICE THE MOVE' : 'DECLARE TO CONTINUE'}
          disabled={!d.declared || all.count === 0}
          onPress={() => navigation.navigate('MoveHomeQuote')}
        />
      </View>
    </View>
  );
}
