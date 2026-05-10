use crate::error::{Result, SymphonyError};
use serde_yaml::Value;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct WorkflowDefinition {
    pub source_path: PathBuf,
    pub config: Value,
    pub prompt_template: String,
}

pub fn load_workflow(path: &Path) -> Result<WorkflowDefinition> {
    let raw = std::fs::read_to_string(path)
        .map_err(|e| SymphonyError::MissingWorkflowFile(format!("{}: {}", path.display(), e)))?;
    parse_workflow(path, &raw)
}

fn parse_workflow(path: &Path, raw: &str) -> Result<WorkflowDefinition> {
    let (front_matter, body) = split_front_matter(raw);
    let config: Value = match front_matter {
        Some(yaml) => serde_yaml::from_str(&yaml)
            .map_err(|e| SymphonyError::WorkflowParseError(e.to_string()))?,
        None => Value::Mapping(Default::default()),
    };
    if !config.is_mapping() {
        return Err(SymphonyError::WorkflowFrontMatterNotAMap);
    }
    Ok(WorkflowDefinition {
        source_path: path.to_path_buf(),
        config,
        prompt_template: body.trim().to_string(),
    })
}

fn split_front_matter(raw: &str) -> (Option<String>, String) {
    // If file starts with `---`, parse until next `---`.
    let mut lines = raw.lines();
    let first = match lines.next() {
        Some(l) => l,
        None => return (None, String::new()),
    };
    if first.trim() != "---" {
        return (None, raw.to_string());
    }
    let mut front = String::new();
    let mut body = String::new();
    let mut in_body = false;
    for line in lines {
        if !in_body && line.trim() == "---" {
            in_body = true;
            continue;
        }
        if in_body {
            body.push_str(line);
            body.push('\n');
        } else {
            front.push_str(line);
            front.push('\n');
        }
    }
    if !in_body {
        // Closing --- never seen — treat whole thing as body (fall back).
        return (None, raw.to_string());
    }
    (Some(front), body)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_with_front_matter() {
        let raw = "---\nfoo: bar\n---\nhello\n";
        let wf = parse_workflow(Path::new("WORKFLOW.md"), raw).unwrap();
        assert_eq!(wf.prompt_template, "hello");
        assert!(wf.config.is_mapping());
    }

    #[test]
    fn parses_without_front_matter() {
        let raw = "just a prompt\n";
        let wf = parse_workflow(Path::new("WORKFLOW.md"), raw).unwrap();
        assert_eq!(wf.prompt_template, "just a prompt");
    }

    #[test]
    fn sanitize() {
        assert_eq!(crate::model::sanitize_workspace_key("ABC-123"), "ABC-123");
        assert_eq!(crate::model::sanitize_workspace_key("a/b c"), "a_b_c");
    }
}
