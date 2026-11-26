//! Open at login: an XDG autostart entry, or the XDG Background portal inside Flatpak.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use gtk::glib::prelude::*; // ToVariant
use gtk::{gio, glib};

pub const APP_ID: &str = "io.github.cybercyclone.nacut";

/// ui.json key holding the open-at-login choice sent to the Background portal (Flatpak only).
const PORTAL_PREF: &str = "openAtLogin";

/// Inside Flatpak, XDG_CONFIG_HOME is the app's private `~/.var/app/<id>/config` and the binary is
/// `/app/bin/nacut-gtk`, so an autostart file written there is never seen by the host session.
fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

fn autostart_file() -> PathBuf {
    autostart_file_in(std::env::var_os("XDG_CONFIG_HOME"), &nacut_core::scanner::home_dir())
}

/// `$XDG_CONFIG_HOME/autostart/<id>.desktop`; an unset or empty XDG_CONFIG_HOME means `~/.config` (as in the core).
fn autostart_file_in(config_home: Option<OsString>, home: &Path) -> PathBuf {
    let config = config_home.filter(|v| !v.is_empty()).map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
    config.join("autostart").join(format!("{APP_ID}.desktop"))
}

/// The autostart entry that runs `exe --background`.
fn desktop_entry(exe: &Path) -> String {
    format!(
        "[Desktop Entry]\nType=Application\nName=NACUT\nComment=Claude usage in the tray\nExec={} --background\nIcon={APP_ID}\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
        exec_quoted(&exe.to_string_lossy())
    )
}

/// One quoted `Exec` argument (Desktop Entry spec, "The Exec key"): inside double quotes `"`, `` ` ``, `$` and `\`
/// get a backslash and `%` is doubled; then the value is escaped as a string, so each `\` is written `\\` (a
/// literal `$` ends up as `\\$`) and control characters as `\n`, `\t`, `\r`.
fn exec_quoted(arg: &str) -> String {
    let mut quoted = String::from("\"");
    for c in arg.chars() {
        match c {
            '"' | '`' | '$' | '\\' => {
                quoted.push('\\');
                quoted.push(c);
            }
            '%' => quoted.push_str("%%"),
            c => quoted.push(c),
        }
    }
    quoted.push('"');
    let mut out = String::with_capacity(quoted.len());
    for c in quoted.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c => out.push(c),
        }
    }
    out
}

pub fn autostart_enabled() -> bool {
    if in_flatpak() {
        // The portal keeps its autostart entry on the host, out of our reach; show what was last requested.
        return crate::prefs::get(PORTAL_PREF, "off") == "on";
    }
    autostart_file().exists()
}

pub fn set_autostart(on: bool) -> std::io::Result<()> {
    if in_flatpak() {
        request_background(on)?;
        crate::prefs::set(PORTAL_PREF, if on { "on" } else { "off" });
        return Ok(());
    }
    let path = autostart_file();
    if !on {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            r => r,
        };
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, desktop_entry(&std::env::current_exe()?))
}

