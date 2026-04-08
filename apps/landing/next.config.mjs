/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  serverExternalPackages: ["firebase-admin"],
  async rewrites() {
    return [
      {
        source: "/merchant/:path*",
        destination: `${process.env.MERCHANT_PORTAL_URL}/merchant/:path*`,
      },
      {
        source: "/admin/:path*",
        destination: `${process.env.ADMIN_PORTAL_URL}/admin/:path*`,
      },
      {
        source: "/customer/:path*",
        destination: `${process.env.CUSTOMER_PORTAL_URL}/customer/:path*`,
      },
      {
        source: "/partner/:path*",
        destination: `${process.env.PARTNER_PORTAL_URL}/partner/:path*`,
      },
    ];
  },
};
export default nextConfig;
