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
    #[cfg(target_os = "ios")]
    {
        let _ = ctx;
        open_url_ios(url);
    }
    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    ctx.open_url(egui::OpenUrl::same_tab(url));
}

#[cfg(target_os = "ios")]
fn open_url_ios(url: &str) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    let Ok(c) = std::ffi::CString::new(url) else {
        return;
    };
    unsafe {
        let ns: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()];
        let nsurl: *mut AnyObject = msg_send![class!(NSURL), URLWithString: ns];
        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        if nsurl.is_null() || app.is_null() {
            return;
        }
        let options: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
        let none: *mut AnyObject = std::ptr::null_mut();
        let _: () = msg_send![app, openURL: nsurl, options: options, completionHandler: none];
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

const LEFT_CLUSTER_PT: f32 = 64.0;
const RIGHT_CLUSTER_PT: f32 = 110.0;
const LOGO_WIDTH_PT: f32 = 150.0;
const ACTIONS_WIDTH_PT: f32 = 200.0;
const COMPACT_LEFT_CLUSTER_PT: f32 = 60.0;
const COMPACT_RIGHT_CLUSTER_PT: f32 = 80.0;
const COMPACT_MIN_SIDE_PT: f32 = 56.0;
const WIDE_PT: f32 = 800.0;

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct HeaderTop {
    pub top: f32,
    pub left: f32,
    pub right: f32,
    pub compact_height: Option<f32>,
}

pub fn header_top(pixels_per_point: f32, width_pt: f32) -> HeaderTop {
    plan_header_top(top_band_px(), &cutout_rects(), pixels_per_point, width_pt)
}

fn plan_header_top(band_px: i32, rects: &[[i32; 3]], pixels_per_point: f32, width_pt: f32) -> HeaderTop {
    let ppp = pixels_per_point.max(1.0);
    let band = band_px as f32 / ppp;
    let below = HeaderTop { top: band, left: 0.0, right: 0.0, compact_height: None };
    if band_px <= 0 || rects.is_empty() {
        return below;
    }
    let first_left = rects.iter().map(|r| r[0]).min().unwrap_or(0) as f32 / ppp;
    let last_right = rects.iter().map(|r| r[1]).max().unwrap_or(0) as f32 / ppp;
    if width_pt >= WIDE_PT {
        let fits = first_left - LEFT_CLUSTER_PT >= LOGO_WIDTH_PT
            && width_pt - RIGHT_CLUSTER_PT - last_right >= ACTIONS_WIDTH_PT;
        if !fits {
            return below;
        }
        return HeaderTop { top: 0.0, left: LEFT_CLUSTER_PT, right: RIGHT_CLUSTER_PT, compact_height: None };
    }
    let fits = first_left - COMPACT_LEFT_CLUSTER_PT >= COMPACT_MIN_SIDE_PT
        && width_pt - COMPACT_RIGHT_CLUSTER_PT - last_right >= COMPACT_MIN_SIDE_PT;
    if !fits {
        return below;
    }
    HeaderTop {
        top: 0.0,
        left: COMPACT_LEFT_CLUSTER_PT,
        right: COMPACT_RIGHT_CLUSTER_PT,
        compact_height: Some(band),
    }
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

    #[test]
    fn compact_header_chosen_for_centered_cutout_on_phone() {
        let plan = plan_header_top(136, &[[414, 666, 126]], 2.825, 382.0);
        assert_eq!(plan.top, 0.0);
        assert_eq!(plan.compact_height, Some(136.0 / 2.825));
    }

    #[test]
    fn header_falls_back_below_band_for_edge_cutout() {
        let plan = plan_header_top(136, &[[0, 136, 136]], 2.625, 411.0);
        assert_eq!(plan.top, 136.0 / 2.625);
        assert_eq!(plan.compact_height, None);
        let right_edge = plan_header_top(136, &[[944, 1080, 136]], 2.625, 411.0);
        assert_eq!(right_edge.top, 136.0 / 2.625);
    }

    #[test]
    fn header_falls_back_when_free_side_is_too_narrow() {
        let plan = plan_header_top(136, &[[300, 780, 90]], 2.625, 411.0);
        assert_eq!(plan.top, 136.0 / 2.625);
    }

    #[test]
    fn wide_header_keeps_full_layout() {
        let plan = plan_header_top(136, &[[1000, 1080, 90]], 2.625, 900.0);
        assert_eq!(plan.top, 0.0);
        assert_eq!(plan.compact_height, None);
        assert_eq!(plan.left, 64.0);
    }

    #[test]
    fn no_cutout_or_band_leaves_header_untouched() {
        assert_eq!(plan_header_top(0, &[], 2.625, 411.0).top, 0.0);
        assert_eq!(plan_header_top(136, &[], 2.625, 900.0).top, 136.0 / 2.625);
    }
}
