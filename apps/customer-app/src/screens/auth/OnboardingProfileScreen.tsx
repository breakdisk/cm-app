/**
 * Customer App — Onboarding Profile Screen
 * Step 2 of onboarding: name and email.
 */
import React, { useState } from "react";
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FadeInView } from '../../components/FadeInView';
import {
  View, Text, StyleSheet, TextInput, Pressable,
  KeyboardAvoidingView, Platform, ScrollView,
} from "react-native";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import { useDispatch, useSelector } from "react-redux";
import * as SecureStore from "expo-secure-store";
import { authActions } from "../../store";
import type { RootState, AppDispatch } from "../../store";
import { A } from "./authTheme";

const CANVAS = A.canvas;
const GLASS  = A.glass;
const BORDER = A.border;

export function OnboardingProfileScreen() {
  const dispatch = useDispatch<AppDispatch>();
  const phone    = useSelector((s: RootState) => s.auth.phone);

  const [name,  setName]  = useState("");
  const [email, setEmail] = useState("");
  const [error, setError] = useState("");

  function handleNext() {
    if (!name.trim()) { setError("Please enter your full name"); return; }
    setError("");
    const trimmedName = name.trim();
    // Persist name so session restore in App.tsx can display it on cold start
    SecureStore.setItemAsync("customer_name", trimmedName).catch(() => {});
    dispatch(authActions.setProfile({
      name:       trimmedName,
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

        <LinearGradient colors={A.wash.profile} style={s.hero}>
          <FadeInView fromY={-16}>
            {/* Progress */}
            <View style={s.progressRow}>
              {[1, 2, 3].map((n) => (
                <View key={n} style={[s.progressDot, n <= 2 ? s.progressActive : s.progressInactive]} />
              ))}
            </View>
            <Text style={s.heroTitle}>Your Profile</Text>
            <Text style={s.heroSub}>Verified number: {phone}</Text>
          </FadeInView>
        </LinearGradient>

        <FadeInView delay={100} fromY={16} style={s.card}>

          <Text style={s.label}>Full Name <Text style={s.required}>*</Text></Text>
          <View style={s.inputWrap}>
            <Ionicons name="person-outline" size={16} color={A.faint} />
            <TextInput
              value={name}
              onChangeText={(t) => { setName(t); setError(""); }}
              placeholder="e.g. Maria Santos"
              placeholderTextColor={A.placeholder}
              style={s.input}
              autoCapitalize="words"
            />
          </View>

          <Text style={s.label}>Email Address <Text style={s.optional}>(optional)</Text></Text>
          <View style={s.inputWrap}>
            <Ionicons name="mail-outline" size={16} color={A.faint} />
            <TextInput
              value={email}
              onChangeText={setEmail}
              placeholder="you@email.com"
              placeholderTextColor={A.placeholder}
              keyboardType="email-address"
              autoCapitalize="none"
              style={s.input}
            />
          </View>

          {error ? <Text style={s.error}>{error}</Text> : null}

          <View style={s.infoBox}>
            <Ionicons name="shield-checkmark-outline" size={15} color={A.accent} />
            <Text style={s.infoText}>
              Your information is encrypted and only used for shipment verification and support.
            </Text>
          </View>

          <Pressable
            onPress={handleNext}
            disabled={!name.trim()}
            style={({ pressed }) => [{ opacity: pressed || !name.trim() ? 0.5 : 1 }]}
          >
            <LinearGradient colors={A.primaryGrad} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.btn}>
              <Text style={s.btnText}>Continue →</Text>
            </LinearGradient>
          </Pressable>

        </FadeInView>
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

const s = StyleSheet.create({
  hero:             { paddingHorizontal: 24, paddingTop: 64, paddingBottom: 24 },
  progressRow:      { flexDirection: "row", gap: 6, marginBottom: 24 },
  progressDot:      { flex: 1, height: 3, borderRadius: 2 },
  progressActive:   { backgroundColor: A.progress.profile },
  progressInactive: { backgroundColor: BORDER },
  heroTitle:        { fontSize: 28, ...A.heading, color: A.ink, marginBottom: 6 },
  heroSub:          { fontSize: 13, color: A.muted, ...A.mono },

  card:      { marginHorizontal: 16, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 20, padding: 24, gap: 14 },
  label:     { fontSize: 11, ...A.mono, color: A.muted, textTransform: "uppercase", letterSpacing: 1 },
  required:  { color: A.danger },
  optional:  { color: A.faint, textTransform: "none", letterSpacing: 0 },
  inputWrap: { flexDirection: "row", alignItems: "center", gap: 10, backgroundColor: A.inputBg, borderWidth: 1, borderColor: BORDER, borderRadius: 12, paddingHorizontal: 14, paddingVertical: 13 },
  input:     { flex: 1, fontSize: 14, color: A.ink, ...A.mono },

  infoBox:   { flexDirection: "row", alignItems: "flex-start", gap: 10, backgroundColor: "rgba(0,229,255,0.05)", borderWidth: 1, borderColor: "rgba(0,229,255,0.15)", borderRadius: 10, padding: 12 },
  infoText:  { flex: 1, fontSize: 12, color: A.muted, lineHeight: 18 },

  btn:       A.button,
  btnText:   { fontSize: 15, ...A.buttonText },
  error:     { fontSize: 12, color: A.danger, ...A.mono, textAlign: "center" },
});
