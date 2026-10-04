//! The keychain hand-over of a macOS update, driven by `.github/scripts/check-keychain-handoff.sh`
//! on a Mac's login keychain (docs/security.md, "Keychain items across updates").
//!
//! The same code the app runs (per_build.rs, handoff.rs, keychain_handoff.rs), with a certificate
//! the CI run makes in place of the release certificate, a keychain service of its own, and the
//! keychain's dialog never shown (nobody answers it on a runner: a read that would need it fails).
//! Built with `LOCKRA_HARNESS_REQUIREMENT` (the run's requirement) and `LOCKRA_HARNESS_BUILD` (a
//! word that makes each build's code, and so its cdhash and keychain partition, differ).
//!
//! Commands, one per run:
//! - `account`: this build's item account
//! - `put <entry> <value>`, `get <entry>`: the store, as the app uses it
//! - `peek <entry> <account>`: read one item without asking (`found <value>`, `missing`, `would-ask`)
//! - `handoff <executable>`: hand this build's entries to `executable`, as an update does
//! - `force-handoff <executable>`: the same without checking this build's own signature (the
//!   check that the staged build refuses a parent signed otherwise)
//! - `wipe <entry>`: remove every item of `entry`
//! - `--lockra-keychain-handoff`: the staged build's side, as in the app

#[cfg(target_os = "macos")]
fn main() {
    std::process::exit(harness::run());
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("keychain_harness: macOS only");
    std::process::exit(2);
}

#[cfg(target_os = "macos")]
mod harness {
    use std::path::Path;
    use std::time::Duration;

    use lockra_core::ports::SecretStore as _;
    use lockra_desktop_lib::handoff::{self, HANDOFF_ARG, PeerCheck};
    use lockra_desktop_lib::keychain_handoff::Release;
    use lockra_desktop_lib::per_build::{Ask, Keychain as _, Read};

    const SERVICE: &str = "dev.lockra.ci-harness";

    fn requirement() -> &'static str {
        option_env!("LOCKRA_HARNESS_REQUIREMENT").unwrap_or("")
    }

    fn build() -> &'static str {
        option_env!("LOCKRA_HARNESS_BUILD").unwrap_or("unnamed")
    }

    struct AnyPeer;

    impl PeerCheck for AnyPeer {
        fn trusted(&self, _pid: u32) -> bool {
            true
        }
    }

    pub fn run() -> i32 {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let Some(release) = Release::new(requirement(), SERVICE, true) else {
            eprintln!("{}: LOCKRA_HARNESS_REQUIREMENT is missing or does not parse", build());
            return 2;
        };
        let words: Vec<&str> = args.iter().map(String::as_str).collect();
        // The staged side shares its parent's stdout: its line goes to stderr, so the parent's
        // output stays the one line the check reads.
        if let [arg] = words.as_slice()
            && *arg == HANDOFF_ARG
        {
            return match release.take_handoff() {
                Ok(n) => {
                    eprintln!("{}: stored {n}", build());
                    0
                }
                Err(error) => {
                    eprintln!("{}: {error}", build());
                    1
                }
            };
        }
        let outcome = match words.as_slice() {
            ["account"] => release.store().map(|store| store.account()).ok_or_else(|| "no store".to_owned()),
            ["put", entry, value] => {
                release.store().ok_or_else(|| "no store".to_owned()).and_then(|store| store.set(entry, value).map(|()| "stored".to_owned()).map_err(|e| e.0))
            }
            ["get", entry] => release.store().ok_or_else(|| "no store".to_owned()).and_then(|store| match store.get(entry) {
                Ok(Some(value)) => Ok(format!("found {}", value.as_str())),
                Ok(None) => Ok("none".to_owned()),
                Err(e) => Err(e.0),
            }),
            ["peek", entry, account] => peek(&release, entry, account),
            ["handoff", executable] => release
                .store()
                .ok_or_else(|| "no store".to_owned())
                .and_then(|store| release.hand_over_to(&store, Path::new(executable)))
                .map(|n| format!("handed {n}")),
            ["force-handoff", executable] => {
                let entries = vec![("forced".to_owned(), Some(zeroize::Zeroizing::new("from an unsigned parent".to_owned())))];
                handoff::start_preinstall(Path::new(executable), [""; 0], &entries, &AnyPeer, Duration::from_secs(20))
                    .map_err(|e| e.to_string())
                    .and_then(|mut child| child.wait().map_err(|e| e.to_string()))
                    .and_then(|status| if status.success() { Ok("handed".to_owned()) } else { Err(format!("the child exited with {status}")) })
            }
            ["wipe", entry] => wipe(entry),
            _ => Err(format!("unknown command {args:?}")),
        };
        match outcome {
            Ok(line) => {
                println!("{line}");
                0
            }
            Err(error) => {
                eprintln!("{}: {error}", build());
                1
            }
        }
    }

    fn peek(release: &Release, entry: &str, account: &str) -> Result<String, String> {
        let store = release.store().ok_or("no store")?;
        match store.keychain().read(&format!("{SERVICE}/{entry}"), account, Ask::Never).map_err(|e| e.0)? {
            Read::Found(value) => Ok(format!("found {}", value.as_str())),
            Read::Missing => Ok("missing".into()),
            Read::WouldAsk => Ok("would-ask".into()),
        }
    }

    fn wipe(entry: &str) -> Result<String, String> {
        let keychain = lockra_desktop_lib::macos_keychain::LoginKeychain::open(true).map_err(|e| e.0)?;
        let service = format!("{SERVICE}/{entry}");
        let items = keychain.items(&service).map_err(|e| e.0)?;
        for item in &items {
            keychain.remove(&item.service, &item.account).map_err(|e| e.0)?;
        }
        keychain.remove(SERVICE, entry).map_err(|e| e.0)?;
        Ok(format!("removed {}", items.len()))
    }
}
