import useSWR from "swr";
import { useParams } from "react-router-dom";
import { listIssues } from "../graphql";
import { IssueRow } from "../components/IssueRow";

export function ProjectView() {
  const { slug } = useParams();
  const { data: issues = [] } = useSWR(["issues_project", slug], () =>
    listIssues({ project: { slugId: { eq: slug } } }),
  );
  return (
    <div>
      <div className="px-6 pt-6 pb-3">
        <div className="text-[20px] font-semibold">{slug}</div>
        <div className="text-2xs text-subtle mt-1">{issues.length} issues</div>
      </div>
      <div>
        {issues.map((i) => (
          <IssueRow key={i.id} issue={i} />
        ))}
      </div>
    </div>
  );
}
