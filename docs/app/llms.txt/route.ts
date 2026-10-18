import meta from "@/content/docs/meta.json";
import { getPageMarkdownUrl } from "@/lib/shared";
import { source } from "@/lib/source";

export const revalidate = false;

const base = "https://omavn.dev";

// Absolute links to the markdown version of each page: agents usually fetch
// this file on its own, without a page to resolve relative URLs against.
export async function GET() {
  const order = (slug: string) => meta.pages.indexOf(slug || "index");
  const pages = source
    .getPages()
    .sort((a, b) => order(a.slugs.join("/")) - order(b.slugs.join("/")))
    .map(
      (page) =>
        `- [${page.data.title}](${base}${getPageMarkdownUrl(page).url}): ${page.data.description ?? ""}`,
    );
  const body = [
    "# omavn",
    "",
    "> Play Windows visual novels on Linux. omavn unwraps whatever was downloaded (nested zips, split RARs, installers), installs it, and launches it natively (Ren'Py) or through Proton in gamescope. Every command accepts --json.",
    "",
    "Install: `curl -fsSL https://omavn.dev/install.sh | sh`, then `omavn install <download>` and `omavn run <game>`.",
    "",
    "## Docs",
    "",
    ...pages,
    "",
    "## Optional",
    "",
    `- [Everything in one file](${base}/llms-full.txt)`,
    "- [Source](https://github.com/Tetraslam/omavn)",
    "",
  ].join("\n");
  return new Response(body, {
    headers: { "Content-Type": "text/plain; charset=utf-8" },
  });
}
