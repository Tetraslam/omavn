import {
  Archive,
  Bot,
  Images,
  LayoutGrid,
  MonitorPlay,
  PackageCheck,
} from "lucide-react";
import Link from "next/link";
import type { ReactNode } from "react";
import { CopyCommand } from "@/components/copy-command";
import { Logo } from "@/components/logo";
import { Clip } from "@/components/media";

const features: { icon: ReactNode; title: string; body: string }[] = [
  {
    icon: <Archive className="size-5" />,
    title: "Any download, as-is",
    body: "Zips around split RARs around self-extracting volumes, Shift-JIS file names, installers with loose data files. Don't unzip anything.",
  },
  {
    icon: <PackageCheck className="size-5" />,
    title: "Installers run themselves",
    body: "NSIS, Inno Setup, and MSI installers run unattended. DirectX redist wizards that hang other tools get skipped.",
  },
  {
    icon: <MonitorPlay className="size-5" />,
    title: "Native when it can be",
    body: "Ren'Py games run on the official Linux Ren'Py runtime. Everything else runs through GE-Proton in its own prefix.",
  },
  {
    icon: <LayoutGrid className="size-5" />,
    title: "Scaled right, clicks right",
    body: "Fixed-resolution games fill your screen through gamescope, with the mouse landing where you see it.",
  },
  {
    icon: <Images className="size-5" />,
    title: "A library worth looking at",
    body: "Covers and screenshots from VNDB, cards for your launcher, an app entry per game.",
  },
  {
    icon: <Bot className="size-5" />,
    title: "Built for agents too",
    body: "--json on every command, no prompts without a terminal, and these docs as llms.txt.",
  },
];

export default function HomePage() {
  return (
    <main className="flex flex-1 flex-col">
      <section className="hero-glow relative px-6 pt-20 pb-16 text-center sm:pt-28">
        <Logo className="mx-auto mb-8 size-20" />
        <h1 className="font-display text-5xl leading-tight tracking-tight sm:text-7xl">
          Visual novels,
          <br />
          <span className="text-gradient italic">at home on Linux.</span>
        </h1>
        <p className="mx-auto mt-6 max-w-xl text-lg text-fd-muted-foreground">
          Point omavn at whatever you downloaded. It unpacks it, installs it,
          and launches it, natively when it can.
        </p>
        <div className="mx-auto mt-10 max-w-xl">
          <CopyCommand command="curl -fsSL https://omavn.dev/install.sh | sh" />
        </div>
        <div className="mt-6 flex flex-wrap justify-center gap-3 text-sm font-medium">
          <Link
            href="/docs"
            className="rounded-full bg-fd-primary px-5 py-2.5 text-fd-primary-foreground transition-opacity hover:opacity-90"
          >
            Get started
          </Link>
          <a
            href="https://github.com/Tetraslam/omavn"
            className="rounded-full border px-5 py-2.5 transition-colors hover:bg-fd-accent"
          >
            GitHub
          </a>
        </div>
      </section>

      <section
        aria-label="omavn installing and starting a game"
        className="px-6 pb-20"
      >
        <div className="mx-auto max-w-4xl">
          <Clip
            name="cli-install"
            alt="Terminal recording: omavn install unpacks Dies irae's three zips and split RAR, downloads GE-Proton, prepares a Wine prefix, fetches art from VNDB, then omavn run starts the game"
            caption="A fresh machine: three zips wrapping a split self-extracting RAR, to playable. Real run, sped up 2.5x."
          />
        </div>
      </section>

      <section className="px-6 pb-24">
        <h2 className="mb-3 text-center font-display text-4xl">
          And then it just plays
        </h2>
        <p className="mx-auto mb-10 max-w-xl text-center text-fd-muted-foreground">
          Recorded on Linux straight after install. No settings touched.
        </p>
        <div className="mx-auto grid max-w-5xl gap-6 md:grid-cols-2">
          <Clip
            name="dies-irae"
            alt="Dies irae's opening running through omavn: Japanese narration with English subtitles"
            caption="Dies irae: video, Japanese text, English subtitles."
          />
          <Clip
            name="yumina"
            alt="Yumina the Ethereal running in gamescope: startup dialog, Start Game, the title screen, then the story"
            caption="Yumina the Ethereal: an 800x600 game, scaled up, clicks landing right."
          />
        </div>
      </section>

      <section className="px-6 pb-24">
        <h2 className="mb-10 text-center font-display text-4xl">
          What it takes care of
        </h2>
        <div className="mx-auto grid max-w-5xl gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {features.map((f) => (
            <div key={f.title} className="rounded-2xl border bg-fd-card p-6">
              <div className="mb-4 inline-flex rounded-lg bg-fd-primary/10 p-2 text-fd-primary">
                {f.icon}
              </div>
              <h3 className="mb-2 font-semibold">{f.title}</h3>
              <p className="text-sm leading-relaxed text-fd-muted-foreground">
                {f.body}
              </p>
            </div>
          ))}
        </div>
      </section>

      <section className="px-6 pb-24">
        <div className="mx-auto max-w-5xl rounded-3xl border bg-gradient-to-br from-sakura/10 to-wisteria/10 p-8 sm:p-12">
          <p className="text-sm font-medium uppercase tracking-widest text-fd-primary">
            On Omarchy
          </p>
          <h2 className="mt-3 font-display text-4xl">
            Feels like it shipped with your desktop
          </h2>
          <p className="mt-4 max-w-2xl text-fd-muted-foreground">
            One command adds Visual Novels to the Omarchy menu, opens your
            library in the same cover grid as the theme picker, sends a
            notification with the cover when an install finishes, and keeps
            games opaque and your screen awake while you read.
          </p>
          <div className="mt-8 max-w-md">
            <CopyCommand command="omavn omarchy setup" />
          </div>
          <div className="mt-8">
            <Clip
              name="omarchy-picker"
              alt="Omarchy's full-screen cover grid showing Dies irae and Yumina the Ethereal, browsed with the arrow keys"
              caption="Super + Shift + V opens your library in Omarchy's cover grid."
            />
          </div>
          <Link
            href="/docs/omarchy"
            className="mt-6 inline-block text-sm font-medium text-fd-primary hover:underline"
          >
            What it changes, and how to undo it →
          </Link>
        </div>
      </section>

      <footer className="border-t px-6 py-10 text-sm text-fd-muted-foreground">
        <div className="mx-auto flex max-w-5xl flex-wrap items-center justify-between gap-4">
          <span>
            omavn is open source under Apache-2.0. Something broke?{" "}
            <code className="font-mono text-fd-foreground">omavn feedback</code>
          </span>
          <nav aria-label="Footer" className="flex gap-5">
            <Link href="/docs" className="hover:text-fd-foreground">
              Docs
            </Link>
            <a href="/llms.txt" className="hover:text-fd-foreground">
              llms.txt
            </a>
            <a
              href="https://github.com/Tetraslam/omavn"
              className="hover:text-fd-foreground"
            >
              GitHub
            </a>
          </nav>
        </div>
      </footer>
    </main>
  );
}
