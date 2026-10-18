import type { Metadata } from "next";
import { Instrument_Serif, Inter, JetBrains_Mono } from "next/font/google";
import { Provider } from "@/components/provider";
import "./global.css";

const inter = Inter({ subsets: ["latin"], variable: "--font-inter" });
const serif = Instrument_Serif({
  subsets: ["latin"],
  weight: "400",
  style: ["normal", "italic"],
  variable: "--font-instrument-serif",
});
const mono = JetBrains_Mono({
  subsets: ["latin"],
  variable: "--font-jetbrains-mono",
});

export const metadata: Metadata = {
  metadataBase: new URL("https://omavn.dev"),
  title: {
    default: "omavn: play visual novels on Linux",
    template: "%s | omavn",
  },
  description:
    "Point omavn at whatever you downloaded and it unpacks, installs, and launches your visual novel on Linux.",
  icons: { icon: "/logo.svg" },
  openGraph: {
    siteName: "omavn",
    type: "website",
    url: "https://omavn.dev",
  },
  alternates: {
    types: { "text/plain": "/llms.txt" },
  },
};

export default function Layout({ children }: LayoutProps<"/">) {
  return (
    <html
      lang="en"
      className={`${inter.className} ${inter.variable} ${serif.variable} ${mono.variable}`}
      suppressHydrationWarning
    >
      <body className="flex flex-col min-h-screen">
        <Provider>{children}</Provider>
      </body>
    </html>
  );
}
