#[cfg(target_os = "android")]
static ACTIVITY: std::sync::OnceLock<usize> = std::sync::OnceLock::new();

#[cfg(target_os = "android")]
pub fn set_activity(ptr: *mut std::ffi::c_void) {
    let _ = ACTIVITY.set(ptr as usize);
}

#[cfg(target_os = "android")]
fn activity_ptr() -> *mut jni::sys::_jobject {
    match ACTIVITY.get() {
        Some(p) => *p as *mut jni::sys::_jobject,
        None => ndk_context::android_context().context().cast(),
    }
}

#[cfg(target_os = "android")]
fn call_activity(method: &str, sig: &str, args: &[jni::objects::JValue]) {
    let ctx = ndk_context::android_context();
    let result = (|| -> jni::errors::Result<()> {
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast())? };
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr()) };
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

#[cfg(target_os = "android")]
fn call_activity_str(method: &str, arg: &str) {
    let ctx = ndk_context::android_context();
    let result = (|| -> jni::errors::Result<()> {
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast())? };
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr()) };
        let s = env.new_string(arg)?;
        env.call_method(
            &activity,
            method,
            "(Ljava/lang/String;)Z",
            &[jni::objects::JValue::Object(&s)],
        )?;
        if env.exception_check()? {
            env.exception_clear()?;
        }
        Ok(())
    })();
    if let Err(e) = result {
        log::warn!("activity call {method} failed: {e}");
    }
}

#[cfg(target_os = "android")]
fn read_clipboard() -> String {
    let ctx = ndk_context::android_context();
    let result = (|| -> jni::errors::Result<String> {
        let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast())? };
        let mut env = vm.attach_current_thread()?;
        let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr()) };
        let value = env
            .call_method(&activity, "getClipboardText", "()Ljava/lang/String;", &[])?
            .l()?;
        if env.exception_check()? {
            env.exception_clear()?;
            return Ok(String::new());
        }
        let text: String = env.get_string(&jni::objects::JString::from(value))?.into();
        Ok(text)
    })();
    result.unwrap_or_else(|e| {
        log::warn!("clipboard read failed: {e}");
        String::new()
    })
}

pub fn clipboard_text() -> String {
    #[cfg(target_os = "android")]
    return read_clipboard();
    #[cfg(not(target_os = "android"))]
    String::new()
}

