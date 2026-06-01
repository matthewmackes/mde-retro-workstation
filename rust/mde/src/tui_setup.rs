//! Text-mode installer (`mde setup` on a headless server) styled after the
//! Windows 2000 NT text-mode Setup: full-screen blue, white text, a bottom
//! key-hint bar. Runs as root, installs everything, registers the greetd
//! session, and switches the machine to graphical startup.

use std::process::{Command, ExitCode};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

const BLUE: Color = Color::Indexed(18); // deep NT setup blue
const TITLE: &str = "MDE-Retro Professional Setup";

// The package set is now the unified `catalogue` (base session + apps + Control
// Panel tools + Xen/XCP-ng guest tools). Step 1 installs the user's selection
// (or the curated default); git/python3 are mandatory `Base System` entries so
// they still land before the "Installing visual assets" fetch step.

#[derive(PartialEq)]
enum Screen {
    NotRoot,
    Welcome,
    Summary,
    Components,
    Progress,
    Finish,
}

struct Step {
    label: &'static str,
    done: bool,
}

/// One line of the Choose-Components list: a category header or a package.
#[derive(Clone, Copy)]
enum CompRow {
    Header(&'static str),
    Item(usize), // index into App::cat
}

struct App {
    screen: Screen,
    steps: Vec<Step>,
    current: usize,
    dry_run: bool,
    /// Steps that failed ("label: error"); shown on the Finish screen so a
    /// broken install is never reported as success.
    failed: Vec<String>,
    /// Packages step 1 installs — from `--packages` (GUI handoff) or, in the
    /// interactive flow, committed from `checked` on leaving the Components screen.
    selection: Vec<String>,
    /// Interactive = no --packages given, so show the Choose-Components screen.
    interactive: bool,
    // --- Choose-Components state (parallel vectors indexed like `cat`) --------
    cat: Vec<crate::catalogue::Component>,
    checked: Vec<bool>,
    /// mandatory || already-installed — shown checked and not toggleable.
    locked: Vec<bool>,
    /// offered by an enabled repo (else greyed, can't be selected).
    avail: Vec<bool>,
    rows: Vec<CompRow>,
    cursor: usize,
}

impl App {
    fn move_cursor(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let last = self.rows.len() - 1;
        let mut c = self.cursor as isize + delta;
        if c < 0 {
            c = 0;
        } else if c as usize > last {
            c = last as isize;
        }
        self.cursor = c as usize;
    }

    fn toggle_cursor(&mut self) {
        match self.rows[self.cursor] {
            CompRow::Item(i) => {
                if !self.locked[i] && self.avail[i] {
                    self.checked[i] = !self.checked[i];
                }
            }
            CompRow::Header(cat) => {
                // Toggle the whole category: if any toggleable item is off, turn
                // them all on; otherwise turn them all off.
                let idxs: Vec<usize> = (0..self.cat.len())
                    .filter(|&i| self.cat[i].category == cat && !self.locked[i] && self.avail[i])
                    .collect();
                let any_off = idxs.iter().any(|&i| !self.checked[i]);
                for i in idxs {
                    self.checked[i] = any_off;
                }
            }
        }
    }

    /// m/s/e presets. Locked (mandatory/installed) are always on; unavailable
    /// can never be on.
    fn preset(&mut self, default_on_only: bool, everything: bool) {
        for i in 0..self.cat.len() {
            self.checked[i] = self.locked[i]
                || (self.avail[i] && (everything || (default_on_only && self.cat[i].default_on)));
        }
    }

