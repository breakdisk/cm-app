/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  basePath: "/merchant",
  serverExternalPackages: ["firebase-admin"],
  transpilePackages: ["three"],
  images: {
    remotePatterns: [
      { protocol: "https", hostname: "**.amazonaws.com" },
    ],
  },
  typescript: { ignoreBuildErrors: true },
  eslint:     { ignoreDuringBuilds: true },
};
export default nextConfig;
