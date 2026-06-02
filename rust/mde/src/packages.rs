//! Add/Remove Programs (B) — a themed, dnf-backed package manager that replaces
//! dnfdragora (which hangs on every launch and can't be killed).
//!
//! `mde add-remove` opens a window listing the curated software catalogue
//! ([`crate::catalogue`]) grouped by category, with each package's installed
//! state read from `rpm`. Install / Remove run `pkexec dnf` **off the UI thread**
//! (polkit handles the privilege prompt), so a slow download never freezes the
//! window; the row refreshes from `rpm` when the operation finishes. Mandatory
//! base-session packages are shown locked (Required), never removable.
//!
//! A second **Updates** tab (B.2c) lists pending updates from `dnf check-update`
//! and offers a single **Update all** (`pkexec dnf upgrade`). `mde add-remove
//! --updates` opens straight onto it (the deep-link the "MackesDE Update" entry
//! can target).

use std::process::ExitCode;

use iced::widget::{button, container, scrollable, text, text_input, Column, Row};
use iced::{Element, Length, Padding, Task};

use mde_ui::{frame, metrics, palette};

use crate::catalogue;

pub fn run(args: &[String]) -> ExitCode {
    let updates = args.iter().any(|a| a == "--updates");
    match launch(updates) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("mde add-remove: {e}");
            ExitCode::FAILURE
        }
    }
}

/// One catalogue package + its live installed state.
struct Pkg {
    package: &'static str,
    category: &'static str,
    name: &'static str,
    /// Base-session package: always installed, never removable.
    mandatory: bool,
    installed: bool,
}

/// One pending package update parsed from `dnf check-update` (B.2c). `pub(crate)`
/// so the Settings ▸ Update page (E13.2) reuses the same parser.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Update {
    /// The dnf package id, e.g. `bash.x86_64`.
    package: String,
    /// The candidate new version, e.g. `5.2.21-1.fc40`.
    version: String,
}

/// Which tab of the window is showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Programs,
    Updates,
}

struct AddRemove {
    rows: Vec<Pkg>,
    /// The package an install/remove (or `(updating all)`) is currently running
    /// for (buttons disable while set, so only one dnf transaction runs at a time).
    busy: Option<String>,
    /// Last result line, shown in the status bar.
    status: Option<String>,
    /// Live filter (B.2a): name/package substring, case-insensitive. Empty = all.
    search: String,
    /// The active tab (Programs / Updates).
    tab: Tab,
    /// Pending updates (B.2c). `None` = not checked yet; `Some(empty)` = up to date.
    updates: Option<Vec<Update>>,
    /// A `dnf check-update` is in flight (the Updates tab shows "Checking…").
    checking: bool,
}

#[derive(Debug, Clone)]
enum Message {
    /// Install (`true`) or remove (`false`) a package.
    Act(String, bool),
    /// The `pkexec dnf` transaction for a package finished.
    Done(String, bool),
    /// Edit the search/filter box (B.2a).
    SearchChanged(String),
    /// Switch the active tab (B.2c).
    SwitchTab(Tab),
    /// Run `dnf check-update` again (B.2c).
    CheckUpdates,
    /// A `dnf check-update` finished: the parsed list, or an error string (B.2c).
    Checked(Result<Vec<Update>, String>),
    /// Run `pkexec dnf upgrade -y` (B.2c).
    UpdateAll,
    /// The upgrade transaction finished (`true` = succeeded) (B.2c).
    UpdatedAll(bool),
}

fn load_rows() -> Vec<Pkg> {
    catalogue::catalogue()
        .into_iter()
        .map(|c| Pkg {
            package: c.package,
            category: c.category,
            name: c.name,
            mandatory: c.mandatory,
            installed: catalogue::is_installed(c.package),
        })
        .collect()
}

