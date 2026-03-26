/**
 * Customer App — Onboarding Profile Screen
 * Step 2 of onboarding: name and email.
 */
import React, { useState } from "react";
import {
  View, Text, StyleSheet, TextInput, Pressable,
  KeyboardAvoidingView, Platform, ScrollView,
} from "react-native";
import Animated, { FadeInDown, FadeInUp } from "react-native-reanimated";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useDispatch, useSelector } from "react-redux";
import { authActions } from "../../store";
import type { RootState, AppDispatch } from "../../store";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const PURPLE = "#A855F7";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

export function OnboardingProfileScreen() {
  const dispatch = useDispatch<AppDispatch>();
  const phone    = useSelector((s: RootState) => s.auth.phone);

  const [name,  setName]  = useState("");
  const [email, setEmail] = useState("");
  const [error, setError] = useState("");

  function handleNext() {
    if (!name.trim()) { setError("Please enter your full name"); return; }
    setError("");
    dispatch(authActions.setProfile({
      name:       name.trim(),
      email:      email.trim() || undefined,
      customerId: "CUST-" + Math.random().toString(36).slice(2, 10).toUpperCase(),
    }));
  }

  return (
    <KeyboardAvoidingView
      style={{ flex: 1, backgroundColor: CANVAS }}
      behavior={Platform.OS === "ios" ? "padding" : undefined}
    >
      <ScrollView contentContainerStyle={{ flexGrow: 1 }} keyboardShouldPersistTaps="handled">

        <LinearGradient colors={["rgba(168,85,247,0.10)", "transparent"]} style={s.hero}>
          <Animated.View entering={FadeInDown.springify()}>
            {/* Progress */}
            <View style={s.progressRow}>
              {[1, 2, 3].map((n) => (
                <View key={n} style={[s.progressDot, n <= 2 ? s.progressActive : s.progressInactive]} />
              ))}
            </View>
            <Text style={s.heroTitle}>Your Profile</Text>
            <Text style={s.heroSub}>Verified number: {phone}</Text>
          </Animated.View>
        </LinearGradient>

        <Animated.View entering={FadeInUp.delay(100).springify()} style={s.card}>

          <Text style={s.label}>Full Name <Text style={s.required}>*</Text></Text>
          <View style={s.inputWrap}>
            <Ionicons name="person-outline" size={16} color="rgba(255,255,255,0.3)" />
            <TextInput
              value={name}
              onChangeText={(t) => { setName(t); setError(""); }}
              placeholder="e.g. Maria Santos"
              placeholderTextColor="rgba(255,255,255,0.2)"
              style={s.input}
              autoCapitalize="words"
            />
          </View>

          <Text style={s.label}>Email Address <Text style={s.optional}>(optional)</Text></Text>
          <View style={s.inputWrap}>
            <Ionicons name="mail-outline" size={16} color="rgba(255,255,255,0.3)" />
            <TextInput
              value={email}
              onChangeText={setEmail}
              placeholder="you@email.com"
              placeholderTextColor="rgba(255,255,255,0.2)"
              keyboardType="email-address"
              autoCapitalize="none"
              style={s.input}
            />
          </View>

          {error ? <Text style={s.error}>{error}</Text> : null}

          <View style={s.infoBox}>
            <Ionicons name="shield-checkmark-outline" size={15} color="rgba(0,229,255,0.6)" />
            <Text style={s.infoText}>
              Your information is encrypted and only used for shipment verification and support.
            </Text>
          </View>

          <Pressable
            onPress={handleNext}
            disabled={!name.trim()}
            style={({ pressed }) => [{ opacity: pressed || !name.trim() ? 0.5 : 1 }]}
          >
            <LinearGradient colors={[CYAN, PURPLE]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btn}>
              <Text style={s.btnText}>Continue →</Text>
            </LinearGradient>
          </Pressable>

        </Animated.View>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  hero:             { paddingHorizontal: 24, paddingTop: 64, paddingBottom: 24 },
  progressRow:      { flexDirection: "row", gap: 6, marginBottom: 24 },
  progressDot:      { flex: 1, height: 3, borderRadius: 2 },
  progressActive:   { backgroundColor: PURPLE },
  progressInactive: { backgroundColor: "rgba(255,255,255,0.08)" },
  heroTitle:        { fontSize: 28, fontFamily: "SpaceGrotesk-Bold", color: "#FFF", marginBottom: 6 },
  heroSub:          { fontSize: 13, color: "rgba(255,255,255,0.4)", fontFamily: "JetBrainsMono-Regular" },

  card:      { marginHorizontal: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: "rgba(255,255,255,0.08)", borderRadius: 20, padding: 24, gap: 14 },
  label:     { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.4)", textTransform: "uppercase", letterSpacing: 1 },
  required:  { color: "#FF3B5C" },
  optional:  { color: "rgba(255,255,255,0.25)", textTransform: "none", letterSpacing: 0 },
  inputWrap: { flexDirection: "row", alignItems: "center", gap: 10, backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingHorizontal: 14, paddingVertical: 13 },
  input:     { flex: 1, fontSize: 14, color: "#FFF", fontFamily: "JetBrainsMono-Regular" },

  infoBox:   { flexDirection: "row", alignItems: "flex-start", gap: 10, backgroundColor: "rgba(0,229,255,0.05)", borderWidth: 1, borderColor: "rgba(0,229,255,0.15)", borderRadius: 10, padding: 12 },
  infoText:  { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.4)", lineHeight: 18 },

  btn:       { borderRadius: 14, paddingVertical: 15, alignItems: "center" },
  btnText:   { fontSize: 15, fontFamily: "SpaceGrotesk-SemiBold", color: CANVAS },
  error:     { fontSize: 12, color: "#FF3B5C", fontFamily: "JetBrainsMono-Regular", textAlign: "center" },
});
