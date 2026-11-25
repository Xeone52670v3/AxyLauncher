#[cfg(target_os = "linux")]
mod linux_tray {
    use crate::icon::render_logo_argb32_be;
    use ksni::blocking::TrayMethods;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct LauncherTray {
        status_label: String,
        icon_64: Vec<u8>,
        icon_32: Vec<u8>,
        on_activate: Arc<dyn Fn() + Send + Sync>,
    }

    impl ksni::Tray for LauncherTray {
        fn id(&self) -> String {
            "axylauncher".to_string()
        }

        fn category(&self) -> ksni::Category {
            ksni::Category::ApplicationStatus
        }

        fn title(&self) -> String {
            "AxyLauncher".to_string()
        }

        // Leave `icon_name` empty so all Linux StatusNotifierItem panels (KDE Plasma, Waybar,
        // GNOME AppIndicator, XFCE, Cinnamon) always render `icon_pixmap` directly!
        fn icon_pixmap(&self) -> Vec<ksni::Icon> {
            vec![
                ksni::Icon {
                    width: 64,
                    height: 64,
                    data: self.icon_64.clone(),
                },
                ksni::Icon {
                    width: 32,
                    height: 32,
                    data: self.icon_32.clone(),
                },
            ]
        }

        fn activate(&mut self, _x: i32, _y: i32) {
            (self.on_activate)();
        }

        fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
            use ksni::menu::*;
            vec![
                StandardItem {
                    label: "AxyLauncher".to_string(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                MenuItem::Separator,
                StandardItem {
                    label: "Открыть AxyLauncher".to_string(),
                    activate: Box::new(|this: &mut Self| {
                        (this.on_activate)();
                    }),
                    ..Default::default()
                }
                .into(),
                StandardItem {
                    label: "Выход".to_string(),
                    activate: Box::new(|_this: &mut Self| {
                        std::process::exit(0);
                    }),
                    ..Default::default()
                }
                .into(),
            ]
        }
    }

    #[derive(Clone)]
    pub struct TrayController {
        handle: Arc<Mutex<Option<ksni::blocking::Handle<LauncherTray>>>>,
        on_activate: Arc<dyn Fn() + Send + Sync>,
    }

    impl TrayController {
        pub fn new(enabled: bool, on_activate: Arc<dyn Fn() + Send + Sync>) -> Self {
            let ctrl = Self {
                handle: Arc::new(Mutex::new(None)),
                on_activate,
            };
            ctrl.set_enabled(enabled, "Готов");
            ctrl
        }

        pub fn set_enabled(&self, enabled: bool, status_label: &str) {
            let mut guard = self.handle.lock().unwrap();
            if enabled {
                if let Some(ref h) = *guard {
                    if !h.is_closed() {
                        let s = status_label.to_string();
                        let _ = h.update(move |t| {
                            t.status_label = s;
                        });
                        return;
                    }
                }
                let tray = LauncherTray {
                    status_label: status_label.to_string(),
                    icon_64: render_logo_argb32_be(64),
                    icon_32: render_logo_argb32_be(32),
                    on_activate: self.on_activate.clone(),
                };
                let spawned = tray
                    .clone()
                    .spawn()
                    .or_else(|_| tray.disable_dbus_name(true).spawn())
                    .ok();
                *guard = spawned;
            } else if let Some(h) = guard.take() {
                h.shutdown();
            }
        }

        #[allow(dead_code)]
        pub fn update_status(&self, status_label: &str) {
            let guard = self.handle.lock().unwrap();
            if let Some(ref h) = *guard {
                let s = status_label.to_string();
                let _ = h.update(move |t| {
                    t.status_label = s;
                });
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
mod other_tray {
    use std::sync::Arc;

    #[derive(Clone)]
    pub struct TrayController;

    impl TrayController {
        pub fn new(_enabled: bool, _on_activate: Arc<dyn Fn() + Send + Sync>) -> Self {
            Self
        }
        pub fn set_enabled(&self, _enabled: bool, _status_label: &str) {}
        #[allow(dead_code)]
        pub fn update_status(&self, _status_label: &str) {}
    }
}

#[cfg(target_os = "linux")]
pub use linux_tray::TrayController;
#[cfg(not(target_os = "linux"))]
pub use other_tray::TrayController;
