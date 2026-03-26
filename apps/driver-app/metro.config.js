const { getDefaultConfig } = require("expo/metro-config");
const path = require("path");

const config = getDefaultConfig(__dirname);

config.resolver.platforms = ["ios", "android", "web"];

const webStub     = path.resolve(__dirname, "web-stub.js");
const webMapsStub = path.resolve(__dirname, "web-maps-stub.js");

/**
 * On web, redirect all react-native imports to react-native-web.
 *
 * - `require('react-native')` → react-native-web root
 * - `require('react-native/<subpath>')` → react-native-web/<subpath>
 *   If the subpath doesn't exist in react-native-web, return an empty stub
 *   so that native-only react-native internals (Renderer, ReactPrivate, etc.)
 *   are never bundled for web.
 * - `require('react-native-maps')` → no-op stub (native-only map library)
 */
config.resolver.resolveRequest = (context, moduleName, platform) => {
  if (platform === "web") {
    if (moduleName === "react-native") {
      return { type: "sourceFile", filePath: require.resolve("react-native-web") };
    }
    if (moduleName.startsWith("react-native/")) {
      const subpath = moduleName.slice("react-native/".length);
      try {
        return context.resolveRequest(context, `react-native-web/${subpath}`, platform);
      } catch {
        return { type: "sourceFile", filePath: webStub };
      }
    }
    // Stub out native-only map libraries
    if (moduleName === "react-native-maps" || moduleName.startsWith("react-native-maps/")) {
      return { type: "sourceFile", filePath: webMapsStub };
    }
  }
  return context.resolveRequest(context, moduleName, platform);
};

module.exports = config;
