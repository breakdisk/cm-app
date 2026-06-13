import type { MetadataRoute } from "next";

const BASE_URL = "https://cargomarket.net";

export default function sitemap(): MetadataRoute.Sitemap {
  return [
    {
      url: BASE_URL,
      lastModified: new Date(),
      changeFrequency: "weekly",
      priority: 1,
    },
    {
      url: `${BASE_URL}/privacy-policy`,
      lastModified: new Date("2026-06-13"),
      changeFrequency: "monthly",
      priority: 0.8,
    },
    {
      url: `${BASE_URL}/terms-of-service`,
      lastModified: new Date("2026-06-13"),
      changeFrequency: "monthly",
      priority: 0.8,
    },
    {
      url: `${BASE_URL}/user-data-deletion`,
      lastModified: new Date("2026-06-13"),
      changeFrequency: "monthly",
      priority: 0.7,
    },
    {
      url: `${BASE_URL}/track`,
      lastModified: new Date(),
      changeFrequency: "always",
      priority: 0.6,
    },
  ];
}
