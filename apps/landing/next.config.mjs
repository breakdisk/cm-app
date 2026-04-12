/** @type {import('next').NextConfig} */
const nextConfig = {
  output: "standalone",
  serverExternalPackages: ["firebase-admin"],

  async rewrites() {
    const merchantUrl = process.env.MERCHANT_PORTAL_URL ?? "http://localhost:3000";
    const adminUrl    = process.env.ADMIN_PORTAL_URL    ?? "http://localhost:3001";
    const partnerUrl  = process.env.PARTNER_PORTAL_URL  ?? "http://localhost:3003";
    const customerUrl = process.env.CUSTOMER_PORTAL_URL ?? "http://localhost:3002";

    return [
      {
        source:      "/merchant/:path*",
        destination: `${merchantUrl}/merchant/:path*`,
      },
      {
        source:      "/admin/:path*",
        destination: `${adminUrl}/admin/:path*`,
      },
      {
        source:      "/partner/:path*",
        destination: `${partnerUrl}/partner/:path*`,
      },
      {
        source:      "/customer/:path*",
        destination: `${customerUrl}/customer/:path*`,
      },
    ];
  },
};

export default nextConfig;
