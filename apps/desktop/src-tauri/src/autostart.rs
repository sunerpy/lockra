//! Starting Lockra at login (Settings › 通用 › 登录时启动 Lockra), the system's own way and for this
//! user only:
//! - Windows: a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run`;
//! - macOS: a LaunchAgent in `~/Library/LaunchAgents`;
//! - Linux: an XDG autostart entry in `$XDG_CONFIG_HOME/autostart`.
//!
//! Each starts the program (`browser::program`: the AppImage itself when Lockra runs from one) with
//! [`AUTOSTART`], and such a start keeps the window in the tray while Lockra runs in the background.
//!
//! The system's entry is the record, nothing of it in settings.json. What the user turned off in the
//! system's own list reads as off: Windows' startup apps (`StartupApproved`), an XDG entry made
//! `Hidden` or `X-GNOME-Autostart-enabled=false`. Every start writes an entry that is on again,
//! since the program may have moved (`refresh`).

use std::io;
use std::path::{Path, PathBuf};

use lockra_core::{CoreError, ErrorCode};
use tauri::State;

use crate::browser::manifest::System;

/// Started with it, Lockra was started at login.
pub const AUTOSTART: &str = "--autostart";

/// The entries' name: the Run value Windows lists, the XDG entry's `Name`.
pub const NAME: &str = "Lockra";

/// The LaunchAgent's label, and its file's name.
pub const LABEL: &str = "dev.lockra.desktop.autostart";

/// The XDG entry's file.
pub const DESKTOP_FILE: &str = "lockra-autostart.desktop";

#[cfg(windows)]
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
#[cfg(windows)]
const APPROVED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";

/// Where this user's entry goes, and what it starts.
#[derive(Debug, Clone)]
pub struct Autostart {
    pub system: System,
    /// The home folder: macOS's LaunchAgents are under it.
    pub home: PathBuf,
    /// The XDG config home (`$XDG_CONFIG_HOME`, else `~/.config`).
    pub config: PathBuf,
    /// The program the entry starts.
    pub program: PathBuf,
    /// The registry's root for the keys (`Software` by default; the tests use their own).
    #[cfg(windows)]
    pub registry_root: Option<String>,
}

impl Autostart {
    /// This user's.
    pub fn current() -> io::Result<Self> {
        let missing = || io::Error::new(io::ErrorKind::NotFound, "no home folder");
        Ok(Self {
            system: System::current(),
            home: dirs::home_dir().ok_or_else(missing)?,
            config: dirs::config_dir().ok_or_else(missing)?,
            program: crate::browser::program()?,
            #[cfg(windows)]
            registry_root: None,
        })
    }

    /// The entry's file on macOS and Linux.
    pub fn file(&self) -> PathBuf {
        match self.system {
            System::MacOs => self.home.join("Library/LaunchAgents").join(format!("{LABEL}.plist")),
            _ => self.config.join("autostart").join(DESKTOP_FILE),
        }
    }

    /// Whether Lockra starts at login.
    pub fn enabled(&self) -> bool {
        match self.system {
            System::Windows => self.registered(),
            System::MacOs => self.file().is_file(),
            System::Linux => std::fs::read_to_string(self.file()).is_ok_and(|entry| desktop_entry_on(&entry)),
        }
    }

    /// Start Lockra at login, or no longer.
    pub fn set(&self, on: bool) -> io::Result<()> {
        match (self.system, on) {
            (System::Windows, _) => self.register(on),
            (_, true) => {
                let file = self.file();
                if let Some(dir) = file.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                let text = match self.system {
                    System::MacOs => launch_agent(&self.program),
                    _ => desktop_entry(&self.program),
                };
                std::fs::write(file, text)
            }
            (_, false) => match std::fs::remove_file(self.file()) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            },
        }
    }

    /// Write an entry that is on again: the program may have moved since. One that is off stays so.
    pub fn refresh(&self) -> io::Result<()> {
        if self.enabled() { self.set(true) } else { Ok(()) }
    }

    #[cfg(windows)]
    fn key(&self, key: &str) -> String {
        match &self.registry_root {
            Some(root) => key.replacen("Software", root, 1),
            None => key.to_owned(),
        }
    }

    #[cfg(windows)]
    fn registered(&self) -> bool {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let run = user.open_subkey(self.key(RUN_KEY)).and_then(|key| key.get_value::<String, _>(NAME)).is_ok();
        let approved = user.open_subkey(self.key(APPROVED_KEY)).and_then(|key| key.get_raw_value(NAME)).ok().map(|value| value.bytes);
        run && approved.as_deref().is_none_or(startup_approved)
    }

    #[cfg(not(windows))]
    fn registered(&self) -> bool {
        false
    }

    /// The Run value, and no word of Task Manager's against it: turned on here, it is on.
    #[cfg(windows)]
    fn register(&self, on: bool) -> io::Result<()> {
        use winreg::RegKey;
        use winreg::enums::HKEY_CURRENT_USER;
        let user = RegKey::predef(HKEY_CURRENT_USER);
        let gone = |result: io::Result<()>| match result {
            Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
        if let Ok(approved) = user.open_subkey_with_flags(self.key(APPROVED_KEY), winreg::enums::KEY_SET_VALUE) {
            gone(approved.delete_value(NAME))?;
        }
        if on {
            let (run, _) = user.create_subkey(self.key(RUN_KEY))?;
            run.set_value(NAME, &run_command(&self.program))
        } else {
            match user.open_subkey_with_flags(self.key(RUN_KEY), winreg::enums::KEY_SET_VALUE) {
                Ok(run) => gone(run.delete_value(NAME)),
                Err(error) => gone(Err(error)),
            }
        }
    }

    #[cfg(not(windows))]
    fn register(&self, _on: bool) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Unsupported, "the Run key is Windows'"))
    }
}

