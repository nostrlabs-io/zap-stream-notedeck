#[derive(Clone, Copy, Default, Debug, PartialEq)]
pub struct Insets {
    pub top: f32,
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
}

#[cfg(target_os = "ios")]
pub fn insets() -> Insets {
    use objc2::encode::{Encode, Encoding};
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct UIEdgeInsets {
        top: f64,
        left: f64,
        bottom: f64,
        right: f64,
    }

    unsafe impl Encode for UIEdgeInsets {
        const ENCODING: Encoding = Encoding::Struct("UIEdgeInsets", &[f64::ENCODING; 4]);
    }

    unsafe {
        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        if app.is_null() {
            return Insets::default();
        }
        let windows: *mut AnyObject = msg_send![app, windows];
        if windows.is_null() {
            return Insets::default();
        }
        let window: *mut AnyObject = msg_send![windows, firstObject];
        if window.is_null() {
            return Insets::default();
        }
        let i: UIEdgeInsets = msg_send![window, safeAreaInsets];
        Insets {
            top: i.top as f32,
            left: i.left as f32,
            bottom: i.bottom as f32,
            right: i.right as f32,
        }
    }
}

#[cfg(not(target_os = "ios"))]
pub fn insets() -> Insets {
    Insets::default()
}

#[cfg(target_os = "ios")]
pub fn activate_playback_audio_session() {
    use objc2::runtime::{AnyObject, Bool};
    use objc2::{class, msg_send};
    static DONE: std::sync::Once = std::sync::Once::new();
    DONE.call_once(|| unsafe {
        let session: *mut AnyObject = msg_send![class!(AVAudioSession), sharedInstance];
        if session.is_null() {
            return;
        }
        let Ok(name) = std::ffi::CString::new("AVAudioSessionCategoryPlayback") else {
            return;
        };
        let category: *mut AnyObject = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
        let err: *mut *mut AnyObject = std::ptr::null_mut();
        let ok: Bool = msg_send![session, setCategory: category, error: err];
        let active: Bool = msg_send![session, setActive: Bool::YES, error: err];
        log::info!("audio session playback category={} active={}", ok.as_bool(), active.as_bool());
    });
}
