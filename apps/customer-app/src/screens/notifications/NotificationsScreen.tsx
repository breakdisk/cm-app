/**
 * Customer App — Notifications Screen
 * Push notification history: delivery alerts, promos, loyalty events.
 */
import React from "react";
import { View, Text, StyleSheet, FlatList, Pressable } from "react-native";
import Animated, { FadeInDown } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const RED    = "#FF3B5C";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

type NotifType = "delivery" | "promo" | "loyalty" | "alert";

interface Notif {
  id:   string;
  type: NotifType;
  title: string;
  body:  string;
  time:  string;
  read:  boolean;
}

const NOTIFS: Notif[] = [
  { id: "N1", type: "delivery", title: "Out for Delivery!",        body: "LS-A1B2C3D4 will be delivered today between 2–4 PM. Stay at home!",                 time: "10m ago",  read: false },
  { id: "N2", type: "delivery", title: "Package Picked Up",        body: "LS-E5F6G7H8 has been picked up from the merchant and is heading to sorting.",     time: "1h ago",   read: false },
  { id: "N3", type: "loyalty",  title: "+50 Loyalty Points!",      body: "You earned 50 points for your last delivery. 380 more to reach Platinum.",         time: "3h ago",   read: true  },
  { id: "N4", type: "alert",    title: "Delivery Attempt Failed",  body: "LS-M3N4O5P6 — we couldn't reach you. We'll retry tomorrow. Tap to reschedule.",   time: "Yesterday", read: true  },
  { id: "N5", type: "promo",    title: "₱50 Off Your Next Booking", body: "Book any shipment today and get ₱50 off. Use code MARCH50. Valid until Mar 31.", time: "Yesterday", read: true  },
  { id: "N6", type: "delivery", title: "Package Delivered ✓",      body: "LS-I9J0K1L2 was delivered and signed for by the recipient. Rate your experience.", time: "2 days ago", read: true },
  { id: "N7", type: "promo",    title: "New: COD in Mindanao",      body: "Cash on Delivery is now available across Mindanao. Try it on your next shipment.", time: "3 days ago", read: true },
];

const TYPE_CONFIG: Record<NotifType, { icon: string; color: string }> = {
  delivery: { icon: "cube-outline",          color: CYAN   },
  promo:    { icon: "megaphone-outline",      color: PURPLE },
  loyalty:  { icon: "star-outline",           color: AMBER  },
  alert:    { icon: "alert-circle-outline",   color: RED    },
};

export function NotificationsScreen() {
  const unreadCount = NOTIFS.filter(n => !n.read).length;

  function renderItem({ item, index }: { item: Notif; index: number }) {
    const { icon, color } = TYPE_CONFIG[item.type];
    return (
      <Animated.View entering={FadeInDown.delay(index * 40).springify()}>
        <Pressable style={({ pressed }) => [s.row, !item.read && s.rowUnread, { opacity: pressed ? 0.8 : 1 }]}>
          <View style={[s.iconWrap, { backgroundColor: color + "20" }]}>
            <Ionicons name={icon as any} size={18} color={color} />
            {!item.read && <View style={[s.unreadDot, { backgroundColor: color }]} />}
          </View>
          <View style={{ flex: 1 }}>
            <Text style={[s.title, !item.read && { color: "#FFF" }]}>{item.title}</Text>
            <Text style={s.body} numberOfLines={2}>{item.body}</Text>
            <Text style={s.time}>{item.time}</Text>
          </View>
        </Pressable>
      </Animated.View>
    );
  }

  return (
    <View style={s.container}>
      {/* Header */}
      <LinearGradient colors={["rgba(0,229,255,0.08)", "transparent"]} style={s.hero}>
        <Text style={s.heroTitle}>Notifications</Text>
        {unreadCount > 0 && (
          <Text style={s.heroSub}>{unreadCount} unread</Text>
        )}
      </LinearGradient>

      <FlatList
        data={NOTIFS}
        keyExtractor={(n) => n.id}
        renderItem={renderItem}
        contentContainerStyle={{ paddingBottom: 40 }}
        ItemSeparatorComponent={() => <View style={{ height: 1, backgroundColor: BORDER, marginHorizontal: 16 }} />}
      />
    </View>
  );
}

const s = StyleSheet.create({
  container:  { flex: 1, backgroundColor: CANVAS },
  hero:       { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 20 },
  heroTitle:  { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:    { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },
  row:        { flexDirection: "row", gap: 14, paddingHorizontal: 16, paddingVertical: 14, backgroundColor: "transparent" },
  rowUnread:  { backgroundColor: "rgba(0,229,255,0.04)" },
  iconWrap:   { position: "relative", width: 44, height: 44, borderRadius: 12, alignItems: "center", justifyContent: "center", flexShrink: 0 },
  unreadDot:  { position: "absolute", top: -3, right: -3, width: 8, height: 8, borderRadius: 4, borderWidth: 2, borderColor: CANVAS },
  title:      { fontSize: 13, fontWeight: "600", color: "rgba(255,255,255,0.85)", marginBottom: 3 },
  body:       { fontSize: 12, color: "rgba(255,255,255,0.4)", lineHeight: 17 },
  time:       { fontSize: 10, color: "rgba(255,255,255,0.2)", fontFamily: "JetBrainsMono-Regular", marginTop: 4 },
});
