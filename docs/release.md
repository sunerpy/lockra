# Releasing Lockra

For maintainers. The pipeline is the github-project-scaffold **tauri profile**
(`.github/scaffold.json`): release-please owns the version, the tag and a draft GitHub Release, and
one workflow run builds, verifies and publishes everything.

## The flow

1. Pull requests are squash-merged into `main` with a Conventional Commit title
   (`CONTRIBUTING.md`).
2. Every push to `main` runs `.github/workflows/release.yml`; its first job runs release-please,
   which opens or updates the release pull request `chore: release X.Y.Z` (version in
   `package.json`, an entry in `CHANGELOG.md`).
3. Merging the release pull request creates the tag `vX.Y.Z` and a **draft** Release, and the same
   run continues:
   - `preflight` checks out the tag, asserts it is the released commit, checks `tauri.conf.json`
     against the version (`.github/scripts/tauri-release.py check-config --expect-version`) and
     renders the matrix from `.github/release-targets.json`;
   - `bundle` builds each target with the lockfile's Tauri CLI: `tauri build --no-bundle` without
     any secret, then `tauri bundle` with the update signing key (deb, rpm and AppImage on Linux x64
     and arm64; the app archive and the dmg for Apple silicon and Intel; NSIS and MSI on Windows
     x64, NSIS on Windows arm64), which signs every package for the in-app update;
   - `bundle-android` builds the phone app's APK and AAB for arm64 unsigned, signs them with the
     Android key in a step of its own and checks them against the pinned certificate (below);
   - `updater` collects every leg, verifies every signature against the public key in
     `tauri.conf.json`, writes `latest.json` (below) and `SHA256SUMS`, attests the files (SLSA
     build provenance) and attaches them to the draft. `.github/scripts/test-tauri-release.py`
     checks every leg and the manifest's keys against the target manifest;
   - `publish-release` re-verifies every remote asset against `SHA256SUMS` and flips the draft to
     public, marking it latest unless it is a prerelease.
4. A failed leg leaves the draft unpublished. Rebuild it after a fix of the workflow or an outage
   with **Run workflow** (`workflow_dispatch`, `tag_name: vX.Y.Z`); a fix of the source needs a new
   version.

Versions: `package.json` is the only source (`tauri.conf.json` points at it, `Cargo.toml` stays at
`0.0.0`). `.release-please-manifest.json` holds the last released version, `0.0.0` until the first
release. The first release is `0.1.0` (`initial-version` in `release-please-config.json`): with no
earlier release tag, release-please would otherwise start at `1.0.0`.

release-please picks the next version from the commits since the last tag: a `fix` bumps the patch,
a `feat` the minor (below 1.0 a breaking change too, `bump-minor-pre-major`). To release another
version, for example only a patch after a `feat`, squash-merge a pull request whose commit message
ends with the footer `Release-As: X.Y.Z` (pass the body to `gh pr merge --body-file`): the release
pull request switches to that version. The footer counts for that release only; later releases
follow the commits again.

## Repository setup

The repository was created and configured on 2026-10-01 with these commands. They double as the
audit list: read a setting back with the same path and `GET`.

```bash
gh repo create sunerpy/lockra --public --source . --remote origin \
  --description "Two-factor codes on your own computer: an encrypted, offline TOTP/HOTP authenticator for Windows, macOS and Linux, with import and export for Google Authenticator and Microsoft Authenticator." \
  --homepage https://firlab.app/lockra/
git push -u origin main          # the only direct push the repository ever gets
gh repo edit sunerpy/lockra \
  --add-topic authenticator --add-topic totp --add-topic hotp --add-topic two-factor-authentication \
  --add-topic tauri --add-topic rust --add-topic react --add-topic desktop-app \
  --add-topic google-authenticator --add-topic microsoft-authenticator
```

The settings that make the pipeline fail closed:

```bash
R=sunerpy/lockra
# Squash only: the pull request title as the subject, its commits' messages as the body, branches
# deleted after the merge.
gh api -X PATCH repos/$R -F allow_squash_merge=true -F allow_merge_commit=false \
  -F allow_rebase_merge=false -F delete_branch_on_merge=true -f squash_merge_commit_title=PR_TITLE
# A read-only default token; release-please may still open its pull request.
gh api -X PUT repos/$R/actions/permissions/workflow \
  -f default_workflow_permissions=read -F can_approve_pull_request_reviews=true
# Every action must be pinned to a full commit SHA.
gh api -X PUT repos/$R/actions/permissions -F enabled=true -f allowed_actions=all -F sha_pinning_required=true
# Pull requests from outside contributors wait for approval before they run.
gh api -X PUT repos/$R/actions/permissions/fork-pr-contributor-approval \
  -f approval_policy=all_external_contributors
# v* tags cannot be moved or deleted, by anyone. Creating one stays open: release-please creates
# the tag with GITHUB_TOKEN, and a repository owned by a user, not an organisation, cannot name the
# GitHub Actions app as the only actor allowed to.
gh api -X POST repos/$R/rulesets --input - <<'JSON'
{
  "name": "release tags",
  "target": "tag",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["refs/tags/v*"], "exclude": [] } },
  "rules": [{ "type": "update" }, { "type": "deletion" }],
  "bypass_actors": []
}
JSON
# main: pull requests only, squash-merged, with CI Success from GitHub Actions on an up-to-date
# branch; no deletion, no force push, nobody on the bypass list. The release job refuses to run
# unless main is protected. Created before the first push; the checks do not apply to creating it.
gh api -X POST repos/$R/rulesets --input - <<'JSON'
{
  "name": "main",
  "target": "branch",
  "enforcement": "active",
  "conditions": { "ref_name": { "include": ["refs/heads/main"], "exclude": [] } },
  "rules": [
    { "type": "deletion" },
    { "type": "non_fast_forward" },
    { "type": "pull_request", "parameters": {
        "required_approving_review_count": 0, "dismiss_stale_reviews_on_push": false,
        "require_code_owner_review": false, "require_last_push_approval": false,
        "required_review_thread_resolution": false, "allowed_merge_methods": ["squash"] } },
    { "type": "required_status_checks", "parameters": {
        "strict_required_status_checks_policy": true, "do_not_enforce_on_create": true,
        "required_status_checks": [{ "context": "CI Success", "integration_id": 15368 }] } }
  ],
  "bypass_actors": []
}
JSON
# The release environment deploys from main only.
gh api -X PUT repos/$R/environments/release --input - <<'JSON'
{ "deployment_branch_policy": { "protected_branches": false, "custom_branch_policies": true } }
JSON
gh api -X POST repos/$R/environments/release/deployment-branch-policies -f name=main -f type=branch
# Private vulnerability reporting (SECURITY.md), Dependabot alerts, immutable releases.
gh api -X PUT repos/$R/private-vulnerability-reporting
gh api -X PUT repos/$R/vulnerability-alerts
gh api -X PUT repos/$R/immutable-releases
```

release-please opens its pull request with `GITHUB_TOKEN`, so the CI and PR Title runs on it wait
in `action_required` until approved; nothing else starts them:

```bash
gh run list --repo $R --branch release-please--branches--main--components--lockra-workspace \
  --json databaseId,status,conclusion --jq '.[] | select(.conclusion == "action_required") | .databaseId' |
  xargs -I{} gh api -X POST repos/$R/actions/runs/{}/approve
```

## Secrets

| Secret                                                                                        | Needed for                             | Notes                                                                                                                        |
| --------------------------------------------------------------------------------------------- | -------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| none                                                                                          | releases                               | `GITHUB_TOKEN` creates the tag, the draft and the assets                                                                     |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`                             | releases (the in-app update)           | the minisign key every package is signed with; `preflight` fails without it (below)                                          |
| `ANDROID_KEYSTORE_BASE64`, `ANDROID_KEYSTORE_PASSWORD`, `ANDROID_KEY_PASSWORD`                | releases (the Android app)             | the PKCS12 keystore (base64, one line) and its password, twice (PKCS12 keeps one); `preflight` fails without them            |
| `MACOS_CERTIFICATE`, `MACOS_CERTIFICATE_PASSWORD`, `MACOS_SIGNING_IDENTITY`                   | releases (the macOS app)               | the self-signed release certificate (`.p12`, base64, one line), its password, its SHA-1; see [macOS signing](#macos-signing) |
| `FIRLAB_DOCS_TOKEN`                                                                           | `publish-site.yml`                     | a fine-grained token for `sunerpy/firlab` only, Contents read and write; see [docs/site/README.md](site/README.md)           |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_API_*`                              | optional Developer ID and notarization | unset: the release certificate above signs, without notarization, and Gatekeeper warns once                                  |
| `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` with `bundle.windows.signCommand` | optional Windows signing               | without them SmartScreen warns                                                                                               |

