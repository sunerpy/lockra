package dev.lockra.mobile

import android.os.Bundle
import android.view.WindowManager
import androidx.activity.enableEdgeToEdge

/**
 * The activity `tauri android init` writes, with two additions: keep them when regenerating the
 * project.
 */
class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    // Codes and secrets never reach a screenshot, a screen recording or the recent apps' preview.
    window.setFlags(WindowManager.LayoutParams.FLAG_SECURE, WindowManager.LayoutParams.FLAG_SECURE)
    // The web app pads itself with env(safe-area-inset-*) (src/screens).
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }
}
