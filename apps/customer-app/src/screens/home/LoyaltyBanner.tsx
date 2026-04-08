import React from 'react';
import { View, Text, TouchableOpacity, StyleSheet } from 'react-native';
import { LinearGradient } from 'expo-linear-gradient';
import { Ionicons } from '@expo/vector-icons';
import { getTier, getNextTier, ptsToNextTier, tierProgress } from '../../utils/loyalty';

interface LoyaltyBannerProps {
  points: number;
  onPress?: () => void;
}

export default function LoyaltyBanner({ points, onPress }: LoyaltyBannerProps) {
  const tier     = getTier(points);
  const nextTier = getNextTier(points);
  const progress = tierProgress(points);
  const toNext   = ptsToNextTier(points);

  // Gradient colors per tier
  const gradients: Record<string, [string, string]> = {
    Bronze:   ["#7C4F20", "#CD7F32"],
    Silver:   ["#6B7280", "#C0C0C0"],
    Gold:     ["#92610A", "#FFAB00"],
    Platinum: ["#0891B2", "#00E5FF"],
  };
  const [gradStart, gradEnd] = gradients[tier.label] ?? ["#A855F7", "#00E5FF"];

  return (
    <TouchableOpacity onPress={onPress} activeOpacity={0.85}>
      <LinearGradient
        colors={[gradStart + "33", gradEnd + "22"]}
        start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }}
        style={s.card}
      >
        {/* Top row */}
        <View style={s.topRow}>
          <View style={s.tierBadge}>
            <Ionicons name={tier.icon as any} size={14} color={tier.color} />
            <Text style={[s.tierLabel, { color: tier.color }]}>{tier.label}</Text>
          </View>
          <Text style={s.discountBadge}>
            {tier.discount > 0 ? `${tier.discount}% off all bookings` : "Earn points on every booking"}
          </Text>
        </View>

        {/* Points */}
        <View style={s.ptsRow}>
          <Text style={s.ptsValue}>{points.toLocaleString()}</Text>
          <Text style={s.ptsLabel}>points</Text>
        </View>

        {/* Progress bar */}
        {nextTier && (
          <View style={s.progressSection}>
            <View style={s.progressBar}>
              <View style={[s.progressFill, { width: `${Math.round(progress * 100)}%` as any, backgroundColor: tier.color }]} />
            </View>
            <Text style={s.progressText}>
              {toNext} pts to {nextTier.label}
            </Text>
          </View>
        )}

        {!nextTier && (
          <Text style={[s.maxTierText, { color: tier.color }]}>
            Maximum tier reached · All perks unlocked
          </Text>
        )}
      </LinearGradient>
    </TouchableOpacity>
  );
}

const s = StyleSheet.create({
  card:           { borderRadius: 16, padding: 16, borderWidth: 1, borderColor: "rgba(255,255,255,0.08)", gap: 10 },
  topRow:         { flexDirection: "row", justifyContent: "space-between", alignItems: "center" },
  tierBadge:      { flexDirection: "row", alignItems: "center", gap: 5, backgroundColor: "rgba(255,255,255,0.06)", borderRadius: 8, paddingHorizontal: 9, paddingVertical: 4 },
  tierLabel:      { fontSize: 11, fontFamily: "SpaceGrotesk-SemiBold" },
  discountBadge:  { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.45)" },
  ptsRow:         { flexDirection: "row", alignItems: "baseline", gap: 5 },
  ptsValue:       { fontSize: 32, fontFamily: "SpaceGrotesk-Bold", color: "#FFF" },
  ptsLabel:       { fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.4)" },
  progressSection:{ gap: 6 },
  progressBar:    { height: 3, backgroundColor: "rgba(255,255,255,0.08)", borderRadius: 2, overflow: "hidden" },
  progressFill:   { height: "100%", borderRadius: 2 },
  progressText:   { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.35)" },
  maxTierText:    { fontSize: 11, fontFamily: "JetBrainsMono-Regular" },
});
