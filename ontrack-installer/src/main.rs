//! ontrack-installer — GUI installer for OnTrack-RS
//!
//! A self-contained egui application that:
//!   1. Welcomes the user
//!   2. Lets them choose an install directory
//!   3. Copies (or downloads) the ontrack + ontrack-gui binaries
//!   4. Creates a desktop shortcut / .desktop launcher
//!   5. Offers to launch OnTrack immediately
//!
//! The binaries are either:
//!   a) Bundled alongside this installer (set BUNDLE_BINARIES=1 at build time)
//!   b) Downloaded from the latest GitHub release (default, no bundling needed)
//!
//! Copyright (C) 2025 Qompass AI, All rights reserved

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use anyhow::{anyhow, Result};
use eframe::egui::{self, Color32, FontId, RichText, ScrollArea, TextEdit, Ui};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

// ── Brand colours ──────────────────────────────────────────────────────────
const TDS_BLUE:    Color32 = Color32::from_rgb(0x00, 0x57, 0xA8);
const TDS_NAVY:    Color32 = Color32::from_rgb(0x00, 0x28, 0x55);
const TDS_ORANGE:  Color32 = Color32::from_rgb(0xF2, 0x65, 0x22);
const TDS_BG:      Color32 = Color32::from_rgb(0x11, 0x18, 0x27);
const TDS_SURFACE: Color32 = Color32::from_rgb(0x1A, 0x25, 0x35);
const TDS_CARD:    Color32 = Color32::from_rgb(0x24, 0x30, 0x44);
const TDS_WHITE:   Color32 = Color32::WHITE;
const TDS_GRAY:    Color32 = Color32::from_rgb(0x6B, 0x72, 0x80);
const TDS_GREEN:   Color32 = Color32::from_rgb(0x22, 0xC5, 0x5E);
const TDS_RED:     Color32 = Color32::from_rgb(0xEF, 0x44, 0x44);

const APP_VERSION: &str = "2.0.0";
const GITHUB_REPO: &str = "qompassai/Python";
const RELEASES_URL: &str =
    "https://github.com/qompassai/Python/releases/latest/download";

// ── Platform helpers ────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
fn default_install_dir() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA")
        .unwrap_or_else(|_| dirs_next::data_local_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "C:\\Users\\Public".to_string()));
    PathBuf::from(local).join("TDS Telecom").join("OnTrack-RS")
}

#[cfg(not(target_os = "windows"))]
fn default_install_dir() -> PathBuf {
    dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join(".local")
        .join("share")
        .join("ontrack-rs")
}

fn desktop_dir() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        dirs_next::desktop_dir().unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("XDG_DESKTOP_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs_next::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join("Desktop")
            })
    }
}

fn exe_name(base: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

fn arch_suffix() -> &'static str {
    if cfg!(target_os = "windows") {
        "x86_64-windows"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64-linux"
    } else {
        "x86_64-linux"
    }
}

// ── Shared installer state ─────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
enum InstallerPage {
    Welcome,
    Options,
    Progress,
    Done { success: bool, error: Option<String> },
}

#[derive(Debug, Clone)]
struct ProgressState {
    message:  String,
    fraction: f32,
    log:      Vec<String>,
}

impl Default for ProgressState {
    fn default() -> Self {
        Self {
            message:  "Preparing…".to_string(),
            fraction: 0.0,
            log:      Vec::new(),
        }
    }
}

// ── Installation logic ─────────────────────────────────────────────────────

