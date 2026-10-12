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

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
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
        Ok(super::Answer {
            text,
            evidence: super::take_evidence(&evidence),
        })
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

    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
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
#[derive(Clone, Debug, serde::Serialize, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    pub result: serde_json::Value,
}

/// An answer with the queries behind it, so every figure can be checked and kept.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Answer {
    pub text: String,
    pub evidence: Vec<ToolCall>,
}

type Evidence = std::sync::Arc<std::sync::Mutex<Vec<ToolCall>>>;

const MAX_TOOL_RESULT: usize = 6_000;
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
        let zone = crate::insights::calendar::LocalZone;
        let result = crate::insights::ledger::Ledger::new(&self.view, self.today, &zone)
            .call(name, arguments);
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
        let text = result.to_string();
        if text.len() > MAX_TOOL_RESULT {
            return serde_json::json!({
                "error": "That result is too large. Ask about a shorter period or fewer rows."
            })
            .to_string();
        }
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
}
