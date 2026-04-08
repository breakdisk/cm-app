/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  basePath: "/customer",
  serverExternalPackages: ["firebase-admin"],
  images: {
    remotePatterns: [
      { protocol: "https", hostname: "**.amazonaws.com" },
    ],
  },
  typescript: { ignoreBuildErrors: true },
  eslint:     { ignoreDuringBuilds: true },
};
export default nextConfig;
