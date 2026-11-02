use crate::signer::{register_pending, AmberRequest, Completion, SignError, Transport};
use jni::objects::{JClass, JObject, JString, JValue};
use jni::JNIEnv;

pub struct AndroidTransport;

static APP: std::sync::OnceLock<android_activity::AndroidApp> = std::sync::OnceLock::new();

pub fn set_app(app: &android_activity::AndroidApp) {
    let _ = APP.set(app.clone());
}

fn launch(json: &str) -> jni::errors::Result<bool> {
    let app = APP.get().ok_or(jni::errors::Error::NullPtr("app"))?;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr().cast())? };
    let mut env = vm.attach_current_thread()?;
    let activity = unsafe { JObject::from_raw(app.activity_as_ptr().cast()) };
    let arg = env.new_string(json)?;
    let ok = env
        .call_method(
            &activity,
            "amberRequest",
            "(Ljava/lang/String;)Z",
            &[JValue::Object(&arg)],
        )
        .and_then(|v| v.z());
    if env.exception_check()? {
        env.exception_describe()?;
        env.exception_clear()?;
        return Ok(false);
    }
    ok
}

impl Transport for AndroidTransport {
    fn send(&self, request: AmberRequest, done: Completion) {
        register_pending(&request.id, done);
        match launch(&request.to_json()) {
            Ok(true) => {}
            other => {
                log::warn!("amber launch failed: {other:?}");
                if let Some(done) = crate::signer::take_pending(&request.id) {
                    done(Err(SignError::Failed("could not reach the signer".into())));
                }
            }
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_stream_zap_app_Nip55Signer_nativeReply(
    mut env: JNIEnv,
    _class: JClass,
    id: JString,
    json: JString,
) {
    let (Ok(id), Ok(json)) = (env.get_string(&id), env.get_string(&json)) else {
        return;
    };
    let id: String = id.into();
    let json: String = json.into();
    crate::signer::deliver_reply(&id, &json);
}
