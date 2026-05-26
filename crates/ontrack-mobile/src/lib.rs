
pub mod controller;

#[cfg(target_os = "android")]
pub mod gps;

slint::include_modules!();

pub fn run() -> anyhow::Result<()> {
    let ui = AppWindow::new()?;
    controller::wire(&ui)?;
    ui.run()?;
    Ok(())
}

#[cfg(target_os = "android")]
#[no_mangle]
fn android_main(app: android_activity::AndroidApp) {
    use android_logger::Config;
    use log::LevelFilter;

    android_logger::init_once(Config::default().with_max_level(LevelFilter::Info).with_tag("OnTrack"));
    log::info!("OnTrack Android starting");

    slint::android::init(app).expect("slint android init");
    if let Err(e) = run() {
        log::error!("OnTrack crashed: {e:?}");
    }
}