fn launch(updates: bool) -> iced::Result {
    iced::application(
        |_: &AddRemove| "Add/Remove Programs - mde".to_string(),
        update,
        view,
    )
    .theme(|_| palette::iced_theme())
    .font(mde_ui::font::REGULAR_BYTES)
    .font(mde_ui::font::BOLD_BYTES)
    .font(mde_ui::font::PLEX_REGULAR_BYTES)
    .font(mde_ui::font::PLEX_BOLD_BYTES)
    .default_font(mde_ui::font::ui())
    .run_with(move || {
        let (tab, status, init): (Tab, Option<String>, Task<Message>) = if updates {
            // --updates deep-link: open on the Updates tab and check immediately.
            (
                Tab::Updates,
                Some("Checking for updates…".to_string()),
                check_task(),
            )
        } else {
            (Tab::Programs, None, Task::none())
        };
        (
            AddRemove {
                rows: load_rows(),
                busy: None,
                status,
                search: String::new(),
                tab,
                updates: None,
                checking: updates,
            },
            init,
        )
    })
}

fn update(state: &mut AddRemove, message: Message) -> Task<Message> {
    match message {
        Message::Act(package, install) => {
            // Only one transaction at a time.
            if state.busy.is_none() {
                state.busy = Some(package.clone());
                let verb = if install { "Installing" } else { "Removing" };
                state.status = Some(format!("{verb} {package}…"));
                return act_task(package, install);
            }
        }
        Message::Done(package, ok) => {
            state.busy = None;
            // Re-read rpm — the source of truth (the user may have cancelled the
            // polkit prompt, or dnf may have refused a dependency).
            let now = catalogue::is_installed(&package);
            if let Some(p) = state.rows.iter_mut().find(|p| p.package == package) {
                p.installed = now;
            }
            state.status = Some(if ok {
                format!("Done: {package}.")
            } else {
                format!("'{package}' was not changed (cancelled or failed).")
            });
        }
        Message::SearchChanged(s) => state.search = s,
        Message::SwitchTab(tab) => {
            state.tab = tab;
            // Lazily check for updates the first time the Updates tab is shown.
            if tab == Tab::Updates && state.updates.is_none() && !state.checking {
                state.checking = true;
                state.status = Some("Checking for updates…".to_string());
                return check_task();
            }
        }
        Message::CheckUpdates => {
            if !state.checking && state.busy.is_none() {
                state.checking = true;
                state.updates = None;
                state.status = Some("Checking for updates…".to_string());
                return check_task();
            }
        }
        Message::Checked(result) => {
            state.checking = false;
            match result {
                Ok(list) => {
                    state.status = Some(match list.len() {
                        0 => "Your programs are up to date.".to_string(),
                        1 => "1 update available.".to_string(),
                        n => format!("{n} updates available."),
                    });
                    state.updates = Some(list);
                }
                Err(e) => {
                    state.status = Some(format!("Could not check for updates: {e}"));
                    state.updates = Some(Vec::new());
                }
            }
        }
        Message::UpdateAll => {
            let has_updates = state.updates.as_ref().is_some_and(|u| !u.is_empty());
            if state.busy.is_none() && !state.checking && has_updates {
                state.busy = Some("(updating all)".to_string());
                state.status = Some("Installing all updates…".to_string());
                return upgrade_task();
            }
        }
        Message::UpdatedAll(ok) => {
            state.busy = None;
            if ok {
                // Re-check so the list reflects the post-upgrade state (→ empty).
                state.checking = true;
                state.status = Some("Updates installed — rechecking…".to_string());
                return check_task();
            }
            state.status = Some("Update was cancelled or failed.".to_string());
        }
    }
    Task::none()
}

/// Run `pkexec dnf install|remove -y <package>` off the UI thread and report back.
fn act_task(package: String, install: bool) -> Task<Message> {
    Task::perform(
        async move {
            let pkg = package.clone();
            let ok = tokio::task::spawn_blocking(move || {
                let verb = if install { "install" } else { "remove" };
                std::process::Command::new("pkexec")
                    .args(["dnf", verb, "-y", &pkg])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            })
            .await
            .unwrap_or(false);
            (package, ok)
        },
        |(package, ok)| Message::Done(package, ok),
    )
}