    /// Commit the checked set as the install selection.
    fn commit_selection(&mut self) {
        self.selection = self
            .cat
            .iter()
            .enumerate()
            .filter(|(i, _)| self.checked[*i])
            .map(|(_, c)| c.package.to_string())
            .collect();
    }
}

/// Build the display rows: a header per category, then its packages, in order.
fn build_rows(cat: &[crate::catalogue::Component]) -> Vec<CompRow> {
    let mut rows = Vec::new();
    for cn in crate::catalogue::categories(cat) {
        rows.push(CompRow::Header(cn));
        for (i, c) in cat.iter().enumerate() {
            if c.category == cn {
                rows.push(CompRow::Item(i));
            }
        }
    }
    rows
}

pub fn run(dry_run: bool, packages: Option<Vec<String>>) -> ExitCode {
    // ratatui::init() (crossterm raw mode) panics without a controlling tty.
    // `mde setup --tui` is meant for a real console, so fail cleanly otherwise.
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        eprintln!("mde setup --tui requires an interactive terminal.");
        return ExitCode::FAILURE;
    }

    // Build the selectable catalogue + its checked/locked/available state. The
    // availability probe (one dnf repoquery) only runs in the interactive flow.
    let interactive = packages.is_none();
    let cat = crate::catalogue::catalogue();
    let n = cat.len();
    let mut checked = vec![false; n];
    let mut locked = vec![false; n];
    let mut avail = vec![true; n];
    if interactive {
        println!("Querying package repositories…");
        let pkgs: Vec<&str> = cat.iter().map(|c| c.package).collect();
        let available = crate::catalogue::available(&pkgs);
        for (i, c) in cat.iter().enumerate() {
            let installed = crate::catalogue::is_installed(c.package);
            locked[i] = c.mandatory || installed;
            avail[i] = available.contains(c.package);
            checked[i] = c.mandatory || installed || (c.default_on && avail[i]);
        }
    }
    let rows = build_rows(&cat);

    let mut app = App {
        screen: if dry_run || is_root() { Screen::Welcome } else { Screen::NotRoot },
        steps: vec![
            Step { label: "Collecting information", done: false },
            Step { label: "Installing packages", done: false },
            Step { label: "Deploying configuration", done: false },
            Step { label: "Installing visual assets", done: false },
            Step { label: "Registering session and login manager", done: false },
            Step { label: "Finalizing installation", done: false },
            Step { label: "Applying MDE Retro branding", done: false },
        ],
        current: 0,
        dry_run,
        failed: Vec::new(),
        // Interactive: filled by commit_selection() when leaving the Components
        // screen. Non-interactive: the explicit --packages set.
        selection: packages.unwrap_or_default(),
        interactive,
        cat,
        checked,
        locked,
        avail,
        rows,
        cursor: 0,
    };

    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn is_root() -> bool {
    Command::new("id")
        .arg("-u")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim() == "0")
        .unwrap_or(false)
}

