/**
 * Expo config plugin — Android Network Security Config
 *
 * Generates a network_security_config.xml that explicitly allows cleartext
 * (HTTP) traffic to the VPS IP used for preview/dev builds, while keeping
 * all other traffic HTTPS-only.
 *
 * This is necessary because:
 * - Android 9+ blocks cleartext HTTP by default.
 * - android:usesCleartextTraffic in AndroidManifest.xml is overridden when
 *   ANY network_security_config.xml is present (e.g. from expo-notifications).
 * - This plugin must be listed LAST in app.json plugins so it wins.
 */

const { withAndroidManifest, withDangerousMod } = require('@expo/config-plugins');
const path = require('path');
const fs = require('fs');

const NETWORK_SECURITY_XML = `<?xml version="1.0" encoding="utf-8"?>
<network-security-config>
    <!-- Allow cleartext HTTP to the dev/preview VPS and localhost.
         All other domains (including production api.cargomarket.net) stay HTTPS-only. -->
    <domain-config cleartextTrafficPermitted="true">
        <domain includeSubdomains="false">75.119.138.135</domain>
        <domain includeSubdomains="true">localhost</domain>
        <domain includeSubdomains="false">10.0.2.2</domain>
    </domain-config>
    <base-config cleartextTrafficPermitted="false">
        <trust-anchors>
            <certificates src="system" />
        </trust-anchors>
    </base-config>
</network-security-config>`;

/** Write the XML into res/xml and wire android:networkSecurityConfig on <application>. */
function withNetworkSecurityConfig(config) {
  // Step 1: write the XML file during the dangerous mod phase
  config = withDangerousMod(config, [
    'android',
    (modConfig) => {
      const xmlDir = path.join(
        modConfig.modRequest.platformProjectRoot,
        'app', 'src', 'main', 'res', 'xml'
      );
      fs.mkdirSync(xmlDir, { recursive: true });
      fs.writeFileSync(path.join(xmlDir, 'network_security_config.xml'), NETWORK_SECURITY_XML);
      return modConfig;
    },
  ]);

  // Step 2: point AndroidManifest.xml at the config file
  config = withAndroidManifest(config, (modConfig) => {
    const app = modConfig.modResults.manifest.application?.[0];
    if (app) {
      app.$['android:networkSecurityConfig'] = '@xml/network_security_config';
      // Remove the legacy attribute — it is ignored when networkSecurityConfig is set.
      delete app.$['android:usesCleartextTraffic'];
    }
    return modConfig;
  });

  return config;
}

module.exports = withNetworkSecurityConfig;
