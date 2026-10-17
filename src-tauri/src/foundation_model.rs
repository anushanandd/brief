#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::{c_char, c_void, CStr, CString},
        sync::{Arc, LazyLock},
        time::Duration,
    };

    use serde::Serialize;
    use tokio::sync::oneshot;

    pub use super::ToolHost;

    struct ResponseContext {
        sender: oneshot::Sender<Result<String, String>>,
        // Keep serialization until Swift acknowledges completion, even after a timeout.
        _generation: tokio::sync::OwnedMutexGuard<()>,
        // Swift calls tools only before completion, so the host lives exactly as long.
        tools: Option<Box<ToolHost>>,
    }
    struct CancelOnDrop(CString);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            unsafe { brief_foundation_model_cancel(self.0.as_ptr()) };
        }
    }

    unsafe extern "C" {
        fn brief_foundation_model_availability() -> i32;
        fn brief_foundation_model_cancel(request: *const c_char);
        fn brief_foundation_model_generate(
            request: *const c_char,
            question: *const c_char,
            context: *const c_char,
            tool_host: *mut c_void,
            tool_callback: extern "C" fn(*mut c_void, *const c_char, *const c_char) -> *mut c_char,
            tool_free: extern "C" fn(*mut c_char),
            completion: *mut c_void,
            callback: extern "C" fn(*mut c_void, *const c_char, *const c_char),
        );
        fn brief_foundation_model_summarize(
            request: *const c_char,
            text: *const c_char,
            completion: *mut c_void,
            callback: extern "C" fn(*mut c_void, *const c_char, *const c_char),
        );
    }

    /// Summarizes a document's text, one document at a time, behind any question in progress.
    pub async fn summarize(text: String) -> Result<String, String> {
        static SUMMARIES: LazyLock<Arc<tokio::sync::Mutex<()>>> =
            LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));
        let generation = SUMMARIES.clone().lock_owned().await;
        if status().state != "available" {
            return Err(status().message.into());
        }
        let text = CString::new(text.replace('\0', " "))
            .map_err(|_| "The document text is invalid".to_string())?;
        let request = CancelOnDrop(
            CString::new(format!("summary-{}", uuid::Uuid::new_v4()))
                .map_err(|_| "Invalid summary request")?,
        );
        let (sender, receiver) = oneshot::channel::<Result<String, String>>();
        let completion = Box::into_raw(Box::new(ResponseContext {
            sender,
            _generation: generation,
            tools: None,
        }))
        .cast::<c_void>();
        unsafe {
            brief_foundation_model_summarize(
                request.0.as_ptr(),
                text.as_ptr(),
                completion,
                complete,
            )
        };
        let text = tokio::time::timeout(Duration::from_secs(90), receiver)
            .await
            .map_err(|_| "Apple Intelligence took too long to respond".to_string())?
            .map_err(|_| "Apple Intelligence stopped before responding".to_string())??;
        Ok(text.trim().to_owned())
    }

    /// Runs one ledger query for Swift. Panics never cross the C boundary.
    extern "C" fn tool_call(
        host: *mut c_void,
        name: *const c_char,
        arguments: *const c_char,
    ) -> *mut c_char {
        if host.is_null() || name.is_null() || arguments.is_null() {
            return std::ptr::null_mut();
        }
        let result = std::panic::catch_unwind(|| {
            let host = unsafe { &*host.cast::<ToolHost>() };
            let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
            let arguments = unsafe { CStr::from_ptr(arguments) }.to_string_lossy();
            host.call(&name, &arguments)
        });
        result
            .ok()
            .and_then(|text| CString::new(text).ok())
            .map_or(std::ptr::null_mut(), CString::into_raw)
    }

    extern "C" fn tool_free(value: *mut c_char) {
        if !value.is_null() {
            drop(unsafe { CString::from_raw(value) });
        }
    }

    #[derive(Serialize, ts_rs::TS)]
    #[serde(rename_all = "camelCase")]
    #[ts(export)]
    pub struct FoundationModelStatus {
        pub state: &'static str,
        pub message: &'static str,
    }

    pub fn status() -> FoundationModelStatus {
        let (state, message) = match unsafe { brief_foundation_model_availability() } {
            0 => ("available", "Apple Intelligence is ready"),
            1 => (
                "unavailable",
                "This Mac does not support Apple Intelligence",
            ),
            2 => ("disabled", "Turn on Apple Intelligence in System Settings"),
            3 => ("notReady", "The on-device language model is not ready yet"),
            _ => ("unavailable", "Apple Intelligence is unavailable"),
        };
        FoundationModelStatus { state, message }
    }

    extern "C" fn complete(context: *mut c_void, result: *const c_char, error: *const c_char) {
        let response = unsafe { Box::from_raw(context.cast::<ResponseContext>()) };
        let value = if !error.is_null() {
            Err(unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned())
        } else if !result.is_null() {
            Ok(unsafe { CStr::from_ptr(result) }
                .to_string_lossy()
                .into_owned())
        } else {
            Err("Apple Intelligence returned no explanation".into())
        };
        let _ = response.sender.send(value);
    }

    pub fn cancel(request_id: String) {
        if let Ok(id) = CString::new(request_id) {
            unsafe { brief_foundation_model_cancel(id.as_ptr()) };
        }
    }

    pub async fn generate(
        question: String,
        context: String,
        request_id: String,
        started: tauri::ipc::Channel<()>,
        tools: ToolHost,
    ) -> Result<super::Answer, String> {
        static GENERATION: LazyLock<Arc<tokio::sync::Mutex<()>>> =
            LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));
        let generation = GENERATION
            .clone()
            .try_lock_owned()
            .map_err(|_| "An answer is already being generated")?;
        let status = status();
        if status.state != "available" {
            return Err(status.message.into());
        }
        let question = question.trim();
        if question.chars().count() < 4 {
            return Err("Ask a question of at least four characters".into());
        }
        if question.chars().count() > 1_000 || context.chars().count() > 2_000 {
            return Err("The question is too long".into());
        }
        let question = CString::new(question)
            .map_err(|_| "The question contains unsupported text".to_string())?;
        let context =
            CString::new(context).map_err(|_| "The question context is invalid".to_string())?;
        if request_id.is_empty() || request_id.len() > 100 {
            return Err("Invalid question request".into());
        }
        let request =
            CancelOnDrop(CString::new(request_id).map_err(|_| "Invalid question request")?);
        let (sender, receiver) = oneshot::channel::<Result<String, String>>();
        let evidence = tools.evidence();
        let mut response = Box::new(ResponseContext {
            sender,
            _generation: generation,
            tools: Some(Box::new(tools)),
        });
        let tool_host = response
            .tools
            .as_deref_mut()
            .map_or(std::ptr::null_mut(), |tools| {
                (tools as *mut ToolHost).cast::<c_void>()
            });
        let completion = Box::into_raw(response).cast::<c_void>();
        unsafe {
            brief_foundation_model_generate(
                request.0.as_ptr(),
                question.as_ptr(),
                context.as_ptr(),
                tool_host,
                tool_call,
                tool_free,
                completion,
                complete,
            )
        };
        // The frontend defers cancellation until native registration has completed.
        started
            .send(())
            .map_err(|_| "Answer listener disconnected")?;

        let text = tokio::time::timeout(Duration::from_secs(60), receiver)
            .await
            .map_err(|_| "Apple Intelligence took too long to respond".to_string())?
            .map_err(|_| "Apple Intelligence stopped before responding".to_string())??;
        Ok(super::Answer::checked(
            &text,
            super::take_evidence(&evidence),
        ))
    }

    #[cfg(test)]
    mod callback_tests {
        use super::*;

        #[tokio::test]
        async fn abandoned_receiver_keeps_generation_locked_until_native_completion() {
            let gate = Arc::new(tokio::sync::Mutex::new(()));
            let (sender, receiver) = oneshot::channel();
            let context = Box::into_raw(Box::new(ResponseContext {
                sender,
                _generation: gate.clone().lock_owned().await,
                tools: Some(Box::new(ToolHost::empty())),
            }))
            .cast::<c_void>();
            drop(receiver);
            assert!(gate.try_lock().is_err());
            let error = CString::new("cancelled").unwrap();
            complete(context, std::ptr::null(), error.as_ptr());
            assert!(gate.try_lock().is_ok());
        }
    }
}

