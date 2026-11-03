use anyhow::Result;
#[cfg(not(target_os = "ios"))]
use directories::ProjectDirs;
#[cfg(not(target_os = "ios"))]
use egui::{Vec2, ViewportBuilder};
use log::{error, info};
use notedeck::Notedeck;
use zap_stream_app::app::ZapStreamApp;

#[tokio::main]
async fn main() -> Result<()> {
    pretty_env_logger::init();

    let options = native_options();

    let data_path = data_path();

    info!("Data path: {}", data_path.display());
    if let Err(e) = eframe::run_native(
        "zap.stream",
        options,
        Box::new(move |cc| {
            let args = zap_stream_app::relays::args(std::env::args().collect());
            let mut notedeck = Notedeck::init(&cc.egui_ctx, data_path.clone(), &args);
            notedeck.setup(&cc.egui_ctx);
            notedeck.set_app(ZapStreamApp::new(cc.wgpu_render_state.clone()));
            Ok(Box::new(notedeck))
        }),
    ) {
        error!("{}", e);
    }
    Ok(())
}

#[cfg(target_os = "ios")]
fn data_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").expect("HOME");
    let path = std::path::Path::new(&home).join("Library/Application Support/zap_stream_app");
    std::fs::create_dir_all(&path).expect("data dir");
    path
}

#[cfg(not(target_os = "ios"))]
fn data_path() -> std::path::PathBuf {
    ProjectDirs::from("stream", "zap", "zap_stream_app")
        .unwrap()
        .data_dir()
        .to_path_buf()
}

#[cfg(target_os = "ios")]
fn native_options() -> eframe::NativeOptions {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.device_descriptor = std::sync::Arc::new(|adapter| egui_wgpu::wgpu::DeviceDescriptor {
        label: Some("zap.stream device"),
        required_limits: adapter.limits(),
        ..Default::default()
    });
    eframe::NativeOptions {
        system_font_fallback: true,
        wgpu_options: egui_wgpu::WgpuConfiguration {
            wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[cfg(not(target_os = "ios"))]
fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: ViewportBuilder::default().with_inner_size(Vec2::new(1300., 900.)),
        ..Default::default()
    }
}
