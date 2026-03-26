/**
 * Driver App — Document Upload Screen
 * Allows driver to submit document number + expiry date for a required doc type.
 */
import { View, Text, StyleSheet, Pressable, TextInput, ScrollView, Image, Platform } from "react-native";
import { useLocalSearchParams, router } from "expo-router";
import { useState } from "react";
import { useDispatch, useSelector } from "react-redux";
import Animated, { FadeInDown } from "react-native-reanimated";
import { Ionicons } from "@expo/vector-icons";
import * as ImagePicker from "expo-image-picker";
import type { RootState, AppDispatch } from "../../../store";
import { complianceActions } from "../../../store";
import type { SubmittedDoc } from "../../../store";

// Color tokens
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const PURPLE = "#A855F7";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

export default function UploadDocumentScreen() {
  const { typeCode } = useLocalSearchParams<{ typeCode: string }>();
  const dispatch     = useDispatch<AppDispatch>();
  const compliance   = useSelector((s: RootState) => s.compliance);
  const docType      = compliance.required_types.find((dt) => dt.code === typeCode);

  const [docNumber,  setDocNumber]  = useState("");
  const [expiryDate, setExpiryDate] = useState("");
  const [submitted,  setSubmitted]  = useState(false);
  const [photoUri,   setPhotoUri]   = useState<string | null>(null);

  async function handleCamera() {
    if (Platform.OS === "web") {
      // On web: use file picker (camera capture not universally available in desktop browsers)
      const result = await ImagePicker.launchImageLibraryAsync({
        mediaTypes: ImagePicker.MediaTypeOptions.Images,
        quality: 0.8,
        allowsEditing: false,
      });
      if (!result.canceled && result.assets[0]) {
        setPhotoUri(result.assets[0].uri);
      }
    } else {
      const { status } = await ImagePicker.requestCameraPermissionsAsync();
      if (status !== "granted") return;
      const result = await ImagePicker.launchCameraAsync({
        quality: 0.8,
        allowsEditing: true,
        aspect: [4, 3],
      });
      if (!result.canceled && result.assets[0]) {
        setPhotoUri(result.assets[0].uri);
      }
    }
  }

  function handleSubmit() {
    if (!docNumber.trim() || !docType) return;
    const newDoc: SubmittedDoc = {
      id:               `doc-${Date.now()}`,
      document_type_id: docType.id,
      document_number:  docNumber.trim(),
      expiry_date:      expiryDate || null,
      status:           "submitted",
      rejection_reason: null,
      submitted_at:     new Date().toISOString(),
    };
    dispatch(complianceActions.upsertDocument(newDoc));
    setSubmitted(true);
  }

  if (!docType) return null;

  if (submitted) {
    return (
      <View style={styles.container}>
        <Animated.View entering={FadeInDown.springify()} style={styles.successCard}>
          <View style={styles.successIcon}>
            <Ionicons name="search" size={32} color={CYAN} />
          </View>
          <Text style={styles.successTitle}>Under Review</Text>
          <Text style={styles.successSub}>
            Your {docType.name} has been received.{"\n"}
            Our compliance team will verify it within 24 hours.
          </Text>
          <Pressable onPress={() => router.back()} style={styles.backBtn}>
            <Text style={styles.backBtnText}>← Back to Profile</Text>
          </Pressable>
        </Animated.View>
      </View>
    );
  }

  return (
    <ScrollView style={styles.container} contentContainerStyle={{ paddingBottom: 40 }}>
      {/* Header */}
      <Animated.View entering={FadeInDown.springify()} style={styles.header}>
        <Pressable onPress={() => router.back()}>
          <Ionicons name="chevron-back" size={20} color="rgba(255,255,255,0.5)" />
        </Pressable>
        <View style={{ flex: 1, marginLeft: 8 }}>
          <Text style={styles.headerTitle}>{docType.name}</Text>
          <Text style={styles.headerSub}>
            Required · {docType.has_expiry ? "Has expiry" : "No expiry"}
          </Text>
        </View>
      </Animated.View>

      {/* Camera / upload area */}
      <Animated.View entering={FadeInDown.delay(60).springify()}>
        <Pressable onPress={handleCamera} style={styles.cameraArea}>
          {photoUri ? (
            <Image source={{ uri: photoUri }} style={styles.photoPreview} resizeMode="cover" />
          ) : (
            <>
              <Ionicons name="camera-outline" size={36} color="rgba(255,255,255,0.2)" />
              <Text style={styles.cameraHint}>
                {Platform.OS === "web" ? "Tap to select document photo" : "Tap to photograph your document"}
              </Text>
              <View style={styles.cameraBtn}>
                <Text style={styles.cameraBtnText}>
                  {Platform.OS === "web" ? "Choose File" : "Open Camera"}
                </Text>
              </View>
            </>
          )}
        </Pressable>
        {photoUri && (
          <Pressable onPress={handleCamera} style={styles.retakeBtn}>
            <Text style={styles.retakeBtnText}>↺ Retake / Replace</Text>
          </Pressable>
        )}
      </Animated.View>

      {/* Document number */}
      <Animated.View entering={FadeInDown.delay(100).springify()} style={styles.field}>
        <Text style={styles.fieldLabel}>Document Number</Text>
        <TextInput
          value={docNumber}
          onChangeText={setDocNumber}
          placeholder="Enter document number"
          placeholderTextColor="rgba(255,255,255,0.2)"
          style={[styles.fieldInput, docNumber ? styles.fieldInputFilled : null]}
        />
      </Animated.View>

      {/* Expiry date */}
      {docType.has_expiry && (
        <Animated.View entering={FadeInDown.delay(140).springify()} style={styles.field}>
          <Text style={styles.fieldLabel}>Expiry Date</Text>
          <TextInput
            value={expiryDate}
            onChangeText={setExpiryDate}
            placeholder="YYYY-MM-DD"
            placeholderTextColor="rgba(255,255,255,0.2)"
            style={[styles.fieldInput, expiryDate ? styles.fieldInputFilled : null]}
          />
        </Animated.View>
      )}

      {/* Submit */}
      <Animated.View entering={FadeInDown.delay(180).springify()} style={{ marginHorizontal: 12 }}>
        <Pressable
          onPress={handleSubmit}
          disabled={!docNumber.trim()}
          style={({ pressed }) => [styles.submitBtn, { opacity: pressed || !docNumber.trim() ? 0.6 : 1 }]}
        >
          <Text style={styles.submitBtnText}>Submit for Review →</Text>
        </Pressable>
      </Animated.View>
    </ScrollView>
  );
}

