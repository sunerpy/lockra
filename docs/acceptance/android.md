# Android app: acceptance

The phone app (`apps/mobile`) is not released yet. What CI proves on every pull request, and what
only a real phone can.

## What CI runs

- `android`: the release APK and AAB for arm64, built unsigned, signed with a key made for the run,
  and checked by `.github/scripts/check-android-package.sh` (one signer, the AAB's certificate,
  16 KB pages, every native library's LOAD segments, package name, version name and code, target
  SDK).
- `android-device`: that APK on an Android 15 emulator (x86_64 running the arm64 code through its
  ARM translation, no camera), driven over adb by `.github/scripts/android-device-smoke.sh`: install
  and start; create a vault with the password typed twice, the page giving way to the keyboard;
  leave the app (it locks) and come back; unlock; add an account by hand and copy its code (the
  clipboard plugin through R8); open the camera's page (without a camera the import says so, and the
  vault stays open behind that page); stay up with nothing fatal in the app's log. The log, the
  crash buffer, the exit reason and the UI tree upload either way.

## On a phone

Install the APK a pull request's `android` job uploaded (artifact `lockra-android-apk`, three days;
signed with that run's throwaway key, so uninstall it before a build signed otherwise):

```bash
gh run download <run id> -n lockra-android-apk -D lockra-apk
adb install -r lockra-apk/Lockra_*_android_arm64.apk
```

| #   | Check                                                                                                                                                                        |
| --- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | A vault is created and opened again with the master password; wrong ones are counted, and after three in a row each try waits longer (1 s, doubling to 30 s).                |
| 2   | Leaving the app (home, another app, the screen off) locks the vault at once; the recent apps show a blank card, and a screenshot of a code is black.                         |
| 3   | Scan a QR code with the camera: the permission is asked only now; a Google Authenticator export of several codes fills one preview, the "Scan the next code" button between. |
| 4   | Leaving the app from the camera's page locks the vault; cancelling it returns to the Add page, unlocked.                                                                     |
| 5   | Read screenshots of QR codes from the photo picker; import an otpauth list, a Lockra backup (its password asked in the preview) and Microsoft's database from files.         |
| 6   | A copied code pastes into another app; the keyboard's clipboard history does not show it; after the configured time it is gone (if nothing else was copied meanwhile).       |
| 7   | Turn on Settings › Security › Unlock with fingerprint (one check), lock, unlock with the fingerprint; a new fingerprint enrolled afterwards makes it ask for the password.   |
| 8   | Turn it off (master password): the fingerprint is no longer offered on the lock screen.                                                                                      |
| 9   | Save a backup where the phone's file picker says, then restore it on a new install (Welcome › Restore a backup): the same accounts.                                          |
| 10  | Export: Google Authenticator's migration codes scanned by another phone match the codes shown beside them; a plain otpauth list saves only after its plain text is ticked.   |
| 11  | The back gesture closes one page at a time and leaves the app from the codes; the keyboard never hides the field being typed in.                                             |
| 12  | Android's backup: in Settings › System › Backup, Lockra has no data to back up.                                                                                              |
