
package com.dvda.maker

import android.app.Activity
import android.app.AlertDialog
import android.content.ContentValues
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.Handler
import android.os.Looper
import android.provider.DocumentsContract
import android.provider.MediaStore
import android.provider.Settings
import android.text.InputType
import android.util.Log
import android.view.View
import android.view.WindowManager
import android.widget.AdapterView
import android.widget.ArrayAdapter
import android.widget.Button
import android.widget.CheckBox
import android.widget.EditText
import android.widget.LinearLayout
import android.widget.ProgressBar
import android.widget.ScrollView
import android.widget.Spinner
import android.widget.TextView
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.util.Locale

/**
 * Classic-view activity: no Compose/AndroidX runtime is needed, which keeps the
 * hand-built APK's dependency surface to the framework, Kotlin stdlib and the
 * native core only. Every control maps 1:1 to the desktop profile keys.
 */
class MainActivity : Activity() {
    private val handler = Handler(Looper.getMainLooper())
    private lateinit var prefs: android.content.SharedPreferences
    private lateinit var logView: TextView
    private lateinit var statusView: TextView
    private lateinit var progressBar: ProgressBar
    private lateinit var bootLog: StringBuilder

    private lateinit var sourceField: EditText
    private lateinit var finalField: EditText
    private lateinit var titleField: EditText
    private lateinit var capacitySpinner: Spinner
    private lateinit var customBytesField: EditText
    private lateinit var advancedCheck: CheckBox
    private lateinit var workField: EditText
    private lateinit var isoPrefixField: EditText
    private lateinit var groupLimitField: EditText
    private lateinit var keepTmpCheck: CheckBox
    private lateinit var titleModeField: EditText
    private lateinit var albumLimitField: EditText
    private lateinit var modeSpinner: Spinner
    private lateinit var rateSpinner: Spinner
    private lateinit var bitsSpinner: Spinner
    private lateinit var ditherCheck: CheckBox
    private lateinit var importField: EditText
    private lateinit var menuCheck: CheckBox
    private lateinit var stillsCheck: CheckBox
    private lateinit var tracksField: EditText
    private lateinit var coverField: EditText
    private lateinit var normSpinner: Spinner
    private lateinit var fontScField: EditText
    private lateinit var fontJpField: EditText
    private lateinit var fontKrField: EditText
    private lateinit var profileField: EditText
    private lateinit var languageSpinner: Spinner
    private lateinit var advancedSection: LinearLayout
    private lateinit var menuSection: LinearLayout
    private lateinit var importRow: LinearLayout

    private var running = false
    private var lastAction = ""

    private val rates = listOf("44100", "48000", "88200", "96000", "176400", "192000")
    private val rateLabels = listOf("44.1 kHz", "48 kHz", "88.2 kHz", "96 kHz", "176.4 kHz", "192 kHz")
    private val bits = listOf("16", "20", "24")
    private val bitLabels = listOf("16 bit", "20 bit", "24 bit")
    private val modes = listOf("mlp-batch", "lpcm", "external")
    private val modeLabels = listOf("MLP", "LPCM", "Import MLP")
    private val norms = listOf("pal", "secam", "ntsc")
    private val normLabels = listOf("PAL", "SECAM", "NTSC")
    private val languageCodes = listOf("", "zh-TW", "en", "ja", "ko")
    private val languageLabels = listOf("简体中文", "繁體中文", "English", "日本語", "한국어")

    override fun attachBaseContext(newBase: Context) {
        val saved = newBase.getSharedPreferences(PREFS, MODE_PRIVATE).getString(KEY_LANGUAGE, "")
        if (saved.isNullOrBlank()) {
            super.attachBaseContext(newBase)
            return
        }
        val config = Configuration(newBase.resources.configuration)
        config.setLocale(Locale.forLanguageTag(saved))
        super.attachBaseContext(newBase.createConfigurationContext(config))
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        bootLog = StringBuilder()
        installCrashHandler()
        bootLog("onCreate start")
        try {
            super.onCreate(savedInstanceState)
            prefs = getSharedPreferences(PREFS, MODE_PRIVATE)
            configureHighRefresh()
            ensureStorageAccess()
            setContentView(buildUi())
            loadSettings()
            bootLog("ui ready")
        } catch (error: Throwable) {
            bootLog("onCreate failed: " + Log.getStackTraceString(error))
            showFatal(error)
        }
        writeBootLog()
    }

