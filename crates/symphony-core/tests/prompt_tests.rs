use chrono::Utc;
use symphony_core::model::{BlockerRef, Issue};
use symphony_core::prompt::render_prompt;

fn issue() -> Issue {
    Issue {
        id: "id1".into(),
        identifier: "ABC-1".into(),
        title: "Do the thing".into(),
        description: Some("Body".into()),
        priority: Some(2),
        state: "Todo".into(),
        branch_name: Some("a/b".into()),
        url: None,
        labels: vec!["bug".into(), "ui".into()],
        blocked_by: vec![BlockerRef {
            id: Some("x".into()),
            identifier: Some("ABC-9".into()),
            state: Some("Todo".into()),
        }],
        created_at: Some(Utc::now()),
        updated_at: Some(Utc::now()),
    }
}

#[test]
fn renders_basic_template() {
    let out = render_prompt(
        "Issue {{ issue.identifier }}: {{ issue.title }}",
        &issue(),
        None,
    )
    .unwrap();
    assert_eq!(out, "Issue ABC-1: Do the thing");
}

#[test]
fn renders_attempt_when_present() {
    let out = render_prompt(
        "{% if attempt %}retry #{{ attempt }}{% else %}first{% endif %}",
        &issue(),
        Some(3),
    )
    .unwrap();
    assert_eq!(out, "retry #3");
}

#[test]
fn renders_first_attempt_branch() {
    let out = render_prompt(
        "{% if attempt %}retry{% else %}first{% endif %}",
        &issue(),
        None,
    )
    .unwrap();
    assert_eq!(out, "first");
}

#[test]
fn unknown_variable_fails() {
    let r = render_prompt("{{ no_such_variable }}", &issue(), None);
    assert!(r.is_err(), "unknown variable should fail strict render");
}

#[test]
fn iterates_labels_and_blockers() {
    let out = render_prompt(
        "labels:{% for l in issue.labels %}{{ l }},{% endfor %} blocked:{% for b in issue.blocked_by %}{{ b.identifier }},{% endfor %}",
        &issue(),
        None,
    ).unwrap();
    assert_eq!(out, "labels:bug,ui, blocked:ABC-9,");
}