pub fn set_clipboard_text(text: &str) {
    #[cfg(target_os = "android")]
    {
        let ctx = ndk_context::android_context();
        let result = (|| -> jni::errors::Result<()> {
            let vm = unsafe { jni::JavaVM::from_raw(ctx.vm().cast())? };
            let mut env = vm.attach_current_thread()?;
            let activity = unsafe { jni::objects::JObject::from_raw(activity_ptr()) };
            let s = env.new_string(text)?;
            env.call_method(
                &activity,
                "setClipboardText",
                "(Ljava/lang/String;)V",
                &[jni::objects::JValue::Object(&s)],
            )?;
            if env.exception_check()? {
                env.exception_clear()?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            log::warn!("clipboard write failed: {e}");
        }
    }
    #[cfg(not(target_os = "android"))]
    let _ = text;
}

const PENDING_PASTE: &str = "pending_paste";

#[derive(Clone)]
struct PendingPaste {
    target: egui::Id,
    text: String,
    frames: u8,
}

pub fn request_paste(ctx: &egui::Context, target: egui::Id) {
    let text = clipboard_text();
    if text.is_empty() {
        return;
    }
    ctx.memory_mut(|m| m.request_focus(target));
    ctx.data_mut(|d| {
        d.insert_temp(
            egui::Id::new(PENDING_PASTE),
            PendingPaste {
                target,
                text,
                frames: 0,
            },
        )
    });
    ctx.request_repaint();
}

pub fn pump(ctx: &egui::Context) {
    let key = egui::Id::new(PENDING_PASTE);
    if let Some(mut pending) = ctx.data(|d| d.get_temp::<PendingPaste>(key)) {
        let focused = ctx.memory(|m| m.focused()) == Some(pending.target);
        if focused {
            ctx.input_mut(|i| i.events.push(egui::Event::Paste(pending.text)));
            ctx.data_mut(|d| d.remove::<PendingPaste>(key));
        } else if pending.frames >= 20 {
            ctx.data_mut(|d| d.remove::<PendingPaste>(key));
        } else {
            pending.frames += 1;
            ctx.memory_mut(|m| m.request_focus(pending.target));
            ctx.data_mut(|d| d.insert_temp(key, pending));
            ctx.request_repaint();
        }
    }
    if !cfg!(target_os = "android") {
        return;
    }
    let copied: Vec<String> = ctx.output(|o| {
        o.commands
            .iter()
            .filter_map(|c| match c {
                egui::OutputCommand::CopyText(t) => Some(t.clone()),
                _ => None,
            })
            .collect()
    });
    if let Some(last) = copied.last() {
        set_clipboard_text(last);
    }
}

pub fn paste_menu(response: &egui::Response) {
    let id = response.id;
    let ctx = response.ctx.clone();
    response.context_menu(|ui| {
        if ui.button("Paste").clicked() {
            request_paste(&ctx, id);
            ui.close();
        }
        if ui.button("Copy").clicked() {
            ctx.memory_mut(|m| m.request_focus(id));
            ctx.input_mut(|i| i.events.push(egui::Event::Copy));
            ui.close();
        }
        if ui.button("Select all").clicked() {
            ctx.memory_mut(|m| m.request_focus(id));
            ctx.input_mut(|i| {
                i.events.push(egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::COMMAND,
                })
            });
            ui.close();
        }
    });
}

pub fn open_url(ctx: &egui::Context, url: &str) {
    #[cfg(target_os = "android")]
    {
        let _ = ctx;
        call_activity_str("openUrl", url);
    }
    #[cfg(not(target_os = "android"))]
    ctx.open_url(egui::OpenUrl::same_tab(url));
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

static TOP_INSET_PX: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static CUTOUT_BOTTOM_PX: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);
static CUTOUT_RECTS: std::sync::Mutex<Vec<[i32; 3]>> = std::sync::Mutex::new(Vec::new());

pub fn set_top_insets(top: i32, rects: &[i32]) {
    let rects: Vec<[i32; 3]> = rects.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    let bottom = rects.iter().map(|r| r[2]).max().unwrap_or(0);
    TOP_INSET_PX.store(top.max(0), std::sync::atomic::Ordering::Relaxed);
    CUTOUT_BOTTOM_PX.store(bottom, std::sync::atomic::Ordering::Relaxed);
    *CUTOUT_RECTS.lock().unwrap() = rects;
}

pub fn top_band_px() -> i32 {
    TOP_INSET_PX
        .load(std::sync::atomic::Ordering::Relaxed)
        .max(CUTOUT_BOTTOM_PX.load(std::sync::atomic::Ordering::Relaxed))
}

pub fn top_band_points(pixels_per_point: f32) -> f32 {
    top_band_px() as f32 / pixels_per_point.max(1.0)
}

pub fn cutout_rects() -> Vec<[i32; 3]> {
    CUTOUT_RECTS.lock().unwrap().clone()
}

#[cfg(target_os = "android")]
#[no_mangle]
pub extern "system" fn Java_stream_zap_app_MainActivity_nativeSetInsets(
    env: jni::JNIEnv,
    _this: jni::objects::JObject,
    top: jni::sys::jint,
    rects: jni::objects::JIntArray,
) {
    let len = env.get_array_length(&rects).unwrap_or(0) as usize;
    let mut buf = vec![0i32; len];
    let _ = env.get_int_array_region(&rects, 0, &mut buf);
    set_top_insets(top, &buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_covers_cutout_taller_than_status_bar() {
        set_top_insets(60, &[500, 580, 90]);
        assert_eq!(top_band_px(), 90);
        assert_eq!(cutout_rects(), vec![[500, 580, 90]]);
        set_top_insets(0, &[]);
        assert_eq!(top_band_px(), 0);
    }
}
