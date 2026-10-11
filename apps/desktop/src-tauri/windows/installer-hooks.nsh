; Lockra's hooks into Tauri's NSIS installer (tauri.conf.json, bundle.windows.nsis.installerHooks;
; the macros Tauri's installer.nsi inserts when they are defined).
;
; An in-app update starts the installer with /UPDATE and /R (start Lockra again when done). The
; releases after 0.7.5 also pass /S, so the installer shows nothing (updater.rs,
; windows_install_mode). Lockra 0.7.5 and earlier pass /P instead, which shows the installer's
; progress page between the update dialog and the new version. The installer they download is the
; new version's, so it hides that page itself as soon as it starts installing. Installing by hand,
; passive or not, is unchanged.
!macro NSIS_HOOK_PREINSTALL
  ${If} $UpdateMode = 1
  ${AndIf} $PassiveMode = 1
    HideWindow
  ${EndIf}
!macroend
