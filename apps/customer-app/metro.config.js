const { getDefaultConfig } = require("expo/metro-config");
const path = require("path");
const fs   = require("fs");

const config  = getDefaultConfig(__dirname);
const srcDir  = path.resolve(__dirname, "src");
const webStub     = path.resolve(__dirname, "web-stub.js");
const webMapsStub = path.resolve(__dirname, "web-maps-stub.js");

config.resolver.platforms = ["ios", "android", "web"];

config.resolver.resolveRequest = (context, moduleName, platform) => {
  // ── @/ path alias ──────────────────────────────────────────────────────────
  if (moduleName.startsWith("@/")) {
    const rel  = moduleName.slice(2);
    const base = path.resolve(srcDir, rel);
    const candidates = [
      base, base + ".ts", base + ".tsx", base + ".js",
      base + "/index.ts", base + "/index.tsx", base + "/index.js",
    ];
    for (const c of candidates) {
      if (fs.existsSync(c)) return { type: "sourceFile", filePath: c };
    }
  }

  // ── Web: redirect react-native → react-native-web ─────────────────────────
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
    if (moduleName === "react-native-maps" || moduleName.startsWith("react-native-maps/")) {
      return { type: "sourceFile", filePath: webMapsStub };
    }
  }

  return context.resolveRequest(context, moduleName, platform);
};

module.exports = config;
