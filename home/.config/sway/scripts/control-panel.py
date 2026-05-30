#!/usr/bin/env python3
"""
Windows 2000-style Control Panel — a real window (GTK3).

Classic Win2000 Control Panel folder layout:
  * a working menu bar (File / Edit / View / Favorites / Tools / Help),
  * a blue "web view" side banner (title + Windows Update / Help links),
  * a white icon grid (FlowBox) of applets with icons + Windows names,
  * a status bar with the object count.

Each applet maps a Windows 2000 Control Panel name to the real Fedora/Red Hat
tool installed on this machine; missing tools are hidden. Double-click or
File > Open launches the tool. Self-styled CSS, so it looks correct regardless
of the system GTK theme.
"""

import shlex
import shutil
import subprocess

import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk, Gdk, GdkPixbuf, GLib  # noqa: E402

# (Windows label, shell command, run-in-terminal, icon-name candidates)
APPLETS = [
    ("Add/Remove Programs", "dnfdragora", False,
     ["dnfdragora", "system-software-install", "package-x-generic"]),
    ("Automatic Updates", "dnfdragora-updater", False,
     ["system-software-update", "software-update-available"]),
    ("Windows Firewall", "firewall-config", False,
     ["firewall-config", "security-high", "preferences-system-firewall"]),
    ("Network and Dial-up Connections", "nm-connection-editor", False,
     ["network-wired", "preferences-system-network", "network-workgroup"]),
    ("Sounds and Multimedia", "pavucontrol", False,
     ["multimedia-volume-control", "audio-volume-high", "audio-card"]),
    ("Disk Management", "gnome-disks", False,
     ["drive-harddisk", "gnome-disks", "disk"]),
    ("Partition Manager", "gparted", False,
     ["gparted", "drive-harddisk"]),
    ("Storage Spaces", "blivet-gui", False,
     ["drive-multidisk", "blivet-gui", "drive-harddisk"]),
    ("Users and Passwords", "seahorse", False,
     ["system-users", "preferences-system-users", "dialog-password"]),
    ("Regional Options", "system-config-language", False,
     ["preferences-desktop-locale", "preferences-desktop-locale-panel"]),
    ("Keyboard", "im-chooser", False,
     ["input-keyboard", "preferences-desktop-keyboard"]),
    ("Event Viewer", "gnome-abrt", False,
     ["logviewer", "utilities-system-monitor", "dialog-warning"]),
    ("Security Center", "sealert -b", False,
     ["security-high", "preferences-system-privacy"]),
    ("Create Installation Media", "mediawriter", False,
     ["media-optical", "drive-removable-media-usb", "media-removable"]),
    ("System", "hostnamectl; echo; read -p 'Press Enter to close '", True,
     ["computer", "computer-laptop", "preferences-system"]),
    ("Date and Time", "timedatectl; echo; read -p 'Press Enter to close '", True,
     ["preferences-system-time", "clock", "x-office-calendar"]),
]

CSS = b"""
* { font-family: "Tahoma", "Droid Sans", sans-serif; font-size: 11px; }
window { background-color: #d4d0c8; }

/* Left blue "web view" banner. */
.cp-sidebar {
    background-image: linear-gradient(160deg, #1d5ca8 0%, #2f7fce 55%, #6fb3e8 100%);
    border-right: 1px solid #808080;
}
.cp-sidebar-title {
    color: #ffffff; font-size: 15px; font-weight: bold;
    padding: 10px 10px 6px 12px;
}
.cp-sidebar-rule { background-color: #ffffff; min-height: 2px; margin: 0 10px; }
.cp-link { color: #ffffff; padding: 2px 12px; background: transparent;
           border: none; box-shadow: none; }
.cp-link:hover { color: #ffe680; }

/* Right icon area = white folder view. */
.cp-iconview { background-color: #ffffff; color: #000000; }
.cp-iconview label { color: #000000; }
.cp-iconview flowboxchild { padding: 5px 3px; border-radius: 0; }
.cp-iconview flowboxchild:selected { background-color: #0a246a; }
.cp-iconview flowboxchild:selected label { color: #ffffff; }

/* Menu bar / status bar = silver, square. */
menubar { background-color: #d4d0c8; color: #000000; border-bottom: 1px solid #808080; }
menubar > menuitem { padding: 2px 8px; }
menubar > menuitem:hover { background-color: #0a246a; color: #ffffff; }
menu { background-color: #d4d0c8; color: #000000; border: 1px solid #404040; }
menu menuitem:hover { background-color: #0a246a; color: #ffffff; }
.cp-status { background-color: #d4d0c8; color: #000000;
             border-top: 1px solid #808080; padding: 1px 6px; }
"""


