#[cfg(target_os = "android")]
mod amber_android;
#[cfg(target_os = "android")]
mod android;
mod android_window;
pub mod app;
mod chat_data;
mod chat_events;
mod chat_model;
mod chat_text;
mod blossom;
mod contacts;
mod deep_link;
mod frame_stats;
mod hls;
mod i18n;
mod relay_health;
mod player_health;
mod poster;
pub mod relays;
mod link;
mod note_ref;
mod note_util;
mod note_view;
mod profile_edit;
mod relay_list;
mod profiles;
mod moderation;
mod route;
mod safe_area;
mod signer;
mod stream_info;
mod sub;
pub mod theme;
mod widgets;
mod zap;
mod zap_api;

#[cfg(target_os = "android")]
pub use lumina_video::*;

#[cfg(target_os = "android")]
#[no_mangle]
pub fn android_main(app: android_activity::AndroidApp) {
    android_logger::init_once(
        android_logger::Config::default()
            .with_tag("zapstream")
            .with_max_level(log::LevelFilter::Info),
    );
    std::panic::set_hook(Box::new(|info| log::error!("panic: {info}")));
    log::info!("android_main entered");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");
    runtime.block_on(async move { android::start_android(app) });
}
