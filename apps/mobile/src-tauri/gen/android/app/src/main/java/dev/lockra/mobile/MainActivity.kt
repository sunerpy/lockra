package dev.lockra.mobile

import android.os.Build
import android.os.Bundle
import android.system.Os
import android.util.Log
import android.view.View
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/**
 * The activity `tauri android init` writes, with four additions: keep them when regenerating the
 * project.
 */
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Who installed this copy, for the in-app update (src/updater.rs): Google Play updates what it
    // installed. Named before super.onCreate, which starts the Rust side that reads it once; set
    // once a process, as a recreated activity must not write the environment Rust is reading.
    if (Os.getenv(INSTALLER_ENV) == null) {
      val installer = installer()
      Log.i(TAG, "installer: ${installer ?: "none"}")
      Os.setenv(INSTALLER_ENV, installer ?: "", true)
    }
    // Codes and secrets never reach a screenshot, a screen recording or the recent apps' preview.
    window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    // The web app pads itself with env(safe-area-inset-*) (src/screens).
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Edge to edge, the window keeps its size when the keyboard opens and would cover the page:
    // the content gives way to the keyboard instead, so the field being typed in and what follows
    // it can scroll above it (android:windowSoftInputMode="adjustResize" in the manifest).
    ViewCompat.setOnApplyWindowInsetsListener(findViewById<View>(android.R.id.content)) { content, insets ->
      content.setPadding(0, 0, 0, insets.getInsets(WindowInsetsCompat.Type.ime()).bottom)
      insets
    }
  }

  /** The package the system says installed this app (`com.android.vending`: Google Play); none for adb or a system image. */
  private fun installer(): String? = try {
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
      packageManager.getInstallSourceInfo(packageName).installingPackageName
    } else {
      @Suppress("DEPRECATION")
      packageManager.getInstallerPackageName(packageName)
    }
  } catch (e: Exception) {
    null
  }

  private companion object {
    const val INSTALLER_ENV = "LOCKRA_INSTALLER"
    const val TAG = "Lockra"
  }
}
