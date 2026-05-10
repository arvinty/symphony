use std::path::PathBuf;
use symphony_core::harness::mcp_bridge::generate_mcp_config_json;

#[test]
fn config_json_lists_linear_server() {
    let exe = PathBuf::from("/usr/bin/symphony");
    let s = generate_mcp_config_json(&exe, "DEMO-1");
    let v: serde_json::Value = serde_json::from_str(&s).unwrap();
    assert!(v["mcpServers"]["linear"]["command"].is_string());
    let args = v["mcpServers"]["linear"]["args"].as_array().unwrap();
    assert!(args.iter().any(|a| a == "mcp-bridge"));
    assert!(args.iter().any(|a| a == "DEMO-1"));
}
