#[cfg(target_os = "android")]
fn call_activity(method: &str, sig: &str, args: &[jni::objects::JValue]) {
    let ctx = ndk_context::android_context();
    let result = (|| -> jni::errors::Result<()> {
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast())? };
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(ctx.context().cast()) };
        env.call_method(&activity, method, sig, args)?;
        if env.exception_check()? {
            env.exception_clear()?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        log::warn!("activity call {method} failed: {e}");
    }
}

pub fn set_landscape_fullscreen(enabled: bool) {
    #[cfg(target_os = "android")]
    call_activity("setLandscapeFullscreen", "(Z)V", &[enabled.into()]);
    #[cfg(not(target_os = "android"))]
    let _ = enabled;
}

pub fn enter_pip() {
    #[cfg(target_os = "android")]
    call_activity("enterPip", "()V", &[]);
}

pub const PIP_AVAILABLE: bool = cfg!(target_os = "android");
