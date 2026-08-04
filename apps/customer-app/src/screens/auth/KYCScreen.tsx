/**
 * Customer App — KYC Identity Verification Screen
 * Step 3 of onboarding: select ID type, scan / upload a document image.
 *
 * Image pipeline (all on-device):
 *   1. Acquire  — native document scanner (VisionKit / ML Kit) with auto-crop
 *                 and perspective correction; falls back to camera / library picker.
 *   2. Grayscale — offscreen WebView canvas (GrayscaleProcessor) applying
 *                  ITU-R BT.601 luma; also pre-scales to ≤ 1280 px.
 *   3. Compress  — expo-image-manipulator: resize (if still oversized) + WebP 75 %
 *                  with JPEG fallback for devices without a WebP encoder.
 *
 * Result: base64 WebP/JPEG that is typically < 200 KB — well under the gateway's
 * 16 MB limit and the backend's 10 MB storage cap.
 */
import React, { useRef, useState } from "react";
import { useSafeAreaInsets } from 'react-native-safe-area-context';
import { FadeInView } from '../../components/FadeInView';
import GrayscaleProcessor, { GrayscaleProcessorRef } from '../../components/GrayscaleProcessor';
import { processKycImage, type KycContentType } from '../../utils/imageProcessing';
import {
  View, Text, StyleSheet, Pressable, Image,
  ScrollView, Platform, Alert, TextInput, ActivityIndicator,
} from "react-native";
import { LinearGradient } from "expo-linear-gradient";
import { Ionicons } from "@expo/vector-icons";
import * as ImagePicker from "expo-image-picker";
import { File, Paths } from "expo-file-system";
import { fetch as expoFetch } from "expo/fetch";
import { useDispatch, useSelector } from "react-redux";
import { authActions } from "../../store";
import type { RootState, AppDispatch, IdType } from "../../store";
import { complianceApi } from "../../services/api/compliance";
import { getStoredToken, logout } from "../../services/api/auth";
import DocumentScanner from "react-native-document-scanner-plugin";

const RED    = "#FF3B5C";
const CANVAS = "#050810";
const CYAN   = "#00E5FF";
const GREEN  = "#00FF88";
const AMBER  = "#FFAB00";
const PURPLE = "#A855F7";
const GLASS  = "rgba(255,255,255,0.04)";
const BORDER = "rgba(255,255,255,0.08)";

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

/**
 * Best-effort cleanup for an intermediate cache file.
 * `File.delete()` throws when the file was never written, so a missing file
 * must never surface as an error to the caller.
 */
function discardTempFile(file: File | null) {
  if (!file) return;
  try {
    if (file.exists) file.delete();
  } catch {
    // Non-fatal: the OS reclaims the cache directory anyway.
  }
}

