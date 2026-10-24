//! System tray icon and menu (SPEC §10.1), with the `tray-icon` crate.
//!
//! Menu and icon events arrive on the thread that runs the window's event
//! loop. They are queued and the window is woken with `request_repaint`, so
//! they are handled even while it is hidden (eframe then calls `App::logic`).

use std::collections::HashMap;

use crossbeam_channel::{Receiver, Sender};
use glyphpad_gui::egui::Context;
use glyphpad_gui::{Tray, TrayAction, icon_rgba};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

pub struct AppTray {
    icon: TrayIcon,
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
    let open = MenuItem::new("Open GlyphPad", true, None);
    let enabled = CheckMenuItem::new("Gestures enabled", true, true, None);
    let reload = MenuItem::new("Reload configuration", true, None);
    let folder = MenuItem::new("Open configuration folder", true, None);
    let updates = MenuItem::new("Check for updates", true, None);
    let quit = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    let items: [&dyn tray_icon::menu::IsMenuItem; 8] = [
        &open,
        &enabled,
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
    let ids: HashMap<MenuId, TrayAction> = [
        (open.id().clone(), TrayAction::Open),
        (enabled.id().clone(), TrayAction::ToggleEnabled),
        (reload.id().clone(), TrayAction::Reload),
        (folder.id().clone(), TrayAction::OpenConfigFolder),
        (updates.id().clone(), TrayAction::CheckUpdates),
        (quit.id().clone(), TrayAction::Quit),
    ]
    .into();

    let (tx, rx) = crossbeam_channel::unbounded();
    let (menu_tx, menu_ctx) = (tx.clone(), ctx.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if let Some(&action) = ids.get(e.id()) {
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

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled_item.set_checked(enabled);
        let _ = self.icon.set_icon(Some(if enabled {
            self.active.clone()
        } else {
            self.paused.clone()
        }));
        let _ = self.icon.set_tooltip(Some(if enabled {
            "GlyphPad"
        } else {
            "GlyphPad (paused)"
        }));
    }
}
