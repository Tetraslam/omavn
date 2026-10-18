import Link from "next/link";

export default function HomePage() {
  return (
    <div className="flex flex-col justify-center items-center text-center flex-1 gap-6 px-4">
      <h1 className="text-4xl font-bold">omavn</h1>
      <p className="text-fd-muted-foreground max-w-xl">
        Play visual novels on Linux. Point it at whatever you downloaded and it
        figures out the rest.
      </p>
      <pre className="text-left bg-fd-muted rounded-lg px-5 py-4 text-sm">
        <code>
          {"omavn install ~/Downloads/your-vn.zip\nomavn run your-vn"}
        </code>
      </pre>
      <Link href="/docs" className="font-medium underline">
        Read the docs
      </Link>
    </div>
  );
}
