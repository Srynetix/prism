//! The macOS menu bar: Prism (About, Settings…, Hide, Quit), File (New Agent), Edit, View,
//! Window and Help (Prism Help, every guide, all shortcuts; macOS adds its search field).
//! Linux and Windows keep a window without a menu bar: Prism's rail and keys do the same.
//!
//! An item of Prism's own (`prism:<action>`) is sent to the window as the `menu` event, whose
//! payload is the action: `settings`, `new-agent`, `agents`, `help`, `help:<guide>`,
//! `profile:<id>` (switch to it), `profile-open:<id>` (open it alongside), `profile-new`, `profiles`.
//!
//! Its labels follow the window's language (`[appearance] language`, which the window resolves and
//! gives with its guides, `prism_base::i18n`): `tr`.

#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use serde::Deserialize;
use tauri::{
    menu::{MenuItem, MenuItemKind, PredefinedMenuItem, Submenu},
    AppHandle,
};

const PREFIX: &str = "prism:";
/// The language the menus were built in (English at launch).
static BUILT: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(prism_base::i18n::Lang::En as u8);

/// The window's language for the menus and Prism's messages (`prism_base::i18n`); `true` when the
/// menus, built in another one, are to be built again.
pub fn set_language(lang: &str) -> bool {
    let lang = prism_base::i18n::resolve(lang);
    prism_base::i18n::set(lang);
    BUILT.swap(lang as u8, std::sync::atomic::Ordering::Relaxed) != lang as u8
}

