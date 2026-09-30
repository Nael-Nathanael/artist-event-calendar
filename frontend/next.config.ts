import type { NextConfig } from "next";

const backendUrl = process.env.BACKEND_URL || "http://127.0.0.1:8081";

const nextConfig: NextConfig = {
  output: "standalone",
  // Only the public read endpoint; the ingest endpoint stays off the internet.
  async rewrites() {
    return [{ source: "/api/events", destination: `${backendUrl}/api/events` }];
  },
};

export default nextConfig;
