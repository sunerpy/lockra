//! The third-party data compiled into the crate, byte for byte as published (`data/SOURCES` says
//! where each file comes from and pins its SHA-256): Apple's password-manager-resources lists and
//! the EFF's large wordlist. Each is parsed once, on first use, by the module that reads it.

/// Groups of domains that share one sign-in (`quirks/shared-credentials.json`).
pub(crate) const SHARED_CREDENTIALS: &str = include_str!("../data/password-manager-resources/shared-credentials.json");

/// Password rules of sites that do not publish their own (`quirks/password-rules.json`).
pub(crate) const PASSWORD_RULES: &str = include_str!("../data/password-manager-resources/password-rules.json");

/// 7776 words, one per line after its five dice digits and a tab.
pub(crate) const EFF_LARGE_WORDLIST: &str = include_str!("../data/eff/eff_large_wordlist.txt");

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::Path;

    use sha2::{Digest, Sha256};

    /// Every pinned file as compiled in (the licence only here, for the check).
    const EMBEDDED: [(&str, &[u8]); 4] = [
        ("password-manager-resources/LICENSE.md", include_bytes!("../data/password-manager-resources/LICENSE.md")),
        ("password-manager-resources/password-rules.json", super::PASSWORD_RULES.as_bytes()),
        ("password-manager-resources/shared-credentials.json", super::SHARED_CREDENTIALS.as_bytes()),
        ("eff/eff_large_wordlist.txt", super::EFF_LARGE_WORDLIST.as_bytes()),
    ];

    /// `(path, sha256, url)` for each file line of `data/SOURCES`, as scripts/update-vendored-data.sh reads it.
    fn pins() -> Vec<(&'static str, &'static str, &'static str)> {
        include_str!("../data/SOURCES")
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .map(|line| {
                let fields: Vec<&str> = line.split_whitespace().collect();
                assert_eq!(fields.len(), 3, "a SOURCES line is a path, a SHA-256 and a URL: {line}");
                (fields[0], fields[1], fields[2])
            })
            .collect()
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn embedded_files_are_the_pinned_bytes() {
        let pins = pins();
        let pinned: BTreeSet<&str> = pins.iter().map(|(path, _, _)| *path).collect();
        let embedded: BTreeSet<&str> = EMBEDDED.iter().map(|(path, _)| *path).collect();
        assert_eq!(pinned, embedded, "data/SOURCES and the embedded files name the same files");
        for (path, sha256, _) in pins {
            let (_, bytes) = EMBEDDED.iter().find(|(embedded, _)| *embedded == path).unwrap();
            assert_eq!(sha256_hex(bytes), sha256, "{path} differs from its pin: update it with scripts/update-vendored-data.sh, never by hand");
        }
    }

    #[test]
    fn every_file_in_data_is_pinned() {
        fn walk(dir: &Path, root: &Path, found: &mut BTreeSet<String>) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, root, found);
                } else {
                    found.insert(path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
                }
            }
        }
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("data");
        let mut found = BTreeSet::new();
        walk(&root, &root, &mut found);
        found.remove("SOURCES");
        let pinned: BTreeSet<String> = pins().iter().map(|(path, _, _)| (*path).to_owned()).collect();
        assert_eq!(found, pinned, "every file under data/ has a line in data/SOURCES");
    }

    #[test]
    fn sources_are_fixed() {
        let mut commits = BTreeSet::new();
        for (path, sha256, url) in pins() {
            assert!(sha256.len() == 64 && sha256.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()), "{path}: {sha256}");
            assert!(url.starts_with("https://"), "{path}: {url}");
            if let Some(rest) = url.strip_prefix("https://raw.githubusercontent.com/apple/password-manager-resources/") {
                let commit = rest.split('/').next().unwrap();
                assert!(commit.len() == 40 && commit.bytes().all(|byte| byte.is_ascii_hexdigit()), "{path} is pinned to a commit, not a branch: {url}");
                commits.insert(commit);
            }
        }
        // The licence and the lists come from one upstream state.
        assert_eq!(commits.len(), 1, "{commits:?}");
    }
}
