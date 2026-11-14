package com.dvda.maker

/**
 * Thin JNI bridge to libdvda_core.so, which is the real dvda-core application
 * entrypoint (AppOptions / run_prepare / run_build / run_verify).
 */
object NativeEngine {
    init {
        System.loadLibrary("dvda_core")
    }

    /** action is "prepare", "build" or "verify"; returns an outcome JSON. */
    external fun runProfile(profileJson: String, action: String): String

    external fun cancel()

    external fun progressDone(): Int

    external fun progressTotal(): Int

    external fun progressText(): String
}