export function KYCScreen() {
  const dispatch = useDispatch<AppDispatch>();
  const name     = useSelector((s: RootState) => s.auth.name);

  const [selectedId,    setSelectedId]    = useState<IdType | null>(null);
  const [imageUri,      setImageUri]      = useState<string | null>(null);
  const [imageBase64,   setImageBase64]   = useState<string | null>(null);
  const [imageMime,     setImageMime]     = useState<KycContentType>("image/jpeg");
  const [docNumber,     setDocNumber]     = useState<string>("");
  const [submitting,    setSubmitting]    = useState(false);
  const [isProcessing,  setIsProcessing]  = useState(false);
  const [isDemo,        setIsDemo]        = useState(false);

  const grayscaleRef = useRef<GrayscaleProcessorRef>(null);

  React.useEffect(() => {
    getStoredToken().then((t) => setIsDemo(!t));
  }, []);

  function clearImage() {
    setImageUri(null);
    setImageBase64(null);
  }

  /**
   * Shared post-acquisition pipeline:
   *   raw URI → grayscale (optional, non-fatal) → resize + WebP → state
   */
  async function processCapture(rawUri: string) {
    setIsProcessing(true);
    setImageUri(rawUri);    // show preview immediately
    setImageBase64(null);   // clear until pipeline finishes

    let processedUri = rawUri;
    let tmpFile: File | null = null;

    try {
      // ── Step 1: grayscale + pre-scale to ≤ 1280 px (WebView canvas) ────────
      if (grayscaleRef.current) {
        try {
          const grayB64 = await grayscaleRef.current.process(rawUri);
          tmpFile = new File(Paths.cache, `kyc_gray_${Date.now()}.jpg`);
          tmpFile.write(grayB64, { encoding: 'base64' });
          processedUri = tmpFile.uri;
        } catch (e) {
          // Non-fatal: proceed without grayscale
          console.warn('[KYC] Grayscale step failed, using original:', e);
        }
      }

      // ── Step 2: resize (guard) + encode to WebP 75 % ────────────────────────
      const { base64, contentType } = await processKycImage(processedUri);
      setImageBase64(base64);
      setImageMime(contentType);

    } catch {
      Alert.alert("Image Error", "Could not process the document. Please try again.");
      setImageUri(null);
    } finally {
      // Clean up the intermediate grayscale temp file
      discardTempFile(tmpFile);
      setIsProcessing(false);
    }
  }

  /**
   * "Take Photo" — tries the native document scanner first (edge detection +
   * perspective correction via VisionKit on iOS / ML Kit on Android).
   * Falls back to the plain camera picker if the scanner is unavailable or
   * the user cancels out of it.
   */
  async function takePhoto() {
    if (Platform.OS === "web") {
      // Web: no scanner, just use placeholder for demo
      setImageUri("https://via.placeholder.com/400x240/0A0F1E/00E5FF?text=ID+Document");
      setImageBase64("placeholder");
      return;
    }

    // ── Native document scanner ────────────────────────────────────────────
    try {
      const { scannedImages, status } = await DocumentScanner.scanDocument({
        maxNumDocuments: 1,
        croppedImageQuality: 90,
      });
      if (status === "cancel" || !scannedImages?.length) return;
      await processCapture(scannedImages[0]!);
      return;
    } catch {
      // Scanner not available (old device, missing Play Services, etc.)
      // → fall through to camera picker
    }

    // ── Fallback: camera picker ────────────────────────────────────────────
    try {
      const perm = await ImagePicker.requestCameraPermissionsAsync();
      if (!perm.granted) {
        Alert.alert("Permission needed", "Go to Settings and allow camera access.");
        return;
      }
      const result = await ImagePicker.launchCameraAsync({
        quality: 1,
        allowsEditing: true,
        aspect: [4, 3],
        base64: false,
      });
      if (!result.canceled && result.assets?.length) {
        await processCapture(result.assets[0]!.uri);
      }
    } catch {
      Alert.alert("Error", "Could not open camera. Please try again.");
    }
  }

  /** "Choose File" — photo library picker → same pipeline. */
  async function pickImage() {
    if (Platform.OS === "web") {
      setImageUri("https://via.placeholder.com/400x240/0A0F1E/00E5FF?text=ID+Document");
      setImageBase64("placeholder");
      return;
    }
    try {
      const perm = await ImagePicker.requestMediaLibraryPermissionsAsync();
      if (!perm.granted) {
        Alert.alert("Permission needed", "Go to Settings and allow photo library access.");
        return;
      }
      const result = await ImagePicker.launchImageLibraryAsync({
        mediaTypes: ['images'] as any,
        quality: 1,
        allowsEditing: true,
        aspect: [4, 3],
        base64: false,   // pipeline handles encoding
      });
      if (!result.canceled && result.assets?.length) {
        await processCapture(result.assets[0]!.uri);
      }
    } catch {
      Alert.alert("Error", "Could not open photo library. Please try again.");
    }
  }

  async function handleSubmit() {
    if (!selectedId || !imageUri || !imageBase64 || isProcessing || docNumber.trim().length === 0) return;

    setSubmitting(true);
    let tempUpload: File | null = null;
    try {
      const token = await getStoredToken();
      if (!token) {
        Alert.alert(
          "Session expired",
          "Your session has expired. Please log out and sign in again.",
          [
            { text: "Log out", style: "destructive", onPress: async () => { await logout(); dispatch(authActions.logout()); } },
            { text: "Cancel", style: "cancel" },
          ],
        );
        return;
      }

      // Step 1 — get a presigned R2 PUT URL from the compliance service
      const { upload_url, s3_key, upload_headers } = await complianceApi.getUploadUrl(imageMime);

      // Step 2 — write the processed base64 image to a temp file so the native
      // layer does the base64 decode, then upload the raw bytes directly to
      // Cloudflare R2 (no base64 body through the backend, no 33% bandwidth
      // overhead). `processKycImage` only ever emits WebP or JPEG.
      const ext = imageMime === 'image/webp' ? 'webp' : 'jpg';
      tempUpload = new File(Paths.cache, `kyc_r2_${Date.now()}.${ext}`);
      tempUpload.write(imageBase64, { encoding: 'base64' });

      // The body must be the raw bytes: anything that makes the request go out
      // as multipart/form-data mismatches the Content-Type signed into the
      // presigned URL and causes R2 to return SignatureDoesNotMatch. A
      // Uint8Array body is passed through verbatim — handing `expoFetch` the
      // File as a Blob would let it overwrite Content-Type with the file's own
      // sniffed mime type.
      const uploadResult = await expoFetch(upload_url, {
        method: 'PUT',
        body: await tempUpload.bytes(),
        headers: { 'Content-Type': imageMime, ...upload_headers },
      });

      if (uploadResult.status === 401) {
        throw Object.assign(new Error('Session expired'), { status: 401 });
      }
      if (uploadResult.status < 200 || uploadResult.status >= 300) {
        throw new Error(`Direct R2 upload failed with status ${uploadResult.status}`);
      }

      // Step 3 — register the uploaded document with the compliance service
      await complianceApi.confirmDocument({
        document_type_code: selectedId,
        document_number:    docNumber.trim(),
        s3_key,
        content_type:       imageMime,
      });

      dispatch(authActions.submitKyc({ idType: selectedId }));
    } catch (err: unknown) {
      const status = (err as { status?: number })?.status;
      if (status === 401) {
        Alert.alert(
          "Session expired",
          "Your session has expired. Please log out and sign in again.",
          [
            { text: "Log out", style: "destructive", onPress: async () => { await logout(); dispatch(authActions.logout()); } },
            { text: "Cancel", style: "cancel" },
          ],
        );
        return;
      }
      const msg = (err as { message?: string })?.message ?? "Please check your connection and try again.";
      Alert.alert("Upload failed", msg);
    } finally {
      discardTempFile(tempUpload);
      setSubmitting(false);
    }
  }

  const canSubmit = !isDemo && !!selectedId && !!imageUri && !!imageBase64 && !isProcessing && docNumber.trim().length > 0;

  return (
    <>
      {/* Off-screen grayscale processor — must stay mounted for the JS context */}
      <GrayscaleProcessor ref={grayscaleRef} />

      <ScrollView style={{ flex: 1, backgroundColor: CANVAS }} contentContainerStyle={{ paddingBottom: 48 }}>

        <LinearGradient colors={["rgba(0,255,136,0.08)", "transparent"]} style={s.hero}>
          <FadeInView fromY={-16}>
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

        {/* Demo-mode warning */}
        {isDemo && (
          <FadeInView fromY={-8} style={s.demoBanner}>
            <Ionicons name="warning-outline" size={16} color={RED} />
            <View style={{ flex: 1 }}>
              <Text style={s.demoBannerTitle}>Demo mode — upload disabled</Text>
              <Text style={s.demoBannerBody}>
                You signed in without connecting to the server. Log out and sign in again with your real phone number to upload documents.
              </Text>
            </View>
            <Pressable
              onPress={async () => { await logout(); dispatch(authActions.logout()); }}
              style={s.demoBannerBtn}
            >
              <Text style={s.demoBannerBtnText}>Log out</Text>
            </Pressable>
          </FadeInView>
        )}

        {/* ID type selector */}
        <FadeInView delay={80} fromY={16} style={s.section}>
          <Text style={s.sectionLabel}>Select ID Type</Text>
          <View style={s.idOptions}>
            {ID_OPTIONS.map((opt) => (
              <Pressable
                key={opt.type}
                onPress={() => { setSelectedId(opt.type); clearImage(); }}
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

        {/* Document number */}
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

            {imageUri ? (
              <FadeInView duration={200} style={s.previewWrap}>
                <Image source={{ uri: imageUri }} style={s.previewImage} resizeMode="cover" />
                {/* Processing overlay */}
                {isProcessing && (
                  <View style={s.processingOverlay}>
                    <ActivityIndicator color={CYAN} size="small" />
                    <Text style={s.processingText}>Optimising image…</Text>
                  </View>
                )}
                {!isProcessing && (
                  <Pressable onPress={clearImage} style={s.removeBtn}>
                    <Ionicons name="close-circle" size={22} color={RED} />
                  </Pressable>
                )}
                {!isProcessing && imageBase64 && (
                  <View style={s.previewCheck}>
                    <Ionicons name="checkmark-circle" size={18} color={GREEN} />
                    <Text style={s.previewCheckText}>Document ready</Text>
                  </View>
                )}
              </FadeInView>
            ) : (
              <View style={s.uploadZone}>
                <Ionicons name="scan-outline" size={32} color="rgba(255,255,255,0.2)" />
                <Text style={s.uploadTitle}>Scan or Upload ID Document</Text>
                <Text style={s.uploadSub}>Auto-crop · Grayscale · WebP · Max 10 MB</Text>
                <View style={s.uploadBtns}>
                  <Pressable onPress={takePhoto} style={s.uploadBtn}>
                    <Ionicons name="scan-outline" size={15} color={CYAN} />
                    <Text style={s.uploadBtnText}>Scan Document</Text>
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

        {/* Emirates ID notice */}
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
            onPress={() => dispatch(authActions.skipKyc())}
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
                {submitting ? "Submitting…" : isProcessing ? "Processing…" : "Submit for Verification"}
              </Text>
            </LinearGradient>
          </Pressable>
        </View>

      </ScrollView>
    </>
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

  previewWrap:      { borderRadius: 16, overflow: "hidden", position: "relative" },
  previewImage:     { width: "100%", height: 180, borderRadius: 16 },
  processingOverlay:{ position: "absolute", inset: 0, backgroundColor: "rgba(5,8,16,0.72)", alignItems: "center", justifyContent: "center", gap: 8, borderRadius: 16 },
  processingText:   { fontSize: 12, fontFamily: "JetBrainsMono-Regular", color: CYAN },
  removeBtn:        { position: "absolute", top: 8, right: 8 },
  previewCheck:     { flexDirection: "row", alignItems: "center", gap: 6, marginTop: 8, justifyContent: "center" },
  previewCheckText: { fontSize: 12, color: GREEN, fontFamily: "JetBrainsMono-Regular" },

  noteBox:         { flexDirection: "row", alignItems: "flex-start", gap: 10, backgroundColor: "rgba(255,171,0,0.07)", borderWidth: 1, borderColor: "rgba(255,171,0,0.2)", borderRadius: 12, padding: 12 },
  noteText:        { flex: 1, fontSize: 12, color: "rgba(255,171,0,0.7)", lineHeight: 18 },

  demoBanner:      { flexDirection: "row", alignItems: "flex-start", gap: 10, marginHorizontal: 16, marginTop: 12, backgroundColor: "rgba(255,59,92,0.10)", borderWidth: 1, borderColor: "rgba(255,59,92,0.35)", borderRadius: 14, padding: 14 },
  demoBannerTitle: { fontSize: 13, fontFamily: "SpaceGrotesk-SemiBold", color: RED, marginBottom: 3 },
  demoBannerBody:  { fontSize: 12, color: "rgba(255,255,255,0.55)", lineHeight: 17 },
  demoBannerBtn:   { paddingHorizontal: 12, paddingVertical: 7, borderRadius: 8, borderWidth: 1, borderColor: "rgba(255,59,92,0.5)", alignSelf: "flex-start", marginTop: 8 },
  demoBannerBtnText: { fontSize: 12, fontFamily: "SpaceGrotesk-SemiBold", color: RED },

  footerBtns:      { flexDirection: "row", gap: 10, marginHorizontal: 16, marginTop: 24 },
  skipBtn:         { paddingHorizontal: 18, paddingVertical: 15, borderRadius: 14, borderWidth: 1, borderColor: BORDER, alignItems: "center", justifyContent: "center" },
  skipText:        { fontSize: 13, color: "rgba(255,255,255,0.4)" },
  submitBtnWrap:   { borderRadius: 14, overflow: "hidden" },
  submitBtn:       { paddingVertical: 15, alignItems: "center" },
  submitBtnText:   { fontSize: 14, fontFamily: "SpaceGrotesk-SemiBold", color: CANVAS },
});
