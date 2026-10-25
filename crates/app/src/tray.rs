//! System tray icon and menu (SPEC §10.1), with the `tray-icon` crate.
//!
//! Menu and icon events arrive on the thread that runs the window's event
//! loop. They are queued and the window is woken with `request_repaint`, so
//! they are handled even while it is hidden (eframe then calls `App::logic`).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crossbeam_channel::{Receiver, Sender};
use glyphpad_gui::egui::Context;
use glyphpad_gui::{Tray, TrayAction, icon_rgba};
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu,
};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

type MenuActions = Arc<Mutex<HashMap<MenuId, TrayAction>>>;

pub struct AppTray {
    icon: TrayIcon,
    profiles: Submenu,
    ids: MenuActions,
    /// The profiles the submenu shows now, with the active one.
    shown: (Vec<String>, String),
    enabled_item: CheckMenuItem,
    actions: Receiver<TrayAction>,
    active: Icon,
    paused: Icon,
}

fn icon(paused: bool) -> Option<Icon> {
    Icon::from_rgba(icon_rgba(32, paused), 32, 32).ok()
}

fn send(tx: &Sender<TrayAction>, ctx: &Context, action: TrayAction) {
    let _ = tx.send(action);
    ctx.request_repaint();
}

pub fn create(ctx: Context) -> Option<Box<dyn Tray>> {
    let open = MenuItem::new(glyphpad_gui::tr("Open GlyphPad"), true, None);
    let enabled = CheckMenuItem::new(glyphpad_gui::tr("Gestures enabled"), true, true, None);
    let reload = MenuItem::new(glyphpad_gui::tr("Reload configuration"), true, None);
    let folder = MenuItem::new(glyphpad_gui::tr("Open configuration folder"), true, None);
    let updates = MenuItem::new(glyphpad_gui::tr("Check for updates"), true, None);
    let quit = MenuItem::new(glyphpad_gui::tr("Quit"), true, None);
    let menu = Menu::new();
    let profiles = Submenu::new(glyphpad_gui::tr("Profile"), true);
    let items: [&dyn tray_icon::menu::IsMenuItem; 9] = [
        &open,
        &enabled,
        &profiles,
        &PredefinedMenuItem::separator(),
        &reload,
        &folder,
        &updates,
        &PredefinedMenuItem::separator(),
        &quit,
    ];
    if let Err(e) = menu.append_items(&items) {
        tracing::warn!("cannot build the tray menu: {e}");
        return None;
    }
    let ids: MenuActions = Arc::new(Mutex::new(
        [
            (open.id().clone(), TrayAction::Open),
            (enabled.id().clone(), TrayAction::ToggleEnabled),
            (reload.id().clone(), TrayAction::Reload),
            (folder.id().clone(), TrayAction::OpenConfigFolder),
            (updates.id().clone(), TrayAction::CheckUpdates),
            (quit.id().clone(), TrayAction::Quit),
        ]
        .into(),
    ));
    let handler_ids = ids.clone();

    let (tx, rx) = crossbeam_channel::unbounded();
    let (menu_tx, menu_ctx) = (tx.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        let action = handler_ids.lock().ok().and_then(|m| m.get(e.id()).cloned());
        if let Some(action) = action {
            send(&menu_tx, &menu_ctx, action);
        }
    }));
    // A left click (or double click) on the icon reopens the window; the
    // right click opens the menu.
    TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| match e {
        TrayIconEvent::Click {
            button: MouseButton::Left,
            button_state: MouseButtonState::Up,
            ..
        }
        | TrayIconEvent::DoubleClick {
            button: MouseButton::Left,
            ..
        } => send(&tx, &ctx, TrayAction::Open),
        _ => {}
    }));

    let (active, paused) = (icon(false)?, icon(true)?);
    let built = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_menu_on_left_click(false)
        .with_tooltip("GlyphPad")
        .with_icon(active.clone())
        .build();
    match built {
        Ok(icon) => Some(Box::new(AppTray {
            icon,
            profiles,
            ids,
            shown: (Vec::new(), String::new()),
            enabled_item: enabled,
            actions: rx,
            active,
            paused,
        })),
        Err(e) => {
            // The window keeps working without a tray (SPEC §7).
            tracing::warn!("no tray icon: {e}");
            None
        }
    }
}

impl Tray for AppTray {
    fn poll(&mut self) -> Vec<TrayAction> {
        self.actions.try_iter().collect()
    }

    fn set_profiles(&mut self, names: &[String], active: &str) {
        if self.shown.0 == names && self.shown.1 == active {
            return;
        }
        self.shown = (names.to_vec(), active.to_string());
        for item in self.profiles.items() {
            if let Some(check) = item.as_check_menuitem() {
                let _ = self.profiles.remove(check);
            }
        }
        let Ok(mut ids) = self.ids.lock() else { return };
        ids.retain(|_, a| !matches!(a, TrayAction::SwitchProfile(_)));
        for name in names {
            let item = CheckMenuItem::new(name, true, name == active, None);
            if self.profiles.append(&item).is_ok() {
                ids.insert(item.id().clone(), TrayAction::SwitchProfile(name.clone()));
            }
        }
        // A single profile is nothing to choose from.
        self.profiles.set_enabled(names.len() > 1);
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled_item.set_checked(enabled);
        let _ = self.icon.set_icon(Some(if enabled {
            self.active.clone()
        } else {
            self.paused.clone()
        }));
        let _ = self.icon.set_tooltip(Some(if enabled {
            "GlyphPad".to_string()
        } else {
            glyphpad_gui::tr("GlyphPad (paused)").to_string()
        }));
    }
}
