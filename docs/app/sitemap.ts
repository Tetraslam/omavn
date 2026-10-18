import type { MetadataRoute } from "next";
import { source } from "@/lib/source";

export const dynamic = "force-static";

export default function sitemap(): MetadataRoute.Sitemap {
  const base = "https://omavn.dev";
  return [
    { url: base, priority: 1 },
    ...source.getPages().map((page) => ({ url: `${base}${page.url}` })),
  ];
}
