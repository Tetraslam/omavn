import type { ReactNode } from "react";

function Frame({
  caption,
  children,
}: {
  caption?: ReactNode;
  children: ReactNode;
}) {
  return (
    <figure className="not-prose my-6">
      <div className="overflow-hidden rounded-xl border bg-fd-card shadow-lg">
        {children}
      </div>
      {caption && (
        <figcaption className="mt-2 text-center text-sm text-fd-muted-foreground">
          {caption}
        </figcaption>
      )}
    </figure>
  );
}

/**
 * A short recording that plays like a GIF: muted, looping, inline (so phones
 * don't open it fullscreen), at a fraction of a GIF's size.
 * `name` is a file stem in /public/media with an .mp4 and a .webp poster.
 */
export function Clip({
  name,
  alt,
  caption,
}: {
  name: string;
  alt: string;
  caption?: ReactNode;
}) {
  return (
    <Frame caption={caption}>
      <video
        className="block h-auto w-full"
        src={`/media/${name}.mp4`}
        poster={`/media/${name}.webp`}
        aria-label={alt}
        autoPlay
        loop
        muted
        playsInline
        preload="metadata"
      />
    </Frame>
  );
}

export function Shot({
  src,
  alt,
  caption,
}: {
  src: string;
  alt: string;
  caption?: ReactNode;
}) {
  return (
    <Frame caption={caption}>
      {/* biome-ignore lint/performance/noImgElement: static export, no image optimizer */}
      <img className="block h-auto w-full" src={src} alt={alt} loading="lazy" />
    </Frame>
  );
}