/// Asks `org.freedesktop.portal.Background` to add or remove our autostart entry. The portal answers later
/// through a Request object's Response signal (the user may be asked); we only log a failed call.
fn request_background(autostart: bool) -> std::io::Result<()> {
    let bus = gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut options: HashMap<String, glib::Variant> = HashMap::new();
    options.insert("reason".into(), "Start NACUT in the tray when you sign in".to_variant());
    options.insert("autostart".into(), autostart.to_variant());
    options.insert("commandline".into(), vec!["nacut-gtk".to_string(), "--background".to_string()].to_variant());
    options.insert("dap-activatable".into(), false.to_variant());
    // (s parent_window, a{sv} options)
    let params = (String::new(), options).to_variant();
    bus.call(
        Some("org.freedesktop.portal.Desktop"),
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.Background",
        "RequestBackground",
        Some(&params),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        None::<&gio::Cancellable>,
        |result| {
            if let Err(e) = result {
                eprintln!("autostart (Background portal): {e}");
            }
        },
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a desktop reads back from an Exec value: string escapes, then quoting, then `%%`.
    fn unescape_exec(value: &str) -> Vec<String> {
        let mut s = String::new();
        let mut chars = value.chars();
        while let Some(c) = chars.next() {
            if c != '\\' {
                s.push(c);
                continue;
            }
            match chars.next() {
                Some('s') => s.push(' '),
                Some('n') => s.push('\n'),
                Some('t') => s.push('\t'),
                Some('r') => s.push('\r'),
                Some('\\') => s.push('\\'),
                other => panic!("invalid string escape \\{other:?}"),
            }
        }
        let (mut args, mut cur, mut in_quotes, mut chars) = (vec![], String::new(), false, s.chars());
        while let Some(c) = chars.next() {
            match c {
                '"' => in_quotes = !in_quotes,
                '\\' if in_quotes => {
                    let e = chars.next().expect("escaped character");
                    assert!(matches!(e, '"' | '`' | '$' | '\\'), "{e:?} must not be escaped");
                    cur.push(e);
                }
                ' ' if !in_quotes => args.push(std::mem::take(&mut cur)),
                '%' => {
                    assert_eq!(chars.next(), Some('%'), "unescaped field code");
                    cur.push('%');
                }
                c => {
                    assert!(in_quotes || !"\"`$\\'><~|&;*?#()".contains(c), "reserved {c:?} outside quotes");
                    cur.push(c);
                }
            }
        }
        assert!(!in_quotes, "unterminated quote");
        args.push(cur);
        args
    }

    fn exec_line(entry: &str) -> &str {
        let lines: Vec<&str> = entry.lines().filter(|l| l.starts_with("Exec=")).collect();
        assert_eq!(lines.len(), 1, "{entry}");
        &lines[0]["Exec=".len()..]
    }

    #[test]
    fn plain_path() {
        let e = desktop_entry(Path::new("/usr/bin/nacut-gtk"));
        assert_eq!(exec_line(&e), "\"/usr/bin/nacut-gtk\" --background");
        assert!(e.starts_with("[Desktop Entry]\n"));
        assert!(e.contains("\nIcon=io.github.cybercyclone.nacut\n"));
        assert!(e.contains("\nX-GNOME-Autostart-enabled=true\n"));
        assert!(e.ends_with('\n'));
    }

    #[test]
    fn spaces_need_only_the_quotes() {
        assert_eq!(exec_quoted("/opt/My Apps/nacut-gtk"), "\"/opt/My Apps/nacut-gtk\"");
    }

    #[test]
    fn spec_escapes() {
        // A literal `$` is `\\$` and a literal `\` is four backslashes (spec, "The Exec key").
        assert_eq!(exec_quoted("/a$b"), r#""/a\\$b""#);
        assert_eq!(exec_quoted(r"/a\b"), r#""/a\\\\b""#);
        assert_eq!(exec_quoted("/a\"b"), r#""/a\\"b""#);
        assert_eq!(exec_quoted("/a`b"), r#""/a\\`b""#);
        assert_eq!(exec_quoted("/100%/x"), "\"/100%%/x\"");
        assert_eq!(exec_quoted("/a\nb\tc\rd"), r#""/a\nb\tc\rd""#);
        assert_eq!(exec_quoted(""), "\"\"");
    }

    #[test]
    fn other_reserved_characters_stay_literal_inside_quotes() {
        assert_eq!(exec_quoted("/x/'~|&;*?#()<>"), "\"/x/'~|&;*?#()<>\"");
    }

    #[test]
    fn exec_round_trips() {
        for exe in [
            "/usr/bin/nacut-gtk",
            "/home/me/My Apps/nacut-gtk",
            "/home/me/$HOME/`id`/nacut-gtk",
            r"/home/me/back\slash\\twice/nacut-gtk",
            "/home/me/\"quoted\"/it's/nacut-gtk",
            "/home/me/100% done/%u/nacut-gtk",
            "/home/me/new\nline/tab\there/nacut-gtk",
            "/home/me/ünïcødé/日本/nacut-gtk",
        ] {
            let entry = desktop_entry(Path::new(exe));
            assert_eq!(unescape_exec(exec_line(&entry)), vec![exe.to_string(), "--background".to_string()], "{exe:?}");
            assert_eq!(entry.lines().count(), 8, "a path must not break the file into extra lines: {exe:?}");
        }
    }

    #[test]
    fn autostart_in_xdg_config_home() {
        let home = Path::new("/home/me");
        assert_eq!(autostart_file_in(Some("/cfg".into()), home), Path::new("/cfg/autostart/io.github.cybercyclone.nacut.desktop"));
    }

    #[test]
    fn autostart_defaults_to_dot_config() {
        let home = Path::new("/home/me");
        let expected = Path::new("/home/me/.config/autostart/io.github.cybercyclone.nacut.desktop");
        assert_eq!(autostart_file_in(None, home), expected);
        // An empty XDG_CONFIG_HOME used to give the relative path `autostart/…`.
        assert_eq!(autostart_file_in(Some(OsString::new()), home), expected);
    }
}
