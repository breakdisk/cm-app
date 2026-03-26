/**
 * Customer App — Support Screen
 * FAQ accordion + simulated AI chat widget.
 */
import React, { useState, useRef } from "react";
import {
  View, Text, StyleSheet, ScrollView, Pressable,
  TextInput, KeyboardAvoidingView, Platform,
} from "react-native";
import Animated, { FadeInDown, FadeInUp, FadeIn } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useSelector } from "react-redux";
import type { RootState } from "../../store";

const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const AMBER   = "#FFAB00";
const PURPLE  = "#A855F7";
const RED     = "#FF3B5C";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

// ── FAQ data ─────────────────────────────────────────────────────────────────

interface FaqItem {
  q: string;
  a: string;
  icon: string;
  color: string;
}

const FAQS: FaqItem[] = [
  {
    q: "How do I track my shipment?",
    a: "Go to the Track tab and enter your AWB number (e.g. LS-A1B2C3D4). You'll see a live timeline of your package's journey including current location, driver details, and estimated delivery time.",
    icon: "cube-outline",
    color: CYAN,
  },
  {
    q: "What is a Balikbayan Box?",
    a: "A Balikbayan Box is a large freight shipment service for overseas workers sending goods home to the Philippines. It supports both Sea Freight (30–45 days, most economical) and Air Freight (5–10 days). A receiver passport copy is required for customs clearance.",
    icon: "globe-outline",
    color: PURPLE,
  },
  {
    q: "How does Cash on Delivery (COD) work?",
    a: "With COD, the recipient pays the declared amount in cash when the package is delivered to their door. The driver collects the payment and it is reconciled back to the merchant. COD is available for local shipments only.",
    icon: "cash-outline",
    color: AMBER,
  },
  {
    q: "What happens if delivery is attempted and I'm not home?",
    a: "Our driver will leave a notification and attempt re-delivery the next business day. You can reschedule via the app or contact support. After 3 failed attempts, the package may be returned to sender.",
    icon: "alert-circle-outline",
    color: AMBER,
  },
  {
    q: "How do I earn and use Loyalty Points?",
    a: "You earn 50 pts for every local booking and 150 pts for international. Points can be redeemed for shipping discounts. Reaching 1,000 pts unlocks Platinum tier with priority handling and free insurance.",
    icon: "star-outline",
    color: PURPLE,
  },
  {
    q: "How long does local delivery take?",
    a: "Same-city deliveries are typically completed within 1–3 business days. Same-day delivery is available in Metro Manila for bookings placed before 10 AM. Remote areas may take 3–7 days.",
    icon: "time-outline",
    color: GREEN,
  },
  {
    q: "What ID is accepted for KYC?",
    a: "Passport is accepted for both local and international shipments. Emirates ID is accepted for local shipments only. For international (Balikbayan Box) shipping, a valid Passport is mandatory for customs requirements.",
    icon: "card-outline",
    color: CYAN,
  },
  {
    q: "Can I cancel or change my booking?",
    a: "You can cancel a booking up to 1 hour after placing it, as long as it has not been picked up yet. Once in transit, cancellation is no longer possible. Contact support to modify delivery address.",
    icon: "close-circle-outline",
    color: RED,
  },
];

// ── Simulated AI responses ────────────────────────────────────────────────────

const AI_RESPONSES: Record<string, string> = {
  track:       "To track your shipment, tap the **Track** tab and enter your AWB number (e.g. LS-A1B2C3D4). You'll see a full timeline with driver details and ETA.",
  balikbayan:  "Balikbayan Box is our international freight service. Sea freight takes 30–45 days (most economical) and air freight takes 5–10 days. A receiver's passport copy is required.",
  cod:         "Cash on Delivery means the recipient pays when the package arrives. The driver collects the cash, and it's remitted to the merchant. COD is for local shipments only.",
  delay:       "If your package is delayed, please check the tracking timeline first. Common reasons include traffic, hub sorting backlogs, or failed delivery attempts. Contact us if it's been more than 2 extra days.",
  points:      "You earn 50 loyalty points per local booking and 150 for international. Redeem points for discounts. 1,000 pts = Platinum tier with perks like priority handling.",
  cancel:      "You can cancel within 1 hour of booking if the package hasn't been picked up. Go to History, tap the shipment, and choose Cancel. After pickup, contact support.",
  default:     "I'm LogisticOS AI Support! I can help you with tracking, booking, Balikbayan Box, COD, loyalty points, and delivery issues. Try asking me something specific.",
};

