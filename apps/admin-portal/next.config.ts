import type { NextConfig } from "next";

const nextConfig: NextConfig = {
  transpilePackages: ["three"],
  images: { remotePatterns: [{ protocol: "https", hostname: "**.amazonaws.com" }] },
};

export default nextConfig;
