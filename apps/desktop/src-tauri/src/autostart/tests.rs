use tempfile::TempDir;

use super::*;

fn at(system: System, dir: &TempDir, program: &str) -> Autostart {
    Autostart {
        system,
        home: dir.path().join("home"),
        config: dir.path().join("config"),
        program: PathBuf::from(program),
        #[cfg(windows)]
        registry_root: None,
    }
}

#[test]
fn an_xdg_entry_starts_the_program_at_login_and_goes_when_turned_off() {
    let dir = TempDir::new().unwrap();
    let autostart = at(System::Linux, &dir, "/usr/bin/lockra-desktop");
    assert!(!autostart.enabled());
    autostart.set(true).unwrap();
    assert_eq!(autostart.file(), dir.path().join("config/autostart/lockra-autostart.desktop"));
    let entry = std::fs::read_to_string(autostart.file()).unwrap();
    assert!(entry.starts_with("[Desktop Entry]\nType=Application\nName=Lockra\n"), "{entry}");
    assert!(entry.contains("\nTryExec=/usr/bin/lockra-desktop\n"), "{entry}");
    assert!(entry.contains("\nExec=/usr/bin/lockra-desktop --autostart\n"), "{entry}");
    assert!(autostart.enabled());
    autostart.set(false).unwrap();
    assert!(!autostart.file().exists() && !autostart.enabled());
    autostart.set(false).unwrap();
}

#[test]
fn what_the_desktop_turned_off_reads_as_off_and_stays_off() {
    let dir = TempDir::new().unwrap();
    let autostart = at(System::Linux, &dir, "/usr/bin/lockra-desktop");
    autostart.set(true).unwrap();
    let entry = std::fs::read_to_string(autostart.file()).unwrap();
    for off in
        [entry.replace("X-GNOME-Autostart-enabled=true", "X-GNOME-Autostart-enabled=false"), format!("{entry}Hidden=true\n"), format!("{entry}Hidden = True\n")]
    {
        std::fs::write(autostart.file(), &off).unwrap();
        assert!(!autostart.enabled(), "{off}");
        autostart.refresh().unwrap();
        assert_eq!(std::fs::read_to_string(autostart.file()).unwrap(), off, "a start leaves the user's choice alone");
    }
    assert!(desktop_entry_on("[Desktop Entry]\nHidden=false\n"));
}

#[test]
fn exec_quotes_a_program_that_needs_it_and_doubles_its_percent_signs() {
    assert_eq!(exec_argument("/usr/bin/lockra-desktop"), "/usr/bin/lockra-desktop");
    assert_eq!(exec_argument("/home/me/My Apps/Lockra.AppImage"), "\"/home/me/My Apps/Lockra.AppImage\"");
    assert_eq!(exec_argument("/opt/100%/lockra"), "/opt/100%%/lockra");
    assert_eq!(exec_argument("/a/$x\"y`z\\w"), "\"/a/\\$x\\\"y\\`z\\\\w\"");
    // The string escapes come after: a literal backslash is four of them in the file.
    let entry = desktop_entry(Path::new("/opt/a\\b/lockra"));
    assert!(entry.contains("\nExec=\"/opt/a\\\\\\\\b/lockra\" --autostart\n"), "{entry}");
    assert!(entry.contains("\nTryExec=/opt/a\\\\b/lockra\n"), "{entry}");
}

#[test]
fn a_launch_agent_runs_the_binary_as_the_user_logs_in() {
    let dir = TempDir::new().unwrap();
    let autostart = at(System::MacOs, &dir, "/Applications/Lockra & Co.app/Contents/MacOS/lockra-desktop");
    autostart.set(true).unwrap();
    assert_eq!(autostart.file(), dir.path().join("home/Library/LaunchAgents/dev.lockra.desktop.autostart.plist"));
    let plist = std::fs::read_to_string(autostart.file()).unwrap();
    assert!(plist.contains("<string>dev.lockra.desktop.autostart</string>"), "{plist}");
    assert!(plist.contains("<string>/Applications/Lockra &amp; Co.app/Contents/MacOS/lockra-desktop</string>\n    <string>--autostart</string>"), "{plist}");
    assert!(plist.contains("<key>RunAtLoad</key>\n  <true/>"), "{plist}");
    assert!(autostart.enabled());
    autostart.set(false).unwrap();
    assert!(!autostart.enabled());
}

#[test]
fn a_start_writes_the_program_again_only_where_the_entry_is_on() {
    let dir = TempDir::new().unwrap();
    let mut autostart = at(System::Linux, &dir, "/home/me/Lockra-0.12.0.AppImage");
    autostart.set(true).unwrap();
    autostart.program = PathBuf::from("/home/me/Apps/Lockra.AppImage");
    autostart.refresh().unwrap();
    assert!(std::fs::read_to_string(autostart.file()).unwrap().contains("\nExec=/home/me/Apps/Lockra.AppImage --autostart\n"));
    autostart.set(false).unwrap();
    autostart.refresh().unwrap();
    assert!(!autostart.file().exists(), "off stays off");
}

#[test]
fn the_run_command_and_task_manager_s_record() {
    assert_eq!(run_command(Path::new(r"C:\Program Files\Lockra\lockra-desktop.exe")), r#""C:\Program Files\Lockra\lockra-desktop.exe" --autostart"#);
    assert!(startup_approved(&[2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]));
    assert!(startup_approved(&[6]));
    assert!(!startup_approved(&[3, 0, 0, 0]));
    assert!(!startup_approved(&[7]));
    assert!(startup_approved(&[]), "no record: as written");
}

#[cfg(windows)]
#[test]
fn the_run_value_goes_and_comes_and_task_manager_s_off_reads_as_off() {
    use winreg::RegKey;
    use winreg::enums::HKEY_CURRENT_USER;
    let root =
        format!("Software\\LockraTest-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
    let dir = TempDir::new().unwrap();
    let autostart = Autostart { registry_root: Some(root.clone()), ..at(System::Windows, &dir, r"C:\Program Files\Lockra\lockra-desktop.exe") };
    let user = RegKey::predef(HKEY_CURRENT_USER);
    let result = std::panic::catch_unwind(|| {
        assert!(!autostart.enabled());
        autostart.set(true).unwrap();
        let run = user.open_subkey(format!(r"{root}\Microsoft\Windows\CurrentVersion\Run")).unwrap();
        assert_eq!(run.get_value::<String, _>("Lockra").unwrap(), r#""C:\Program Files\Lockra\lockra-desktop.exe" --autostart"#);
        assert!(autostart.enabled());
        // Turned off in Task Manager: off here too, until turned on here again.
        let (approved, _) = user.create_subkey(format!(r"{root}\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run")).unwrap();
        approved
            .set_raw_value("Lockra", &winreg::RegValue { vtype: winreg::enums::REG_BINARY, bytes: vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0].into() })
            .unwrap();
        assert!(!autostart.enabled());
        autostart.set(true).unwrap();
        assert!(autostart.enabled());
        autostart.set(false).unwrap();
        assert!(!autostart.enabled());
        autostart.set(false).unwrap();
    });
    let _ = user.delete_subkey_all(&root);
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