#[cfg(not(target_os = "macos"))]
mod fallback {
    use serde::Serialize;

    #[derive(Serialize, ts_rs::TS)]
    #[serde(rename_all = "camelCase")]
    #[ts(export)]
    pub struct FoundationModelStatus {
        pub state: &'static str,
        pub message: &'static str,
    }

    pub fn status() -> FoundationModelStatus {
        FoundationModelStatus {
            state: "unavailable",
            message: "Apple Intelligence is available only on supported Macs",
        }
    }

    pub fn cancel(_request_id: String) {}

    pub async fn summarize(_text: String) -> Result<String, String> {
        Err(status().message.into())
    }

    pub async fn generate(
        _question: String,
        _context: String,
        _request_id: String,
        _started: tauri::ipc::Channel<()>,
        _tools: super::ToolHost,
    ) -> Result<super::Answer, String> {
        Err(status().message.into())
    }
}

#[cfg(not(target_os = "macos"))]
pub use fallback::*;
#[cfg(target_os = "macos")]
pub use macos::*;

/// Read-only ledger queries for one Apple Intelligence request, over the snapshot as it was
/// when the question was asked. Results are bounded so they fit the on-device context.
pub struct ToolHost {
    view: std::sync::Arc<crate::insights::View>,
    today: chrono::NaiveDate,
    evidence: Evidence,
}

