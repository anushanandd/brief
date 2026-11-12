//! Apple Intelligence, through the on-device Foundation Models bridge: answers about the
//! ledger (`generate`), document summaries, and the readings Rust checks for the investment
//! summary (`read`). The model has no authority. An answer reads only what the read-only ledger
//! queries (`insights::assistant_tools`) return and comes back with the queries that ran; any
//! figure it states that none of them printed is listed (`figures_not_in`), and an answer the
//! queries couldn't support is marked unavailable rather than failing. Requests run one at a
//! time.
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

    /// The serialization lock a request holds: released when Swift completes it, or by Rust
    /// once a timed-out, cancelled request still hasn't completed after `CANCEL_GRACE`.
    type Held = Arc<std::sync::Mutex<Option<tokio::sync::OwnedMutexGuard<()>>>>;

    /// How long a cancelled request has to complete before its lock is released anyway.
    const CANCEL_GRACE: Duration = Duration::from_secs(15);

    fn held(guard: tokio::sync::OwnedMutexGuard<()>) -> Held {
        Arc::new(std::sync::Mutex::new(Some(guard)))
    }

    fn release(generation: &Held) {
        generation
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
    }

    struct ResponseContext {
        sender: oneshot::Sender<Result<String, String>>,
        // Keep serialization until Swift acknowledges completion, even after a timeout,
        // unless that completion is lost (see `CANCEL_GRACE`).
        generation: Held,
        // Swift calls tools only before completion, so the host lives exactly as long. It
        // is never freed by Rust's side of a timeout, so a late call still finds it.
        tools: Option<Box<ToolHost>>,
    }

    /// Waits up to `limit` for Swift's completion. When it doesn't come, `cancel` asks Swift
    /// to stop and the question fails now; if Swift still hasn't completed within `grace`,
    /// the lock is released so a lost completion can't block every later request. The
    /// context, and the tool host in it, stay Swift's to free whenever it completes.
    async fn completion(
        mut receiver: oneshot::Receiver<Result<String, String>>,
        generation: Held,
        cancel: impl FnOnce(),
        limit: Duration,
        grace: Duration,
    ) -> Result<String, String> {
        if let Ok(result) = tokio::time::timeout(limit, &mut receiver).await {
            return result
                .map_err(|_| "Apple Intelligence stopped before responding".to_string())?;
        }
        cancel();
        tauri::async_runtime::spawn(async move {
            if tokio::time::timeout(grace, receiver).await.is_err() {
                release(&generation);
            }
        });
        Err("Apple Intelligence took too long to respond".into())
    }
    struct CancelOnDrop(CString);
    impl Drop for CancelOnDrop {
        fn drop(&mut self) {
            // SAFETY: the bridge copies the NUL-terminated id (`String(cString:)`) during the
            // call; cancelling an unknown or finished request does nothing.
            unsafe { brief_foundation_model_cancel(self.0.as_ptr()) };
        }
    }

    // The Swift bridge (native/FoundationModelBridge.swift). Every string argument is copied
    // with `String(cString:)` before the call returns, and every path of a request calls its
    // `callback` exactly once with the `completion` it was given, from another thread.
    unsafe extern "C" {
        fn brief_foundation_model_availability() -> i32;
        fn brief_foundation_model_cancel(request: *const c_char);
        fn brief_foundation_model_generate(
            request: *const c_char,
            question: *const c_char,
            context: *const c_char,
            tool_names: *const c_char,
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
        fn brief_foundation_model_read(
            request: *const c_char,
            task: *const c_char,
            instructions: *const c_char,
            input: *const c_char,
            completion: *mut c_void,
            callback: extern "C" fn(*mut c_void, *const c_char, *const c_char),
        );
    }

    /// Background work for the model, one piece at a time: document summaries and readings.
    static BACKGROUND: LazyLock<Arc<tokio::sync::Mutex<()>>> =
        LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));

    /// Runs one reading task (`cause` or `guidance`) over text Rust
    /// prepared, with Rust's instructions, returning the model's JSON for Rust to check.
    pub async fn read(
        task: &'static str,
        instructions: &'static str,
        input: String,
    ) -> Result<String, String> {
        let generation = BACKGROUND.clone().lock_owned().await;
        if status().state != "available" {
            return Err(status().message.into());
        }
        let task = CString::new(task).map_err(|_| "Invalid reading task".to_string())?;
        let instructions =
            CString::new(instructions).map_err(|_| "Invalid reading task".to_string())?;
        let input = CString::new(input.replace('\0', " "))
            .map_err(|_| "The reading text is invalid".to_string())?;
        let request = CancelOnDrop(
            CString::new(format!("reading-{}", uuid::Uuid::new_v4()))
                .map_err(|_| "Invalid reading request")?,
        );
        let (sender, receiver) = oneshot::channel::<Result<String, String>>();
        let generation = held(generation);
        let completion = Box::into_raw(Box::new(ResponseContext {
            sender,
            generation: generation.clone(),
            tools: None,
        }))
        .cast::<c_void>();
        // SAFETY: the strings outlive the call and are copied during it; `completion` is a
        // leaked `ResponseContext` that the bridge hands to `complete` once, which reclaims it.
        unsafe {
            brief_foundation_model_read(
                request.0.as_ptr(),
                task.as_ptr(),
                instructions.as_ptr(),
                input.as_ptr(),
                completion,
                complete,
            )
        };
        completion_of(receiver, generation, request, Duration::from_secs(60)).await
    }

    /// `completion` for a native request, cancelled by dropping it.
    async fn completion_of(
        receiver: oneshot::Receiver<Result<String, String>>,
        generation: Held,
        request: CancelOnDrop,
        limit: Duration,
    ) -> Result<String, String> {
        completion(
            receiver,
            generation,
            move || drop(request),
            limit,
            CANCEL_GRACE,
        )
        .await
    }

    /// Summarizes a document's text, one document at a time, behind any question in progress.
    pub async fn summarize(text: String) -> Result<String, String> {
        let generation = BACKGROUND.clone().lock_owned().await;
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
        let generation = held(generation);
        let completion = Box::into_raw(Box::new(ResponseContext {
            sender,
            generation: generation.clone(),
            tools: None,
        }))
        .cast::<c_void>();
        // SAFETY: as in `read`: strings copied during the call, `completion` reclaimed once by
        // `complete`.
        unsafe {
            brief_foundation_model_summarize(
                request.0.as_ptr(),
                text.as_ptr(),
                completion,
                complete,
            )
        };
        let text = completion_of(receiver, generation, request, Duration::from_secs(90)).await?;
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
            // SAFETY: `host` points into the `ResponseContext` that `generate` leaked, which
            // `complete` frees only after the model's last tool call: Swift calls tools inside
            // `session.respond`, which returns before the completion callback. The host is
            // only read (`call` takes `&self`).
            let host = unsafe { &*host.cast::<ToolHost>() };
            // SAFETY: Swift passes NUL-terminated strings (`withCString`) that live for this call.
            let name = unsafe { CStr::from_ptr(name) }.to_string_lossy();
            // SAFETY: as for `name`.
            let arguments = unsafe { CStr::from_ptr(arguments) }.to_string_lossy();
            let text = host.call(&name, &arguments);
            CString::new(text).ok()
        });
        result
            .ok()
            .flatten()
            .map_or(std::ptr::null_mut(), CString::into_raw)
    }

    /// Frees a tool result. Panics never cross the C boundary.
    extern "C" fn tool_free(value: *mut c_char) {
        if !value.is_null() {
            // SAFETY: `value` came from `CString::into_raw` in `tool_call`, and Swift frees each
            // result once (`defer { release(result) }`) after copying it.
            let _ = std::panic::catch_unwind(|| drop(unsafe { CString::from_raw(value) }));
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
        // SAFETY: takes nothing and returns an integer.
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

    /// Swift's completion of a request: frees its context and releases its lock. Panics never
    /// cross the C boundary; the context is freed while unwinding all the same.
    extern "C" fn complete(context: *mut c_void, result: *const c_char, error: *const c_char) {
        if context.is_null() {
            return;
        }
        // SAFETY: `context` is the `Box<ResponseContext>` a request leaked, and the bridge calls
        // this once per request, so the box is reclaimed exactly once.
        let response = unsafe { Box::from_raw(context.cast::<ResponseContext>()) };
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
            let ResponseContext {
                sender,
                generation,
                tools,
            } = *response;
            // SAFETY: a non-null `error` or `result` is a NUL-terminated string from Swift's
            // `withCString`, alive for this call.
            let value = if !error.is_null() {
                Err(unsafe { CStr::from_ptr(error) }
                    .to_string_lossy()
                    .into_owned())
            } else if !result.is_null() {
                // SAFETY: as for `error`.
                Ok(unsafe { CStr::from_ptr(result) }
                    .to_string_lossy()
                    .into_owned())
            } else {
                Err("Apple Intelligence returned no explanation".into())
            };
            let _ = sender.send(value);
            drop(tools);
            release(&generation);
        }));
    }

    pub fn cancel(request_id: String) {
        if let Ok(id) = CString::new(request_id) {
            // SAFETY: the bridge copies the NUL-terminated id during the call.
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
        // Rust settles the question's dates and, when one query clearly answers it, runs that
        // query itself; otherwise only the tools the question points to are offered, so their
        // definitions fit the context.
        let mut tools = tools;
        let (offered, context) = super::planned(question, context, &mut tools);
        let tool_names = CString::new(offered).map_err(|_| "Invalid tool names".to_string())?;
        let question = CString::new(question)
            .map_err(|_| "The question contains unsupported text".to_string())?;
        let context = CString::new(context.replace('\0', " "))
            .map_err(|_| "The question context is invalid".to_string())?;
        if request_id.is_empty() || request_id.len() > 100 {
            return Err("Invalid question request".into());
        }
        let request =
            CancelOnDrop(CString::new(request_id).map_err(|_| "Invalid question request")?);
        let (sender, receiver) = oneshot::channel::<Result<String, String>>();
        let evidence = tools.evidence();
        let generation = held(generation);
        let mut response = Box::new(ResponseContext {
            sender,
            generation: generation.clone(),
            tools: Some(Box::new(tools)),
        });
        let tool_host = response
            .tools
            .as_deref_mut()
            .map_or(std::ptr::null_mut(), |tools| {
                (tools as *mut ToolHost).cast::<c_void>()
            });
        let completion = Box::into_raw(response).cast::<c_void>();
        // SAFETY: strings are copied during the call; `completion` (holding the tool host) is
        // reclaimed once by `complete`; `tool_call` and `tool_free` match the bridge's types.
        unsafe {
            brief_foundation_model_generate(
                request.0.as_ptr(),
                question.as_ptr(),
                context.as_ptr(),
                tool_names.as_ptr(),
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

        let text = completion_of(receiver, generation, request, Duration::from_secs(60)).await?;
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
                generation: held(gate.clone().lock_owned().await),
                tools: Some(Box::new(ToolHost::empty())),
            }))
            .cast::<c_void>();
            drop(receiver);
            assert!(gate.try_lock().is_err());
            let error = CString::new("cancelled").unwrap();
            complete(context, std::ptr::null(), error.as_ptr());
            assert!(gate.try_lock().is_ok());
        }

        #[tokio::test]
        async fn a_lost_completion_fails_the_question_and_releases_the_lock_after_grace() {
            let gate = Arc::new(tokio::sync::Mutex::new(()));
            let (sender, receiver) = oneshot::channel();
            let generation = held(gate.clone().lock_owned().await);
            let context = Box::into_raw(Box::new(ResponseContext {
                sender,
                generation: generation.clone(),
                tools: Some(Box::new(ToolHost::empty())),
            }));
            let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let flag = cancelled.clone();
            let short = Duration::from_millis(20);
            let answer = completion(
                receiver,
                generation,
                move || flag.store(true, std::sync::atomic::Ordering::SeqCst),
                short,
                Duration::from_millis(300),
            )
            .await;
            assert_eq!(
                answer.unwrap_err(),
                "Apple Intelligence took too long to respond"
            );
            assert!(cancelled.load(std::sync::atomic::Ordering::SeqCst));
            // Still held during the grace period, then released though Swift never answered.
            assert!(gate.try_lock().is_err());
            let released = tokio::time::timeout(Duration::from_secs(5), gate.clone().lock_owned());
            assert!(released.await.is_ok());
            // A completion that arrives late still frees its context and tool host safely.
            let error = CString::new("cancelled").unwrap();
            complete(context.cast::<c_void>(), std::ptr::null(), error.as_ptr());
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

    pub async fn read(
        _task: &'static str,
        _instructions: &'static str,
        _input: String,
    ) -> Result<String, String> {
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
    /// The days the question named, as Rust resolved them; date-taking queries use these
    /// rather than dates the model computed.
    range: Option<(chrono::NaiveDate, chrono::NaiveDate)>,
}

/// Plans `question` (`insights::assistant_planner`) for one request: the tool names to offer,
/// comma-separated, and the context the model reads. When Rust can run the one query that
/// answers it, it runs it here, so the model is offered no tools and reads the result in its
/// context; the query is evidence like any other, and the answer's figures are checked
/// against it. Otherwise the context says what the question's date phrase means.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn planned(question: &str, context: String, tools: &mut ToolHost) -> (String, String) {
    let plan = crate::insights::assistant_planner::plan(question, &tools.view, tools.today);
    tools.range = plan.range.as_ref().map(|range| (range.start, range.end));
    if let Some((name, arguments)) = &plan.direct {
        if let Ok(result) = tools.query(name, arguments) {
            if let Some((result, text)) = fit_result(result, MAX_TOOL_RESULT) {
                if let Ok(mut evidence) = tools.evidence.lock() {
                    evidence.push(ToolCall {
                        name: (*name).to_owned(),
                        arguments: arguments.clone(),
                        result,
                    });
                }
                return (
                    String::new(),
                    format!("{context}\n\nQuery result ({name}):\n{text}"),
                );
            }
        }
    }
    let context = match &plan.range {
        Some(range) => format!(
            "{context} In this question, \"{}\" means {} through {}.",
            range.phrase, range.start, range.end
        ),
        None => context,
    };
    (plan.tools.join(","), context)
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
            range: None,
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
        let result = crate::insights::assistant_tools::Ledger::new(&self.view, self.today)
            .call(name, arguments);
        match result.get("error").and_then(serde_json::Value::as_str) {
            Some(error) => Err(error.to_owned()),
            None => Ok(result),
        }
    }

    #[cfg(all(test, target_os = "macos"))]
    fn empty() -> Self {
        let seed = serde_json::from_str(include_str!("../../src/data/seed.json"))
            .expect("the seed is a snapshot");
        Self::new(crate::insights::View::of(seed), chrono::NaiveDate::MIN)
    }

    pub fn call(&self, name: &str, arguments: &str) -> String {
        let mut arguments = serde_json::from_str::<serde_json::Value>(arguments)
            .ok()
            .filter(serde_json::Value::is_object)
            .unwrap_or_else(|| serde_json::json!({}));
        // The question's dates, as Rust resolved them, replace any the model computed.
        if let Some((start, end)) = self.range {
            if crate::insights::assistant_planner::DATED.contains(&name) {
                arguments["start"] = serde_json::json!(start.to_string());
                arguments["end"] = serde_json::json!(end.to_string());
            }
        }
        // The on-device model can call one query again and again until its context overflows;
        // a repeat gets a few words pointing back, not the result again.
        let repeated = self.evidence.lock().is_ok_and(|calls| {
            calls
                .iter()
                .any(|call| call.name == name && call.arguments == arguments)
        });
        if repeated {
            #[cfg(test)]
            eprintln!("  tool {name} {arguments} repeated");
            return serde_json::json!({
                "note": "This query already ran and its result is above. Answer from it now."
            })
            .to_string();
        }
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
        // The eval set's run shows each call the model made, to diagnose context use.
        #[cfg(test)]
        eprintln!("  tool {name} {arguments} -> {} chars", text.len());
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

        // A repeat points back to the result it already has; successful queries are kept as
        // evidence once each, and failed ones are not.
        assert!(host.call("listAccounts", "{}").contains("already ran"));
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
    fn a_clear_question_is_queried_by_rust_and_dates_reach_the_model_resolved() {
        let seed = serde_json::from_str(include_str!("../../src/data/seed.json")).unwrap();
        let view = crate::insights::View::of(seed);
        let today = chrono::NaiveDate::from_ymd_opt(2026, 8, 31).unwrap();

        // Rust runs the one query that answers it; the model is offered no tools.
        let mut host = ToolHost::new(view.clone(), today);
        let (tools, context) = planned(
            "What is my brokerage account worth?",
            "Today.".into(),
            &mut host,
        );
        assert_eq!(tools, "");
        assert!(context.starts_with("Today.\n\nQuery result (listAccounts):\n{"));
        let evidence = take_evidence(&host.evidence());
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].name, "listAccounts");

        // A search is the model's to phrase; the context says what "last month" means, and
        // the query the model makes uses those days, whatever dates it chose.
        let mut host = ToolHost::new(view, today);
        let (tools, context) = planned(
            "When did I buy something at Netflix last month?",
            "Today.".into(),
            &mut host,
        );
        assert!(tools.split(',').any(|tool| tool == "searchTransactions"));
        assert!(context.ends_with("\"last month\" means 2026-07-01 through 2026-07-31."));
        host.call(
            "searchTransactions",
            r#"{"text": "netflix", "start": "2020-01-01", "end": "2020-01-31"}"#,
        );
        let evidence = take_evidence(&host.evidence());
        assert_eq!(evidence[0].arguments["start"], "2026-07-01");
        assert_eq!(evidence[0].arguments["end"], "2026-07-31");
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
