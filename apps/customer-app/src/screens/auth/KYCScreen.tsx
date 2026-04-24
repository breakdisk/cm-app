/**
 * Customer App — KYC Identity Verification Screen
 * Step 3 of onboarding: select ID type (Passport / Emirates ID), upload front page.
 *
 * Local shipments: Passport OR Emirates ID accepted.
 * International shipments: Passport only (customs requirement).
 */
import React, { useState } from "react";
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FadeInView } from '../../components/FadeInView';
import {
  View, Text, StyleSheet, Pressable, Image,
  ScrollView, Platform, Alert, TextInput,
} from "react-native";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import * as ImagePicker from "expo-image-picker";
import * as FileSystem from "expo-file-system";
import { useDispatch, useSelector } from "react-redux";
import { authActions } from "../../store";
import type { RootState, AppDispatch, IdType } from "../../store";
import { complianceApi, type ContentType } from "../../services/api/compliance";

const CANVAS  = "#050810";
const CYAN    = "#00E5FF";
const GREEN   = "#00FF88";
const AMBER   = "#FFAB00";
const PURPLE  = "#A855F7";
const GLASS   = "rgba(255,255,255,0.04)";
const BORDER  = "rgba(255,255,255,0.08)";

/** Map an ImagePicker mimeType or file extension into one of the three
 *  content-types the compliance backend accepts. Default to JPEG for
 *  camera captures (the most common case). */
function inferMime(hint: string): ContentType {
  const h = hint.toLowerCase();
  if (h.includes("png"))  return "image/png";
  if (h.includes("pdf"))  return "application/pdf";
  return "image/jpeg";
}

const ID_OPTIONS: Array<{
  type:     IdType;
  label:    string;
  sublabel: string;
  icon:     string;
  color:    string;
  note:     string;
}> = [
  {
    type:     "passport",
    label:    "Passport",
    sublabel: "Philippine Passport",
    icon:     "book-outline",
    color:    CYAN,
    note:     "Valid for local & international shipments",
  },
  {
    type:     "emirates_id",
    label:    "Emirates ID",
    sublabel: "UAE Identity Card",
    icon:     "card-outline",
    color:    AMBER,
    note:     "Valid for local shipments only",
  },
];