/// One ledger query the model ran and the figures it returned: the evidence for an answer.
#[derive(Clone, Debug, serde::Serialize, PartialEq, ts_rs::TS)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
}

mod figures;

/// The figures `text` states that `source` doesn't print.
pub fn figures_not_in(text: &str, source: &str) -> Vec<String> {
    figures::unverified(
        text,
        &[ToolCall {
            name: "document".into(),
            arguments: serde_json::Value::Null,
            result: serde_json::Value::String(source.into()),
        }],
    )
}

/// Whether the queries returned what the question needed. Not having the figures is an answer,
/// shown as such, not an error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "AnswerOutcome")]
pub enum Outcome {
    Answered,
    Unavailable,
}

/// An answer with the queries behind it, so every figure can be checked and kept, and the
/// figures it states that none of those queries returned.
#[derive(Clone, Debug, serde::Serialize, ts_rs::TS)]
#[ts(export, rename = "AssistantAnswer")]
pub struct Answer {
    pub text: String,
    pub outcome: Outcome,
    pub evidence: Vec<ToolCall>,
    pub unverified: Vec<String>,
}

impl Answer {
    /// The model's reply, `{"outcome", "text"}` as JSON or plain text, checked against the
    /// queries that ran.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn checked(reply: &str, evidence: Vec<ToolCall>) -> Self {
        #[derive(serde::Deserialize)]
        struct Reply {
            outcome: String,
            text: String,
        }
        let (text, outcome) = match serde_json::from_str::<Reply>(reply) {
            Ok(reply) if reply.outcome == "unavailable" => (reply.text, Outcome::Unavailable),
            Ok(reply) => (reply.text, Outcome::Answered),
            Err(_) => (reply.to_owned(), Outcome::Answered),
        };
        let text = text.trim().to_owned();
        Self {
            unverified: figures::unverified(&text, &evidence),
            text,
            outcome,
            evidence,
        }
    }
}

type Evidence = std::sync::Arc<std::sync::Mutex<Vec<ToolCall>>>;

/// The on-device model reads about 4,096 tokens per question on macOS 26, shared by the
/// instructions, every tool's description, the question, and each result, so one result stays
/// near 2,000 characters and a long one gives up its last rows rather than failing.
const MAX_TOOL_RESULT: usize = 2_000;
const MAX_EVIDENCE: usize = 6;

/// The successful queries of one request, in order, without repeats.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn take_evidence(evidence: &Evidence) -> Vec<ToolCall> {
    let mut calls = Vec::<ToolCall>::new();
    for call in evidence
        .lock()
        .map(|calls| calls.clone())
        .unwrap_or_default()
    {
        if !calls
            .iter()
            .any(|known| known.name == call.name && known.arguments == call.arguments)
        {
            calls.push(call);
        }
    }
    calls.truncate(MAX_EVIDENCE);
    calls
}

/// A result within `max` characters: the result itself, or the result with its longest list
/// cut from the end and the number of rows left out, so the model knows it saw only part.
fn fit_result(mut result: serde_json::Value, max: usize) -> Option<(serde_json::Value, String)> {
    let text = result.to_string();
    if text.len() <= max {
        return Some((result, text));
    }
    let longest = result.as_object().and_then(|fields| {
        fields
            .iter()
            .filter_map(|(key, value)| Some((key.clone(), value.as_array()?.len())))
            .max_by_key(|(_, rows)| *rows)
            .map(|(key, _)| key)
    })?;
    let mut omitted = 0usize;
    loop {
        let rows = result.get_mut(&longest)?.as_array_mut()?;
        rows.pop()?;
        omitted += 1;
        result["omittedRows"] = serde_json::json!(omitted);
        let text = result.to_string();
        if text.len() <= max {
            return Some((result, text));
        }
    }
}

impl ToolHost {
    pub fn new(view: std::sync::Arc<crate::insights::View>, today: chrono::NaiveDate) -> Self {
        Self {
            view,
            today,
            evidence: Evidence::default(),
        }
    }

    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn evidence(&self) -> Evidence {
        self.evidence.clone()
    }

    /// Runs one query and returns its figures, or the reason it could not be answered.
    pub fn query(
        &self,
        name: &str,
        arguments: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let result =
            crate::insights::ledger::Ledger::new(&self.view, self.today).call(name, arguments);
        match result.get("error").and_then(serde_json::Value::as_str) {
            Some(error) => Err(error.to_owned()),
            None => Ok(result),
        }
    }

