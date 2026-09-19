/**
 * The customer's places on the priority waitlist: where they stand in line,
 * and a window held for them — booked from here, or from the push.
 */
import React, { useCallback, useEffect, useState } from 'react';
import { Alert, Pressable, Text, View } from 'react-native';
import { useFocusEffect } from '@react-navigation/native';
import { getWaitlist, holdLeft, slotDay, withdrawWaitlist, type WaitlistPlace } from '../../../services/api/homeMove';
import { M } from '../theme';
import { Panel } from '../ui';
import { apiMessage, h } from './homeUi';

export function WaitlistPanel({ offsetMin = 480, onClaim }: { offsetMin?: number; onClaim: (id: string) => void }) {
  const [places, setPlaces] = useState<WaitlistPlace[]>([]);
  const [, tick] = useState(0);

  const load = useCallback(() => {
    getWaitlist().then(setPlaces).catch(() => setPlaces([]));
  }, []);
  useFocusEffect(load);
  // A held window counts down on screen.
  const held = places.some((p) => p.status === 'offered');
  useEffect(() => {
    if (!held) return;
    const t = setInterval(() => tick((n) => n + 1), 1000);
    return () => clearInterval(t);
  }, [held]);

  if (places.length === 0) return null;

  function withdraw(p: WaitlistPlace) {
    Alert.alert('Leave the waitlist?', `You'll lose your place for ${slotDay(`${p.wanted_date}T12:00:00Z`, 0)}.`, [
      { text: 'Stay' },
      {
        text: 'Leave',
        style: 'destructive',
        onPress: () => withdrawWaitlist(p.id).then(load).catch((e) => Alert.alert("Couldn't leave", apiMessage(e))),
      },
    ]);
  }

  return (
    <View style={{ gap: 10 }}>
      {places.map((p) => {
        const day = slotDay(`${p.wanted_date}T12:00:00Z`, 0);
        const left = p.status === 'offered' && p.hold_expires_at ? holdLeft(p.hold_expires_at) : null;
        if (left && p.offered_move_at) {
          return (
            <Pressable key={p.id} onPress={() => onClaim(p.id)} accessibilityRole="button">
              <Panel tone="accent">
                <Text style={[h.note, { letterSpacing: 1.4, color: M.accent }]}>A TEAM OPENED UP · HELD FOR {left}</Text>
                <Text style={[h.rowLabel, { marginTop: 4 }]}>{slotDay(p.offered_move_at, offsetMin)} — tap to book it</Text>
              </Panel>
            </Pressable>
          );
        }
        return (
          <Panel key={p.id}>
            <Text style={[h.note, { letterSpacing: 1.4 }]}>PRIORITY WAITLIST</Text>
            <View style={[h.row, { marginTop: 4 }]}>
              <Text style={[h.rowLabel, { flex: 1 }]}>
                {day} · {p.ahead ? `${p.ahead} ahead of you` : "you're next"}
              </Text>
              <Pressable onPress={() => withdraw(p)} accessibilityRole="button" hitSlop={10}>
                <Text style={{ color: M.faint, fontSize: 13 }}>Leave</Text>
              </Pressable>
            </View>
            <Text style={h.note}>We'll push you the moment a team opens — you'll have 5 minutes to book.</Text>
          </Panel>
        );
      })}
    </View>
  );
}
