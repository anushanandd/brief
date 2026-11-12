//! The assistant's eval set: questions about the synthetic career person, each with the
//! ledger tool and arguments that answer it and the figures that answer holds.
//!
//! The deterministic tests run every tool call and check its figures, that the Swift bridge
//! defines every tool, and that the planner leads with each question's tool, resolves its
//! dates, and runs directly only what it can answer, so a changed figure, a renamed tool, or a
//! routing regression fails in CI. The ignored test asks Apple Intelligence each question on a
//! Mac that has it, checks that it queried, quoted the figures with none unverified, and
//! called the unanswerable unavailable, and appends its pass rate to the private
//! `notes/assistant-eval-history.jsonl`:
//! `cargo test assistant_answers_the_eval_set -- --ignored --nocapture`.
use serde::Deserialize;
use serde_json::Value;

use crate::{insights::assistant_tools::Ledger, synthetic::career};

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
    /// The days the planner resolves the question's date phrase to, when it has one.
    #[serde(default)]
    range: Option<DateRange>,
}

#[derive(Debug, Deserialize, PartialEq)]
struct DateRange {
    start: String,
    end: String,
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
    for (name, _) in crate::insights::assistant_tools::TOOLS {
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
    // Only a run of the whole set is a pass rate worth keeping.
    if only.is_none() {
        record_run(evals.len() - failures.len(), evals.len(), &failures);
    }
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

/// The planner, without the model, over every eval question: the leading tool is the one that
/// answers it, the resolved dates are the ones it names, a query Rust runs itself returns the
/// figures the eval expects, and an unanswerable question is never run directly.
#[test]
fn the_planner_routes_every_eval_question_and_resolves_its_dates() {
    use crate::insights::assistant_planner::plan;
    let view = view();
    let ledger = Ledger::new(&view, career::today());
    let mut wrong = Vec::new();
    let mut per_tool = std::collections::BTreeMap::<&str, usize>::new();
    let mut direct = 0;
    let evals = evals();
    for eval in &evals {
        let plan = plan(&eval.question, &view, career::today());
        let range = plan.range.as_ref().map(|range| DateRange {
            start: range.start.to_string(),
            end: range.end.to_string(),
        });
        if range != eval.range {
            wrong.push(format!(
                "{}: dates {range:?}, expected {:?}",
                eval.id, eval.range
            ));
        }
        let Some(tool) = &eval.tool else {
            if plan.direct.is_some() {
                wrong.push(format!("{}: unanswerable, but run directly", eval.id));
            }
            continue;
        };
        *per_tool.entry(tool.as_str()).or_default() += 1;
        if plan.tools.first() != Some(&tool.as_str()) {
            let scores = crate::insights::assistant_planner::scores(
                &eval.question,
                &view,
                plan.range.as_ref(),
            );
            wrong.push(format!(
                "{}: {tool} is not first in {:?}",
                eval.id,
                &scores[..4]
            ));
        }
        if let Some((name, arguments)) = &plan.direct {
            direct += 1;
            let result = ledger.call(name, arguments);
            for (pointer, expected) in &eval.expect {
                if result.pointer(pointer) != Some(expected) {
                    wrong.push(format!(
                        "{}: ran {name} {arguments} directly, {pointer} is {:?}",
                        eval.id,
                        result.pointer(pointer)
                    ));
                }
            }
        }
    }
    for (tool, _) in crate::insights::assistant_tools::TOOLS {
        if !per_tool.contains_key(tool) {
            wrong.push(format!("no eval question needs {tool}"));
        }
    }
    eprintln!("planner: {direct} run directly; questions per tool {per_tool:?}");
    assert!(wrong.is_empty(), "planner:\n{}", wrong.join("\n"));
}

/// Appends one on-device run's pass rate to `notes/assistant-eval-history.jsonl`, so the rate
/// is kept over time. `notes/` is gitignored: a run's record is this Mac's, not the repo's.
#[cfg(target_os = "macos")]
fn record_run(passed: usize, total: usize, failing: &[String]) {
    use std::io::Write;
    let os = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .filter(|version| !version.is_empty());
    let line = serde_json::json!({
        "date": chrono::Local::now().date_naive().to_string(),
        "macos": os,
        "passed": passed,
        "total": total,
        "failing": failing,
    });
    let notes = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../notes");
    let _ = std::fs::create_dir_all(&notes);
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(notes.join("assistant-eval-history.jsonl"))
    {
        let _ = writeln!(file, "{line}");
    }
}