    #[cfg(all(test, target_os = "macos"))]
    fn empty() -> Self {
        let seed = serde_json::from_str(include_str!("../../src/data/seed.json")).unwrap();
        Self::new(crate::insights::View::of(seed), chrono::NaiveDate::MIN)
    }

    pub fn call(&self, name: &str, arguments: &str) -> String {
        let arguments = serde_json::from_str::<serde_json::Value>(arguments)
            .ok()
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}));
        let result = match self.query(name, &arguments) {
            Ok(result) => result,
            Err(error) => return serde_json::json!({ "error": error }).to_string(),
        };
        let Some((result, text)) = fit_result(result, MAX_TOOL_RESULT) else {
            return serde_json::json!({
                "error": "That result is too large. Ask about a shorter period or fewer rows."
            })
            .to_string();
        };
        // Only figures the model actually received count as evidence.
        if let Ok(mut evidence) = self.evidence.lock() {
            evidence.push(ToolCall {
                name: name.to_owned(),
                arguments,
                result,
            });
        }
        text
    }
}

#[cfg(test)]
mod tool_tests {
    use super::*;

    #[test]
    fn tools_answer_from_saved_data_and_bound_their_output() {
        let seed = serde_json::from_str(include_str!("../../src/data/seed.json")).unwrap();
        let host = ToolHost::new(
            crate::insights::View::of(seed),
            chrono::NaiveDate::from_ymd_opt(2026, 8, 31).unwrap(),
        );
        let accounts: serde_json::Value =
            serde_json::from_str(&host.call("listAccounts", "not json")).unwrap();
        assert!(accounts["accounts"]
            .as_array()
            .is_some_and(|rows| !rows.is_empty()));
        let wide: serde_json::Value = serde_json::from_str(&host.call(
            "searchTransactions",
            r#"{"limit": 1000, "start": "2020-01-01"}"#,
        ))
        .unwrap();
        assert!(wide["shown"].as_u64().unwrap() <= 25);
        assert!(host.call("searchTransactions", "{}").len() <= MAX_TOOL_RESULT);

        // Successful queries are kept as evidence once each; failed ones are not.
        host.call("listAccounts", "{}");
        host.call("noSuchTool", "{}");
        let evidence = take_evidence(&host.evidence());
        assert_eq!(
            evidence
                .iter()
                .map(|call| call.name.as_str())
                .collect::<Vec<_>>(),
            vec!["listAccounts", "searchTransactions", "searchTransactions"]
        );
        assert_eq!(evidence[0].result, accounts);
        assert_eq!(evidence[1].arguments["limit"], 1000);
        assert!(host.query("noSuchTool", &serde_json::json!({})).is_err());
        assert!(host.query("cashFlow", &serde_json::json!({})).is_ok());
    }

    #[test]
    fn replies_carry_their_outcome_and_unverified_figures() {
        let evidence = vec![ToolCall {
            name: "cashFlow".into(),
            arguments: serde_json::json!({}),
            result: serde_json::json!({ "spending": -812.4 }),
        }];
        let answer = Answer::checked(
            r#"{"outcome":"answered","text":" You spent $812.40, not $900. "}"#,
            evidence.clone(),
        );
        assert_eq!(answer.outcome, Outcome::Answered);
        assert_eq!(answer.text, "You spent $812.40, not $900.");
        assert_eq!(answer.unverified, vec!["$900"]);

        let missing = Answer::checked(
            r#"{"outcome":"unavailable","text":"No saved data covers 2019."}"#,
            Vec::new(),
        );
        assert_eq!(missing.outcome, Outcome::Unavailable);
        assert!(missing.unverified.is_empty());

        let plain = Answer::checked("You spent $812.40.", evidence);
        assert_eq!(plain.outcome, Outcome::Answered);
        assert!(plain.unverified.is_empty());
    }

    #[test]
    fn long_results_drop_their_last_rows_and_say_how_many() {
        let rows = (0..200)
            .map(|index| serde_json::json!({ "name": format!("Row {index}"), "amount": index }))
            .collect::<Vec<_>>();
        let (fitted, text) =
            fit_result(serde_json::json!({ "shown": 200, "rows": rows }), 500).unwrap();
        assert!(text.len() <= 500);
        let kept = fitted["rows"].as_array().unwrap().len();
        assert_eq!(fitted["omittedRows"], 200 - kept);
        assert_eq!(fitted["rows"][0]["name"], "Row 0");

        let small = serde_json::json!({ "total": 12.5 });
        assert_eq!(fit_result(small.clone(), 500).unwrap().0, small);
        assert!(fit_result(serde_json::json!({ "text": "x".repeat(600) }), 500).is_none());
    }
}
