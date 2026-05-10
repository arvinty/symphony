use crate::error::{Result, SymphonyError};
use crate::model::Issue;
use liquid::ParserBuilder;
use serde_json::{json, Value};

pub fn render_prompt(template: &str, issue: &Issue, attempt: Option<u32>) -> Result<String> {
    let parser = ParserBuilder::with_stdlib()
        .build()
        .map_err(|e| SymphonyError::TemplateParseError(e.to_string()))?;
    let tmpl = parser
        .parse(template)
        .map_err(|e| SymphonyError::TemplateParseError(e.to_string()))?;

    let issue_json = serde_json::to_value(issue)
        .map_err(|e| SymphonyError::TemplateRenderError(e.to_string()))?;
    let mut globals = json!({
        "issue": issue_json,
        "attempt": match attempt { Some(n) => Value::from(n), None => Value::Null },
    });
    inject_string_keys(&mut globals);
    let object: liquid::Object = serde_json::from_value(globals)
        .map_err(|e| SymphonyError::TemplateRenderError(e.to_string()))?;
    tmpl.render(&object)
        .map_err(|e| SymphonyError::TemplateRenderError(e.to_string()))
}

fn inject_string_keys(_v: &mut Value) {
    // serde_json maps already use string keys; helper kept as a no-op for parity with spec wording.
}
