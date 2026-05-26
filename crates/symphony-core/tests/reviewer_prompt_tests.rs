use symphony_core::reviewer::{render_reviewer_prompt, DEFAULT_REVIEWER_PROMPT};

#[test]
fn default_template_substitutes_variables() {
    let out = render_reviewer_prompt(
        DEFAULT_REVIEWER_PROMPT,
        "DEMO-1",
        "Add feature X",
        "https://github.com/foo/bar/pull/42",
    )
    .unwrap();
    assert!(out.contains("DEMO-1"));
    assert!(out.contains("Add feature X"));
    assert!(out.contains("https://github.com/foo/bar/pull/42"));
    assert!(out.contains("linear_graphql.add_comment"));
}

#[test]
fn custom_template_renders_with_same_vars() {
    let tpl = "Review {{issue_identifier}} at {{pr_url}}";
    let out = render_reviewer_prompt(tpl, "DEMO-1", "ignored", "https://x").unwrap();
    assert_eq!(out, "Review DEMO-1 at https://x");
}

#[test]
fn missing_variable_returns_error() {
    // liquid::ParserBuilder::with_stdlib() builds a strict parser where
    // unknown variables are errors. This protects against silently-empty
    // reviewer prompts when a template typo or missing variable is the
    // cause.
    let tpl = "{{nonexistent}}";
    let err = render_reviewer_prompt(tpl, "DEMO-1", "t", "u").unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("Unknown variable") || msg.contains("nonexistent"),
        "{msg}"
    );
}

#[test]
fn malformed_template_returns_error() {
    let tpl = "{% if foo %}";
    let err = render_reviewer_prompt(tpl, "DEMO-1", "t", "u").unwrap_err();
    // Liquid surfaces a parse error; we just confirm it's an error path.
    let _ = err;
}
