type Props = {
  url: string;
  title?: string | null;
};

function safeAttachmentUrl(url: string): string | null {
  try {
    const parsed = new URL(url);
    return parsed.protocol === "http:" || parsed.protocol === "https:"
      ? parsed.toString()
      : null;
  } catch {
    return null;
  }
}

export function AttachmentChip({ url, title }: Props) {
  const safeUrl = safeAttachmentUrl(url);
  const label = title ?? url;
  if (!safeUrl) {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-zinc-700 bg-zinc-800 px-2 py-1 text-xs">
        <span className="font-medium text-emerald-400">PR</span>
        <span className="max-w-[16rem] truncate text-zinc-200">{label}</span>
      </span>
    );
  }

  return (
    <a
      href={safeUrl}
      target="_blank"
      rel="noopener noreferrer"
      className="inline-flex items-center gap-1 rounded-full border border-zinc-700 bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700"
    >
      <span className="font-medium text-emerald-400">PR</span>
      <span className="max-w-[16rem] truncate text-zinc-200">{label}</span>
    </a>
  );
}
