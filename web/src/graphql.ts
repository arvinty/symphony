import { GraphQLClient } from "graphql-request";

export const gql = new GraphQLClient("/graphql");

export type WorkflowState = {
  id: string;
  name: string;
  type: string;
  position: number;
  color?: string | null;
};

export type Issue = {
  id: string;
  identifier: string;
  number: number;
  title: string;
  description?: string | null;
  priority?: number | null;
  branchName?: string | null;
  url?: string | null;
  createdAt: string;
  updatedAt: string;
  state: WorkflowState;
  assignee?: { id: string; displayName: string; avatarUrl?: string | null } | null;
  team: { id: string; key: string; name: string; color?: string | null };
  project?: { id: string; slugId: string; name: string; color?: string | null } | null;
  labels: { nodes: { id: string; name: string; color?: string | null }[] };
  attachments: { nodes: { id: string; url: string; title?: string | null; kind: string; createdAt: string }[] };
};

export const ISSUE_FIELDS = `
  id identifier number title description priority branchName url createdAt updatedAt
  state { id name type position color }
  assignee { id displayName avatarUrl }
  team { id key name color }
  project { id slugId name color }
  labels { nodes { id name color } }
  attachments { nodes { id url title kind createdAt } }
`;

export async function listIssues(filter?: any): Promise<Issue[]> {
  const data: any = await gql.request(
    `query Issues($filter: IssueFilter) {
       issues(first: 200, filter: $filter) { nodes { ${ISSUE_FIELDS} } }
     }`,
    { filter },
  );
  return data.issues.nodes;
}

export async function listStates(): Promise<WorkflowState[]> {
  const data: any = await gql.request(
    `query States { workflowStates { id name type position color } }`,
  );
  return data.workflowStates;
}

export async function listProjects(): Promise<any[]> {
  const data: any = await gql.request(
    `query Projects { projects { id slugId name color icon description } }`,
  );
  return data.projects;
}

export async function listTeams(): Promise<any[]> {
  const data: any = await gql.request(`query Teams { teams { id key name color icon } }`);
  return data.teams;
}

export async function updateIssueState(id: string, stateId: string) {
  await gql.request(`mutation($id: ID!, $sid: ID!) { updateIssueState(id: $id, stateId: $sid) }`, {
    id,
    sid: stateId,
  });
}

export async function createIssue(input: any): Promise<string> {
  const data: any = await gql.request(
    `mutation($input: CreateIssueInput!) { createIssue(input: $input) }`,
    { input },
  );
  return data.createIssue as string;
}
