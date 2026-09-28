#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::{c_char, c_void, CStr, CString},
        sync::{Arc, LazyLock},
        time::Duration,
    };

    use serde::Serialize;
    use tokio::sync::oneshot;

    struct ResponseContext {
        sender: oneshot::Sender<Result<String, String>>,
        // Keep serialization until Swift acknowledges completion, even after a timeout.
        _generation: tokio::sync::OwnedMutexGuard<()>,
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
            prompt: *const c_char,
            context: *mut c_void,
            callback: extern "C" fn(*mut c_void, *const c_char, *const c_char),
        );
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
            }))
            .cast::<c_void>();
            drop(receiver);
            assert!(gate.try_lock().is_err());
            let error = CString::new("cancelled").unwrap();
            complete(context, std::ptr::null(), error.as_ptr());
            assert!(gate.try_lock().is_ok());
        }
    }

    pub fn cancel(request_id: String) {
        if let Ok(id) = CString::new(request_id) {
            unsafe { brief_foundation_model_cancel(id.as_ptr()) };
        }
    }

    pub async fn generate(
        evidence: String,
        request_id: String,
        started: tauri::ipc::Channel<()>,
    ) -> Result<String, String> {
        static GENERATION: LazyLock<Arc<tokio::sync::Mutex<()>>> =
            LazyLock::new(|| Arc::new(tokio::sync::Mutex::new(())));
        let generation = GENERATION
            .clone()
            .try_lock_owned()
            .map_err(|_| "An explanation is already being generated")?;
        if status().state != "available" {
            return Err(status().message.into());
        }
        let evidence = evidence.trim();
        if evidence.is_empty() {
            return Err("Add evidence before requesting an explanation".into());
        }
        if evidence.chars().count() > 12_000 {
            return Err("Explanation evidence exceeds the 12,000 character limit".into());
        }
        let prompt = CString::new(evidence)
            .map_err(|_| "The explanation evidence contains unsupported text".to_string())?;
        if request_id.is_empty() || request_id.len() > 100 {
            return Err("Invalid explanation request".into());
        }
        let request =
            CancelOnDrop(CString::new(request_id).map_err(|_| "Invalid explanation request")?);
        let (sender, receiver) = oneshot::channel::<Result<String, String>>();
        let context = Box::into_raw(Box::new(ResponseContext {
            sender,
            _generation: generation,
        }))
        .cast::<c_void>();
        unsafe {
            brief_foundation_model_generate(request.0.as_ptr(), prompt.as_ptr(), context, complete)
        };
        // The frontend defers cancellation until native registration has completed.
        started
            .send(())
            .map_err(|_| "Explanation listener disconnected")?;

        tokio::time::timeout(Duration::from_secs(45), receiver)
            .await
            .map_err(|_| "Apple Intelligence took too long to respond".to_string())?
            .map_err(|_| "Apple Intelligence stopped before responding".to_string())?
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
        _evidence: String,
        _request_id: String,
        _started: tauri::ipc::Channel<()>,
    ) -> Result<String, String> {
        Err(status().message.into())
    }
}

#[cfg(not(target_os = "macos"))]
pub use fallback::*;
#[cfg(target_os = "macos")]
pub use macos::*;