function getAiReply(msg: string): string {
  const m = msg.toLowerCase();
  if (m.includes("track") || m.includes("awb"))          return AI_RESPONSES.track;
  if (m.includes("balikbayan") || m.includes("international") || m.includes("overseas")) return AI_RESPONSES.balikbayan;
  if (m.includes("cod") || m.includes("cash"))           return AI_RESPONSES.cod;
  if (m.includes("delay") || m.includes("late"))         return AI_RESPONSES.delay;
  if (m.includes("point") || m.includes("loyalty"))      return AI_RESPONSES.points;
  if (m.includes("cancel"))                              return AI_RESPONSES.cancel;
  return AI_RESPONSES.default;
}

// ── Sub-components ────────────────────────────────────────────────────────────

function FaqCard({ item }: { item: FaqItem }) {
  const [open, setOpen] = useState(false);
  return (
    <Pressable onPress={() => setOpen(v => !v)} style={[s.faqCard, open && { borderColor: item.color + "40" }]}>
      <View style={s.faqHeader}>
        <View style={[s.faqIcon, { backgroundColor: item.color + "15" }]}>
          <Ionicons name={item.icon as any} size={16} color={item.color} />
        </View>
        <Text style={s.faqQ}>{item.q}</Text>
        <Ionicons name={open ? "chevron-up" : "chevron-down"} size={14} color="rgba(255,255,255,0.3)" />
      </View>
      {open && (
        <Animated.View entering={FadeIn.duration(200)} style={s.faqBody}>
          <Text style={s.faqA}>{item.a}</Text>
        </Animated.View>
      )}
    </Pressable>
  );
}

interface ChatMsg { role: "user" | "ai"; text: string; }

// ── Main screen ───────────────────────────────────────────────────────────────

