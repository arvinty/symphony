import { useEffect } from "react";
import useSWR from "swr";
import { useParams } from "react-router-dom";
import { gql, ISSUE_FIELDS, listStates, updateIssueState } from "../graphql";
import { AttachmentChip } from "../components/AttachmentChip";
import { useEventStream } from "../hooks/useEventStream";

export function IssueDetailView() {
  const { identifier } = useParams();
  const { data, mutate } = useSWR(["issue", identifier], async () => {
    const data: any = await gql.request(
      `query($filter: IssueFilter) { issues(first: 1, filter: $filter) { nodes { ${ISSUE_FIELDS} } } }`,
      { filter: {} },
    );
    return (data.issues.nodes as any[]).find((i) => i.identifier === identifier);
  });
  const { data: states = [] } = useSWR("states", listStates);
  const events = useEventStream(data?.id);
  const lastEvent = events[events.length - 1];

  useEffect(() => {
    if (lastEvent?.kind === "pr_opened") void mutate();
  }, [lastEvent, mutate]);

  if (!data)
    return <div className="px-6 py-10 text-subtle text-[13px]">Loading…</div>;
  return (
    <div className="grid grid-cols-[1fr_280px] gap-6 px-8 py-6 max-w-[1100px]">
      <article>
        <div className="text-2xs text-muted font-mono">{data.identifier}</div>
        <h1 className="text-[22px] font-semibold mt-1">{data.title}</h1>
        <p className="mt-4 text-[14px] leading-7 text-subtle whitespace-pre-wrap">
          {data.description ?? "No description."}
        </p>
        {data.attachments?.nodes?.length > 0 && (
          <div className="mt-4 flex flex-wrap gap-2">
            {data.attachments.nodes.map((a: any) => (
              <AttachmentChip key={a.id} url={a.url} title={a.title} />
            ))}
          </div>
        )}
        {events.length > 0 && (
          <div className="mt-6 rounded border border-border bg-elevated p-3">
            <div className="text-2xs uppercase text-muted tracking-wide mb-2">Live activity</div>
            <ul className="space-y-1 text-[12px] font-mono text-subtle max-h-48 overflow-auto">
              {events.slice(-20).map((e, i) => (
                <li key={i}>
                  <span className="text-muted">{e.kind}</span>
                  {"tool" in e && <> · {e.tool}</>}
                  {"branch" in e && <> · {e.branch}</>}
                  {"url" in e && <> · {e.url}</>}
                </li>
              ))}
            </ul>
          </div>
        )}
      </article>
      <aside className="space-y-4 text-[12px]">
        <Field label="Status">
          <select
            value={data.state.id}
            onChange={async (e) => {
              await updateIssueState(data.id, e.target.value);
              mutate();
            }}
            className="bg-elevated border border-border rounded px-2 h-7 w-full"
          >
            {states.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </Field>
        <Field label="Assignee">{data.assignee?.displayName ?? "—"}</Field>
        <Field label="Priority">{["—", "Urgent", "High", "Medium", "Low"][data.priority ?? 0]}</Field>
        <Field label="Labels">
          <div className="flex flex-wrap gap-1">
            {data.labels.nodes.length === 0 && <span className="text-muted">—</span>}
            {data.labels.nodes.map((l: any) => (
              <span key={l.id} className="pill">
                {l.name}
              </span>
            ))}
          </div>
        </Field>
        <Field label="Project">{data.project?.name ?? "—"}</Field>
        <Field label="Branch">
          <code className="text-2xs">{data.branchName ?? "—"}</code>
        </Field>
      </aside>
    </div>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <div className="text-2xs uppercase text-muted tracking-wide mb-1">{label}</div>
      <div>{children}</div>
    </div>
  );
}