/// Run `dnf check-update` off the UI thread and parse the pending updates (B.2c).
/// dnf exits 0 (up to date), 100 (updates available), or other (error) — captured
/// so the tab can tell "up to date" apart from "couldn't check".
fn check_task() -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(|| {
                let out = match std::process::Command::new("dnf")
                    .args(["check-update", "-q"])
                    .output()
                {
                    Ok(o) => o,
                    Err(e) => return Err(format!("dnf is not available: {e}")),
                };
                match out.status.code() {
                    Some(0) => Ok(Vec::new()), // up to date
                    Some(100) => Ok(parse_check_update(&String::from_utf8_lossy(&out.stdout))),
                    _ => {
                        let err = String::from_utf8_lossy(&out.stderr);
                        Err(if err.trim().is_empty() {
                            "dnf check-update failed".to_string()
                        } else {
                            err.trim().to_string()
                        })
                    }
                }
            })
            .await
            .unwrap_or_else(|e| Err(format!("check task panicked: {e}")))
        },
        Message::Checked,
    )
}

/// Run `pkexec dnf upgrade -y` off the UI thread (the Update-all action, B.2c).
fn upgrade_task() -> Task<Message> {
    Task::perform(
        async move {
            tokio::task::spawn_blocking(|| {
                std::process::Command::new("pkexec")
                    .args(["dnf", "upgrade", "-y"])
                    .status()
                    .map(|s| s.success())
                    .unwrap_or(false)
            })
            .await
            .unwrap_or(false)
        },
        Message::UpdatedAll,
    )
}

/// Parse `dnf check-update` stdout into the pending updates (B.2c). Each update is
/// a `name.arch  new-version  repo` row; the metadata-check line and blank lines
/// have no dotted first column and are skipped, and the trailing "Obsoleting
/// Packages" block (a different layout) ends the scan.
pub(crate) fn parse_check_update(output: &str) -> Vec<Update> {
    let mut out = Vec::new();
    for line in output.lines() {
        if line.trim().eq_ignore_ascii_case("Obsoleting Packages") {
            break;
        }
        let cols: Vec<&str> = line.split_whitespace().collect();
        if cols.len() >= 3 && cols[0].contains('.') {
            out.push(Update {
                package: cols[0].to_string(),
                version: cols[1].to_string(),
            });
        }
    }
    out
}

fn pad(t: f32, r: f32, b: f32, l: f32) -> Padding {
    Padding {
        top: t,
        right: r,
        bottom: b,
        left: l,
    }
}

fn section_header(label: &str) -> Element<'static, Message> {
    container(
        text(label.to_string())
            .size(metrics::UI_PX)
            .font(mde_ui::font::ui_bold()),
    )
    .padding(pad(8.0, 0.0, 2.0, 2.0))
    .into()
}

/// One package row: name + a right-aligned action (Install / Remove / Required),
/// disabled while any transaction is in flight.
fn pkg_row(p: &Pkg, busy: bool) -> Element<'static, Message> {
    let name = text(p.name.to_string())
        .size(metrics::UI_PX)
        .width(Length::FillPortion(5));
    let pkg = text(p.package.to_string())
        .size(metrics::UI_PX)
        .width(Length::FillPortion(3))
        .color(palette::color(palette::GRAY_TEXT));

    let action: Element<Message> = if p.mandatory {
        text("Required")
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT))
            .into()
    } else {
        let (label, install) = if p.installed {
            ("Remove", false)
        } else {
            ("Install", true)
        };
        let msg = (!busy).then(|| Message::Act(p.package.to_string(), install));
        button(text(label).size(metrics::UI_PX))
            .on_press_maybe(msg)
            .padding(pad(2.0, 10.0, 2.0, 10.0))
            .into()
    };

    Row::new()
        .spacing(8.0)
        .align_y(iced::Alignment::Center)
        .push(name)
        .push(pkg)
        .push(container(action).align_x(iced::alignment::Horizontal::Right))
        .padding(pad(2.0, 6.0, 2.0, 6.0))
        .into()
}

