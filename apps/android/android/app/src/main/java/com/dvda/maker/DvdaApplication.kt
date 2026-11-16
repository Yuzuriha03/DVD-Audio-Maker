package com.dvda.maker

import android.app.Application
import android.util.Log
import java.io.File

class DvdaApplication : Application() {
    override fun attachBaseContext(base: android.content.Context) {
        super.attachBaseContext(base)
        Log.i(TAG, "application attachBaseContext")
    }

    override fun onCreate() {
        super.onCreate()
        installCrashHandler()
        Log.i(TAG, "application onCreate")
    }

    private fun installCrashHandler() {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            // Diagnostics stay in app-private storage. Nothing is written to the
            // user's public Download folder from the application object.
            val trace = Log.getStackTraceString(error)
            runCatching { File(filesDir, "crash.log").writeText(trace) }
            if (previous != null) {
                previous.uncaughtException(thread, error)
            } else {
                android.os.Process.killProcess(android.os.Process.myPid())
            }
        }
    }

    companion object {
        private const val TAG = "DvdaMaker"
    }
}
