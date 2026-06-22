/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  basePath: "/admin",
  serverExternalPackages: ["firebase-admin"],
  transpilePackages: ["three"],
  images: {
    remotePatterns: [
      { protocol: "https", hostname: "**.amazonaws.com" },
      { protocol: "https", hostname: "**.r2.cloudflarestorage.com" },
    ],
  },
  typescript: { ignoreBuildErrors: true },
  eslint:     { ignoreDuringBuilds: true },
};
export default nextConfig;