async fn run_install(
    install_dir: PathBuf,
    create_shortcut: bool,
    progress: Arc<Mutex<ProgressState>>,
) -> Result<()> {
    macro_rules! prog {
        ($frac:expr, $msg:expr) => {{
            let mut p = progress.lock().unwrap();
            p.fraction = $frac;
            p.message = $msg.to_string();
            p.log.push($msg.to_string());
        }};
    }

    // ── Step 1: create install directory ─────────────────────────────────
    prog!(0.05, format!("Creating directory: {}", install_dir.display()));
    std::fs::create_dir_all(&install_dir)?;

    // ── Step 2: copy or download binaries ─────────────────────────────────
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|pp| pp.to_path_buf()));

    let cli_src = bundled.as_ref().map(|b| b.join(exe_name("ontrack")));
    let gui_src = bundled.as_ref().map(|b| b.join(exe_name("ontrack-gui")));

    let cli_bundled = cli_src.as_ref().map(|p| p.exists()).unwrap_or(false);
    let gui_bundled = gui_src.as_ref().map(|p| p.exists()).unwrap_or(false);

    if cli_bundled && gui_bundled {
        prog!(0.20, "Copying bundled binaries…");
        std::fs::copy(cli_src.unwrap(), install_dir.join(exe_name("ontrack")))?;
        std::fs::copy(gui_src.unwrap(), install_dir.join(exe_name("ontrack-gui")))?;
    } else {
        prog!(0.15, "Downloading OnTrack binaries from GitHub…");
        let client = reqwest::Client::builder()
            .user_agent("OnTrack-Installer/2.0")
            .build()?;

        for (name, dest_name) in [
            (format!("ontrack-{}", arch_suffix()), exe_name("ontrack")),
            (format!("ontrack-gui-{}", arch_suffix()), exe_name("ontrack-gui")),
        ] {
            let url = format!("{RELEASES_URL}/{name}");
            prog!(0.25, format!("Downloading {name}…"));

            let resp = client.get(&url).send().await?;
            if !resp.status().is_success() {
                return Err(anyhow!(
                    "Download failed for {name} (HTTP {}). \
                     Check your internet connection.",
                    resp.status()
                ));
            }
            let bytes = resp.bytes().await?;
            let dest = install_dir.join(&dest_name);
            std::fs::write(&dest, &bytes)?;

            // Make executable on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = std::fs::metadata(&dest)?.permissions();
                perms.set_mode(0o755);
                std::fs::set_permissions(&dest, perms)?;
            }
        }
    }

    prog!(0.70, "Binaries installed.");

    // ── Step 3: write a .env.example ─────────────────────────────────────
    prog!(0.75, "Writing configuration template…");
    let env_example = install_dir.join(".env.example");
    std::fs::write(
        &env_example,
        "# OnTrack RS — copy to .env and fill in your values\n\
         # All features work without any API keys (OSRM + Nominatim are free).\n\n\
         GOOGLE_MAPS_API_KEY=\"\"\n\
         OSRM_BASE_URL=\"http://router.project-osrm.org\"\n\
         ARCGIS_ITEM_ID=\"\"\n",
    )?;

    // ── Step 4: desktop shortcut ──────────────────────────────────────────
    if create_shortcut {
        prog!(0.85, "Creating desktop shortcut…");
        create_desktop_shortcut(&install_dir)?;
        prog!(0.92, "Desktop shortcut created.");
    }

    prog!(1.0, "Installation complete!");
    Ok(())
}

