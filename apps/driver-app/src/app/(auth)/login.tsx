/**
 * Driver App — Login Screen
 * Glassmorphism dark design matching the app's visual language.
 */
import { View, Text, TextInput, Pressable, StyleSheet, ActivityIndicator, Alert } from "react-native";
import { router } from "expo-router";
import { useState } from "react";
import { useDispatch } from "react-redux";
import Animated, { FadeInDown } from "react-native-reanimated";

import { IDENTITY_URL } from "../../services/api/client";
import { tokenStore }   from "../../services/auth/token-store";
import { tokenRef }     from "../_layout";
import { authActions }  from "../../store";
import type { AppDispatch } from "../../store";

const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const PURPLE = "#A855F7";

export default function LoginScreen() {
  const dispatch = useDispatch<AppDispatch>();
  const [email,      setEmail]      = useState("");
  const [password,   setPassword]   = useState("");
  const [tenantSlug, setTenantSlug] = useState("demo");
  const [loading,    setLoading]    = useState(false);

  async function handleLogin() {
    if (!email.trim() || !password.trim()) return;
    setLoading(true);
    try {
      const res = await fetch(`${IDENTITY_URL}/v1/auth/login`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ email: email.trim(), password, tenant_slug: tenantSlug.trim() }),
      });
      const json = await res.json();
      if (!res.ok || !json.data?.access_token) {
        throw new Error(json.error?.message ?? "Login failed");
      }
      const { access_token, refresh_token } = json.data;

      // Persist tokens
      try {
        await tokenStore.setTokens(access_token, refresh_token ?? "");
      } catch {
        // SecureStore unavailable on web — skip persistence
      }
      tokenRef.current = access_token;

      dispatch(authActions.setCredentials({
        token:    access_token,
        driverId: "",
        name:     email.trim(),
      }));

      router.replace("/(tabs)");
    } catch (err: unknown) {
      Alert.alert("Login Failed", err instanceof Error ? err.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  }

  return (
    <View style={s.container}>
      <Animated.View entering={FadeInDown.springify()} style={s.card}>
        <Text style={s.logo}>◈ LogisticOS</Text>
        <Text style={s.title}>Driver Login</Text>
        <Text style={s.sub}>DRIVER SUPER APP</Text>

        <TextInput
          style={s.input}
          placeholder="Tenant (e.g. demo)"
          placeholderTextColor="rgba(255,255,255,0.2)"
          value={tenantSlug}
          onChangeText={setTenantSlug}
          autoCapitalize="none"
        />
        <TextInput
          style={s.input}
          placeholder="Email"
          placeholderTextColor="rgba(255,255,255,0.2)"
          value={email}
          onChangeText={setEmail}
          autoCapitalize="none"
          keyboardType="email-address"
        />
        <TextInput
          style={s.input}
          placeholder="Password"
          placeholderTextColor="rgba(255,255,255,0.2)"
          value={password}
          onChangeText={setPassword}
          secureTextEntry
        />

        <Pressable
          onPress={handleLogin}
          disabled={loading}
          style={({ pressed }) => [s.btn, { opacity: pressed || loading ? 0.6 : 1 }]}
        >
          {loading
            ? <ActivityIndicator color={CYAN} />
            : <Text style={s.btnText}>Sign In →</Text>}
        </Pressable>
      </Animated.View>
    </View>
  );
}

const s = StyleSheet.create({
  container: { flex: 1, backgroundColor: CANVAS, justifyContent: "center", padding: 20 },
  card: {
    backgroundColor: "rgba(255,255,255,0.04)",
    borderRadius: 16, padding: 24,
    borderWidth: 1, borderColor: "rgba(0,229,255,0.15)",
  },
  logo:    { fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: CYAN, marginBottom: 16, letterSpacing: 2 },
  title:   { fontSize: 24, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff", marginBottom: 2 },
  sub:     { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)", letterSpacing: 2, marginBottom: 24 },
  input: {
    backgroundColor: "rgba(255,255,255,0.04)",
    borderWidth: 1, borderColor: "rgba(255,255,255,0.08)",
    borderRadius: 8, padding: 12, marginBottom: 12,
    fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.8)",
  },
  btn: {
    borderRadius: 12, paddingVertical: 14, alignItems: "center",
    backgroundColor: `${PURPLE}20`, borderWidth: 1, borderColor: `${PURPLE}50`,
    marginTop: 4,
  },
  btnText: { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff" },
});
