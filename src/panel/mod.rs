//! `nudge panel install` and `nudge panel remove`: put the Herdr desktop panel in place and dock it, with nothing
//! to do by hand, and undo that. The mod's files go into Herdr's `mods/nudge/`; while Herdr's window is open the
//! panel is docked on the right of the layout it shows (see `dock`), and nothing restarts: not the window, not
//! herdr-core, not a terminal. Both are safe to run again, and with Herdr closed.
mod clicks;
mod dock;
mod edit;
mod files;
mod layout;
mod record;
mod undock;
mod window;

use std::path::{Path, PathBuf};

use crate::env::{Env, Run};
use crate::error::Result;
use crate::state::state_dir;
use dock::Docked;
use files::Removed;
use layout::PANEL_MOD;
use undock::Kept;

/// `nudge panel install`: install the panel's files, then dock it in the open window.
pub(crate) fn install(env: &Env, run: &dyn Run) -> Result<u8> {
    let home = app_home(env);
    let mods = home.join("mods").join(PANEL_MOD);
    let changed = files::install_mod(&mods)?;
    let verb = if changed { "Installed" } else { "Already installed:" };
    println!("{verb} the Nudge panel in {}", mods.display());
    for name in dock::bound_all(&home)? {
        println!(
            "Kept the panel in your layout \"{name}\" between 280 and 360 px wide (it had no limits), so a width \
             Herdr saves for it can't take the window."
        );
    }
    let message = match dock::dock(&home, &record_path(env), window::ask(env, run)?)? {
        Docked::Already { name, restored } => {
            format!("It's docked in your layout \"{name}\".{}", restored_note(&restored))
        }
        Docked::Added { name, block } => format!(
            "Docked it on the right of your layout \"{name}\": added to {} as the last child of its top row:\n  \
             {block}\n`nudge panel remove` takes it out again.",
            layout_path(&home, &name).display()
        ),
        Docked::Copied { name, from, restored } => format!(
            "Docked it on the right of \"{name}\", your copy of Herdr's \"{from}\" layout, which Herdr shows now. \
             `nudge panel remove` undoes it.{}",
            restored_note(&restored)
        ),
        Docked::Closed => "Herdr's window isn't open, so nudge can't dock the panel or check that it is: run \
                           `nudge panel install` again while it's open."
            .to_owned(),
        Docked::NoApp => no_app("dock the panel"),
    };
    println!("{message}");
    Ok(0)
}

/// What install put back of the window's saved state for the panel, and what the window refused, as lines to add;
/// empty when nothing.
fn restored_note(restored: &clicks::Restored) -> String {
    let width = restored
        .width
        .map(|width| format!("\nHerdr had saved the panel {width} px wide; it's back to its layout's width."));
    let hidden = restored
        .hidden
        .then_some("\nHerdr had the panel hidden; it shows again.");
    let refused = restored
        .refused
        .iter()
        .map(|refused| format!("\n{refused}; drag the panel's edge back by hand, or run `nudge panel install` again."))
        .collect::<Vec<_>>()
        .concat();
    format!("{}{}{refused}", width.unwrap_or_default(), hidden.unwrap_or_default())
}

/// `nudge panel remove`: undock the panel, then delete its files.
pub(crate) fn remove(env: &Env, run: &dyn Run) -> Result<u8> {
    let home = app_home(env);
    let window = window::ask(env, run)?;
    if matches!(window, window::Window::NoApp) {
        println!("{}", no_app("show another layout"));
    }
    let undocked = undock::undock(&home, &record_path(env), window)?;
    for refused in &undocked.refused {
        println!("{refused}; the window may keep its saved size for the panel until it shows another layout.");
    }
    if let Some((copy, back)) = &undocked.switched {
        println!("Herdr showed \"{copy}\"; it shows \"{back}\" again.");
    }
    for name in &undocked.taken_out {
        println!(
            "Took the panel out of your layout \"{name}\" ({}).",
            layout_path(&home, name).display()
        );
    }
    for name in &undocked.deleted {
        println!("Deleted the layout \"{name}\" that `nudge panel install` made.");
    }
    for (name, why) in &undocked.kept {
        println!(
            "{}",
            match why {
                Kept::Changed => format!(
                    "Kept the layout \"{name}\": it changed since `nudge panel install` made it, so it's yours now."
                ),
                Kept::Shown => format!(
                    "Kept the layout \"{name}\": Herdr opens with it and its window isn't open; run `nudge panel remove` \
                 again while it's open to show the layout it copies and delete it."
                ),
                Kept::NotMade => format!("Kept the layout \"{name}\": `nudge panel install` didn't make it."),
            }
        );
    }
    let mods = home.join("mods").join(PANEL_MOD);
    println!(
        "{}",
        match files::remove_mod(&mods)? {
            Removed::Nothing => format!("The Nudge panel's files aren't in {}.", mods.display()),
            Removed::Folder => format!("Removed the Nudge panel's files ({}).", mods.display()),
            Removed::Kept(others) => format!(
                "Removed the Nudge panel's files; kept {} for the other files in it: {}.",
                mods.display(),
                others.join(", ")
            ),
        }
    );
    Ok(0)
}

fn no_app(what: &str) -> String {
    format!("`herdr-app` isn't on PATH, so nudge can't reach Herdr's window to {what}.")
}

/// `$HERDR_APP_HOME`, else `~/.config/herdr-app`, as Herdr itself picks it.
fn app_home(env: &Env) -> PathBuf {
    env.get("HERDR_APP_HOME")
        .map_or_else(|| env.home().join(".config").join("herdr-app"), PathBuf::from)
}

/// `panel.json` in nudge's state dir: the layouts install made.
fn record_path(env: &Env) -> PathBuf {
    state_dir(env).join("panel.json")
}

fn layout_path(home: &Path, name: &str) -> PathBuf {
    home.join("layouts").join(format!("{name}.json"))
}

#[cfg(test)]
mod tests;