/// A label of the menus, in their language; the English one when it has no translation.
pub fn tr(en: &'static str) -> &'static str {
    if prism_base::i18n::lang() != prism_base::i18n::Lang::Fr {
        return en;
    }
    match en {
        "About Prism" => "À propos de Prism",
        "Settings…" => "Réglages…",
        "Profile" => "Profil",
        "Services" => "Services",
        "Hide Prism" => "Masquer Prism",
        "Hide Others" => "Masquer les autres",
        "Show All" => "Tout afficher",
        "Quit Prism" => "Quitter Prism",
        "Show Prism" => "Afficher Prism",
        "File" => "Fichier",
        "New Agent" => "Nouvel agent",
        "Close Window" => "Fermer la fenêtre",
        "Edit" => "Édition",
        "Undo" => "Annuler",
        "Redo" => "Rétablir",
        "Cut" => "Couper",
        "Copy" => "Copier",
        "Paste" => "Coller",
        "Select All" => "Tout sélectionner",
        "View" => "Présentation",
        "Command Palette…" => "Palette de commandes…",
        "Agents" => "Agents",
        "Help" => "Aide",
        "Zen Mode" => "Mode zen",
        "Enter Full Screen" => "Plein écran",
        "Window" => "Fenêtre",
        "Minimize" => "Placer dans le Dock",
        "Zoom" => "Réduire/agrandir",
        "Bring All to Front" => "Tout ramener au premier plan",
        "Prism Help" => "Aide de Prism",
        "Send Feedback…" => "Envoyer un retour…",
        "Open Alongside" => "Ouvrir à côté",
        "New Profile…" => "Nouveau profil…",
        "Manage Profiles…" => "Gérer les profils…",
        _ => en,
    }
}
const HELP: &str = "help-menu";
const PROFILE: &str = "profile-menu";

/// A guide of the in-app Help, as the window lists them.
#[derive(Debug, Deserialize)]
pub struct Guide {
    pub id: String,
    pub title: String,
    pub group: String,
}

/// Builds the menu bar and routes Prism's items to the window.
#[cfg(target_os = "macos")]
pub fn install(app: &AppHandle) -> tauri::Result<()> {
    use tauri::Emitter;
    build(app)?;
    app.on_menu_event(|app, event| {
        let Some(action) = event.id().as_ref().strip_prefix(PREFIX) else { return };
        // A click toggles a check item: the active profile stays the checked one.
        if action.starts_with("profile:") || action.starts_with("profile-open:") {
            let _ = set_profiles(app);
        }
        // To the window in front: a machine's acts on its machine; otherwise the main one, shown.
        use tauri::Manager;
        let front = app.webview_windows().into_values().find(|w| w.label().starts_with("machine-") && w.is_focused().unwrap_or(false));
        match front {
            Some(w) => {
                let _ = w.emit_to(tauri::EventTarget::webview_window(w.label()), "menu", action.to_string());
            }
            None => {
                crate::app::show_main(app);
                let _ = app.emit_to("main", "menu", action.to_string());
            }
        }
    });
    Ok(())
}

/// The menu bar, in the menus' language: built at launch, again when the language changes.
#[cfg(target_os = "macos")]
pub fn build(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::{AboutMetadata, Menu};
    let item = |id: &str, label: &str, accel: Option<&str>| MenuItem::with_id(app, format!("{PREFIX}{id}"), label, true, accel);
    let sep = || PredefinedMenuItem::separator(app);

    let about = AboutMetadata {
        name: Some("Prism".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        // The commit, and the team's blocks built in (`+ acme 5786d86`), if any.
        short_version: Some(env!("PRISM_BUILD").to_string()).filter(|c| !c.is_empty()),
        credits: Some("A local dashboard of a team's work: merge requests and pipelines, tickets, clusters, coding agents. Everything runs on this machine.".into()),
        icon: Some(tauri::include_image!("icons/128x128@2x.png")),
        ..Default::default()
    };
    let prism = Submenu::with_items(
        app,
        "Prism",
        true,
        &[
            &PredefinedMenuItem::about(app, Some(tr("About Prism")), Some(about))?,
            &sep()?,
            &item("settings", tr("Settings…"), Some("CmdOrCtrl+,"))?,
            &Submenu::with_id(app, PROFILE, tr("Profile"), true)?,
            &sep()?,
            &PredefinedMenuItem::services(app, Some(tr("Services")))?,
            &sep()?,
            &PredefinedMenuItem::hide(app, Some(tr("Hide Prism")))?,
            &PredefinedMenuItem::hide_others(app, Some(tr("Hide Others")))?,
            &PredefinedMenuItem::show_all(app, Some(tr("Show All")))?,
            &sep()?,
            &PredefinedMenuItem::quit(app, Some(tr("Quit Prism")))?,
        ],
    )?;
    // Closing the window only hides it (see `main`): Prism stays in the menu bar.
    let file = Submenu::with_items(app, tr("File"), true, &[&item("new-agent", tr("New Agent"), Some("CmdOrCtrl+N"))?, &sep()?, &PredefinedMenuItem::close_window(app, Some(tr("Close Window")))?])?;
    let edit = Submenu::with_items(
        app,
        tr("Edit"),
        true,
        &[
            &PredefinedMenuItem::undo(app, Some(tr("Undo")))?,
            &PredefinedMenuItem::redo(app, Some(tr("Redo")))?,
            &sep()?,
            &PredefinedMenuItem::cut(app, Some(tr("Cut")))?,
            &PredefinedMenuItem::copy(app, Some(tr("Copy")))?,
            &PredefinedMenuItem::paste(app, Some(tr("Paste")))?,
            &PredefinedMenuItem::select_all(app, Some(tr("Select All")))?,
        ],
    )?;
    // No shortcut here for what Prism's keys already do and the config may change (pages, console).
    let view = Submenu::with_items(app, tr("View"), true, &[&item("palette", tr("Command Palette…"), None)?, &sep()?, &item("agents", tr("Agents"), None)?, &item("help", tr("Help"), None)?, &sep()?, &item("zen", tr("Zen Mode"), Some("CmdOrCtrl+Shift+F"))?, &PredefinedMenuItem::fullscreen(app, Some(tr("Enter Full Screen")))?])?;
    let window = Submenu::with_items(
        app,
        tr("Window"),
        true,
        &[&PredefinedMenuItem::minimize(app, Some(tr("Minimize")))?, &PredefinedMenuItem::maximize(app, Some(tr("Zoom")))?, &sep()?, &PredefinedMenuItem::bring_all_to_front(app, Some(tr("Bring All to Front")))?],
    )?;
    // The guides are added by the window (`set_help_guides`), which knows them.
    let help = Submenu::with_id_and_items(app, HELP, tr("Help"), true, &[&item("help", tr("Prism Help"), None)?, &item("feedback", tr("Send Feedback…"), None)?])?;

    let menu = Menu::with_items(app, &[&prism, &file, &edit, &view, &window, &help])?;
    app.set_menu(menu)?;
    set_profiles(app)?;
    window.set_as_windows_menu_for_nsapp()?;
    help.set_as_help_menu_for_nsapp()?;
    Ok(())
}

/// The Profile menu: every profile, the active one checked, ⌥⌘1… in order; then New Profile… and
/// Manage Profiles…. Rebuilt when a profile is created, renamed or removed.
pub fn set_profiles(app: &AppHandle) -> tauri::Result<()> {
    use tauri::menu::CheckMenuItem;
    let Some(menu) = app.menu() else { return Ok(()) };
    let Some(MenuItemKind::Submenu(sub)) = menu.items()?.into_iter().find_map(|m| match m {
        MenuItemKind::Submenu(s) => s.get(PROFILE),
        _ => None,
    }) else {
        return Ok(());
    };
    while !sub.items()?.is_empty() {
        sub.remove_at(0)?;
    }
    let names = crate::profile::names();
    for (i, (id, name)) in names.iter().enumerate() {
        let accel = (i < 9).then(|| format!("Alt+CmdOrCtrl+{}", i + 1));
        sub.append(&CheckMenuItem::with_id(app, format!("{PREFIX}profile:{id}"), name, true, id == crate::profile::active(), accel.as_deref())?)?;
    }
    sub.append(&PredefinedMenuItem::separator(app)?)?;
    // Another profile in its own Prism, next to this one.
    let others: Vec<MenuItem<tauri::Wry>> = names
        .iter()
        .filter(|(id, _)| id != crate::profile::active())
        .map(|(id, name)| MenuItem::with_id(app, format!("{PREFIX}profile-open:{id}"), name, true, None::<&str>))
        .collect::<tauri::Result<_>>()?;
    if !others.is_empty() {
        let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = others.iter().map(|m| m as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
        sub.append(&Submenu::with_items(app, tr("Open Alongside"), true, &refs)?)?;
    }
    sub.append(&MenuItem::with_id(app, format!("{PREFIX}profile-new"), tr("New Profile…"), true, None::<&str>)?)?;
    sub.append(&MenuItem::with_id(app, format!("{PREFIX}profiles"), tr("Manage Profiles…"), true, None::<&str>)?)?;
    Ok(())
}

/// The Help menu: Prism Help, then the guides by group (Reference: all blocks, shortcuts, settings).
pub fn set_help_guides(app: &AppHandle, guides: Vec<Guide>) -> tauri::Result<()> {
    let Some(MenuItemKind::Submenu(help)) = app.menu().and_then(|m| m.get(HELP)) else { return Ok(()) };
    // Rebuilt each time (a reload of the window): everything after "Prism Help" and "Send Feedback…" goes.
    while help.items()?.len() > 2 {
        help.remove_at(2)?;
    }
    help.append(&PredefinedMenuItem::separator(app)?)?;
    let mut groups: Vec<&str> = Vec::new();
    for g in &guides {
        if !groups.contains(&g.group.as_str()) {
            groups.push(&g.group);
        }
    }
    for group in groups {
        let items = guides
            .iter()
            .filter(|g| g.group == group)
            .map(|g| MenuItem::with_id(app, format!("{PREFIX}help:{}", g.id), &g.title, true, None::<&str>))
            .collect::<tauri::Result<Vec<_>>>()?;
        let refs: Vec<&dyn tauri::menu::IsMenuItem<tauri::Wry>> = items.iter().map(|i| i as &dyn tauri::menu::IsMenuItem<tauri::Wry>).collect();
        help.append(&Submenu::with_items(app, group, true, &refs)?)?;
    }
    Ok(())
}
