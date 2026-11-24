import createNextIntlPlugin from "next-intl/plugin";

const withNextIntl = createNextIntlPlugin("./src/i18n/request.ts");

/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  basePath: "/partner",
  serverExternalPackages: ["firebase-admin"],
  images: {
    remotePatterns: [
      { protocol: "https", hostname: "**.amazonaws.com" },
    ],
  },
  typescript: { ignoreBuildErrors: true },
  eslint:     { ignoreDuringBuilds: true },
};
export default withNextIntl(nextConfig);
