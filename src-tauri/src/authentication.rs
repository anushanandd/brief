#[cfg(target_os = "macos")]
pub async fn authenticate_sensitive_action() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        use std::sync::mpsc;

        use block2::RcBlock;
        use objc2::runtime::Bool;
        use objc2_foundation::{NSError, NSString};
        use objc2_local_authentication::{LAContext, LAPolicy};

        // SAFETY: LAContext is created, used, and retained on this blocking thread until the
        // framework invokes the reply block. All arguments use the generated framework bindings.
        let context = unsafe { LAContext::new() };
        // macOS manages Touch ID and the normal password fallback for this policy.
        let policy = LAPolicy::DeviceOwnerAuthentication;
        unsafe { context.canEvaluatePolicy_error(policy) }.map_err(|error| error.to_string())?;

        let reason = NSString::from_str("edit provider credentials");
        let (sender, receiver) = mpsc::sync_channel(1);
        let reply = RcBlock::new(move |success: Bool, _error: *mut NSError| {
            let _ = sender.send(success.as_bool());
        });
        unsafe {
            context.evaluatePolicy_localizedReason_reply(policy, &reason, &reply);
        }
        receiver
            .recv()
            .map_err(|_| "Authentication was interrupted".to_string())?
            .then_some(())
            .ok_or_else(|| "Authentication was not completed".to_string())
    })
    .await
    .map_err(|error| format!("Authentication task failed: {error}"))?
}

#[cfg(not(target_os = "macos"))]
pub async fn authenticate_sensitive_action() -> Result<(), String> {
    Err("System authentication for credential changes is currently available on macOS".into())
}
