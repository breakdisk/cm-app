/** Move app — Cancelled. The receipt for a cancellation. */
import React from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { HEADING, HEADING_LIGHT, M } from './theme';
import { Ambient, PrimaryButton } from './ui';

export function MoveCancelledScreen({ navigation, route }: { navigation: any; route: any }) {
  const insets = useSafeAreaInsets();
  const { awb, refundText } = route.params ?? {};
  return (
    <View style={[s.root, { paddingTop: insets.top + 20, paddingBottom: insets.bottom + 30 }]}>
      <Ambient />
      <View style={s.body}>
        <Text style={s.tag}>{awb} · Cancelled</Text>
        <Text style={s.h1Light}>Move cancelled.</Text>
        {!!refundText && <Text style={s.h1Strong}>{refundText} back.</Text>}
        <Text style={s.note}>
          The driver has been released. Anything refunded goes back the way you paid, and can take a few business days
          to show.
        </Text>
      </View>
      <PrimaryButton label="DONE" onPress={() => navigation.popToTop()} />
    </View>
  );
}

const s = StyleSheet.create({
  root:     { flex: 1, backgroundColor: M.ground, paddingHorizontal: 24 },
  body:     { flex: 1, justifyContent: 'center' },
  tag:      { alignSelf: 'flex-start', fontSize: 10, letterSpacing: 2, textTransform: 'uppercase', color: M.penalty, paddingHorizontal: 11, paddingVertical: 6, borderRadius: 8, borderWidth: 1, borderColor: M.penaltyBorder },
  h1Light:  { fontFamily: HEADING_LIGHT, fontWeight: '300', fontSize: 36, lineHeight: 40, color: M.ink, marginTop: 18 },
  h1Strong: { fontFamily: HEADING, fontWeight: '700', fontSize: 36, lineHeight: 40, color: M.ink },
  note:     { marginTop: 14, fontSize: 13, lineHeight: 21, color: M.muted },
});