/// The entry the shell keeps: this user's (`run`), the tests' folders, or none.
pub struct AutostartState(pub Option<Autostart>);

/// Whether Lockra starts at login; `None` where this build keeps no entry.
#[tauri::command]
pub async fn autostart_get(state: State<'_, AutostartState>) -> Result<Option<bool>, CoreError> {
    let Some(autostart) = state.0.clone() else { return Ok(None) };
    blocking(move || Ok(autostart.enabled())).await.map(Some)
}

/// Start Lockra at login, or no longer: whether it now does.
#[tauri::command]
pub async fn autostart_set(state: State<'_, AutostartState>, on: bool) -> Result<bool, CoreError> {
    let Some(autostart) = state.0.clone() else { return Err(ErrorCode::IoFailed.into()) };
    blocking(move || {
        autostart.set(on)?;
        Ok(autostart.enabled())
    })
    .await
}

async fn blocking<T: Send + 'static>(work: impl FnOnce() -> io::Result<T> + Send + 'static) -> Result<T, CoreError> {
    match tauri::async_runtime::spawn_blocking(work).await {
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => {
            tracing::warn!(%error, "the login entry");
            Err(ErrorCode::IoFailed.into())
        }
        Err(_) => Err(ErrorCode::Internal.into()),
    }
}

/// The Run value's command: the program quoted, then the flag.
pub fn run_command(program: &Path) -> String {
    format!("\"{}\" {AUTOSTART}", program.display())
}

/// Whether Task Manager's record of a Run value leaves it on: its first byte is even (2 or 6) when
/// on, odd (3 or 7) once the user turned it off.
pub fn startup_approved(bytes: &[u8]) -> bool {
    bytes.first().is_none_or(|first| first % 2 == 0)
}

/// The LaunchAgent that starts `program` at login.
pub fn launch_agent(program: &Path) -> String {
    let program = xml_text(&program.to_string_lossy());
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n\
         <plist version=\"1.0\">\n\
         <dict>\n\
         \x20 <key>Label</key>\n\
         \x20 <string>{LABEL}</string>\n\
         \x20 <key>ProgramArguments</key>\n\
         \x20 <array>\n\
         \x20   <string>{program}</string>\n\
         \x20   <string>{AUTOSTART}</string>\n\
         \x20 </array>\n\
         \x20 <key>RunAtLoad</key>\n\
         \x20 <true/>\n\
         \x20 <key>ProcessType</key>\n\
         \x20 <string>Interactive</string>\n\
         </dict>\n\
         </plist>\n"
    )
}

fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The XDG autostart entry that starts `program` at login. `TryExec` makes the desktop pass over it
/// once the program is gone (uninstalled, moved).
pub fn desktop_entry(program: &Path) -> String {
    let program = program.to_string_lossy();
    format!(
        "[Desktop Entry]\nType=Application\nName={NAME}\nComment=Start Lockra at login\nTryExec={}\nExec={} {AUTOSTART}\nTerminal=false\nX-GNOME-Autostart-enabled=true\n",
        string_value(&program),
        string_value(&exec_argument(&program)),
    )
}

/// An argument of `Exec`, quoted as the Desktop Entry spec asks when it holds a reserved character
/// (inside the quotes `"`, `` ` ``, `$` and `\` take a backslash), and its `%` doubled (a field code
/// otherwise).
pub fn exec_argument(argument: &str) -> String {
    const RESERVED: &[char] = &[' ', '\t', '\n', '"', '\'', '\\', '>', '<', '~', '|', '&', ';', '$', '*', '?', '#', '(', ')', '`'];
    let argument = argument.replace('%', "%%");
    if !argument.contains(RESERVED) {
        return argument;
    }
    let mut quoted = String::from("\"");
    for c in argument.chars() {
        if matches!(c, '"' | '`' | '$' | '\\') {
            quoted.push('\\');
        }
        quoted.push(c);
    }
    quoted.push('"');
    quoted
}

/// A value of the entry's string type: its own escapes, applied after `Exec`'s quoting.
fn string_value(value: &str) -> String {
    value.replace('\\', "\\\\").replace('\n', "\\n").replace('\t', "\\t").replace('\r', "\\r")
}

/// Whether an XDG entry is on: not `Hidden`, and not turned off in GNOME's startup applications.
pub fn desktop_entry_on(entry: &str) -> bool {
    let value = |key: &str| entry.lines().find_map(|line| line.trim().strip_prefix(key)?.trim_start().strip_prefix('=').map(str::trim));
    !value("Hidden").is_some_and(|v| v.eq_ignore_ascii_case("true")) && !value("X-GNOME-Autostart-enabled").is_some_and(|v| v.eq_ignore_ascii_case("false"))
}

#[cfg(test)]
mod tests;