/// True once branding has been applied — by the boot one-shot
/// (mde-activate-branding.service) or a prior setup. install-branding.sh writes
/// this marker on success; revert-branding.sh removes it. We use it to avoid
/// re-running branding (a needless initramfs rebuild) and to keep LightDM as the
/// display manager instead of flipping back to greetd.
fn branding_active() -> bool {
    std::path::Path::new("/var/lib/mde-branding/.activated").exists()
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> ExitCode {
    loop {
        let _ = terminal.draw(|f| ui(f, app));

        if app.screen == Screen::Progress {
            // Run one step per frame so the screen updates between steps.
            if app.current < app.steps.len() {
                if let Err(e) = run_step(app.current, app.dry_run, &app.selection) {
                    let label = app.steps[app.current].label;
                    app.failed.push(format!("{label}: {e}"));
                }
                app.steps[app.current].done = true;
                app.current += 1;
            } else {
                app.screen = Screen::Finish;
            }
            // Drain any pending key (allow F3 abort) without blocking.
            if let Ok(true) = event::poll(Duration::from_millis(10)) {
                if let Ok(Event::Key(k)) = event::read() {
                    if k.code == KeyCode::F(3) {
                        return ExitCode::from(1);
                    }
                }
            }
            continue;
        }

        if let Ok(Event::Key(k)) = event::read() {
            match (&app.screen, k.code) {
                (Screen::NotRoot, _) => return ExitCode::from(1),
                (Screen::Welcome, KeyCode::Enter) => app.screen = Screen::Summary,
                (Screen::Summary, KeyCode::Enter) => {
                    app.screen = if app.interactive { Screen::Components } else { Screen::Progress };
                }
                (Screen::Components, KeyCode::Enter) => {
                    app.commit_selection();
                    app.screen = Screen::Progress;
                }
                (Screen::Components, KeyCode::Up) => app.move_cursor(-1),
                (Screen::Components, KeyCode::Down) => app.move_cursor(1),
                (Screen::Components, KeyCode::Char(' ')) => app.toggle_cursor(),
                (Screen::Components, KeyCode::Char('m' | 'M')) => app.preset(false, false),
                (Screen::Components, KeyCode::Char('s' | 'S')) => app.preset(true, false),
                (Screen::Components, KeyCode::Char('e' | 'E')) => app.preset(false, true),
                (Screen::Finish, KeyCode::Enter) => return ExitCode::SUCCESS,
                (_, KeyCode::F(3)) | (_, KeyCode::Esc) => return ExitCode::from(1),
                _ => {}
            }
        }
    }
}

// --- install steps ---------------------------------------------------------

/// Run a command and treat a non-zero exit (or spawn failure) as an error, so a
/// failed install step is recorded rather than silently marked done.
fn run_status(cmd: &mut Command) -> Result<(), String> {
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("command exited with {s}")),
        Err(e) => Err(e.to_string()),
    }
}

fn run_step(i: usize, dry_run: bool, selection: &[String]) -> Result<(), String> {
    if dry_run {
        std::thread::sleep(Duration::from_millis(450));
        return Ok(());
    }
    match i {
        0 => Ok(()), // collecting information (root already checked)
        1 => {
            // Install the chosen components that aren't already present. dnf
            // --skip-unavailable tolerates packages missing from enabled repos
            // (e.g. the Xen guest agent before a COPR is added).
            let to_install: Vec<&str> = selection
                .iter()
                .map(String::as_str)
                .filter(|p| !crate::catalogue::is_installed(p))
                .collect();
            let mut r = Ok(());
            if !to_install.is_empty() {
                let mut args = vec!["install", "-y", "--skip-unavailable"];
                args.extend(to_install.iter().copied());
                r = run_status(Command::new("dnf").args(&args));
            }
            // If the XCP-ng/XenServer guest agent landed, turn its service on.
            crate::catalogue::enable_guest_agent();
            r
        }
        2 => {
            // configs are shipped by the RPM under /usr/share/mde/skel
            run_status(
                Command::new("sh")
                    .arg("-c")
                    .arg("cp -rn /usr/share/mde/skel/. /etc/skel/ 2>/dev/null; true"),
            )
        }
        3 => {
            // Per locked decision #7 the RPM ships no asset bytes — the visual
            // assets are FETCHED per user. Trigger the fetch for the invoking
            // admin's real user (SUDO_USER) now; everyone else's runs from their
            // session autostart on first login. (Running as root would only
            // populate /root, so we drop privileges to the target user.)
            match std::env::var("SUDO_USER").ok().filter(|u| u != "root") {
                Some(user) => run_status(
                    Command::new("runuser").args(["-u", &user, "--", "mde", "install", "--assets"]),
                ),
                None => Ok(()),
            }
        }
        4 => register_session(),
        5 => {
            run_status(Command::new("systemctl").args(["set-default", "graphical.target"]))?;
            // If the boot one-shot already applied branding, LightDM is the login
            // manager — don't flip back to greetd (that would undo it). Otherwise
            // greetd is the base login until branding (step 6) switches it.
            if branding_active() {
                run_status(Command::new("systemctl").args(["enable", "lightdm"]))
            } else {
                run_status(Command::new("systemctl").args(["enable", "--now", "greetd"]))
            }
        }
        6 => {
            // Rebrand the install as MDE Retro Workstation (os-release, Plymouth,
            // GRUB, console, fastfetch, wallpaper, LightDM login). Switches the
            // display manager greetd -> LightDM, so it runs after step 5. Skip if
            // the boot one-shot (mde-activate-branding.service) already applied it
            // — re-running would needlessly rebuild the initramfs.
            if branding_active() {
                Ok(())
            } else {
                run_status(
                    Command::new("bash").arg("/usr/share/mde/branding/scripts/install-branding.sh"),
                )
            }
        }
        _ => Ok(()),
    }
}

