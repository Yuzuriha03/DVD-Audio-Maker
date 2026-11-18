package com.dvda.maker

import android.app.Application
import android.util.Log

class DvdaApplication : Application() {
    override fun attachBaseContext(base: android.content.Context) {
        super.attachBaseContext(base)
        Log.i(TAG, "application attachBaseContext")
    }

    override fun onCreate() {
        super.onCreate()
        Log.i(TAG, "application onCreate")
    }

    companion object {
        private const val TAG = "DvdaMaker"
    }
}