`FIRLAB_DOCS_TOKEN` is the owner's fine-grained token `lockra-docs-sync` (Settings, Developer
settings, Fine-grained tokens): repository access `sunerpy/firlab` only, permission Contents read
and write. It expires on 2027-10-01. After that `publish-site.yml` fails at its push with an
authentication error and publishes nothing. To renew it, open the token, choose **Regenerate
token** with a new expiry (the old value stops working at once), and store the new value without
printing it:

```bash
gh secret set FIRLAB_DOCS_TOKEN --repo sunerpy/lockra   # paste the value at the prompt
```

The next run that has something to push proves the new token; a run with nothing new to publish
stops before it uses the token.

## In-app updates

From 0.2.0 every release carries what the app's updater reads (`docs/security.md`, "Updates"):

- `bundle.createUpdaterArtifacts: true` makes `tauri bundle` sign every package with
  `TAURI_SIGNING_PRIVATE_KEY`; the signature's trusted comment records the version
  (`file:Lockra_0.2.0_amd64.deb	version:0.2.0`), which `plugins.updater.requireSignedVersion`
  makes the app check. Do not turn that off: it is what refuses an older package offered as new.
- `latest.json` names the version, the release notes (the Release's body) and one entry per
  platform key, each a URL pinned to this release's asset and that asset's signature: an
  `{os}-{arch}-{installer}` key for every signed package (`linux-x86_64-deb`, `linux-x86_64-rpm`,
  `linux-x86_64-appimage`, `windows-x86_64-nsis`, `windows-x86_64-msi`, `darwin-aarch64-app`, …)
  and an `{os}-{arch}` key for the updater bundle of each target. The app asks for the key of the
  way it was installed first, so a copy installed from the `.deb` updates through a `.deb`.
- The app asks `https://github.com/sunerpy/lockra/releases/latest/download/latest.json`
  (`plugins.updater.endpoints`): only the release marked latest reaches installed copies, so a
  prerelease never does.

**The key.** It was generated with `pnpm tauri signer generate` (password protected) on
2026-10-01; its public half is `plugins.updater.pubkey` in `tauri.conf.json`, the private key and
its password are the two secrets above, and the maintainer keeps an offline copy of both.
Installed copies trust only this key: **losing it ends updates** (each copy would need a reinstall of a build with a new
public key), and replacing it takes one release signed with the old key that ships the new public
key. Keep the offline copy outside GitHub.

Local packages (`make linux-x64`, `make windows-x64`, `make pre-ci`) are built without the key and
therefore unsigned: the scripts pass `--config '{"bundle":{"createUpdaterArtifacts":false}}'` when
`TAURI_SIGNING_PRIVATE_KEY` is not set. A packaged build checks for updates against the real
manifest; `apps/desktop/src-tauri/tests/update.rs` checks the updater against a local manifest
instead, and `docs/acceptance/updates.md` is the update checked by hand on each system.

## Android

From 0.7.0 every release carries the phone app for arm64: `Lockra_X.Y.Z_android_arm64.apk` to
install from the releases page and `Lockra_X.Y.Z_android_arm64.aab` for Google Play, both in
`SHA256SUMS` and the attestations, neither in `latest.json` (the phone has no in-app updater: a
newer APK signed with the same key installs over the old one and keeps its vault).

`bundle-android` builds both unsigned with the lockfile's Tauri CLI, as CI's `android` job does,
so the key is not there while the dependencies' build code runs. The next step decodes the
keystore into the job's temporary directory, signs (`.github/scripts/sign-android-package.sh`:
`zipalign -P 16`, then apksigner with schemes v2 and v3 for the APK, jarsigner for the AAB) and
removes it. `.github/scripts/check-android-package.sh` then requires one signer, the pinned
certificate on both packages, 16 KB alignment, and the release's package name, version and
version code. CI runs the same signing and checks with a key made for each run.

