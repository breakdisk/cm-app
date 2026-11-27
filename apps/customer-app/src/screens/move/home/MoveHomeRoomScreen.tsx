/**
 * Whole-home move, A3 — one room. Tap a preset to add it; set how many and
 * whether it is dismantled and rebuilt or specially packed. Sizes and
 * weights are the catalogue's, served per tenant — nothing here is typed.
 *
 * The design's other capture modes (scan, photo, video) resolve a detection
 * to a catalogue entry; they need a detection service that does not exist,
 * so this screen offers the list and says so rather than showing dead tabs.
 */
import React, { useEffect, useState } from 'react';
import { ActivityIndicator, Pressable, ScrollView, StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { getCatalogue, m3, roomTotals, type HomeCatalogue } from '../../../services/api/homeMove';
import { M } from '../theme';
import { Ambient, Label, Panel, PrimaryButton, Stepper, Toggle, TopBar } from '../ui';
import { setItem, useHomeDraft } from './homeDraft';
import { apiMessage, h } from './homeUi';

export function MoveHomeRoomScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const d = useHomeDraft();
  const roomKey: string = route.params?.room;
  const [catalogue, setCatalogue] = useState<HomeCatalogue | null | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    getCatalogue(d.property.type).then(setCatalogue).catch((e) => setError(apiMessage(e)));
  }, [d.property.type]);

  const room = catalogue?.rooms.find((r) => r.key === roomKey);
  const listed = d.inventory[roomKey] ?? {};
  const t = room ? roomTotals(d.inventory, room) : { volumeL: 0, count: 0 };

  return (
    <View style={h.root}>
      <Ambient />
      <TopBar label={room?.name ?? 'Room'} onBack={() => navigation.goBack()} />
      <ScrollView contentContainerStyle={[h.content, { paddingBottom: 24 }]}>
        {error && <Panel tone="amber"><Text style={h.body}>{error}</Text></Panel>}
        {!room && !error && <ActivityIndicator color={M.accent} style={{ marginTop: 24 }} />}
        {room && (
          <>
            <Text style={h.note}>
              {t.count} item{t.count === 1 ? '' : 's'} · {m3(t.volumeL)}. Tap to add — scanning and photos come later; the list is everything for now.
            </Text>
            <View style={s.grid}>
              {room.items.map((item) => (
                <Pressable
                  key={item.key}
                  onPress={() => setItem(roomKey, item.key, { qty: (listed[item.key]?.qty ?? 0) + 1 }, { dismantle: item.assembly, packing: item.packing })}
                  accessibilityRole="button"
                  accessibilityLabel={`Add ${item.name}`}
                  style={({ pressed }) => [s.preset, !!listed[item.key] && s.presetOn, pressed && { transform: [{ scale: 0.96 }] }]}
                >
                  <Text style={s.presetName} numberOfLines={2}>{item.name}</Text>
                  <Text style={s.presetMeta}>{m3(item.volume_l)} · {item.weight_kg} kg{listed[item.key] ? ` · ×${listed[item.key].qty}` : ''}</Text>
                </Pressable>
              ))}
            </View>

            {Object.keys(listed).length > 0 && <Label>In this room</Label>}
            {room.items.filter((i) => listed[i.key]).map((item) => {
              const line = listed[item.key];
              return (
                <Panel key={item.key}>
                  <View style={h.row}>
                    <Text style={[h.rowLabel, { flex: 1 }]}>{item.name}</Text>
                    <Stepper label={item.name} value={line.qty} min={0} max={200} onChange={(qty) => setItem(roomKey, item.key, { qty })} />
                  </View>
                  {item.assembly && (
                    <View style={[h.row, { marginTop: 10 }]}>
                      <Text style={h.body}>Dismantle and rebuild</Text>
                      <Toggle label={`Dismantle ${item.name}`} value={line.dismantle} onChange={(dismantle) => setItem(roomKey, item.key, { dismantle })} />
                    </View>
                  )}
                  <View style={[h.row, { marginTop: 10 }]}>
                    <Text style={h.body}>Special packing</Text>
                    <Toggle label={`Pack ${item.name}`} value={line.packing} onChange={(packing) => setItem(roomKey, item.key, { packing })} />
                  </View>
                </Panel>
              );
            })}
          </>
        )}
      </ScrollView>
      <View style={[h.footer, { paddingBottom: insets.bottom + 12 }]}>
        <PrimaryButton label="DONE WITH THIS ROOM" onPress={() => navigation.goBack()} />
      </View>
    </View>
  );
}

const s = StyleSheet.create({
  grid:       { flexDirection: 'row', flexWrap: 'wrap', gap: 8 },
  preset:     { width: '48%', flexGrow: 1, minHeight: 64, padding: 12, borderRadius: 14, borderWidth: 1, borderColor: M.hairline, backgroundColor: 'rgba(255,255,255,0.03)' },
  presetOn:   { borderColor: M.accentBorder, backgroundColor: M.accentTint },
  presetName: { fontSize: 13, lineHeight: 17, color: M.ink },
  presetMeta: { fontSize: 10, color: M.faint, marginTop: 5 },
});
