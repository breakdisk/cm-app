/**
 * A waitlisted window held for this customer: priced for it, then straight
 * to the dates screen with that window chosen and the hold counting down.
 */
import React, { useEffect, useState } from 'react';
import { ActivityIndicator, Text, View } from 'react-native';
import { claimWaitlist } from '../../../services/api/homeMove';
import { M } from '../theme';
import { Ambient, GhostButton, Panel, TopBar } from '../ui';
import { apiMessage, h } from './homeUi';

export function MoveHomeWaitlistScreen({ navigation, route }: { navigation: any; route: any }) {
  const id: string | undefined = route.params?.id;
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!id) {
      setError('That waitlist link is incomplete.');
      return;
    }
    claimWaitlist(id)
      .then((c) => navigation.replace('MoveHomeSchedule', {
        quote: c.quote,
        waitlist: { id: c.waitlist_id, move_at: c.move_at, hold_expires_at: c.hold_expires_at },
      }))
      .catch((e) => {
        const msg = apiMessage(e);
        setError(/HOLD_LAPSED/.test(msg)
          ? 'That window was held for 5 minutes and has gone to the next in line. You can rejoin the waitlist from the dates screen.'
          : /NOT_OFFERED/.test(msg)
            ? "No window is held for you yet — we'll push you when one opens."
            : msg);
      });
  }, [id, navigation]);

  return (
    <View style={h.root}>
      <Ambient />
      <TopBar label="Waitlist" onBack={() => navigation.goBack()} />
      <View style={[h.content, { paddingTop: 24 }]}>
        {!error && <ActivityIndicator color={M.accent} />}
        {error && (
          <>
            <Panel tone="amber"><Text style={h.body}>{error}</Text></Panel>
            <GhostButton label="Back" onPress={() => navigation.goBack()} />
          </>
        )}
      </View>
    </View>
  );
}
