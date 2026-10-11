package dev.lockra.mobile

import android.os.Bundle
import android.view.View
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

/**
 * The activity `tauri android init` writes, with three additions: keep them when regenerating the
 * project.
 */
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
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
}