#[cfg(target_os = "windows")]
fn create_desktop_shortcut(install_dir: &PathBuf) -> Result<()> {
    let exe = install_dir.join("ontrack-gui.exe");
    let lnk = desktop_dir().join("OnTrack.lnk");

    // Use PowerShell — no extra crates needed, works on all modern Windows
    let script = format!(
        r#"$ws = New-Object -ComObject WScript.Shell; \
$s = $ws.CreateShortcut('{lnk}'); \
$s.TargetPath = '{exe}'; \
$s.WorkingDirectory = '{dir}'; \
$s.Description = 'OnTrack TDS Field Route Optimizer'; \
$s.Save()"#,
        lnk = lnk.display(),
        exe = exe.display(),
        dir = install_dir.display(),
    );

    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .output()?;

    if !out.status.success() {
        return Err(anyhow!(
            "PowerShell shortcut creation failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn create_desktop_shortcut(install_dir: &PathBuf) -> Result<()> {
    let exe  = install_dir.join("ontrack-gui");
    let icon = install_dir.join("ontrack.png"); // optional

    let desktop_entry = format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Name=OnTrack\n\
         Comment=TDS Telecom Field Route Optimizer\n\
         Exec={exe}\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Utility;Geography;\n",
        exe  = exe.display(),
        icon = icon.display(),
    );

    // ~/.local/share/applications
    let apps_dir = dirs_next::home_dir()
        .ok_or_else(|| anyhow!("Cannot locate home directory"))?
        .join(".local").join("share").join("applications");
    std::fs::create_dir_all(&apps_dir)?;
    let app_file = apps_dir.join("ontrack.desktop");
    std::fs::write(&app_file, &desktop_entry)?;
    make_executable(&app_file)?;

    // ~/Desktop (optional — may not exist)
    let desk = desktop_dir().join("OnTrack.desktop");
    if desk.parent().map(|p| p.exists()).unwrap_or(false) {
        std::fs::write(&desk, &desktop_entry)?;
        make_executable(&desk)?;
    }

    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &PathBuf) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut perms = std::fs::metadata(path)?.permissions();
    perms.set_mode(perms.mode() | 0o111);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}
#[cfg(not(unix))]
fn make_executable(_path: &PathBuf) -> Result<()> { Ok(()) }

// ── Uninstall logic ────────────────────────────────────────────────────────

fn run_uninstall(install_dir: &PathBuf) -> Result<()> {
    if install_dir.exists() {
        std::fs::remove_dir_all(install_dir)?;
    }

    #[cfg(not(target_os = "windows"))]
    {
        let app_file = dirs_next::home_dir()
            .unwrap_or_default()
            .join(".local").join("share").join("applications").join("ontrack.desktop");
        let _ = std::fs::remove_file(&app_file);

        let desk = desktop_dir().join("OnTrack.desktop");
        let _ = std::fs::remove_file(&desk);
    }

    Ok(())
}

// ── App struct ─────────────────────────────────────────────────────────────

struct InstallerApp {
    page:             InstallerPage,
    install_dir:      String,
    create_shortcut:  bool,
    add_to_path:      bool,
    progress:         Arc<Mutex<ProgressState>>,
    rt:               Arc<tokio::runtime::Runtime>,
    task_running:     bool,
    show_uninstall:   bool,
}

impl InstallerApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            page:            InstallerPage::Welcome,
            install_dir:     default_install_dir().to_string_lossy().to_string(),
            create_shortcut: true,
            add_to_path:     cfg!(target_os = "windows"),
            progress:        Arc::new(Mutex::new(ProgressState::default())),
            rt:              Arc::new(tokio::runtime::Runtime::new().unwrap()),
            task_running:    false,
            show_uninstall:  std::env::args().any(|a| a == "--uninstall"),
        }
    }

    fn start_install(&mut self) {
        if self.task_running { return; }
        self.task_running = true;
        self.page = InstallerPage::Progress;

        let install_dir    = PathBuf::from(&self.install_dir);
        let create_shortcut = self.create_shortcut;
        let progress       = Arc::clone(&self.progress);
        let progress2      = Arc::clone(&self.progress);

        self.rt.spawn(async move {
            match run_install(install_dir, create_shortcut, progress).await {
                Ok(()) => {
                    let mut p = progress2.lock().unwrap();
                    p.message  = "✓ Installation complete!".to_string();
                    p.fraction = 1.0;
                }
                Err(e) => {
                    let mut p = progress2.lock().unwrap();
                    p.message  = format!("✗ Error: {e}");
                    p.fraction = -1.0; // sentinel for error
                }
            }
        });
    }

    fn launch_gui(&self) {
        let exe = PathBuf::from(&self.install_dir).join(exe_name("ontrack-gui"));
        if exe.exists() {
            let _ = std::process::Command::new(&exe)
                .current_dir(&self.install_dir)
                .spawn();
        }
    }
}

