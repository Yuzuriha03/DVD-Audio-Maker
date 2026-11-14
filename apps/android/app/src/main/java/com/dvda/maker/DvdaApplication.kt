
package com.dvda.maker

import android.app.Application
import android.content.ContentValues
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import android.util.Log
import java.io.File

class DvdaApplication : Application() {
    override fun attachBaseContext(base: android.content.Context) {
        super.attachBaseContext(base)
        bootLog("application attachBaseContext")
    }

    override fun onCreate() {
        super.onCreate()
        installCrashHandler()
        bootLog("application onCreate")
    }

    private fun bootLog(message: String) {
        Log.i(TAG, message)
        val line = System.currentTimeMillis().toString() + " " + message + "\n"
        runCatching { File(filesDir, "boot.log").appendText(line) }
        writePublic("dvda-app-boot.log", line)
    }

    private fun writePublic(name: String, text: String) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            runCatching {
                val values = ContentValues().apply {
                    put(MediaStore.Downloads.DISPLAY_NAME, name)
                    put(MediaStore.Downloads.MIME_TYPE, "text/plain")
                    put(
                        MediaStore.Downloads.RELATIVE_PATH,
                        Environment.DIRECTORY_DOWNLOADS + "/DSH-Go",
                    )
                }
                val uri = contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
                uri?.let {
                    contentResolver.openOutputStream(it)?.use { stream ->
                        stream.write(text.toByteArray())
                    }
                }
            }
        }
        runCatching {
            val dir = File(
                Environment.getExternalStoragePublicDirectory(Environment.DIRECTORY_DOWNLOADS),
                "DSH-Go",
            )
            if (dir.isDirectory || dir.mkdirs()) {
                File(dir, name).writeText(text)
            }
        }
    }

    private fun installCrashHandler() {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            val trace = Log.getStackTraceString(error)
            runCatching { File(filesDir, "crash.log").writeText(trace) }
            writePublic("dvda-crash.log", trace)
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