export function KYCScreen() {
  const dispatch = useDispatch<AppDispatch>();
  const name     = useSelector((s: RootState) => s.auth.name);

  const [selectedId,  setSelectedId]  = useState<IdType | null>(null);
  const [imageUri,    setImageUri]    = useState<string | null>(null);
  const [imageMime,   setImageMime]   = useState<ContentType>("image/jpeg");
  const [docNumber,   setDocNumber]   = useState<string>("");
  const [submitting,  setSubmitting]  = useState(false);

  async function pickImage() {
    if (Platform.OS === "web") {
      setImageUri("https://via.placeholder.com/400x240/0A0F1E/00E5FF?text=ID+Document");
      return;
    }
    try {
      const permission = await ImagePicker.requestMediaLibraryPermissionsAsync();
      if (!permission.granted) {
        Alert.alert("Permission needed", "Go to Settings and allow photo library access.");
        return;
      }
      const result = await ImagePicker.launchImageLibraryAsync({
        mediaTypes: ['images'] as any,
        quality: 0.85,
        allowsEditing: true,
        aspect: [4, 3],
      });
      if (!result.canceled && result.assets.length > 0) {
        const asset = result.assets[0];
        setImageUri(asset.uri);
        setImageMime(inferMime(asset.mimeType ?? asset.uri));
      }
    } catch (err) {
      Alert.alert("Error", "Could not open photo library. Please try again.");
    }
  }

  async function takePhoto() {
    if (Platform.OS === "web") { pickImage(); return; }
    try {
      const permission = await ImagePicker.requestCameraPermissionsAsync();
      if (!permission.granted) {
        Alert.alert("Permission needed", "Go to Settings and allow camera access.");
        return;
      }
      const result = await ImagePicker.launchCameraAsync({
        quality: 0.85,
        allowsEditing: true,
        aspect: [4, 3],
      });
      if (!result.canceled && result.assets.length > 0) {
        const asset = result.assets[0];
        setImageUri(asset.uri);
        setImageMime(inferMime(asset.mimeType ?? asset.uri));
      }
    } catch (err) {
      Alert.alert("Error", "Could not open camera. Please try again.");
    }
  }

  async function handleSubmit() {
    if (!selectedId || !imageUri || docNumber.trim().length === 0) return;
    setSubmitting(true);
    try {
      // Read the selected image as base64. expo-file-system reads from the
      // device-local URI that ImagePicker returns; no extra permission needed.
      const fileBase64 = await FileSystem.readAsStringAsync(imageUri, {
        encoding: FileSystem.EncodingType.Base64,
      });

      await complianceApi.uploadDocument({
        document_type_code: selectedId,   // "passport" | "emirates_id"
        document_number:    docNumber.trim(),
        file_base64:        fileBase64,
        content_type:       imageMime,
      });

      // Only mark KYC as submitted after the upload succeeds.
      dispatch(authActions.submitKyc({ idType: selectedId }));
    } catch (err: unknown) {
      const msg = (err as { message?: string })?.message ?? "Please check your connection and try again.";
      Alert.alert("Upload failed", msg);
    } finally {
      setSubmitting(false);
    }
  }

  const canSubmit = !!selectedId && !!imageUri && docNumber.trim().length > 0;

  return (
    <ScrollView style={{ flex: 1, backgroundColor: CANVAS }} contentContainerStyle={{ paddingBottom: 48 }}>

      <LinearGradient colors={["rgba(0,255,136,0.08)", "transparent"]} style={s.hero}>
        <FadeInView fromY={-16}>
          {/* Progress */}
          <View style={s.progressRow}>
            {[1, 2, 3].map((n) => (
              <View key={n} style={[s.progressDot, { backgroundColor: n <= 3 ? GREEN : "rgba(255,255,255,0.08)" }]} />
            ))}
          </View>
          <Text style={s.heroTitle}>Verify Your Identity</Text>
          <Text style={s.heroSub}>
            Hi {name?.split(" ")[0] ?? "there"}, one last step before you can book shipments.
          </Text>
        </FadeInView>
      </LinearGradient>

      {/* ID type selector */}
      <FadeInView delay={80} fromY={16} style={s.section}>
        <Text style={s.sectionLabel}>Select ID Type</Text>
        <View style={s.idOptions}>
          {ID_OPTIONS.map((opt) => (
            <Pressable
              key={opt.type}
              onPress={() => { setSelectedId(opt.type); setImageUri(null); }}
              style={[
                s.idOption,
                selectedId === opt.type && { borderColor: opt.color + "60", backgroundColor: opt.color + "0D" },
              ]}
            >
              <View style={[s.idIconWrap, { backgroundColor: opt.color + "15" }]}>
                <Ionicons name={opt.icon as never} size={22} color={opt.color} />
              </View>
              <Text style={[s.idLabel, selectedId === opt.type && { color: opt.color }]}>{opt.label}</Text>
              <Text style={s.idSublabel}>{opt.sublabel}</Text>
              <Text style={[s.idNote, { color: opt.color + "80" }]}>{opt.note}</Text>
              {selectedId === opt.type && (
                <View style={[s.selectedCheck, { backgroundColor: opt.color }]}>
                  <Ionicons name="checkmark" size={11} color={CANVAS} />
                </View>
              )}
            </Pressable>
          ))}
        </View>
      </FadeInView>

      {/* Document number — free-text, validated server-side (1-100 chars) */}
      {selectedId && (
        <FadeInView duration={300} style={s.section}>
          <Text style={s.sectionLabel}>
            {selectedId === "passport" ? "Passport Number" : "Emirates ID Number"}
          </Text>
          <TextInput
            value={docNumber}
            onChangeText={setDocNumber}
            placeholder={selectedId === "passport" ? "e.g. P1234567A" : "784-XXXX-XXXXXXX-X"}
            placeholderTextColor="rgba(255,255,255,0.2)"
            autoCapitalize="characters"
            autoCorrect={false}
            maxLength={100}
            style={{
              backgroundColor: "rgba(255,255,255,0.03)",
              borderWidth: 1,
              borderColor: "rgba(255,255,255,0.08)",
              borderRadius: 12,
              paddingHorizontal: 14,
              paddingVertical: 12,
              color: "#FFF",
              fontFamily: "JetBrainsMono-Regular",
              fontSize: 14,
              marginHorizontal: 20,
            }}
          />
        </FadeInView>
      )}

      {/* Upload section */}
      {selectedId && (
        <FadeInView duration={300} style={s.section}>
          <Text style={s.sectionLabel}>
            Upload {selectedId === "passport" ? "Passport Bio-data Page" : "Emirates ID (Front)"}
          </Text>

          {/* Requirements */}
          <View style={s.requirementsBox}>
            {[
              "Clear photo — all text must be readable",
              "No blur, glare, or cut-off corners",
              selectedId === "passport" ? "Show the page with your photo & details" : "Show the front side with your photo",
              "Must not be expired",
            ].map((req, i) => (
              <View key={i} style={s.reqRow}>
                <Ionicons name="checkmark-circle-outline" size={13} color="rgba(0,255,136,0.6)" />
                <Text style={s.reqText}>{req}</Text>
              </View>
            ))}
          </View>

          {/* Image preview or upload zone */}
          {imageUri ? (
            <FadeInView duration={200} style={s.previewWrap}>
              <Image source={{ uri: imageUri }} style={s.previewImage} resizeMode="cover" />
              <Pressable onPress={() => setImageUri(null)} style={s.removeBtn}>
                <Ionicons name="close-circle" size={22} color="#FF3B5C" />
              </Pressable>
              <View style={s.previewCheck}>
                <Ionicons name="checkmark-circle" size={18} color={GREEN} />
                <Text style={s.previewCheckText}>Document uploaded</Text>
              </View>
            </FadeInView>
          ) : (
            <View style={s.uploadZone}>
              <Ionicons name="cloud-upload-outline" size={32} color="rgba(255,255,255,0.2)" />
              <Text style={s.uploadTitle}>Upload ID Document</Text>
              <Text style={s.uploadSub}>JPG or PNG · Max 10MB</Text>
              <View style={s.uploadBtns}>
                <Pressable onPress={takePhoto} style={s.uploadBtn}>
                  <Ionicons name="camera-outline" size={15} color={CYAN} />
                  <Text style={s.uploadBtnText}>Take Photo</Text>
                </Pressable>
                <Pressable onPress={pickImage} style={s.uploadBtn}>
                  <Ionicons name="images-outline" size={15} color={PURPLE} />
                  <Text style={[s.uploadBtnText, { color: PURPLE }]}>Choose File</Text>
                </Pressable>
              </View>
            </View>
          )}
        </FadeInView>
      )}

      {/* Emirates ID note */}
      {selectedId === "emirates_id" && (
        <FadeInView duration={200} style={[s.section, { paddingTop: 0 }]}>
          <View style={s.noteBox}>
            <Ionicons name="information-circle-outline" size={15} color={AMBER} />
            <Text style={s.noteText}>
              Emirates ID is accepted for local (domestic) shipments only. For international or Balikbayan Box shipping, a valid Passport is required.
            </Text>
          </View>
        </FadeInView>
      )}

      {/* Skip / Submit */}
      <View style={s.footerBtns}>
        <Pressable
          onPress={() => dispatch(authActions.submitKyc({ idType: selectedId ?? "passport" }))}
          style={s.skipBtn}
        >
          <Text style={s.skipText}>Skip for now</Text>
        </Pressable>

        <Pressable
          onPress={handleSubmit}
          disabled={!canSubmit || submitting}
          style={[s.submitBtnWrap, { flex: 1, opacity: canSubmit && !submitting ? 1 : 0.4 }]}
        >
          <LinearGradient colors={[GREEN, CYAN]} start={{ x: 0, y: 0 }} end={{ x: 1, y: 0 }} style={s.submitBtn}>
            <Text style={s.submitBtnText}>
              {submitting ? "Submitting…" : "Submit for Verification"}
            </Text>
          </LinearGradient>
        </Pressable>
      </View>

    </ScrollView>
  );
}