// ── egui App impl ──────────────────────────────────────────────────────────

impl eframe::App for InstallerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll background task for completion
        if self.task_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
            let frac = self.progress.lock().unwrap().fraction;
            if frac >= 1.0 {
                self.task_running = false;
                self.page = InstallerPage::Done { success: true, error: None };
            } else if frac < 0.0 {
                let msg = self.progress.lock().unwrap().message.clone();
                self.task_running = false;
                self.page = InstallerPage::Done {
                    success: false,
                    error: Some(msg),
                };
            }
        }

        // Uninstall mode
        if self.show_uninstall {
            self.render_uninstall(ctx);
            return;
        }

        self.render_header(ctx);

        match self.page.clone() {
            InstallerPage::Welcome             => self.render_welcome(ctx),
            InstallerPage::Options             => self.render_options(ctx),
            InstallerPage::Progress            => self.render_progress(ctx),
            InstallerPage::Done { success, error } => self.render_done(ctx, success, error),
        }
    }
}

// ── Page renderers ──────────────────────────────────────────────────────────

impl InstallerApp {
    fn render_header(&self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::default().fill(TDS_NAVY).inner_margin(14.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("🗺  OnTrack")
                            .color(TDS_WHITE)
                            .font(FontId::proportional(24.0))
                            .strong(),
                    );
                    ui.label(
                        RichText::new(format!("  v{APP_VERSION}  ·  TDS Telecom  ·  Rust Edition"))
                            .color(TDS_GRAY)
                            .font(FontId::proportional(13.0)),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new("Installer")
                                .color(TDS_GRAY)
                                .font(FontId::proportional(12.0)),
                        );
                    });
                });
            });
    }

    fn render_welcome(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(TDS_BG).inner_margin(40.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Welcome to OnTrack Installer")
                            .color(TDS_WHITE)
                            .strong()
                            .size(22.0),
                    );
                    ui.add_space(12.0);

                    egui::Frame::default()
                        .fill(TDS_SURFACE)
                        .rounding(10.0)
                        .inner_margin(20.0)
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!(
                                    "This installer will set up OnTrack v{APP_VERSION} (Rust edition)\n\
                                     on your computer.\n\n\
                                     OnTrack optimizes driving routes for TDS Telecom field technicians.\n\
                                     It integrates with Google Maps and ArcGIS FieldMaps.\n\n\
                                     No API key is required — all core features work out of the box.\n\
                                     The app uses free OpenStreetMap data for routing and map previews."
                                ))
                                .color(Color32::from_rgb(0xC7, 0xD9, 0xF0))
                                .size(14.0),
                            );
                        });

                    ui.add_space(16.0);
                    ui.label(
                        RichText::new("Disk space required: ~20 MB  ·  Internet connection needed for first launch")
                            .color(TDS_GRAY)
                            .size(12.0),
                    );
                    ui.add_space(20.0);

                    let btn = egui::Button::new(
                        RichText::new("Next →").color(TDS_WHITE).strong().size(16.0),
                    )
                    .fill(TDS_ORANGE)
                    .min_size(egui::vec2(160.0, 44.0));

                    if ui.add(btn).clicked() {
                        self.page = InstallerPage::Options;
                    }
                });
            });
    }

    fn render_options(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(TDS_BG).inner_margin(32.0))
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Installation Options")
                        .color(TDS_WHITE)
                        .strong()
                        .size(20.0),
                );
                ui.add_space(16.0);

                // Install directory
                egui::Frame::default()
                    .fill(TDS_SURFACE)
                    .rounding(10.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("Install directory").color(TDS_WHITE).strong().size(14.0),
                        );
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            ui.add(
                                TextEdit::singleline(&mut self.install_dir)
                                    .desired_width(ui.available_width() - 90.0),
                            );
                            if ui.button("Browse…").clicked() {
                                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                    self.install_dir = dir.to_string_lossy().to_string();
                                }
                            }
                        });
                    });

                ui.add_space(10.0);

                // Checkboxes
                egui::Frame::default()
                    .fill(TDS_SURFACE)
                    .rounding(10.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.label(RichText::new("Options").color(TDS_WHITE).strong().size(14.0));
                        ui.add_space(6.0);
                        ui.checkbox(
                            &mut self.create_shortcut,
                            RichText::new("Create desktop shortcut").color(TDS_WHITE),
                        );
                        if cfg!(target_os = "windows") {
                            ui.checkbox(
                                &mut self.add_to_path,
                                RichText::new("Add to PATH (run 'ontrack' from any terminal)")
                                    .color(TDS_WHITE),
                            );
                        }
                    });

                ui.add_space(20.0);

                ui.label(
                    RichText::new("Binaries will be downloaded from github.com/qompassai/Python")
                        .color(TDS_GRAY)
                        .size(12.0),
                );

                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    if ui.add(
                        egui::Button::new(RichText::new("← Back").color(TDS_WHITE))
                            .fill(TDS_CARD)
                            .min_size(egui::vec2(110.0, 38.0))
                    ).clicked() {
                        self.page = InstallerPage::Welcome;
                    }

                    ui.add_space(8.0);

                    let install_btn = egui::Button::new(
                        RichText::new("Install").color(TDS_WHITE).strong().size(16.0),
                    )
                    .fill(TDS_ORANGE)
                    .min_size(egui::vec2(150.0, 38.0));

                    if ui.add(install_btn).clicked() {
                        self.start_install();
                    }
                });
            });
    }

    fn render_progress(&mut self, ctx: &egui::Context) {
        let (frac, msg, log) = {
            let p = self.progress.lock().unwrap();
            (p.fraction, p.message.clone(), p.log.clone())
        };

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(TDS_BG).inner_margin(40.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Installing…")
                            .color(TDS_WHITE)
                            .strong()
                            .size(22.0),
                    );
                    ui.add_space(20.0);

                    let display_frac = frac.max(0.0);
                    ui.add(
                        egui::ProgressBar::new(display_frac)
                            .desired_width(480.0)
                            .show_percentage(),
                    );
                    ui.add_space(10.0);
                    ui.label(RichText::new(&msg).color(Color32::from_rgb(0x9D, 0xB8, 0xD6)).size(13.0));
                    ui.add_space(16.0);

                    egui::Frame::default()
                        .fill(TDS_CARD)
                        .rounding(8.0)
                        .inner_margin(12.0)
                        .show(ui, |ui| {
                            ScrollArea::vertical()
                                .max_height(140.0)
                                .stick_to_bottom(true)
                                .show(ui, |ui| {
                                    for line in &log {
                                        ui.label(
                                            RichText::new(line)
                                                .color(TDS_GRAY)
                                                .font(FontId::monospace(11.0)),
                                        );
                                    }
                                });
                        });

                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Please wait — do not close this window.")
                            .color(TDS_GRAY)
                            .size(11.0),
                    );
                });
            });
    }

    fn render_done(
        &self,
        ctx: &egui::Context,
        success: bool,
        error: Option<String>,
    ) {
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(TDS_BG).inner_margin(40.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    if success {
                        ui.label(RichText::new("✓").color(TDS_GREEN).size(56.0));
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Installation Complete")
                                .color(TDS_WHITE)
                                .strong()
                                .size(22.0),
                        );
                        ui.add_space(10.0);
                        ui.label(
                            RichText::new(format!(
                                "OnTrack has been installed to:\n{}{}",
                                self.install_dir,
                                if self.create_shortcut { "\n\nA desktop shortcut has been created." } else { "" }
                            ))
                            .color(Color32::from_rgb(0xC7, 0xD9, 0xF0))
                            .size(14.0),
                        );
                        ui.add_space(24.0);

                        ui.horizontal(|ui| {
                            let launch_btn = egui::Button::new(
                                RichText::new("🚀 Launch OnTrack").color(TDS_WHITE).strong().size(15.0),
                            )
                            .fill(TDS_ORANGE)
                            .min_size(egui::vec2(180.0, 44.0));

                            if ui.add(launch_btn).clicked() {
                                self.launch_gui();
                                std::process::exit(0);
                            }

                            ui.add_space(8.0);

                            if ui.add(
                                egui::Button::new(RichText::new("Close").color(TDS_WHITE))
                                    .fill(TDS_SURFACE)
                                    .min_size(egui::vec2(100.0, 44.0))
                            ).clicked() {
                                std::process::exit(0);
                            }
                        });
                    } else {
                        ui.label(RichText::new("✗").color(TDS_RED).size(56.0));
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new("Installation Failed")
                                .color(TDS_WHITE)
                                .strong()
                                .size(22.0),
                        );
                        ui.add_space(10.0);
                        if let Some(err) = &error {
                            egui::Frame::default()
                                .fill(TDS_CARD)
                                .rounding(8.0)
                                .inner_margin(14.0)
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(err)
                                            .color(Color32::from_rgb(0xFF, 0xAA, 0xAA))
                                            .size(13.0),
                                    );
                                });
                        }
                        ui.add_space(16.0);
                        ui.label(
                            RichText::new(
                                "Check that you have write permission to the install directory\n\
                                 and an active internet connection, then try again."
                            )
                            .color(TDS_GRAY)
                            .size(12.0),
                        );
                        ui.add_space(16.0);
                        if ui.add(
                            egui::Button::new(RichText::new("Close").color(TDS_WHITE))
                                .fill(TDS_SURFACE)
                                .min_size(egui::vec2(100.0, 40.0))
                        ).clicked() {
                            std::process::exit(1);
                        }
                    }
                });
            });
    }

    fn render_uninstall(&self, ctx: &egui::Context) {
        self.render_header(ctx);

        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(TDS_BG).inner_margin(40.0))
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new("Uninstall OnTrack")
                            .color(TDS_WHITE)
                            .strong()
                            .size(22.0),
                    );
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new(format!(
                            "This will remove OnTrack (Rust edition) from:\n{}",
                            default_install_dir().display()
                        ))
                        .color(TDS_GRAY)
                        .size(13.0),
                    );
                    ui.add_space(24.0);

                    ui.horizontal(|ui| {
                        let del_btn = egui::Button::new(
                            RichText::new("Uninstall").color(TDS_WHITE).strong().size(15.0),
                        )
                        .fill(TDS_RED)
                        .min_size(egui::vec2(130.0, 42.0));

                        if ui.add(del_btn).clicked() {
                            let dir = default_install_dir();
                            match run_uninstall(&dir) {
                                Ok(()) => {
                                    let _ = rfd::MessageDialog::new()
                                        .set_title("Uninstalled")
                                        .set_description("OnTrack has been removed.")
                                        .show();
                                }
                                Err(e) => {
                                    let _ = rfd::MessageDialog::new()
                                        .set_title("Error")
                                        .set_description(&e.to_string())
                                        .show();
                                }
                            }
                            std::process::exit(0);
                        }

                        ui.add_space(8.0);

                        if ui.add(
                            egui::Button::new(RichText::new("Cancel").color(TDS_WHITE))
                                .fill(TDS_SURFACE)
                                .min_size(egui::vec2(100.0, 42.0))
                        ).clicked() {
                            std::process::exit(0);
                        }
                    });
                });
            });
    }
}

// ── main ───────────────────────────────────────────────────────────────────

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .init();

    eframe::run_native(
        &format!("OnTrack v{APP_VERSION} — Installer"),
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size([680.0, 500.0])
                .with_resizable(false)
                .with_title(format!("OnTrack v{APP_VERSION} — Installer")),
            ..Default::default()
        },
        Box::new(|cc| Box::new(InstallerApp::new(cc))),
    )
}