fn register_session() -> Result<(), String> {
    let e = |x: std::io::Error| x.to_string();
    let session = "[Desktop Entry]\nName=MDE-Retro\nComment=Windows 2000 desktop\nExec=labwc\nType=Application\n";
    std::fs::create_dir_all("/usr/share/wayland-sessions").map_err(e)?;
    std::fs::write("/usr/share/wayland-sessions/mde-retro.desktop", session).map_err(e)?;
    let greetd = "[terminal]\nvt = 1\n\n[default_session]\ncommand = \"tuigreet --remember --sessions /usr/share/wayland-sessions\"\nuser = \"greetd\"\n";
    std::fs::create_dir_all("/etc/greetd").map_err(e)?;
    std::fs::write("/etc/greetd/config.toml", greetd).map_err(e)?;
    Ok(())
}

// --- rendering -------------------------------------------------------------

fn ui(f: &mut Frame, app: &App) {
    let area = f.area();
    // Paint the whole screen NT-setup blue.
    f.render_widget(Block::default().style(Style::default().bg(BLUE)), area);

    let rows = Layout::vertical([
        Constraint::Length(1), // title
        Constraint::Min(0),    // body
        Constraint::Length(1), // key bar
    ])
    .split(area);

    let title = Paragraph::new(Line::from(Span::styled(
        TITLE,
        Style::default().fg(Color::White).bg(BLUE).add_modifier(Modifier::BOLD),
    )))
    .alignment(Alignment::Center);
    f.render_widget(title, rows[0]);

    let keys = match app.screen {
        Screen::NotRoot => " Press any key to exit ",
        Screen::Welcome => " ENTER=Continue    F3=Exit ",
        Screen::Summary => " ENTER=Choose Components    F3=Exit ",
        Screen::Components => {
            " \u{2191}\u{2193} Move   SPACE Toggle   M/S/E Minimal/Standard/Everything   ENTER Install   F3 Exit "
        }
        Screen::Progress => " Installing, please wait... ",
        Screen::Finish => " ENTER=Finish ",
    };

    if app.screen == Screen::Components {
        render_components(f, app, rows[1]);
    } else {
        let body = match app.screen {
            Screen::NotRoot => not_root_body(),
            Screen::Welcome => welcome_body(),
            Screen::Summary => summary_body(),
            Screen::Progress => progress_body(app),
            Screen::Finish => finish_body(app),
            Screen::Components => unreachable!(),
        };
        let inner = centered(rows[1], 64, 16);
        f.render_widget(
            Paragraph::new(body)
                .style(Style::default().fg(Color::White).bg(BLUE))
                .wrap(Wrap { trim: false }),
            inner,
        );
    }

    let keybar = Paragraph::new(Line::from(Span::styled(
        keys,
        Style::default().fg(Color::Black).bg(Color::Gray),
    )))
    .alignment(Alignment::Left);
    f.render_widget(keybar, rows[2]);
}

