import { ImageResponse } from "next/og";

export const dynamic = "force-static";
export const alt = "omavn: visual novels, at home on Linux";
export const size = { width: 1200, height: 630 };
export const contentType = "image/png";

export default function Image() {
  const pink = "#f5a3c7";
  const violet = "#b69cf7";
  return new ImageResponse(
    <div
      style={{
        width: "100%",
        height: "100%",
        display: "flex",
        flexDirection: "column",
        justifyContent: "center",
        padding: "0 96px",
        background:
          "radial-gradient(700px 360px at 20% 0%, rgba(245,163,199,0.25), transparent), radial-gradient(700px 400px at 85% 20%, rgba(182,156,247,0.22), transparent), #1e1e2e",
        color: "#cdd6f4",
      }}
    >
      <svg
        width="120"
        height="120"
        viewBox="0 0 64 64"
        role="img"
        aria-label="omavn"
      >
        <rect
          x="4"
          y="18"
          width="56"
          height="36"
          rx="8"
          fill="none"
          stroke={violet}
          strokeWidth="4"
        />
        <rect
          x="9"
          y="11"
          width="26"
          height="13"
          rx="6.5"
          fill="#1e1e2e"
          stroke={pink}
          strokeWidth="3"
        />
        <circle cx="16" cy="17.5" r="2" fill={pink} />
        <rect x="20" y="16" width="10" height="3" rx="1.5" fill={pink} />
        <rect x="14" y="29" width="30" height="4" rx="2" fill={pink} />
        <rect x="14" y="38" width="20" height="4" rx="2" fill={violet} />
        <path d="M44 40h8l-4 6z" fill={pink} />
      </svg>
      <div
        style={{
          fontSize: 88,
          marginTop: 36,
          letterSpacing: -2,
          lineHeight: 1.1,
          display: "flex",
          flexDirection: "column",
        }}
      >
        <span>Visual novels,</span>
        <span style={{ color: pink }}>at home on Linux.</span>
      </div>
      <div
        style={{
          fontSize: 30,
          marginTop: 24,
          color: "#a6adc8",
          display: "flex",
        }}
      >
        Point omavn at whatever you downloaded. It figures out the rest.
      </div>
      <div
        style={{ fontSize: 30, marginTop: 48, color: pink, display: "flex" }}
      >
        omavn.dev
      </div>
    </div>,
    size,
  );
}
