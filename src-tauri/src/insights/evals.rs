//! The assistant's eval set: questions about the synthetic career person, each with the
//! ledger tool and arguments that answer it and the figures that answer holds.
//!
//! The deterministic test runs every tool call and checks its figures, that the router
//! offers each question its tool, and that the Swift bridge defines every tool, so a changed
//! figure or a renamed tool fails in CI. The ignored test asks Apple Intelligence each
//! question on a Mac that has it, and checks that it queried, quoted the figures with none
//! unverified, and called the unanswerable unavailable:
//! `cargo test assistant_answers_the_eval_set -- --ignored --nocapture`.
use serde::Deserialize;
use serde_json::Value;

use crate::{insights::ledger::Ledger, synthetic::career};

#[derive(Deserialize)]
struct Eval {
    id: String,
    question: String,
    tool: Option<String>,
    arguments: Value,
    /// JSON pointers into the tool's result and the values found there.
    expect: serde_json::Map<String, Value>,
    /// Each entry is a list of forms, one of which the answer must contain.
    says: Vec<Vec<String>>,
    #[serde(default)]
    unavailable: bool,
}

fn evals() -> Vec<Eval> {
    include_str!("assistant-evals.jsonl")
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("each eval line is valid"))
        .collect()
}

fn view() -> std::sync::Arc<crate::insights::View> {
    use chrono::TimeZone;
    let today = career::today();
    let now = chrono::Utc.from_utc_datetime(&today.and_hms_opt(12, 0, 0).unwrap());
    let (view, _) = crate::storage::derive_view(
        &career::state(),
        None,
        &Default::default(),
        now,
        &crate::insights::calendar::NamedZone::Utc,
    )
    .unwrap();
    std::sync::Arc::new(view)
}

#[test]
fn every_eval_question_has_a_tool_answer_with_its_figures() {
    let evals = evals();
    assert!(
        evals.len() >= 50,
        "the eval set has {} questions",
        evals.len()
    );
    let mut ids = std::collections::BTreeSet::new();
    let view = view();
    let ledger = Ledger::new(&view, career::today());
    let bridge = include_str!("../../native/FoundationModelBridge.swift");
    // Every tool a question can be offered is one the bridge defines and the ledger runs.
    for (name, _) in crate::insights::ledger::TOOLS {
        assert!(
            bridge.contains(&format!("name: \"{name}\"")),
            "the Swift bridge offers {name}"
        );
        let result = ledger.call(name, &serde_json::json!({}));
        assert_ne!(
            result["error"],
            serde_json::json!(format!("Unknown tool {name}"))
        );
    }
    let mut unrouted = Vec::new();
    for eval in &evals {
        assert!(ids.insert(eval.id.as_str()), "{} is repeated", eval.id);
        let Some(tool) = &eval.tool else {
            assert!(eval.unavailable && eval.expect.is_empty(), "{}", eval.id);
            continue;
        };
        assert!(
            bridge.contains(&format!("name: \"{tool}\"")),
            "the Swift bridge offers {tool}"
        );
        let offered = crate::insights::ledger::tools_for(&eval.question, &view);
        if !offered.contains(&tool.as_str()) {
            unrouted.push(format!("{}: {tool} not in {offered:?}", eval.id));
        }
        let result = ledger.call(tool, &eval.arguments);
        assert!(result.get("error").is_none(), "{}: {result}", eval.id);
        for (pointer, expected) in &eval.expect {
            assert_eq!(
                result.pointer(pointer),
                Some(expected),
                "{}: {pointer} in {result}",
                eval.id
            );
        }
        assert!(
            !eval.says.is_empty(),
            "{} names what its answer says",
            eval.id
        );
    }
    assert!(
        unrouted.is_empty(),
        "questions offered the wrong tools:\n{}",
        unrouted.join("\n")
    );
}

/// Whether an answer contains one of the forms, ignoring case, dollar signs, and commas in
/// figures.
fn contains_any(text: &str, forms: &[String]) -> bool {
    let even = |text: &str| text.to_lowercase().replace(['$', ','], "");
    let text = even(text);
    forms.iter().any(|form| text.contains(&even(form)))
}

#[cfg(target_os = "macos")]
#[tokio::test]
#[ignore]
async fn assistant_answers_the_eval_set() {
    let status = crate::foundation_model::status();
    if status.state != "available" {
        eprintln!("Apple Intelligence is unavailable: {}", status.message);
        return;
    }
    let view = view();
    let mut failures = Vec::new();
    // `BRIEF_EVAL_ONLY=id,id` asks only those questions.
    let only = std::env::var("BRIEF_EVAL_ONLY").ok();
    let evals = evals()
        .into_iter()
        .filter(|eval| {
            only.as_deref()
                .is_none_or(|ids| ids.split(',').any(|id| id == eval.id))
        })
        .collect::<Vec<_>>();
    for eval in &evals {
        // A question that timed out holds the model until Swift finishes it; wait for that.
        let mut answer = Err(String::new());
        for _ in 0..60 {
            let host = crate::foundation_model::ToolHost::new(view.clone(), career::today());
            answer = crate::foundation_model::generate(
                eval.question.clone(),
                "Today is 2026-10-17.".into(),
                format!("eval-{}", eval.id),
                tauri::ipc::Channel::new(|_| Ok(())),
                host,
            )
            .await;
            match &answer {
                Err(error) if error == "An answer is already being generated" => {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
                _ => break,
            }
        }
        let verdict = match &answer {
            Err(error) => Some(format!("failed: {error}")),
            Ok(answer) if eval.unavailable => (answer.outcome
                != crate::foundation_model::Outcome::Unavailable)
                .then(|| format!("answered: {}", answer.text)),
            Ok(answer) => {
                // Any query that returned the figures will do; which one the deterministic test
                // pins down.
                let used = !answer.evidence.is_empty();
                let missing = eval
                    .says
                    .iter()
                    .filter(|forms| !contains_any(&answer.text, forms))
                    .collect::<Vec<_>>();
                (!used || !missing.is_empty() || !answer.unverified.is_empty()).then(|| {
                    format!(
                        "tool used: {used}; missing {missing:?}; unverified {:?}: {}",
                        answer.unverified, answer.text
                    )
                })
            }
        };
        if let Some(verdict) = verdict {
            eprintln!("{}: {verdict}", eval.id);
            failures.push(eval.id.clone());
        }
    }
    eprintln!("{} of {} passed", evals.len() - failures.len(), evals.len());
    assert!(failures.is_empty(), "failed: {failures:?}");
}

#[test]
fn answers_match_figures_however_they_are_written() {
    assert!(contains_any(
        "You earned $8,200.00 in September.",
        &["8200".into()]
    ));
    assert!(contains_any(
        "It posted on July 15, 2026.",
        &["2026-07-15".into(), "July 15".into()]
    ));
    assert!(!contains_any("About $8,000.", &["8200".into()]));
}