    // ---------------------------------------------------------------- logging
    private fun writeBootLog() {
        val text = bootLog.toString()
        runCatching { File(filesDir, "boot.log").writeText(text) }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            runCatching {
                val values = ContentValues().apply {
                    put(MediaStore.Downloads.DISPLAY_NAME, "dvda-app-boot.log")
                    put(MediaStore.Downloads.MIME_TYPE, "text/plain")
                    put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/DSH-Go")
                }
                contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)?.let { uri ->
                    contentResolver.openOutputStream(uri)?.use { it.write(text.toByteArray()) }
                }
            }
        }
    }

    private fun bootLog(message: String) {
        Log.i(TAG, message)
        bootLog.append(System.currentTimeMillis()).append(' ').append(message).append('\n')
        if (isFinishing) return
        runCatching {
            if (::statusView.isInitialized) statusView.text = message
        }
    }

    private fun installCrashHandler() {
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            val trace = Log.getStackTraceString(error)
            bootLog.append("uncaught: ").append(trace).append('\n')
            writeBootLog()
            runCatching { File(filesDir, "crash.log").writeText(trace) }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                runCatching {
                    val values = ContentValues().apply {
                        put(MediaStore.Downloads.DISPLAY_NAME, "dvda-crash.log")
                        put(MediaStore.Downloads.MIME_TYPE, "text/plain")
                        put(MediaStore.Downloads.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/DSH-Go")
                    }
                    contentResolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)?.let { uri ->
                        contentResolver.openOutputStream(uri)?.use { it.write(trace.toByteArray()) }
                    }
                }
            }
            if (previous != null) previous.uncaughtException(thread, error)
        }
    }

    private fun showFatal(error: Throwable) {
        val layout = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL; setPadding(32, 96, 32, 32) }
        layout.addView(TextView(this).apply {
            text = "启动失败：\n" + Log.getStackTraceString(error).take(4000)
            textSize = 12f
        })
        runCatching { setContentView(layout) }
    }

    // ------------------------------------------------------------- refresh rate
    private fun configureHighRefresh() {
        val display = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) display else {
            @Suppress("DEPRECATION")
            windowManager.defaultDisplay
        }
        val mode = display?.supportedModes?.maxByOrNull { it.refreshRate } ?: return
        window.attributes = window.attributes.apply {
            preferredDisplayModeId = mode.modeId
            preferredRefreshRate = mode.refreshRate
        }
    }

    // --------------------------------------------------------------- permissions
    private fun ensureStorageAccess() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && !Environment.isExternalStorageManager()) {
            runCatching {
                startActivity(
                    Intent(Settings.ACTION_MANAGE_APP_ALL_FILES_ACCESS_PERMISSION)
                        .setData(Uri.parse("package:$packageName")),
                )
            }
        }
        val permission = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            android.Manifest.permission.READ_MEDIA_AUDIO
        } else {
            android.Manifest.permission.READ_EXTERNAL_STORAGE
        }
        if (checkSelfPermission(permission) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(permission), 1)
        }
    }

    // --------------------------------------------------------------------- UI
    private fun buildUi(): View {
        val root = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            setPadding(24, 24, 24, 24)
        }
        root.addView(TextView(this).apply {
            text = "DVD-Audio Maker"
            textSize = 20f
        })
        statusView = TextView(this).apply { text = "就绪"; textSize = 12f }
        root.addView(statusView)
        progressBar = ProgressBar(this, null, android.R.attr.progressBarStyleHorizontal).apply {
            max = 1000
            visibility = View.GONE
        }
        root.addView(progressBar)

        val profileRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        profileField = EditText(this).apply { hint = "方案路径" }
        profileRow.addView(profileField, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        profileRow.addView(button("打开方案") { pickProfile() })
        profileRow.addView(button("保存方案") { saveProfile() })
        root.addView(profileRow)

        val languageRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        languageRow.addView(TextView(this).apply { text = "语言" })
        languageSpinner = spinner(languageLabels)
        languageRow.addView(languageSpinner)
        root.addView(languageRow)
        languageSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                val code = languageCodes[position]
                prefs.edit().putString(KEY_LANGUAGE, code).apply()
                recreate()
            }
            override fun onNothingSelected(parent: AdapterView<*>?) {}
        }

        root.addView(section("基本设置"))
        sourceField = browseRow(root, "音源路径") { pickDirectory { sourceField.setText(it) } }
        finalField = browseRow(root, "成品保存路径") { pickDirectory { finalField.setText(it) } }
        titleField = fieldRow(root, "光盘名称", InputType.TYPE_CLASS_TEXT)
        capacitySpinner = spinner(listOf("DVD5 (4.7 GB)", "DVD9 (8.5 GB)", "自定义容量"))
        labelledRow(root, "光盘容量", capacitySpinner)
        customBytesField = fieldRow(root, "自定义容量 (字节)", InputType.TYPE_CLASS_NUMBER)
        advancedCheck = CheckBox(this).apply { text = "显示高级设置" }
        root.addView(advancedCheck)
        advancedSection = LinearLayout(this).apply {
            orientation = LinearLayout.VERTICAL
            visibility = View.GONE
        }
        workField = browseRowIn(advancedSection, "临时工作路径") { pickDirectory { workField.setText(it) } }
        isoPrefixField = fieldRowIn(advancedSection, "镜像文件名前缀", InputType.TYPE_CLASS_TEXT)
        groupLimitField = fieldRowIn(advancedSection, "每组最多曲目", InputType.TYPE_CLASS_NUMBER)
        titleModeField = fieldRowIn(advancedSection, "标题分组（album / one / 每组曲数）", InputType.TYPE_CLASS_TEXT)
        albumLimitField = fieldRowIn(advancedSection, "限制专辑数量（0 表示不限）", InputType.TYPE_CLASS_NUMBER)
        keepTmpCheck = CheckBox(this).apply { text = "保留临时/中间文件" }
        advancedSection.addView(keepTmpCheck)
        root.addView(advancedSection)
        advancedCheck.setOnCheckedChangeListener { _, checked ->
            advancedSection.visibility = if (checked) View.VISIBLE else View.GONE
        }

        root.addView(section("音频编码"))
        modeSpinner = spinner(modeLabels)
        labelledRow(root, "音频编码方式", modeSpinner)
        rateSpinner = spinner(rateLabels)
        labelledRow(root, "采样率", rateSpinner)
        bitsSpinner = spinner(bitLabels)
        labelledRow(root, "音频位深", bitsSpinner)
        ditherCheck = CheckBox(this).apply { text = "16-bit 输出抖动（推荐）" }
        root.addView(ditherCheck)
        importRow = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL; visibility = View.GONE }
        importField = browseRowIn(importRow, "MLP 文件路径") { pickDirectory { importField.setText(it) } }
        root.addView(importRow)
        modeSpinner.onItemSelectedListener = object : AdapterView.OnItemSelectedListener {
            override fun onItemSelected(parent: AdapterView<*>?, view: View?, position: Int, id: Long) {
                importRow.visibility = if (modes[position] == "external") View.VISIBLE else View.GONE
            }
            override fun onNothingSelected(parent: AdapterView<*>?) {}
        }

        root.addView(section("光盘菜单"))
        menuCheck = CheckBox(this).apply { text = "制作选曲菜单" }
        stillsCheck = CheckBox(this).apply { text = "在菜单中显示专辑封面" }
        root.addView(menuCheck)
        root.addView(stillsCheck)
        tracksField = fieldRow(root, "每页显示几首", InputType.TYPE_CLASS_NUMBER)
        coverField = fieldRow(root, "封面暗化程度 (%)", InputType.TYPE_CLASS_NUMBER)
        normSpinner = spinner(normLabels)
        labelledRow(root, "视频制式", normSpinner)
        fontScField = fieldRow(root, "中文菜单字体", InputType.TYPE_CLASS_TEXT)
        fontJpField = fieldRow(root, "日文菜单字体", InputType.TYPE_CLASS_TEXT)
        fontKrField = fieldRow(root, "韩文菜单字体", InputType.TYPE_CLASS_TEXT)

        val buttons = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        buttons.addView(button("检查音源") { launchAction("prepare") })
        buttons.addView(button("开始制作") { startBuild() })
        buttons.addView(button("停止任务") { NativeEngine.cancel() })
        root.addView(buttons)
        val buttons2 = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        buttons2.addView(button("验证成品") { launchAction("verify") })
        buttons2.addView(button("查看成品") { openOutput() })
        root.addView(buttons2)

        logView = TextView(this).apply { textSize = 11f }
        root.addView(logView)
        return ScrollView(this).apply { addView(root) }
    }

    private fun section(title: String): TextView = TextView(this).apply {
        text = title
        textSize = 16f
        setPadding(0, 24, 0, 8)
    }

    private fun button(label: String, action: () -> Unit): Button = Button(this).apply {
        text = label
        setOnClickListener { action() }
    }

    private fun spinner(labels: List<String>): Spinner = Spinner(this).apply {
        adapter = ArrayAdapter(this@MainActivity, android.R.layout.simple_spinner_dropdown_item, labels)
    }

    private fun labelledRow(parent: LinearLayout, label: String, view: View) {
        val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        row.addView(TextView(this).apply { text = label })
        row.addView(view)
        parent.addView(row)
    }

    private fun fieldRow(parent: LinearLayout, label: String, inputType: Int): EditText =
        fieldRowIn(parent, label, inputType)

    private fun fieldRowIn(parent: LinearLayout, label: String, inputType: Int): EditText {
        val edit = EditText(this).apply { this.inputType = inputType; hint = label }
        parent.addView(edit)
        return edit
    }

    private fun browseRow(parent: LinearLayout, label: String, onBrowse: () -> Unit): EditText =
        browseRowIn(parent, label, onBrowse)

    private fun browseRowIn(parent: LinearLayout, label: String, onBrowse: () -> Unit): EditText {
        val row = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
        val edit = EditText(this).apply { hint = label; isFocusable = false }
        row.addView(edit, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(button("浏览...") { onBrowse() })
        parent.addView(row)
        return edit
    }

    // --------------------------------------------------------------- settings IO
    private fun loadSettings() {
        sourceField.setText(prefs.getString("source", "/sdcard/Music"))
        finalField.setText(prefs.getString("final", "/sdcard/Download/DSH-Go"))
        titleField.setText(prefs.getString("title", "DVD-Audio"))
        capacitySpinner.setSelection(prefs.getInt("capacity", 0))
        customBytesField.setText(prefs.getString("custom_bytes", "4707319808"))
        advancedCheck.isChecked = prefs.getBoolean("advanced", false)
        workField.setText(prefs.getString("work_dir", filesDir.absolutePath + "/build"))
        isoPrefixField.setText(prefs.getString("iso_prefix", ""))
        groupLimitField.setText(prefs.getString("group_limit", "99"))
        titleModeField.setText(prefs.getString("title_mode", "album"))
        albumLimitField.setText(prefs.getString("album_limit", "0"))
        keepTmpCheck.isChecked = prefs.getBoolean("keep_tmp", false)
        modeSpinner.setSelection(prefs.getInt("mode", 0))
        rateSpinner.setSelection(rates.indexOf(prefs.getString("rate", "48000")).coerceAtLeast(0))
        bitsSpinner.setSelection(bits.indexOf(prefs.getString("bits", "24")).coerceAtLeast(0))
        ditherCheck.isChecked = prefs.getBoolean("dither", true)
        importField.setText(prefs.getString("import_dir", ""))
        menuCheck.isChecked = prefs.getBoolean("menu", false)
        stillsCheck.isChecked = prefs.getBoolean("stills", true)
        tracksField.setText(prefs.getString("tracks", "12"))
        coverField.setText(prefs.getString("cover", "35"))
        normSpinner.setSelection(prefs.getInt("norm", 0))
        fontScField.setText(prefs.getString("font_sc", DEFAULT_FONT))
        fontJpField.setText(prefs.getString("font_jp", ""))
        fontKrField.setText(prefs.getString("font_kr", ""))
        val language = prefs.getString(KEY_LANGUAGE, "") ?: ""
        languageSpinner.setSelection(languageCodes.indexOf(language).coerceAtLeast(0))
        profileField.setText(prefs.getString("profile", ""))
    }

    private fun persist() {
        prefs.edit()
            .putString("source", sourceField.text.toString())
            .putString("final", finalField.text.toString())
            .putString("title", titleField.text.toString())
            .putInt("capacity", capacitySpinner.selectedItemPosition)
            .putString("custom_bytes", customBytesField.text.toString())
            .putBoolean("advanced", advancedCheck.isChecked)
            .putString("work_dir", workField.text.toString())
            .putString("iso_prefix", isoPrefixField.text.toString())
            .putString("group_limit", groupLimitField.text.toString())
            .putString("title_mode", titleModeField.text.toString())
            .putString("album_limit", albumLimitField.text.toString())
            .putBoolean("keep_tmp", keepTmpCheck.isChecked)
            .putInt("mode", modeSpinner.selectedItemPosition)
            .putString("rate", rates[rateSpinner.selectedItemPosition])
            .putString("bits", bits[bitsSpinner.selectedItemPosition])
            .putBoolean("dither", ditherCheck.isChecked)
            .putString("import_dir", importField.text.toString())
            .putBoolean("menu", menuCheck.isChecked)
            .putBoolean("stills", stillsCheck.isChecked)
            .putString("tracks", tracksField.text.toString())
            .putString("cover", coverField.text.toString())
            .putInt("norm", normSpinner.selectedItemPosition)
            .putString("font_sc", fontScField.text.toString())
            .putString("font_jp", fontJpField.text.toString())
            .putString("font_kr", fontKrField.text.toString())
            .apply()
    }

    private fun profileJson(): String {
        val discBytes = when (capacitySpinner.selectedItemPosition) {
            1 -> DVD9_BYTES
            2 -> customBytesField.text.toString().toLongOrNull() ?: DVD5_BYTES
            else -> DVD5_BYTES
        }
        val values = JSONObject()
        values.put("DVDA_SRC", sourceField.text.toString())
        values.put("DVDA_FINAL_DIR", finalField.text.toString())
        values.put("DVDA_TITLE", titleField.text.toString())
        values.put("DVDA_DISC_BYTES", discBytes.toString())
        values.put("DVDA_BUILD_DIR", workField.text.toString())
        values.put("DVDA_ISO_PREFIX", isoPrefixField.text.toString())
        values.put("DVDA_GROUP_TRACK_LIMIT", groupLimitField.text.toString())
        values.put("DVDA_MLP_SOURCE", modes[modeSpinner.selectedItemPosition])
        values.put("DVDA_MLP_SAMPLE_RATE", rates[rateSpinner.selectedItemPosition])
        values.put("DVDA_MLP_BITS", bits[bitsSpinner.selectedItemPosition])
        values.put("DVDA_DITHER_16", if (ditherCheck.isChecked) "on" else "off")
        values.put("DVDA_MLP_EXTERNAL_DIR", importField.text.toString())
        values.put("DVDA_MENU", if (menuCheck.isChecked) "on" else "off")
        values.put("DVDA_MENU_TRACKS_PER_PAGE", tracksField.text.toString())
        values.put("DVDA_MENU_STILLPICS", if (stillsCheck.isChecked) "on" else "off")
        values.put("DVDA_MENU_COVER_DIM", coverField.text.toString())
        values.put("DVDA_MENU_NORM", norms[normSpinner.selectedItemPosition])
        values.put("DVDA_MENU_FONT", fontScField.text.toString().ifBlank { if (menuCheck.isChecked) DEFAULT_FONT else "" })
        values.put("DVDA_MENU_FONT_JP", fontJpField.text.toString())
        values.put("DVDA_MENU_FONT_KR", fontKrField.text.toString())
        values.put("DVDA_KEEP_TMP", if (keepTmpCheck.isChecked) "on" else "off")
        values.put("DVDA_KEEP_INTERMEDIATE", if (keepTmpCheck.isChecked) "on" else "off")
        values.put("DVDA_TITLE_MODE", titleModeField.text.toString())
        values.put("DVDA_ALBUM_LIMIT", albumLimitField.text.toString())
        values.put("DVDA_MENU_INDEX_MIN_ALBUMS", "4")
        values.put("DVDA_RESUME", "on")
        val profile = JSONObject()
        profile.put("Version", 1)
        val code = languageCodes[languageSpinner.selectedItemPosition]
        profile.put("Language", if (code.isBlank()) "zh-CN" else code)
        profile.put("Values", values)
        return profile.toString()
    }

    // ---------------------------------------------------------------- actions
    private fun startBuild() {
        if (running) return
        persist()
        Thread {
            val result = NativeEngine.runProfile(profileJson(), "status")
            val unfinished = runCatching { JSONObject(result).optBoolean("Unfinished") }.getOrDefault(false)
            handler.post {
                if (unfinished) showRecoveryDialog() else launchAction("build")
            }
        }.start()
    }

    private fun launchAction(action: String) {
        if (running) return
        persist()
        running = true
        lastAction = action
        progressBar.visibility = View.VISIBLE
        progressBar.progress = 0
        logView.text = ""
        statusView.text = "运行中..."
        Thread {
            val result = NativeEngine.runProfile(profileJson(), action)
            handler.post { finishAction(action, result) }
        }.start()
    }

    private fun finishAction(action: String, result: String) {
        running = false
        progressBar.visibility = View.GONE
        val json = runCatching { JSONObject(result) }.getOrNull()
        val ok = json?.optBoolean("Ok") ?: false
        val message = json?.optString("Message").orEmpty()
        val diagnostics = json?.optJSONArray("Diagnostics")
        val report = StringBuilder()
        report.append(if (ok) "完成: " else "失败: ").append(message).append('\n')
        if (diagnostics != null) {
            for (index in 0 until diagnostics.length()) {
                val item = diagnostics.optJSONObject(index)
                report.append(item?.optString("Code")).append(": ").append(item?.optString("Message")).append('\n')
            }
        }
        logView.text = report.toString()
        statusView.text = if (ok) "完成" else "失败"
        if (!ok && action.startsWith("build")) {
            Thread {
                val status = NativeEngine.runProfile(profileJson(), "status")
                val unfinished = runCatching { JSONObject(status).optBoolean("Unfinished") }.getOrDefault(false)
                if (unfinished) handler.post { showRetentionDialog() }
            }.start()
        }
    }

    private fun showRecoveryDialog() {
        AlertDialog.Builder(this)
            .setTitle("发现未完成的制作")
            .setMessage("继续制作会校验并使用已保存的进度；重新制作会删除该任务的中间文件，从头开始。音源和已输出的 ISO 不会删除。")
            .setPositiveButton("继续制作") { _, _ -> launchAction("build_continue") }
            .setNegativeButton("重新制作") { _, _ -> launchAction("build_restart") }
            .show()
    }

    private fun showRetentionDialog() {
        AlertDialog.Builder(this)
            .setTitle("制作未完成")
            .setMessage("是否保留本次进度，以便下次继续制作？删除进度只会删除此任务的中间文件，不会删除音源或已输出的 ISO。")
            .setPositiveButton("保留进度") { _, _ -> }
            .setNegativeButton("删除进度") { _, _ ->
                Thread { NativeEngine.runProfile(profileJson(), "discard") }.start()
            }
            .show()
    }

    private fun openOutput() {
        val dir = finalField.text.toString()
        val opened = runCatching {
            val root = Environment.getExternalStorageDirectory().absolutePath
            val relative = dir.removePrefix(root).trimStart('/')
            val uri = DocumentsContract.buildTreeDocumentUri(
                "com.android.externalstorage.documents",
                "primary:" + relative,
            )
            startActivity(
                Intent(Intent.ACTION_VIEW)
                    .setDataAndType(uri, "vnd.android.document/directory")
                    .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
            )
        }.isSuccess
        if (!opened) {
            runCatching {
                startActivity(
                    Intent(Intent.ACTION_VIEW)
                        .setDataAndType(Uri.parse("file://" + dir), "resource/folder"),
                )
            }
        }
    }

    // --------------------------------------------------------------- pickers
    private var pendingDirTarget: ((String) -> Unit)? = null
    private var lastProfileText = ""

    private fun pickDirectory(onPicked: (String) -> Unit) {
        pendingDirTarget = onPicked
        startActivityForResult(Intent(Intent.ACTION_OPEN_DOCUMENT_TREE), REQUEST_DIR)
    }

    private fun pickProfile() {
        startActivityForResult(Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/json"
        }, REQUEST_OPEN_PROFILE)
    }

    private fun saveProfile() {
        persist()
        lastProfileText = profileJson()
        startActivityForResult(
            Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
                addCategory(Intent.CATEGORY_OPENABLE)
                type = "application/json"
                putExtra(Intent.EXTRA_TITLE, "dvda-profile.json")
            },
            REQUEST_SAVE_PROFILE,
        )
    }

    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        if (resultCode != RESULT_OK) return
        val uri = data?.data ?: return
        when (requestCode) {
            REQUEST_DIR -> {
                val path = treeUriToPath(uri) ?: return
                pendingDirTarget?.invoke(path)
                pendingDirTarget = null
            }
            REQUEST_SAVE_PROFILE -> runCatching {
                contentResolver.openOutputStream(uri)?.use { it.write(lastProfileText.toByteArray()) }
            }
            REQUEST_OPEN_PROFILE -> {
                val text = runCatching {
                    contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() }
                }.getOrNull() ?: return
                applyProfile(text)
            }
        }
    }

    private fun treeUriToPath(uri: Uri): String? = runCatching {
        val document = DocumentsContract.getTreeDocumentId(uri)
        val parts = document.split(":", limit = 2)
        val base = if (parts[0] == "primary") {
            Environment.getExternalStorageDirectory().absolutePath
        } else {
            "/storage/" + parts[0]
        }
        if (parts.size < 2 || parts[1].isEmpty()) base else base + "/" + parts[1]
    }.getOrNull()

    private fun applyProfile(text: String) {
        runCatching {
            val root = JSONObject(text)
            val values = root.optJSONObject("Values") ?: return
            sourceField.setText(values.optString("DVDA_SRC", sourceField.text.toString()))
            finalField.setText(values.optString("DVDA_FINAL_DIR", finalField.text.toString()))
            titleField.setText(values.optString("DVDA_TITLE", titleField.text.toString()))
            val discBytes = values.optString("DVDA_DISC_BYTES", "")
            capacitySpinner.setSelection(
                when (discBytes) {
                    DVD5_BYTES.toString() -> 0
                    DVD9_BYTES.toString() -> 1
                    "" -> 0
                    else -> 2
                },
            )
            if (discBytes.isNotEmpty()) customBytesField.setText(discBytes)
            workField.setText(values.optString("DVDA_BUILD_DIR", workField.text.toString()))
            isoPrefixField.setText(values.optString("DVDA_ISO_PREFIX", isoPrefixField.text.toString()))
            groupLimitField.setText(values.optString("DVDA_GROUP_TRACK_LIMIT", groupLimitField.text.toString()))
            modeSpinner.setSelection(modes.indexOf(values.optString("DVDA_MLP_SOURCE")).coerceAtLeast(0))
            rateSpinner.setSelection(rates.indexOf(values.optString("DVDA_MLP_SAMPLE_RATE")).coerceAtLeast(0))
            bitsSpinner.setSelection(bits.indexOf(values.optString("DVDA_MLP_BITS")).coerceAtLeast(0))
            ditherCheck.isChecked = values.optString("DVDA_DITHER_16", "on") == "on"
            importField.setText(values.optString("DVDA_MLP_EXTERNAL_DIR", importField.text.toString()))
            menuCheck.isChecked = values.optString("DVDA_MENU", "off") == "on"
            tracksField.setText(values.optString("DVDA_MENU_TRACKS_PER_PAGE", tracksField.text.toString()))
            stillsCheck.isChecked = values.optString("DVDA_MENU_STILLPICS", "on") == "on"
            coverField.setText(values.optString("DVDA_MENU_COVER_DIM", coverField.text.toString()))
            normSpinner.setSelection(norms.indexOf(values.optString("DVDA_MENU_NORM")).coerceAtLeast(0))
            fontScField.setText(values.optString("DVDA_MENU_FONT", fontScField.text.toString()))
            fontJpField.setText(values.optString("DVDA_MENU_FONT_JP", fontJpField.text.toString()))
            fontKrField.setText(values.optString("DVDA_MENU_FONT_KR", fontKrField.text.toString()))
            keepTmpCheck.isChecked = values.optString("DVDA_KEEP_TMP", "off") == "on"
            titleModeField.setText(values.optString("DVDA_TITLE_MODE", titleModeField.text.toString()))
            albumLimitField.setText(values.optString("DVDA_ALBUM_LIMIT", albumLimitField.text.toString()))
        }
    }

    companion object {
        private const val TAG = "DvdaMaker"
        private const val PREFS = "dvda"
        private const val KEY_LANGUAGE = "language"
        private const val DEFAULT_FONT = "/system/fonts/NotoSansCJK-Regular.ttc"
        private const val DVD5_BYTES = 4_707_319_808L
        private const val DVD9_BYTES = 8_540_123_136L
        private const val REQUEST_DIR = 11
        private const val REQUEST_SAVE_PROFILE = 12
        private const val REQUEST_OPEN_PROFILE = 13
    }
}
