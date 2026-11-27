/**
 * Expo config. Builds one of two apps from this codebase:
 *
 * - default: the existing LogisticOS customer app (io.logisticos.customer).
 * - APP_VARIANT=move: "LogisticOS Move", the consumer moving app from the
 *   mobile design handoff, as its own package so it installs beside the
 *   existing app instead of over it.
 *
 * APP_VARIANT is read here at prebuild; the JS bundle reads
 * EXPO_PUBLIC_APP_VARIANT, which is baked in at build time. Both come from the
 * `move` profile in eas.json.
 */
module.exports = ({ config }) => {
  if (process.env.APP_VARIANT !== 'move') return config;
  return {
    ...config,
    name: 'LogisticOS Move',
    scheme: 'logisticos-move',
    ios: { ...config.ios, bundleIdentifier: 'io.logisticos.customer.move' },
    android: { ...config.android, package: 'io.logisticos.customer.move' },
    plugins: [
      ...(config.plugins ?? []),
      // Voice input on the Move home prompt. Move build only, so the default
      // app does not start asking for the microphone.
      [
        'expo-speech-recognition',
        {
          microphonePermission: 'Allow LogisticOS Move to use the microphone so you can say what needs moving.',
          speechRecognitionPermission: 'Allow LogisticOS Move to turn what you say into your move request.',
          androidSpeechServicePackages: ['com.google.android.googlequicksearchbox'],
        },
      ],
    ],
  };
};