/// One pending-update row: package id + the candidate version (B.2c).
fn update_row(u: &Update) -> Element<'static, Message> {
    Row::new()
        .spacing(8.0)
        .align_y(iced::Alignment::Center)
        .push(
            text(u.package.clone())
                .size(metrics::UI_PX)
                .width(Length::FillPortion(5)),
        )
        .push(
            text(u.version.clone())
                .size(metrics::UI_PX)
                .width(Length::FillPortion(3))
                .color(palette::color(palette::GRAY_TEXT)),
        )
        .padding(pad(2.0, 6.0, 2.0, 6.0))
        .into()
}

/// Case-insensitive substring match over a package's display name + its id
/// (B.2a). `q` must already be trimmed + lowercased; empty matches everything.
fn pkg_matches(p: &Pkg, q: &str) -> bool {
    q.is_empty() || p.name.to_lowercase().contains(q) || p.package.to_lowercase().contains(q)
}

/// One header tab button; the active tab reads as bold (and is a no-op press).
fn tab_button(label: &'static str, this: Tab, active: Tab) -> Element<'static, Message> {
    let is_active = this == active;
    let txt = text(label).size(metrics::UI_PX);
    let txt = if is_active {
        txt.font(mde_ui::font::ui_bold())
    } else {
        txt
    };
    button(txt)
        .on_press_maybe((!is_active).then_some(Message::SwitchTab(this)))
        .padding(pad(3.0, 12.0, 3.0, 12.0))
        .into()
}

/// The Programs tab: the categorized catalogue list, live-filtered by the search
/// box (B.2a). Returns the body element and the status-bar text.
fn programs_view(state: &AddRemove) -> (Element<'_, Message>, String) {
    let busy = state.busy.is_some();
    let q = state.search.trim().to_lowercase();
    let matches = |p: &Pkg| pkg_matches(p, &q);
    let shown = state.rows.iter().filter(|p| matches(p)).count();

    let mut list = Column::new().spacing(0.0).padding(pad(2.0, 8.0, 2.0, 8.0));
    for cat in catalogue::categories(&catalogue::catalogue()) {
        let cat_rows: Vec<&Pkg> = state
            .rows
            .iter()
            .filter(|p| p.category == cat && matches(p))
            .collect();
        if cat_rows.is_empty() {
            continue; // hide a category with no match under the current filter
        }
        list = list.push(section_header(cat));
        for p in cat_rows {
            list = list.push(pkg_row(p, busy));
        }
    }

    let status = if q.is_empty() {
        format!("{} programs", state.rows.len())
    } else {
        format!("{} of {} programs", shown, state.rows.len())
    };
    (boxed_list(list), status)
}

/// The Updates tab (B.2c): a Check-for-updates / Update-all control row over the
/// pending-update list (or a Checking / up-to-date message). Returns the body
/// element and the status-bar text.
fn updates_view(state: &AddRemove) -> (Element<'_, Message>, String) {
    let busy = state.busy.is_some();
    let has_updates = state.updates.as_ref().is_some_and(|u| !u.is_empty());

    let check_msg = (!state.checking && !busy).then_some(Message::CheckUpdates);
    let update_msg = (!state.checking && !busy && has_updates).then_some(Message::UpdateAll);
    let controls = Row::new()
        .spacing(8.0)
        .align_y(iced::Alignment::Center)
        .push(
            button(text("Check for updates").size(metrics::UI_PX))
                .on_press_maybe(check_msg)
                .padding(pad(2.0, 10.0, 2.0, 10.0)),
        )
        .push(
            button(text("Update all").size(metrics::UI_PX))
                .on_press_maybe(update_msg)
                .padding(pad(2.0, 10.0, 2.0, 10.0)),
        );

    let mut list = Column::new().spacing(0.0).padding(pad(2.0, 8.0, 2.0, 8.0));
    let body_status = if state.checking {
        list = list.push(note("Checking for updates…"));
        "Checking for updates…".to_string()
    } else {
        match &state.updates {
            None => {
                list = list.push(note("Press “Check for updates”."));
                "Updates".to_string()
            }
            Some(u) if u.is_empty() => {
                list = list.push(note("Your programs are up to date."));
                "Your programs are up to date.".to_string()
            }
            Some(u) => {
                for upd in u {
                    list = list.push(update_row(upd));
                }
                match u.len() {
                    1 => "1 update available.".to_string(),
                    n => format!("{n} updates available."),
                }
            }
        }
    };

    let body = Column::new()
        .spacing(6.0)
        .push(controls)
        .push(container(boxed_list(list)).height(Length::Fill));
    (body.into(), body_status)
}

/// A short centered note line for the Updates tab's empty/checking states.
fn note(s: &str) -> Element<'static, Message> {
    container(
        text(s.to_string())
            .size(metrics::UI_PX)
            .color(palette::color(palette::GRAY_TEXT)),
    )
    .padding(pad(8.0, 0.0, 2.0, 2.0))
    .into()
}

