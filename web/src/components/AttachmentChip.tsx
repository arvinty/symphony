type Props = {
  url: string;
  title?: string | null;
};

export function AttachmentChip({ url, title }: Props) {
  return (
    <a
      href={url}
      target="_blank"
      rel="noreferrer"
      className="inline-flex items-center gap-1 rounded-full border border-zinc-700 bg-zinc-800 px-2 py-1 text-xs hover:bg-zinc-700"
    >
      <span className="font-medium text-emerald-400">PR</span>
      <span className="max-w-[16rem] truncate text-zinc-200">{title ?? url}</span>
    </a>
  );
}