const s = StyleSheet.create({
  hero:            { paddingHorizontal: 24, paddingTop: 56, paddingBottom: 24 },
  progressRow:     { flexDirection: "row", gap: 6, marginBottom: 20 },
  progressDot:     { flex: 1, height: 3, borderRadius: 2 },
  heroTitle:       { fontSize: 26, fontFamily: "SpaceGrotesk-Bold", color: "#FFF", marginBottom: 6 },
  heroSub:         { fontSize: 14, color: "rgba(255,255,255,0.4)", lineHeight: 22 },

  section:         { paddingHorizontal: 16, paddingTop: 16, gap: 12 },
  sectionLabel:    { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.4)", textTransform: "uppercase", letterSpacing: 1, marginBottom: 2 },

  idOptions:       { flexDirection: "row", gap: 10 },
  idOption:        { flex: 1, backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, padding: 14, alignItems: "center", gap: 6, position: "relative" },
  idIconWrap:      { width: 44, height: 44, borderRadius: 12, alignItems: "center", justifyContent: "center", marginBottom: 2 },
  idLabel:         { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: "#FFF" },
  idSublabel:      { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.35)", textAlign: "center" },
  idNote:          { fontSize: 9, fontFamily: "JetBrainsMono-Regular", textAlign: "center", lineHeight: 14 },
  selectedCheck:   { position: "absolute", top: 10, right: 10, width: 18, height: 18, borderRadius: 9, alignItems: "center", justifyContent: "center" },

  requirementsBox: { backgroundColor: "rgba(0,255,136,0.04)", borderWidth: 1, borderColor: "rgba(0,255,136,0.12)", borderRadius: 12, padding: 12, gap: 8 },
  reqRow:          { flexDirection: "row", alignItems: "flex-start", gap: 8 },
  reqText:         { flex: 1, fontSize: 12, color: "rgba(255,255,255,0.5)", lineHeight: 18 },

  uploadZone:      { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 16, borderStyle: "dashed", paddingVertical: 32, alignItems: "center", gap: 8 },
  uploadTitle:     { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: "rgba(255,255,255,0.6)" },
  uploadSub:       { fontSize: 11, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.25)" },
  uploadBtns:      { flexDirection: "row", gap: 12, marginTop: 8 },
  uploadBtn:       { flexDirection: "row", alignItems: "center", gap: 6, backgroundColor: "rgba(255,255,255,0.05)", borderWidth: 1, borderColor: BORDER, borderRadius: 10, paddingHorizontal: 16, paddingVertical: 9 },
  uploadBtnText:   { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold", color: CYAN },

  previewWrap:     { borderRadius: 16, overflow: "hidden", position: "relative" },
  previewImage:    { width: "100%", height: 180, borderRadius: 16 },
  removeBtn:       { position: "absolute", top: 8, right: 8 },
  previewCheck:    { flexDirection: "row", alignItems: "center", gap: 6, marginTop: 8, justifyContent: "center" },
  previewCheckText:{ fontSize: 12, color: GREEN, fontFamily: "JetBrainsMono-Regular" },

  noteBox:         { flexDirection: "row", alignItems: "flex-start", gap: 10, backgroundColor: "rgba(255,171,0,0.07)", borderWidth: 1, borderColor: "rgba(255,171,0,0.2)", borderRadius: 12, padding: 12 },
  noteText:        { flex: 1, fontSize: 12, color: "rgba(255,171,0,0.7)", lineHeight: 18 },

  footerBtns:      { flexDirection: "row", gap: 10, marginHorizontal: 16, marginTop: 24 },
  skipBtn:         { paddingHorizontal: 18, paddingVertical: 15, borderRadius: 14, borderWidth: 1, borderColor: BORDER, alignItems: "center", justifyContent: "center" },
  skipText:        { fontSize: 13, color: "rgba(255,255,255,0.4)" },
  submitBtnWrap:   { borderRadius: 14, overflow: "hidden" },
  submitBtn:       { paddingVertical: 15, alignItems: "center" },
  submitBtnText:   { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: CANVAS },
});
