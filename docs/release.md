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
     any secret, then `tauri bundle` (deb, rpm and AppImage on Linux x64 and arm64; app and dmg for
     Apple silicon and Intel; NSIS and MSI on Windows x64, NSIS on Windows arm64);
   - `updater` collects every leg, writes `SHA256SUMS`, attests the files (SLSA build provenance)
     and attaches them to the draft. Lockra has no in-app updater (`bundle.createUpdaterArtifacts`
     is not set), so there is no `latest.json`, and macOS ships its dmg alone: without the updater
     Tauri does not archive the `.app`, which the dmg carries. `tauri-release.py collect` carries
     that one change from the scaffold's template (`drift_allow` in `.github/scaffold.json`), and
     `.github/scripts/test-tauri-release.py` checks every leg against the manifest;
   - `publish-release` re-verifies every remote asset against `SHA256SUMS` and flips the draft to
     public, marking it latest unless it is a prerelease.
4. A failed leg leaves the draft unpublished. Rebuild it after a fix of the workflow or an outage
   with **Run workflow** (`workflow_dispatch`, `tag_name: vX.Y.Z`); a fix of the source needs a new
   version.

Versions: `package.json` is the only source (`tauri.conf.json` points at it, `Cargo.toml` stays at
`0.0.0`). `.release-please-manifest.json` holds the last released version, `0.0.0` until the first
release. The first release is `0.1.0` (`initial-version` in `release-please-config.json`): with no
earlier release tag, release-please would otherwise start at `1.0.0`.

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

| Secret                                                                                        | Needed for                              | Notes                                                                                                              |
| --------------------------------------------------------------------------------------------- | --------------------------------------- | ------------------------------------------------------------------------------------------------------------------ |
| none                                                                                          | releases                                | `GITHUB_TOKEN` creates the tag, the draft and the assets                                                           |
| `FIRLAB_DOCS_TOKEN`                                                                           | `publish-site.yml`                      | a fine-grained token for `sunerpy/firlab` only, Contents read and write; see [docs/site/README.md](site/README.md) |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_API_*`    | optional macOS signing and notarization | without them the app is unsigned and Gatekeeper warns                                                              |
| `AZURE_TENANT_ID`, `AZURE_CLIENT_ID`, `AZURE_CLIENT_SECRET` with `bundle.windows.signCommand` | optional Windows signing                | without them SmartScreen warns                                                                                     |

## Checking a release

```bash
gh run view RUN_ID --json jobs --jq '.jobs[] | [.name, .conclusion] | @tsv'
gh release view vX.Y.Z --json isDraft,isLatest,assets --jq '{isDraft, isLatest, assets: [.assets[].name]}'
gh release download vX.Y.Z --pattern SHA256SUMS && sha256sum -c SHA256SUMS --ignore-missing
gh attestation verify Lockra_X.Y.Z_x64-setup.exe --repo sunerpy/lockra \
  --signer-workflow sunerpy/lockra/.github/workflows/release.yml
```

Every bundle leg must appear by its target name; a green run with a skipped leg is not a release.

## The documentation site

<https://firlab.app/lockra/> goes live when `sunerpy/firlab` merges the change that adds its
`lockra/` sub-site. The pages describe the released app and link to the releases page, so that
merge comes after the first release is public. Then add `FIRLAB_DOCS_TOKEN` (above): from there
on, `publish-site.yml` updates the site after every merge that touches `docs/site/` or a design
document, and `docs-site.yml` builds it on pull requests (docs/site/README.md). Before that merge
both workflows fail without publishing anything.

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
