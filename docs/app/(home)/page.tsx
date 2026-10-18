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

const demo = [
  { kind: "cmd", text: 'omavn install ~/Downloads/"Dies irae"' },
  { kind: "step", text: "extracting MG1041_1.zip" },
  { kind: "step", text: "extracting MG1041_2.zip" },
  { kind: "step", text: "extracting MG1041_3.zip" },
  { kind: "step", text: "extracting Dies Irae.part1.exe" },
  { kind: "step", text: "installing to ~/.local/share/omavn/games/dies-irae" },
  { kind: "step", text: "fetching metadata and art" },
  { kind: "info", text: "  VNDB: Dies irae ~Acta est Fabula~ (v548)" },
  { kind: "ok", text: "installed Dies irae as `dies-irae`" },
  { kind: "cmd", text: "omavn run dies" },
  { kind: "info", text: "▶ Dies irae" },
] as const;

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

      <section aria-label="Example session" className="px-6 pb-20">
        <div className="mx-auto max-w-2xl overflow-hidden rounded-2xl border bg-fd-card shadow-2xl">
          <div
            className="flex items-center gap-1.5 border-b px-4 py-3"
            aria-hidden="true"
          >
            <span className="size-3 rounded-full bg-fd-muted-foreground/30" />
            <span className="size-3 rounded-full bg-fd-muted-foreground/30" />
            <span className="size-3 rounded-full bg-fd-muted-foreground/30" />
          </div>
          <pre className="overflow-x-auto p-5 font-mono text-[13px] leading-6">
            {demo.map((l) => (
              <div
                key={l.text}
                className={l.kind === "cmd" ? "mt-3 first:mt-0" : undefined}
              >
                {l.kind === "cmd" && (
                  <>
                    <span className="text-fd-primary">$ </span>
                    <span>{l.text}</span>
                  </>
                )}
                {l.kind === "step" && (
                  <>
                    <span className="text-wisteria">→ </span>
                    <span className="text-fd-muted-foreground">{l.text}</span>
                  </>
                )}
                {l.kind === "info" && (
                  <span className="text-fd-muted-foreground">{l.text}</span>
                )}
                {l.kind === "ok" && (
                  <>
                    <span className="text-emerald-400">✓ </span>
                    <span>{l.text}</span>
                  </>
                )}
              </div>
            ))}
          </pre>
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