const styles = StyleSheet.create({
  container:        { flex: 1, backgroundColor: CANVAS },
  header:           { flexDirection: "row", alignItems: "center", padding: 16, paddingTop: 20 },
  headerTitle:      { fontSize: 16, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff" },
  headerSub:        { fontSize: 10, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.3)", marginTop: 2 },
  cameraArea:       { margin: 12, borderRadius: 12, height: 160, backgroundColor: "rgba(255,255,255,0.03)", borderWidth: 1.5, borderColor: "rgba(255,255,255,0.1)", borderStyle: "dashed", alignItems: "center", justifyContent: "center", gap: 8, overflow: "hidden" },
  cameraHint:       { fontSize: 11, color: "rgba(255,255,255,0.2)" },
  cameraBtn:        { paddingHorizontal: 16, paddingVertical: 6, borderRadius: 20, backgroundColor: "rgba(168,85,247,0.12)", borderWidth: 1, borderColor: "rgba(168,85,247,0.3)" },
  cameraBtnText:    { fontSize: 11, color: PURPLE },
  photoPreview:     { width: "100%", height: "100%" },
  retakeBtn:        { marginHorizontal: 12, marginTop: 6, alignItems: "center" },
  retakeBtnText:    { fontSize: 11, color: "rgba(255,255,255,0.3)", fontFamily: "JetBrainsMono-Regular" },
  field:            { marginHorizontal: 12, marginBottom: 10 },
  fieldLabel:       { fontSize: 9, fontFamily: "JetBrainsMono-Regular", textTransform: "uppercase", letterSpacing: 1, color: "rgba(255,255,255,0.3)", marginBottom: 6 },
  fieldInput:       { backgroundColor: GLASS, borderWidth: 1, borderColor: BORDER, borderRadius: 8, padding: 10, fontSize: 13, fontFamily: "JetBrainsMono-Regular", color: "rgba(255,255,255,0.6)" },
  fieldInputFilled: { borderColor: "rgba(0,229,255,0.3)", color: CYAN, backgroundColor: "rgba(0,229,255,0.04)" },
  submitBtn:        { borderRadius: 12, paddingVertical: 14, alignItems: "center", backgroundColor: "rgba(168,85,247,0.18)", borderWidth: 1, borderColor: "rgba(168,85,247,0.35)" },
  submitBtnText:    { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: "#fff" },
  successCard:      { margin: 16, borderRadius: 14, backgroundColor: "rgba(0,229,255,0.06)", borderWidth: 1, borderColor: "rgba(0,229,255,0.2)", padding: 24, alignItems: "center", marginTop: 60 },
  successIcon:      { width: 60, height: 60, borderRadius: 30, backgroundColor: "rgba(0,229,255,0.1)", alignItems: "center", justifyContent: "center", marginBottom: 14 },
  successTitle:     { fontSize: 18, fontFamily: "SpaceGrotesk-SemiBold", color: CYAN, marginBottom: 8 },
  successSub:       { fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: "rgba(0,229,255,0.5)", textAlign: "center", lineHeight: 18 },
  backBtn:          { marginTop: 20, padding: 12 },
  backBtnText:      { fontSize: 13, color: "rgba(255,255,255,0.4)" },
});
