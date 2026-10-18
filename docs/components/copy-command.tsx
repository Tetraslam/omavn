"use client";

import { Check, Copy } from "lucide-react";
import { useState } from "react";

export function CopyCommand({ command }: { command: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex items-center gap-3 rounded-xl border bg-fd-card/80 px-4 py-3 font-mono text-sm shadow-lg backdrop-blur">
      <span className="select-none text-fd-primary" aria-hidden="true">
        $
      </span>
      <code className="flex-1 overflow-x-auto whitespace-nowrap text-left">
        {command}
      </code>
      <button
        type="button"
        onClick={() => {
          navigator.clipboard.writeText(command);
          setCopied(true);
          setTimeout(() => setCopied(false), 1500);
        }}
        className="rounded-md p-1.5 text-fd-muted-foreground transition-colors hover:bg-fd-accent hover:text-fd-foreground focus-visible:outline-2 focus-visible:outline-fd-ring"
        aria-label={copied ? "Copied" : "Copy install command"}
      >
        {copied ? <Check className="size-4" /> : <Copy className="size-4" />}
      </button>
    </div>
  );
}