/// Wrap a list column in the shared sunken scroll frame.
fn boxed_list(list: Column<'_, Message>) -> Element<'_, Message> {
    iced::widget::stack![
        frame::sunken().face(palette::color(palette::WINDOW)),
        container(scrollable(list).style(mde_ui::scrollbar))
            .width(Length::Fill)
            .height(Length::Fill),
    ]
    .into()
}

fn view(state: &AddRemove) -> Element<'_, Message> {
    let (body, default_status) = match state.tab {
        Tab::Programs => programs_view(state),
        Tab::Updates => updates_view(state),
    };

    let status = text(state.status.clone().unwrap_or(default_status))
        .size(metrics::UI_PX)
        .color(palette::color(palette::WINDOW_TEXT));

    // Tab strip on the left; on the Programs tab a live search box on the right.
    let mut header = Row::new()
        .spacing(8.0)
        .align_y(iced::Alignment::Center)
        .push(tab_button("Programs", Tab::Programs, state.tab))
        .push(tab_button("Updates", Tab::Updates, state.tab))
        .push(iced::widget::horizontal_space());
    if state.tab == Tab::Programs {
        header = header.push(
            text_input("Search programs", &state.search)
                .on_input(Message::SearchChanged)
                .size(metrics::UI_PX)
                .width(Length::Fixed(220.0)),
        );
    }

    container(
        Column::new()
            .spacing(6.0)
            .padding(8.0)
            .push(header)
            .push(container(body).height(Length::Fill))
            .push(status),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkg(name: &'static str, package: &'static str) -> Pkg {
        Pkg {
            package,
            category: "Test",
            name,
            mandatory: false,
            installed: false,
        }
    }

    #[test]
    fn pkg_matches_name_and_id_case_insensitively() {
        let p = pkg("Web Browser (Firefox)", "firefox");
        assert!(pkg_matches(&p, "")); // empty matches all
        assert!(pkg_matches(&p, "browser")); // display name
        assert!(pkg_matches(&p, "firefox")); // package id
        assert!(pkg_matches(&p, "fire")); // substring of the id
        assert!(!pkg_matches(&p, "chrome"));
    }

    #[test]
    fn parses_dnf_check_update_output() {
        // A representative `dnf check-update` body: a metadata line, blanks, real
        // rows (incl. an epoch version), and a trailing Obsoleting block to ignore.
        let out = "\
Last metadata expiration check: 0:12:34 ago on Mon 02 Jun 2026.

bash.x86_64                  5.2.21-1.fc40            updates
vim-enhanced.x86_64          2:9.1.0-1.fc40           updates
kernel.x86_64                6.8.5-301.fc40           fedora

Obsoleting Packages
old-thing.noarch             1.0-1.fc40               updates
";
        let ups = parse_check_update(out);
        assert_eq!(ups.len(), 3, "three updates, obsoletes excluded");
        assert_eq!(ups[0].package, "bash.x86_64");
        assert_eq!(ups[0].version, "5.2.21-1.fc40");
        assert_eq!(ups[1].version, "2:9.1.0-1.fc40"); // epoch preserved
        assert_eq!(ups[2].package, "kernel.x86_64");
        assert!(!ups.iter().any(|u| u.package.starts_with("old-thing")));
    }

    #[test]
    fn parses_empty_and_metadata_only_output() {
        assert!(parse_check_update("").is_empty());
        assert!(parse_check_update("Last metadata expiration check: now\n").is_empty());
    }
}
