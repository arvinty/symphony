use crate::tools::linear_graphql::LinearGraphqlClient;
use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub fn generate_mcp_config_json(symphony_exe: &Path, issue_identifier: &str) -> String {
    json!({
        "mcpServers": {
            "linear": {
                "command": symphony_exe.to_string_lossy(),
                "args": ["mcp-bridge", "--issue", issue_identifier]
            }
        }
    })
    .to_string()
}

pub async fn run_mcp_server(issue_id: String) -> Result<()> {
    std::panic::set_hook(Box::new(|info| {
        eprintln!("MCP_BRIDGE_PANIC: {info}");
    }));
    let token = std::env::var("SYMPHONY_LINEAR_TOKEN")
        .context("SYMPHONY_LINEAR_TOKEN is required for mcp-bridge")?;
    let endpoint = std::env::var("SYMPHONY_LINEAR_ENDPOINT")
        .context("SYMPHONY_LINEAR_ENDPOINT is required for mcp-bridge")?;
    let cli = LinearGraphqlClient::new(endpoint, token);

    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin).lines();
    let mut stdout = tokio::io::stdout();

    while let Some(line) = reader.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("mcp_bridge_parse_error: {e} line={line}");
                continue;
            }
        };
        let id = req.get("id").cloned();
        let method = req.get("method").and_then(|v| v.as_str()).unwrap_or("");
        let params = req.get("params").cloned().unwrap_or(Value::Null);

        let response = match (method, id.as_ref()) {
            ("initialize", Some(_)) => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "symphony-linear-graphql", "version": "0.1"}
                }
            })),
            ("notifications/initialized", _) => None,
            ("tools/list", Some(_)) => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {"tools": [
                    {
                        "name": "get_issue",
                        "description": "Fetch an issue by id.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"id": {"type": "string"}},
                            "required": ["id"]
                        }
                    },
                    {
                        "name": "list_comments",
                        "description": "List comments on an issue.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {"id": {"type": "string"}},
                            "required": ["id"]
                        }
                    },
                    {
                        "name": "add_comment",
                        "description": "Post a comment on an issue.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "issue_id": {"type": "string"},
                                "body": {"type": "string"}
                            },
                            "required": ["body"]
                        }
                    },
                    {
                        "name": "link_pull_request",
                        "description": "Attach a pull request URL to an issue.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "issue_id": {"type": "string"},
                                "url": {"type": "string"},
                                "title": {"type": "string"}
                            },
                            "required": ["url"]
                        }
                    }
                ]}
            })),
            ("tools/call", Some(_)) => {
                let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(Value::Null);
                Some(handle_tool_call(&cli, &issue_id, name, args, id.clone()).await)
            }
            _ => Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {method}")}
            })),
        };

        if let Some(resp) = response {
            let mut s = serde_json::to_string(&resp)?;
            s.push('\n');
            stdout.write_all(s.as_bytes()).await?;
            stdout.flush().await?;
        }
    }
    Ok(())
}

async fn handle_tool_call(
    cli: &LinearGraphqlClient,
    bound_issue: &str,
    name: &str,
    args: Value,
    id: Option<Value>,
) -> Value {
    let result_text = match name {
        "get_issue" => {
            let arg_id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            cli.get_issue(arg_id).await.map(|v| v.to_string())
        }
        "list_comments" => {
            let arg_id = args.get("id").and_then(|v| v.as_str()).unwrap_or("");
            cli.list_comments(arg_id).await.map(|v| v.to_string())
        }
        "add_comment" => {
            let issue_id = args
                .get("issue_id")
                .and_then(|v| v.as_str())
                .unwrap_or(bound_issue);
            if issue_id != bound_issue {
                return tool_error(
                    id,
                    format!("issue_id mismatch (bound={bound_issue}, got={issue_id})"),
                );
            }
            let body = args.get("body").and_then(|v| v.as_str()).unwrap_or("");
            cli.add_comment(issue_id, body)
                .await
                .map(|s| format!(r#"{{"id":"{s}"}}"#))
        }
        "link_pull_request" => {
            let issue_id = args
                .get("issue_id")
                .and_then(|v| v.as_str())
                .unwrap_or(bound_issue);
            if issue_id != bound_issue {
                return tool_error(
                    id,
                    format!("issue_id mismatch (bound={bound_issue}, got={issue_id})"),
                );
            }
            let url = args.get("url").and_then(|v| v.as_str()).unwrap_or("");
            let title = args.get("title").and_then(|v| v.as_str());
            cli.link_pull_request(issue_id, url, title)
                .await
                .map(|s| format!(r#"{{"id":"{s}"}}"#))
        }
        _ => return tool_error(id, format!("unknown tool: {name}")),
    };
    match result_text {
        Ok(text) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": {"content": [{"type": "text", "text": text}], "isError": false}
        }),
        Err(e) => tool_error(id, e.to_string()),
    }
}

fn tool_error(id: Option<Value>, msg: String) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {"content": [{"type": "text", "text": msg}], "isError": true}
    })
}