export function SupportScreen() {
  const name = useSelector((s: RootState) => s.auth.name);

  const [tab,     setTab]     = useState<"faq" | "chat">("faq");
  const [input,   setInput]   = useState("");
  const [typing,  setTyping]  = useState(false);
  const [msgs,    setMsgs]    = useState<ChatMsg[]>([
    { role: "ai", text: `Hi ${name?.split(" ")[0] ?? "there"}! 👋 I'm the LogisticOS AI Support agent. How can I help you today?` },
  ]);
  const scrollRef = useRef<ScrollView>(null);

  function sendMessage() {
    const text = input.trim();
    if (!text) return;
    const updated: ChatMsg[] = [...msgs, { role: "user", text }];
    setMsgs(updated);
    setInput("");
    setTyping(true);
    setTimeout(() => {
      setMsgs(prev => [...prev, { role: "ai", text: getAiReply(text) }]);
      setTyping(false);
      setTimeout(() => scrollRef.current?.scrollToEnd({ animated: true }), 100);
    }, 900);
  }

  const QUICK_PROMPTS = [
    { label: "Track my parcel",    msg: "How do I track my shipment?" },
    { label: "Balikbayan Box",     msg: "Tell me about Balikbayan Box" },
    { label: "COD explained",      msg: "How does COD work?" },
    { label: "Loyalty points",     msg: "How do I earn loyalty points?" },
  ];

  return (
    <KeyboardAvoidingView
      style={{ flex: 1, backgroundColor: CANVAS }}
      behavior={Platform.OS === "ios" ? "padding" : undefined}
    >
      {/* Hero */}
      <LinearGradient colors={["rgba(255,171,0,0.09)", "transparent"]} style={s.hero}>
        <Animated.View entering={FadeInDown.springify()}>
          <Text style={s.heroTitle}>Help & Support</Text>
          <Text style={s.heroSub}>FAQ or chat with our AI agent</Text>
        </Animated.View>
      </LinearGradient>

      {/* Tab switcher */}
      <Animated.View entering={FadeInDown.delay(60).springify()} style={s.tabRow}>
        <Pressable onPress={() => setTab("faq")} style={[s.tabBtn, tab === "faq" && s.tabBtnActive]}>
          <Ionicons name="help-circle-outline" size={15} color={tab === "faq" ? AMBER : "rgba(255,255,255,0.35)"} />
          <Text style={[s.tabBtnText, { color: tab === "faq" ? AMBER : "rgba(255,255,255,0.35)" }]}>FAQ</Text>
        </Pressable>
        <Pressable onPress={() => setTab("chat")} style={[s.tabBtn, tab === "chat" && s.tabBtnActiveChat]}>
          <Ionicons name="chatbubble-ellipses-outline" size={15} color={tab === "chat" ? CYAN : "rgba(255,255,255,0.35)"} />
          <Text style={[s.tabBtnText, { color: tab === "chat" ? CYAN : "rgba(255,255,255,0.35)" }]}>AI Chat</Text>
          <View style={s.aiBadge}><Text style={s.aiBadgeText}>AI</Text></View>
        </Pressable>
      </Animated.View>

      {/* ── FAQ ── */}
      {tab === "faq" && (
        <ScrollView contentContainerStyle={{ paddingHorizontal: 16, paddingBottom: 40, gap: 8 }}>
          {/* Quick links */}
          <Animated.View entering={FadeInUp.delay(80).springify()} style={s.quickLinks}>
            {[
              { icon: "cube-outline",      label: "Track Parcel",      color: CYAN   },
              { icon: "alert-circle-outline", label: "Report Issue",   color: RED    },
              { icon: "refresh-circle-outline", label: "Reschedule",   color: GREEN  },
              { icon: "call-outline",       label: "Call Us",          color: AMBER  },
            ].map((q) => (
              <Pressable key={q.label} style={({ pressed }) => [s.quickLink, { opacity: pressed ? 0.7 : 1 }]}>
                <View style={[s.quickLinkIcon, { backgroundColor: q.color + "18" }]}>
                  <Ionicons name={q.icon as any} size={18} color={q.color} />
                </View>
                <Text style={s.quickLinkText}>{q.label}</Text>
              </Pressable>
            ))}
          </Animated.View>

          <Text style={s.sectionLabel}>Frequently Asked Questions</Text>
          {FAQS.map((faq, i) => (
            <Animated.View key={i} entering={FadeInUp.delay(i * 30).springify()}>
              <FaqCard item={faq} />
            </Animated.View>
          ))}

          {/* Contact strip */}
          <Animated.View entering={FadeInUp.delay(200).springify()} style={s.contactRow}>
            <View style={s.contactItem}>
              <Ionicons name="chatbubble-outline" size={18} color={CYAN} />
              <Text style={s.contactLabel}>Live Chat</Text>
              <Text style={s.contactSub}>Available 8AM–10PM</Text>
            </View>
            <View style={[s.contactDivider]} />
            <View style={s.contactItem}>
              <Ionicons name="mail-outline" size={18} color={PURPLE} />
              <Text style={s.contactLabel}>Email</Text>
              <Text style={s.contactSub}>support@logisticos.ph</Text>
            </View>
          </Animated.View>
        </ScrollView>
      )}

      {/* ── AI Chat ── */}
      {tab === "chat" && (
        <View style={{ flex: 1 }}>
          <ScrollView
            ref={scrollRef}
            contentContainerStyle={{ paddingHorizontal: 16, paddingVertical: 16, gap: 10 }}
            onContentSizeChange={() => scrollRef.current?.scrollToEnd({ animated: true })}
          >
            {msgs.map((m, i) => (
              <Animated.View
                key={i}
                entering={FadeIn.duration(250)}
                style={[s.msgRow, m.role === "user" ? s.msgRowUser : s.msgRowAi]}
              >
                {m.role === "ai" && (
                  <View style={s.aiBubbleIcon}>
                    <Ionicons name="logo-electron" size={14} color={CYAN} />
                  </View>
                )}
                {m.role === "user" ? (
                  <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }} style={[s.bubble, s.bubbleUser]}>
                    <Text style={[s.bubbleText, { color: CANVAS }]}>{m.text}</Text>
                  </LinearGradient>
                ) : (
                  <View style={[s.bubble, s.bubbleAi]}>
                    <Text style={s.bubbleText}>{m.text}</Text>
                  </View>
                )}
              </Animated.View>
            ))}
            {typing && (
              <Animated.View entering={FadeIn.duration(200)} style={[s.msgRow, s.msgRowAi]}>
                <View style={s.aiBubbleIcon}>
                  <Ionicons name="logo-electron" size={14} color={CYAN} />
                </View>
                <View style={[s.bubble, s.bubbleAi]}>
                  <Text style={s.typingDots}>• • •</Text>
                </View>
              </Animated.View>
            )}
          </ScrollView>

          {/* Quick prompt chips */}
          <ScrollView horizontal showsHorizontalScrollIndicator={false} contentContainerStyle={s.promptChips}>
            {QUICK_PROMPTS.map((p) => (
              <Pressable
                key={p.label}
                onPress={() => { setInput(p.msg); }}
                style={({ pressed }) => [s.promptChip, { opacity: pressed ? 0.7 : 1 }]}
              >
                <Text style={s.promptChipText}>{p.label}</Text>
              </Pressable>
            ))}
          </ScrollView>

          {/* Input bar */}
          <View style={s.inputBar}>
            <TextInput
              value={input}
              onChangeText={setInput}
              placeholder="Ask me anything..."
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.chatInput}
              returnKeyType="send"
              onSubmitEditing={sendMessage}
              multiline
            />
            <Pressable
              onPress={sendMessage}
              disabled={!input.trim() || typing}
              style={({ pressed }) => [s.sendBtn, { opacity: pressed || !input.trim() ? 0.5 : 1 }]}
            >
              <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 1 }} style={s.sendBtnGrad}>
                <Ionicons name="send" size={16} color={CANVAS} />
              </LinearGradient>
            </Pressable>
          </View>
        </View>
      )}
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  hero:           { paddingHorizontal: 20, paddingTop: 52, paddingBottom: 16 },
  heroTitle:      { fontSize: 26, fontWeight: "700", color: "#FFF", fontFamily: "SpaceGrotesk-Bold" },
  heroSub:        { fontSize: 13, color: "rgba(255,255,255,0.4)", marginTop: 4 },

  tabRow:         { flexDirection: "row", marginHorizontal: 16, marginBottom: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, padding: 4, gap: 4 },
  tabBtn:         { flex: 1, flexDirection: "row", alignItems: "center", justifyContent: "center", gap: 6, paddingVertical: 9, borderRadius: 8 },
  tabBtnActive:   { backgroundColor: "rgba(255,171,0,0.10)", borderWidth: 1, borderColor: "rgba(255,171,0,0.25)" },
  tabBtnActiveChat:{ backgroundColor: "rgba(0,229,255,0.08)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)" },
  tabBtnText:     { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold" },
  aiBadge:        { paddingHorizontal: 5, paddingVertical: 1, borderRadius: 4, backgroundColor: CYAN + "25" },
  aiBadgeText:    { fontSize: 8, fontFamily: "JetBrainsMono-Regular", color: CYAN, letterSpacing: 0.5 },

  quickLinks:     { flexDirection: "row", gap: 10, marginBottom: 8 },
  quickLink:      { flex: 1, alignItems: "center", gap: 6 },
  quickLinkIcon:  { width: 48, height: 48, borderRadius: 14, alignItems: "center", justifyContent: "center", borderWidth: 1, borderColor: BORDER },
  quickLinkText:  { fontSize: 10, color: "rgba(255,255,255,0.5)", fontFamily: "JetBrainsMono-Regular", textAlign: "center" },

  sectionLabel:   { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", textTransform: "uppercase", letterSpacing: 1, marginBottom: 4, marginTop: 4 },

  faqCard:        { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 14, padding: 14, gap: 0 },
  faqHeader:      { flexDirection: "row", alignItems: "center", gap: 10 },
  faqIcon:        { width: 32, height: 32, borderRadius: 9, alignItems: "center", justifyContent: "center" },
  faqQ:           { flex: 1, fontSize: 13, color: "#FFF", fontFamily: "SpaceGrotesk-SemiBold", lineHeight: 18 },
  faqBody:        { marginTop: 10, paddingTop: 10, borderTopWidth: 1, borderTopColor: BORDER },
  faqA:           { fontSize: 13, color: "rgba(255,255,255,0.5)", lineHeight: 20 },

  contactRow:     { flexDirection: "row", backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 14, marginTop: 8 },
  contactItem:    { flex: 1, alignItems: "center", gap: 4, padding: 16 },
  contactLabel:   { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold", color: "#FFF" },
  contactSub:     { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)" },
  contactDivider: { width: 1, backgroundColor: BORDER, marginVertical: 12 },

  msgRow:         { flexDirection: "row", alignItems: "flex-end", gap: 8 },
  msgRowUser:     { justifyContent: "flex-end" },
  msgRowAi:       { justifyContent: "flex-start" },
  aiBubbleIcon:   { width: 28, height: 28, borderRadius: 9, backgroundColor: CYAN + "15", alignItems: "center", justifyContent: "center", marginBottom: 2 },
  bubble:         { maxWidth: "78%", borderRadius: 16, paddingHorizontal: 14, paddingVertical: 10 },
  bubbleAi:       { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderBottomLeftRadius: 4 },
  bubbleUser:     { borderBottomRightRadius: 4 },
  bubbleText:     { fontSize: 13, color: "rgba(255,255,255,0.8)", lineHeight: 19 },
  typingDots:     { fontSize: 18, color: CYAN, letterSpacing: 3 },

  promptChips:    { paddingHorizontal: 16, paddingVertical: 8, gap: 8 },
  promptChip:     { paddingHorizontal: 12, paddingVertical: 6, borderRadius: 16, borderWidth: 1, borderColor: BORDER, backgroundColor: GLASS },
  promptChipText: { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.5)" },

  inputBar:       { flexDirection: "row", gap: 10, paddingHorizontal: 16, paddingVertical: 12, borderTopWidth: 1, borderTopColor: BORDER },
  chatInput:      { flex: 1, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingHorizontal: 14, paddingVertical: 10, fontSize: 13, color: "#FFF", fontFamily: "JetBrainsMono-Regular", maxHeight: 80 },
  sendBtn:        { alignSelf: "flex-end" },
  sendBtnGrad:    { width: 42, height: 42, borderRadius: 12, alignItems: "center", justifyContent: "center" },
});
