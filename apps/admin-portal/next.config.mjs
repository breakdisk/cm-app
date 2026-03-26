/** @type {import('next').NextConfig} */
const nextConfig = {
  transpilePackages: ["three"],
  images: {
    remotePatterns: [
      { protocol: "https", hostname: "**.amazonaws.com" },
    ],
  },
};

export default nextConfig;
