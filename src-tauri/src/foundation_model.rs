#[cfg(target_os = "macos")]
mod macos {
    use std::{
        ffi::{c_char, c_void, CStr, CString},
        time::Duration,
    };

    use serde::Serialize;
    use tokio::sync::oneshot;

    type ResponseSender = oneshot::Sender<Result<String, String>>;

    unsafe extern "C" {
        fn brief_foundation_model_availability() -> i32;
        fn brief_foundation_model_generate(
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
        let sender = unsafe { Box::from_raw(context.cast::<ResponseSender>()) };
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
        let _ = sender.send(value);
    }

    pub async fn generate(evidence: String) -> Result<String, String> {
        static GENERATION: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
        let _generation = GENERATION
            .try_lock()
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
        let (sender, receiver) = oneshot::channel::<Result<String, String>>();
        let context = Box::into_raw(Box::new(sender)).cast::<c_void>();
        unsafe { brief_foundation_model_generate(prompt.as_ptr(), context, complete) };

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

    pub async fn generate(_evidence: String) -> Result<String, String> {
        Err(status().message.into())
    }
}

#[cfg(not(target_os = "macos"))]
pub use fallback::*;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(test)]
mod tests {
    use super::status;

    #[test]
    fn reports_a_known_availability_state() {
        assert!(["available", "unavailable", "disabled", "notReady",].contains(&status().state));
    }
}