/// Render the Choose-Components list (category headers + package checkboxes),
/// windowed so the cursor row stays visible, with locked/unavailable styling.
fn render_components(f: &mut Frame, app: &App, area: Rect) {
    let h = area.height.max(1) as usize;
    let total = app.rows.len();
    // Keep the cursor in view: scroll so it sits within [start, start+h).
    let start = if app.cursor + 1 > h { app.cursor + 1 - h } else { 0 };
    let end = (start + h).min(total);

    let mut lines: Vec<Line> = Vec::with_capacity(end - start);
    for ri in start..end {
        let cursor = ri == app.cursor;
        let (text, mut style) = match app.rows[ri] {
            CompRow::Header(cat) => (
                format!("  {cat}"),
                Style::default().fg(Color::Rgb(0xFF, 0xFF, 0x66)).bg(BLUE).add_modifier(Modifier::BOLD),
            ),
            CompRow::Item(i) => {
                let c = &app.cat[i];
                let mark = if app.checked[i] { "[X]" } else { "[ ]" };
                let (suffix, st) = if !app.avail[i] {
                    (" (unavailable)", Style::default().fg(Color::DarkGray).bg(BLUE))
                } else if app.locked[i] {
                    let tag = if c.mandatory { " (required)" } else { " (installed)" };
                    (tag, Style::default().fg(Color::Gray).bg(BLUE))
                } else {
                    ("", Style::default().fg(Color::White).bg(BLUE))
                };
                (format!("    {mark} {}{}", c.name, suffix), st)
            }
        };
        if cursor {
            style = style.add_modifier(Modifier::REVERSED);
        }
        lines.push(Line::from(Span::styled(text, style)));
    }
    f.render_widget(
        Paragraph::new(ratatui::text::Text::from(lines)).style(Style::default().bg(BLUE)),
        area,
    );
}

fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let w = width.min(area.width);
    let h = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(w)) / 2;
    let y = area.y + (area.height.saturating_sub(h)) / 2;
    Rect { x, y, width: w, height: h }
}

fn not_root_body() -> ratatui::text::Text<'static> {
    "Setup cannot continue.\n\nMDE-Retro Setup must be run with administrator (root) privileges.\n\n    Please run:   sudo mde setup\n".into()
}

fn welcome_body() -> ratatui::text::Text<'static> {
    "Welcome to Setup.\n\nThis portion of Setup prepares MDE-Retro -- a Windows 2000 desktop for Fedora -- to run on your computer.\n\n  - To set up MDE-Retro now, press ENTER.\n  - To quit Setup without installing, press F3.".into()
}

fn summary_body() -> ratatui::text::Text<'static> {
    "Setup will perform the following on this computer:\n\n  - Install the components you choose (desktop, apps, system tools, and\n    Xen/XCP-ng guest tools when running in a VM)\n  - Deploy the MDE-Retro configuration (system-wide and per user)\n  - Install the Windows 2000 icons, cursors, sounds and fonts\n  - Register the MDE-Retro session and the login manager\n  - Switch the system to graphical startup\n\n  To choose which components to install, press ENTER.".into()
}

fn progress_body(app: &App) -> ratatui::text::Text<'static> {
    let mut lines: Vec<Line> = Vec::new();
    for (i, s) in app.steps.iter().enumerate() {
        let mark = if s.done {
            "[done]    "
        } else if i == app.current {
            "[working] "
        } else {
            "[ ]       "
        };
        lines.push(Line::from(format!("{mark}{}", s.label)));
    }
    ratatui::text::Text::from(lines)
}

fn finish_body(app: &App) -> ratatui::text::Text<'static> {
    if app.failed.is_empty() {
        return "MDE-Retro has been installed on this computer.\n\nThe graphical environment (greetd) will now start, and the MDE-Retro logon screen will appear.\n\n  Press ENTER to start the graphical environment.".into();
    }
    // A failed step must not masquerade as success.
    let mut lines = vec![
        Line::from("Setup completed with errors — some steps did not finish:"),
        Line::from(""),
    ];
    for f in &app.failed {
        lines.push(Line::from(format!("  - {f}")));
    }
    lines.push(Line::from(""));
    lines.push(Line::from(
        "The system may be incomplete. Review the errors and re-run `sudo mde setup`.",
    ));
    ratatui::text::Text::from(lines)
}
