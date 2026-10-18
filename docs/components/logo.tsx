/**
 * The omavn mark: a visual novel dialogue box with its name plate.
 * Flat colors rather than an SVG gradient: layouts render the mark more than
 * once, and a gradient id defined inside a hidden copy breaks the visible one.
 */
export function Logo({ className }: { className?: string }) {
  const pink = "#f5a3c7";
  const violet = "#b69cf7";
  return (
    <svg viewBox="0 0 64 64" className={className} aria-hidden="true">
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
        className="fill-fd-background"
        stroke={pink}
        strokeWidth="3"
      />
      <circle cx="16" cy="17.5" r="2" fill={pink} />
      <rect x="20" y="16" width="10" height="3" rx="1.5" fill={pink} />
      <rect
        x="14"
        y="29"
        width="30"
        height="4"
        rx="2"
        fill={pink}
        opacity=".85"
      />
      <rect
        x="14"
        y="38"
        width="20"
        height="4"
        rx="2"
        fill={violet}
        opacity=".7"
      />
      <path d="M44 40h8l-4 6z" fill={pink} className="advance" />
    </svg>
  );
}
