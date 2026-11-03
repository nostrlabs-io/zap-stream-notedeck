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