def first_available_icon(theme, names):
    for n in names:
        if theme.has_icon(n):
            return n
    return "application-x-executable"


def load_pixbuf(theme, names, size=32):
    name = first_available_icon(theme, names)
    try:
        return theme.load_icon(name, size, Gtk.IconLookupFlags.FORCE_SIZE)
    except GLib.Error:
        return None


def launch(cmd, in_terminal=False):
    if in_terminal:
        cmd = "foot -e sh -c " + shlex.quote(cmd)
    subprocess.Popen(["setsid", "-f", "sh", "-c", cmd],
                     stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                     start_new_session=True)


class ControlPanel(Gtk.Window):
    def __init__(self):
        super().__init__(title="Control Panel")
        self.set_default_size(600, 440)
        self.set_position(Gtk.WindowPosition.CENTER)

        self.theme = Gtk.IconTheme.get_default()
        self.applets = [a for a in APPLETS
                        if shutil.which(a[1].replace(";", " ").split()[0])]
        self.icon_size = 32

        root = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        self.add(root)
        root.pack_start(self._build_menubar(), False, False, 0)

        main = Gtk.Box(orientation=Gtk.Orientation.HORIZONTAL)
        root.pack_start(main, True, True, 0)
        main.pack_start(self._build_sidebar(), False, False, 0)

        self.flow = Gtk.FlowBox()
        self.flow.set_valign(Gtk.Align.START)
        self.flow.set_max_children_per_line(4)
        self.flow.set_min_children_per_line(1)
        self.flow.set_selection_mode(Gtk.SelectionMode.SINGLE)
        self.flow.set_homogeneous(True)
        self.flow.set_row_spacing(6)
        self.flow.set_column_spacing(2)
        self.flow.set_border_width(8)
        self.flow.get_style_context().add_class("cp-iconview")
        self.flow.connect("child-activated", self._on_child_activated)
        self._populate_flow()

        scroll = Gtk.ScrolledWindow()
        scroll.set_policy(Gtk.PolicyType.NEVER, Gtk.PolicyType.AUTOMATIC)
        scroll.add(self.flow)
        main.pack_start(scroll, True, True, 0)

        self.status = Gtk.Label(label="", xalign=0.0)
        self.status.get_style_context().add_class("cp-status")
        root.pack_start(self.status, False, False, 0)
        self._update_status()

    # ---------------------------------------------------------------- menu
    def _build_menubar(self):
        bar = Gtk.MenuBar()

        file_menu = self._menu(bar, "File")
        self._item(file_menu, "Open", self._open_selected)
        file_menu.append(Gtk.SeparatorMenuItem())
        self._item(file_menu, "Close", lambda *_: self.close())

        edit_menu = self._menu(bar, "Edit")
        self._item(edit_menu, "Select All",
                   lambda *_: self.flow.select_all())
        self._item(edit_menu, "Clear Selection",
                   lambda *_: self.flow.unselect_all())

        view_menu = self._menu(bar, "View")
        grp = []
        large = Gtk.RadioMenuItem.new_with_label(grp, "Large Icons")
        grp = large.get_group()
        small = Gtk.RadioMenuItem.new_with_label(grp, "Small Icons")
        large.set_active(True)
        large.connect("toggled", self._set_icon_size, 32)
        small.connect("toggled", self._set_icon_size, 16)
        view_menu.append(large)
        view_menu.append(small)
        view_menu.append(Gtk.SeparatorMenuItem())
        self._item(view_menu, "Refresh", lambda *_: self._populate_flow())
        status_item = Gtk.CheckMenuItem(label="Status Bar")
        status_item.set_active(True)
        status_item.connect("toggled",
                            lambda it: self.status.set_visible(it.get_active()))
        view_menu.append(status_item)

        fav_menu = self._menu(bar, "Favorites")
        for label, cmd in (("Windows Update", "dnfdragora-updater"),
                           ("Add/Remove Programs", "dnfdragora"),
                           ("Network Connections", "nm-connection-editor")):
            if shutil.which(cmd):
                self._item(fav_menu, label, lambda *_a, c=cmd: launch(c))

        tools_menu = self._menu(bar, "Tools")
        self._item(tools_menu, "Command Prompt", lambda *_: launch("foot"))
        self._item(tools_menu, "System Tools…",
                   lambda *_: launch(
                       f"{shlex.quote(SYSTEM_MENU)} system"))
        tools_menu.append(Gtk.SeparatorMenuItem())
        folder_opts = Gtk.MenuItem(label="Folder Options…")
        folder_opts.set_sensitive(False)
        tools_menu.append(folder_opts)

        help_menu = self._menu(bar, "Help")
        self._item(help_menu, "Help and Support Center",
                   lambda *_: launch("xdg-open https://docs.fedoraproject.org"))
        help_menu.append(Gtk.SeparatorMenuItem())
        self._item(help_menu, "About Control Panel", self._show_about)

        return bar

    def _menu(self, bar, title):
        item = Gtk.MenuItem(label=title)
        submenu = Gtk.Menu()
        item.set_submenu(submenu)
        bar.append(item)
        return submenu

    def _item(self, submenu, label, handler):
        mi = Gtk.MenuItem(label=label)
        mi.connect("activate", handler)
        submenu.append(mi)
        return mi

    # ------------------------------------------------------------- sidebar
    def _build_sidebar(self):
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL)
        box.set_size_request(186, -1)
        box.get_style_context().add_class("cp-sidebar")

        title = Gtk.Label(label="Control Panel", xalign=0.0)
        title.get_style_context().add_class("cp-sidebar-title")
        box.pack_start(title, False, False, 0)

        rule = Gtk.Box()
        rule.get_style_context().add_class("cp-sidebar-rule")
        box.pack_start(rule, False, False, 4)

        for text, cmd, term in (
            ("Windows Update", "dnfdragora-updater", False),
            ("Help and Support", "xdg-open https://docs.fedoraproject.org", False),
        ):
            if not shutil.which(cmd.split()[0]):
                continue
            btn = Gtk.Button(label=text)
            btn.set_relief(Gtk.ReliefStyle.NONE)
            btn.set_halign(Gtk.Align.START)
            btn.get_style_context().add_class("cp-link")
            btn.connect("clicked", lambda _b, c=cmd, t=term: launch(c, t))
            box.pack_start(btn, False, False, 0)

        return box

    # --------------------------------------------------------------- grid
    def _populate_flow(self):
        for child in self.flow.get_children():
            self.flow.remove(child)
        for applet in self.applets:
            self.flow.add(self._make_applet(applet))
        self.flow.show_all()

    def _make_applet(self, applet):
        label, _cmd, _term, icons = applet
        box = Gtk.Box(orientation=Gtk.Orientation.VERTICAL, spacing=3)
        box.set_size_request(108, -1)
        pb = load_pixbuf(self.theme, icons, self.icon_size)
        img = (Gtk.Image.new_from_pixbuf(pb) if pb else
               Gtk.Image.new_from_icon_name("application-x-executable",
                                            Gtk.IconSize.DIALOG))
        box.pack_start(img, False, False, 0)
        lbl = Gtk.Label(label=label)
        lbl.set_line_wrap(True)
        lbl.set_justify(Gtk.Justification.CENTER)
        lbl.set_max_width_chars(15)
        box.pack_start(lbl, False, False, 0)
        child = Gtk.FlowBoxChild()
        child.add(box)
        return child

    # ------------------------------------------------------------ actions
    def _on_child_activated(self, _flow, child):
        self._launch_index(child.get_index())

    def _open_selected(self, *_):
        sel = self.flow.get_selected_children()
        if sel:
            self._launch_index(sel[0].get_index())

    def _launch_index(self, idx):
        if 0 <= idx < len(self.applets):
            _label, cmd, term, _icons = self.applets[idx]
            launch(cmd, term)

    def _set_icon_size(self, item, size):
        if item.get_active():
            self.icon_size = size
            self._populate_flow()

    def _update_status(self):
        self.status.set_text(f"{len(self.applets)} objects")

    def _show_about(self, *_):
        dlg = Gtk.MessageDialog(
            transient_for=self, modal=True, message_type=Gtk.MessageType.INFO,
            buttons=Gtk.ButtonsType.OK, text="Control Panel")
        dlg.format_secondary_text(
            "Windows 2000-style Control Panel\n"
            "Maps Fedora/Red Hat system tools to classic Windows names.")
        dlg.run()
        dlg.destroy()


SYSTEM_MENU = "/home/mm/.config/sway/scripts/win95-menu.sh"


def main():
    provider = Gtk.CssProvider()
    provider.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_screen(
        Gdk.Screen.get_default(), provider,
        Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)

    win = ControlPanel()
    win.connect("destroy", Gtk.main_quit)
    win.show_all()
    Gtk.main()


if __name__ == "__main__":
    main()