**The key.** Generated with `keytool` on 2026-10-03 on the maintainer's machine: RSA 4096,
SHA256withRSA, valid until 2126-09-09, alias `lockra`, `CN=Lockra, O=Lockra`, PKCS12. Its
certificate's SHA-256, `5ac2ccffe00d12e80d13dbfc23425cd3b89eec77cd5d21adda4fdac1cf4dcc28`, is
pinned in `.github/android-signing.json` with the alias (which is not a secret; CI checks the file
with `.github/scripts/android-signing.sh`). The keystore and its password are the three secrets
above, which GitHub never gives back, so the maintainer keeps both in a password manager and an
offline copy. Google Play keeps the same key: when Play App Signing is set up, choose to upload
an existing key from a Java keystore (encrypted with the PEPK tool Play Console offers) instead of
letting Google make one, so the app from Play and the APK from the releases page update each
other.

- **Lost:** no APK can update an installed Lockra any more; users uninstall and install again,
  and uninstalling deletes the vault (the app is kept out of Android's backups), so they need a
  Lockra backup first. On Play, request a reset of the upload key; Play keeps signing with its copy.
- **Leaked:** rotate with APK Signature Scheme v3 (`apksigner rotate`) and Play's app signing key
  upgrade, in a release that carries the lineage; to be designed when needed.
- **Checking a key:** `keytool -list -v -keystore lockra-release.jks -alias lockra` shows the
  SHA-256 above.

## The install scripts

`scripts/install.sh` (Linux, macOS) and `scripts/install.ps1` (Windows) install the latest release
from its assets, by the names the release workflow gives them, after checking `SHA256SUMS`; the
documentation prints them as `curl … | sh` and `irm … | iex` from `main`. Change them together
with any asset name. `scripts/test-install.sh` and `scripts/test-install.ps1` test them offline
(CI's repository gates, `make check`); `.github/workflows/install-scripts.yml` runs them against
the real latest release on Linux (apt, dnf, AppImage), macOS and Windows (x64 and ARM64) when they
change and every week.

## Checking a release

```bash
gh run view RUN_ID --json jobs --jq '.jobs[] | [.name, .conclusion] | @tsv'
gh release view vX.Y.Z --json isDraft,isLatest,assets --jq '{isDraft, isLatest, assets: [.assets[].name]}'
gh release download vX.Y.Z --pattern SHA256SUMS && sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify Lockra_X.Y.Z_x64-setup.exe --repo sunerpy/lockra \
  --signer-workflow sunerpy/lockra/.github/workflows/release.yml
gh release download vX.Y.Z --pattern latest.json --output - | jq '.version, (.platforms | keys)'
gh release download vX.Y.Z --pattern '*_android_arm64.apk' &&
  apksigner verify --print-certs Lockra_X.Y.Z_android_arm64.apk | grep 'SHA-256'   # the pinned digest
```

Every bundle leg must appear by its target name; a green run with a skipped leg is not a release.

## The documentation site

<https://firlab.app/lockra/> goes live when `sunerpy/firlab` merges the change that adds its
`lockra/` sub-site. The pages describe the released app and link to the releases page, so that
merge comes after the first release is public. Then add `FIRLAB_DOCS_TOKEN` (above): from there
on, `publish-site.yml` updates the site after every merge that touches `docs/site/` or a design
document, and `docs-site.yml` builds it on pull requests (docs/site/README.md). Before that merge
both workflows fail without publishing anything.

## macOS signing

The macOS packages are signed with the project's own self-signed code signing certificate,
"Lockra Code Signing", the same for every release, so each build can hand its keychain items to
the next instead of making macOS ask again after an update (docs/security.md, "Keychain items
across updates"; the method Voltip uses). It is not a Developer ID: Gatekeeper still asks once
before the first start of a hand-installed copy.

**The certificate.** Made with OpenSSL on 2026-10-04 on the maintainer's machine: RSA 2048,
SHA-256, valid until 2126-10-05, key usage digital signature, extended key usage code signing,
exported as PKCS12 with `-certpbe PBE-SHA1-3DES -keypbe PBE-SHA1-3DES -macalg sha1` (macOS's
`security import` does not read OpenSSL 3's default algorithms). Its SHA-1,
`D660014E62B11FA7743E582E5FD52C110DA66855`, is pinned in `.github/macos-signing.json`, and the app
compiles in the same requirement (`code_identity::RELEASE_REQUIREMENT`; a test keeps the two
equal). The `.p12` and its password are two of the secrets above, which GitHub never gives back:
the maintainer keeps both in a password manager and an offline copy.

**In the release.** After phase 1 (no signing material while dependency code runs),
`.github/scripts/macos-signing.sh import` puts the certificate in a keychain of the job and trusts
it for code signing; Tauri signs with `APPLE_SIGNING_IDENTITY` (the SHA-1); `check` then requires
that the app and the app inside the updater's `.app.tar.gz` carry exactly the pinned designated
requirement; `remove` deletes the job's keychain. Local and CI builds stay ad hoc, and an ad hoc
build keeps the keyring store and hands nothing over.

**In CI.** `macos-check` runs `.github/scripts/check-keychain-handoff.sh` on the runner's login
keychain (a keychain made with `security create-keychain` skips the partition check): three builds
of `examples/keychain_harness.rs` with different code, signed with certificates made for the run,
show that a build cannot read another build's item without the dialog, that the hand-over before
installation lets the new build read its own item without it, and that a build signed otherwise
is handed nothing and gives nothing.

- **Lost:** sign the next release with a new certificate (below); every Mac user allows the
  keychain once more after that update.
- **Leaked or rotated:** make a new certificate the same way, update the three secrets,
  `.github/macos-signing.json` and `RELEASE_REQUIREMENT` in one pull request, and say in the release
  notes that this update asks for the keychain once. The old build refuses to hand over to the new
  one (another requirement), so the new build reads the old item once, with the dialog.

## Manual checks on real devices

What the automated gates cannot cover, before announcing a release:

1. **macOS**: open the app, create a vault, turn on _Remember on this device_ (the Keychain prompts
   once), lock, unlock with it; the traffic lights sit in the title strip.
2. **Windows**: install the NSIS package, repeat the steps with Credential Manager, copy a code and
   check that _Win+V_ clipboard history does not show it; reveal a secret and check that a
   screenshot shows the window black.
3. **Google Authenticator**: export two accounts from Lockra (one page), scan them with _Transfer
   accounts → Import accounts_, compare the codes; export eleven or more (two pages) and import both.
4. **Microsoft Authenticator**: scan one exported code with _+ → Other account_, compare the code.
5. **PhoneFactor**: import the two files from a rooted phone; personal (8-digit) and third-party
   accounts arrive, work accounts are listed as unsupported.
6. **Linux keychain**: on GNOME (Secret Service) and KDE, turn on _Remember on this device_,
   restart, unlock with it.
7. **Touch ID** (a Mac with Touch ID): locked with nothing set up, the unlock screen points to
   _Settings › Security_; there, with _Remember on this device_ off, turn on _Unlock with Touch ID_
   (the prompt comes once, and _Remember on this device_ turns on with it); lock; _Unlock with Touch
   ID_ opens the vault after the fingerprint, a cancelled prompt leaves it locked without a message,
   the master password still unlocks; turning the switch off asks for the master password and leaves
   _Remember on this device_ on. With _Default unlock_ on _Touch ID_ (the default): quit and start
   Lockra, and the prompt comes by itself; `Ctrl L` brings no prompt until you switch to another app
   and back; a cancelled prompt stays away until the same; an auto-lock (one minute) with Lockra in
   front prompts at once, and with Lockra behind another app only when you switch back. With
   _Master password_, nothing comes by itself.
8. **Windows Hello** (a PC with Windows Hello): the same steps; the Hello dialog comes up in front of
   Lockra, and its PIN is accepted as well as a fingerprint or the face.
9. **Keychain across an update** (a Mac with _Remember on this device_ or Touch ID on): updating
   from 0.7 or earlier to the first release with the hand-over asks once for the keychain item (the
   old build has no hand-over to give); from there on, _Update now_ and the restart that follows
   open Lockra without the system's keychain dialog, and Touch ID still unlocks. A dmg installed by
   hand asks once.
10. **Android** (an arm64 phone): install the release's APK and go through
    `docs/acceptance/android.md`; an APK from a pull request (signed with that run's key) has to be
    uninstalled first.
