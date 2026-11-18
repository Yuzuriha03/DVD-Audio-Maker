package com.dvda.maker

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.ContentResolver
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Environment
import android.os.StatFs
import android.provider.DocumentsContract
import android.provider.OpenableColumns
import android.view.Surface
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.android.FlutterSurfaceView
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.io.BufferedInputStream
import java.io.BufferedOutputStream
import java.io.File
import java.io.InputStream
import java.net.URLConnection
import java.security.DigestOutputStream
import java.security.MessageDigest
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

/// Raised when the user cancels the SAF export adapter.
private class SafOperationCancelled : Exception("Cancelled by the user")

/// One clean failure of the SAF export transaction; exported files are named
/// by the code so the Dart layer only localizes the generic message.
private class ExportFailure(val code: String, val file: String) : Exception(code)

/// The app-private verification mirror would not fit in the free space.
private class InsufficientSpaceException(
    val requiredBytes: Long,
    val availableBytes: Long,
) : Exception(
    "Not enough app-private space for the operation: "
        + "required $requiredBytes bytes, available $availableBytes bytes",
)

/**
 * Android shell for the Flutter application.
 *
 * All user files are reached through the Storage Access Framework. The shell
 * never asks for broad storage permissions and never resolves a filesystem path
 * outside the app sandbox. For native readers it mirrors a granted SAF tree
 * under `filesDir/saf-mirror` by streaming every document through
 * `ContentResolver.openInputStream` into regular private files. The mirror
 * preserves the directory layout and never creates symlinks or platform
 * reparse points.
 */
class MainActivity : FlutterActivity() {
    private var directoryResult: MethodChannel.Result? = null
    private var directoryLinkForNative = false
    private var openProfileResult: MethodChannel.Result? = null
    private val pendingTextSaves = mutableMapOf<Int, PendingTextSave>()
    private var pickFontResult: MethodChannel.Result? = null
    private var flutterSurfaceView: FlutterSurfaceView? = null
    private var pendingNotification: NotificationPayload? = null
    private var notificationPermissionRequested = false
    private var methodChannel: MethodChannel? = null
    private val exportCancelled = AtomicBoolean(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            createNotificationChannel()
        }
        applyAdaptiveFrameRate()
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == REQUEST_NOTIFICATIONS && grantResults.firstOrNull() == PackageManager.PERMISSION_GRANTED) {
            pendingNotification?.let { postStatusNotification(it) }
        }
        pendingNotification = null
    }

    override fun onFlutterSurfaceViewCreated(flutterSurfaceView: FlutterSurfaceView) {
        super.onFlutterSurfaceViewCreated(flutterSurfaceView)
        this.flutterSurfaceView = flutterSurfaceView
        applyAdaptiveFrameRate()
    }

    override fun onResume() {
        super.onResume()
        applyAdaptiveFrameRate()
    }

    /**
     * LTPO: advertise the panel's peak rate with default compatibility and
     * seamless-only switching. The system can still downshift for static UI.
     */
    private fun applyAdaptiveFrameRate() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            return
        }
        val modes = display?.supportedModes ?: return
        val peak = modes.maxOfOrNull { it.refreshRate } ?: return
        runCatching {
            val surface = flutterSurfaceView?.holder?.surface ?: return
            if (surface.isValid) {
                surface.setFrameRate(
                    peak,
                    Surface.FRAME_RATE_COMPATIBILITY_DEFAULT,
                    Surface.CHANGE_FRAME_RATE_ONLY_IF_SEAMLESS,
                )
            }
        }
    }

    private data class PendingTextSave(val result: MethodChannel.Result, val content: String)

    private class SafMirrorState {
        var entries: Int = 0
        var truncated: Boolean = false
        var failure: String? = null
    }

    private data class SafChild(val uri: Uri, val name: String, val isDirectory: Boolean)

    private data class NotificationPayload(
        val title: String,
        val text: String,
        val progress: Double?,
        val ongoing: Boolean,
    )

    override fun configureFlutterEngine(flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        // Cache cleanup is not retried on the next launch: the export adapter
        // releases the caches of the task it delivered, and the explicit
        // "clean internal caches" entry removes leftovers from older versions.
        val channel = MethodChannel(flutterEngine.dartExecutor.binaryMessenger, CHANNEL)
        methodChannel = channel
        channel.setMethodCallHandler { call, result -> handleCall(call, result) }
        // Install the bundled menu font as early as the Flutter engine starts;
        // the Dart side also awaits the same idempotent call before a task.
        safExecutor.execute { runCatching { ensureRuntimeFonts() } }
    }

    private fun handleCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "pickDirectory" -> pickDirectory(call, result)
            "appPaths" -> {
                runCatching {
                    val files = filesDir.canonicalFile
                    if (!files.isDirectory || !files.canRead() || !files.canWrite()) {
                        error("App files directory is not readable/writable: $files")
                    }
                    val build = privateChildDirectory(files, "build")
                    // The task product is published into the task-owned
                    // workspace and exported to SAF; no app-private output
                    // directory is created (matches Windows publication).
                    result.success(
                        mapOf(
                            "filesDir" to files.path,
                            "buildDir" to build.path,
                        ),
                    )
                }.getOrElse {
                    result.error("APP_PATHS", it.message ?: "Cannot prepare app directories", null)
                }
            }
            "validatePrivateDirectory" -> {
                val path = call.argument<String>("path").orEmpty()
                val directory = if (path.isEmpty()) null else privateSource(path)
                result.success(directory != null && directory.isDirectory && directory.canRead())
            }
            "linkDirectory", "ensureSourceLink" -> {
                val rebuild = call.method == "linkDirectory"
                val treeUri = call.argument<String>("treeUri").orEmpty()
                safExecutor.execute {
                    val selection = runCatching {
                        if (treeUri.isEmpty()) null else linkDirectory(treeUri, rebuild)
                    }.getOrElse {
                        when (it) {
                            is InsufficientSpaceException -> result.error(
                                "SAF_NO_SPACE",
                                it.message ?: "Not enough app-private space for the folder mirror",
                                null,
                            )
                            else -> result.error(
                                "SAF_MIRROR",
                                it.message ?: "Cannot read the selected folder",
                                null,
                            )
                        }
                        return@execute
                    }
                    result.success(selection)
                }
            }
            "exportDirectory" -> {
                val sourcePath = call.argument<String>("sourcePath").orEmpty()
                val treeUri = call.argument<String>("treeUri").orEmpty()
                val cachePaths =
                    call.argument<List<*>>("cachePaths")?.filterIsInstance<String>().orEmpty()
                safExecutor.execute {
                    result.success(
                        if (sourcePath.isEmpty() || treeUri.isEmpty()) {
                            exportFailure("SOURCE_MISSING", "")
                        } else {
                            exportDirectory(sourcePath, treeUri, cachePaths)
                        },
                    )
                }
            }
            "cancelExport" -> {
                exportCancelled.set(true)
                result.success(null)
            }
            "ensureRuntimeFonts" -> {
                safExecutor.execute { result.success(ensureRuntimeFonts()) }
            }
            "cleanupInternalCaches" -> {
                safExecutor.execute { result.success(cleanupInternalCaches()) }
            }
            "openOutput" -> {
                val treeUri = call.argument<String>("treeUri").orEmpty()
                result.success(openOutput(treeUri))
            }
            "openProfile" -> pickProfile(result)
            "saveProfile", "saveTextFile" -> saveTextDocument(call, result)
            "pickFont" -> pickFont(result)
            "appVersion" -> appVersion(result)
            "showUpdateNotification" -> showUpdateNotification(call, result)
            "clearUpdateNotification" -> clearUpdateNotification(result)
            "openUpdateUrl" -> openUpdateUrl(call, result)
            "updateStatusNotification" -> {
                val payload = NotificationPayload(
                    title = call.argument<String>("title").orEmpty(),
                    text = call.argument<String>("text").orEmpty(),
                    progress = call.argument<Double>("progress"),
                    ongoing = call.argument<Boolean>("ongoing") ?: true,
                )
                updateStatusNotification(payload)
                result.success(null)
            }
            "clearStatusNotification" -> {
                getSystemService(NotificationManager::class.java).cancel(NOTIFICATION_ID)
                result.success(null)
            }
            else -> result.notImplemented()
        }
    }

    private fun pickDirectory(call: MethodCall, result: MethodChannel.Result) {
        directoryResult?.success(null)
        directoryResult = result
        directoryLinkForNative = call.argument<Boolean>("linkForNative") ?: false
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
                    if (directoryLinkForNative) 0 else Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                treeUriForPath(call.argument<String>("initial"))?.let { uri ->
                    putExtra(DocumentsContract.EXTRA_INITIAL_URI, uri)
                }
            }
        }
        runCatching {
            startActivityForResult(intent, REQUEST_DIRECTORY)
        }.onFailure {
            directoryResult = null
            result.error("SAF_PICKER", it.message ?: "Cannot open the folder picker", null)
        }
    }

    private fun pickProfile(result: MethodChannel.Result) {
        openProfileResult?.success(null)
        openProfileResult = result
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "application/json"
        }
        startActivityForResult(intent, REQUEST_OPEN_PROFILE)
    }

    private fun pickFont(result: MethodChannel.Result) {
        pickFontResult?.success(null)
        pickFontResult = result
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = "*/*"
            putExtra(Intent.EXTRA_MIME_TYPES, FONT_MIME_TYPES)
        }
        startActivityForResult(intent, REQUEST_PICK_FONT)
    }

    private fun saveTextDocument(call: MethodCall, result: MethodChannel.Result) {
        val profile = call.method == "saveProfile"
        val requestCode = if (profile) REQUEST_SAVE_PROFILE else REQUEST_SAVE_TEXT
        pendingTextSaves[requestCode]?.result?.success(false)
        pendingTextSaves[requestCode] = PendingTextSave(
            result, call.argument<String>("content").orEmpty(),
        )
        val requested = call.argument<String>("suggestedName")?.trim()
        val defaultName = if (profile) DEFAULT_PROFILE_NAME else DEFAULT_TEXT_NAME
        val requestedMime = if (profile) "application/json" else call.argument<String>("mimeType")?.trim()
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = if (requestedMime.isNullOrEmpty()) DEFAULT_TEXT_MIME else requestedMime
            putExtra(Intent.EXTRA_TITLE, if (requested.isNullOrEmpty()) defaultName else requested)
        }
        startActivityForResult(intent, requestCode)
    }

    private fun openOutput(treeUriText: String): Boolean {
        if (treeUriText.isBlank()) {
            return false
        }
        val uri = contentUri(treeUriText) ?: return false
        val document = runCatching {
            DocumentsContract.buildDocumentUriUsingTree(
                uri,
                DocumentsContract.getTreeDocumentId(uri),
            )
        }.getOrDefault(uri)
        return viewDirectory(document)
    }

    private fun viewDirectory(uri: Uri): Boolean = runCatching {
        startActivity(
            Intent(Intent.ACTION_VIEW)
                .setDataAndType(uri, DocumentsContract.Document.MIME_TYPE_DIR)
                .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION),
        )
    }.isSuccess

    private fun persistTreePermission(uri: Uri, write: Boolean = false) {
        runCatching {
            contentResolver.takePersistableUriPermission(
                uri,
                Intent.FLAG_GRANT_READ_URI_PERMISSION,
            )
        }
        if (write) {
            runCatching {
                contentResolver.takePersistableUriPermission(
                    uri,
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
                )
            }
        }
    }

    private fun contentUri(value: String): Uri? =
        runCatching { Uri.parse(value) }.getOrNull()
            ?.takeIf { it.scheme == ContentResolver.SCHEME_CONTENT }

    private fun selectionOf(uri: Uri, nativePath: String = ""): Map<String, String> = mapOf(
        "displayPath" to (treeDisplayPath(uri) ?: uri.toString()),
        "nativePath" to nativePath,
        "treeUri" to uri.toString(),
    )

    private fun mirrorMarker(key: String): File =
        File(File(filesDir, SAF_MIRROR_DIRECTORY), "$key.complete")

    private fun linkDirectory(treeUriText: String, rebuild: Boolean): Map<String, String>? {
        val uri = contentUri(treeUriText) ?: return null
        val key = treeKey(uri)
        if (!rebuild) {
            val root = File(File(filesDir, SAF_MIRROR_DIRECTORY), key)
            if (root.isDirectory && mirrorMarker(key).isFile) {
                val nativePath = runCatching { root.canonicalPath }.getOrNull()
                if (nativePath != null) return selectionOf(uri, nativePath)
            }
        }
        persistTreePermission(uri)
        return selectionOf(uri, mirrorTree(uri, key))
    }

    private fun mirrorTree(treeUri: Uri, key: String): String {
        val root = File(File(filesDir, SAF_MIRROR_DIRECTORY), key)
        mirrorMarker(key).delete()
        deleteTree(root)
        if (!root.mkdirs() && !root.isDirectory) {
            error("Cannot create the app-private folder mirror: $root")
        }
        val state = SafMirrorState()
        try {
            val rootId = DocumentsContract.getTreeDocumentId(treeUri)
            if (!walkTree(treeUri, rootId, root, 0, state)) {
                error(state.failure ?: "Cannot read the selected folder")
            }
            if (state.truncated) {
                error(state.failure ?: "The selected folder is too large to mirror")
            }
            mirrorMarker(key).writeText("v1")
            return root.canonicalPath
        } catch (failure: Throwable) {
            deleteTree(root)
            throw failure
        }
    }

    private fun walkTree(
        treeUri: Uri,
        documentId: String,
        directory: File,
        depth: Int,
        state: SafMirrorState,
    ): Boolean {
        if (depth > MAX_SAF_DEPTH) {
            state.truncated = true
            state.failure = "The selected folder is nested deeper than $MAX_SAF_DEPTH levels"
            return false
        }
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, documentId)
        val cursor = contentResolver.query(
            childrenUri,
            arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            ),
            null,
            null,
            null,
        )
        if (cursor == null) {
            state.truncated = true
            state.failure = "Cannot list the folder ${directory.name}"
            return false
        }
        cursor.use {
            while (it.moveToNext()) {
                if (state.entries >= MAX_SAF_ENTRIES) {
                    state.truncated = true
                    state.failure =
                        "The selected folder has more than $MAX_SAF_ENTRIES entries"
                    return@use
                }
                state.entries++
                val childId = it.getString(0)
                if (childId != null) {
                    val name = safeSafDocumentName(it.getString(1), ENTRY_FALLBACK_NAME)
                    val mimeType = it.getString(2)
                    val childUri = DocumentsContract.buildDocumentUriUsingTree(treeUri, childId)
                    val target = uniqueChild(directory, name)
                    if (mimeType == DocumentsContract.Document.MIME_TYPE_DIR) {
                        if (target.mkdirs() || target.isDirectory) {
                            walkTree(treeUri, childId, target, depth + 1, state)
                        } else {
                            state.truncated = true
                            state.failure = "Cannot create the folder ${target.name}"
                        }
                    } else if (!copyDocument(childUri, target)) {
                        state.truncated = true
                        state.failure = "Cannot read the file ${target.name}"
                    }
                }
            }
        }
        return true
    }

    private fun ensureMirrorSpace(uri: Uri) {
        val available = StatFs(filesDir.path).availableBytes
        val required = documentSize(uri)
        if (required != null && required > 0L) {
            if (available < required + MIRROR_SPACE_MARGIN_BYTES) {
                throw InsufficientSpaceException(required, available)
            }
            return
        }
        // The provider does not report a size: still refuse to start a copy when
        // the device is already inside the safety margin.
        if (available < MIRROR_SPACE_MARGIN_BYTES) {
            throw InsufficientSpaceException(0L, available)
        }
    }

    private fun copyDocument(uri: Uri, target: File): Boolean {
        // Keep the app-private space margin before copying each mirror file.
        ensureMirrorSpace(uri)
        val parent = target.parentFile ?: return false
        parent.mkdirs()
        val temporary = File(parent, target.name + ".part")
        val copied = runCatching {
            val input = contentResolver.openInputStream(uri) ?: return@runCatching false
            input.use { raw ->
                BufferedInputStream(raw).use { source ->
                    BufferedOutputStream(temporary.outputStream()).use { sink ->
                        source.copyTo(sink, COPY_BUFFER_BYTES)
                    }
                }
            }
            true
        }.getOrDefault(false)
        if (!copied) {
            temporary.delete()
            return false
        }
        target.delete()
        if (!temporary.renameTo(target)) {
            temporary.delete()
            return false
        }
        return true
    }

    private fun deleteTree(file: File) {
        // Mirrors contain regular files/directories only. Legacy /proc links
        // pointed at regular files, so isDirectory stays false and delete()
        // removes the link itself without following it.
        if (file.isDirectory) {
            file.listFiles()?.forEach(::deleteTree)
        }
        file.delete()
    }

    private fun treeKey(uri: Uri): String {
        val digest = MessageDigest.getInstance("SHA-256")
            .digest(uri.toString().toByteArray(Charsets.UTF_8))
        return digest.take(TREE_KEY_BYTES).joinToString("") {
            "%02x".format(it.toInt() and 0xff)
        }
    }

    private fun uniqueChild(directory: File, name: String): File {
        var candidate = File(directory, name)
        var index = 1
        while (candidate.exists()) {
            candidate = File(directory, withSuffix(name, index))
            index++
        }
        return candidate
    }

    private fun withSuffix(name: String, index: Int): String {
        val dot = name.lastIndexOf('.')
        return if (dot > 0) {
            "${name.substring(0, dot)} ($index)${name.substring(dot)}"
        } else {
            "$name ($index)"
        }
    }

    private fun privateChildDirectory(parent: File, name: String): File {
        val child = File(parent, name)
        if ((!child.isDirectory && !child.mkdirs()) || !child.canRead() || !child.canWrite()) {
            error("App directory is not readable/writable: $child")
        }
        val canonical = child.canonicalFile
        if (canonical.parentFile != parent) {
            error("App directory escapes filesDir: $canonical")
        }
        return canonical
    }

    private fun privateSource(sourcePath: String): File? {
        val filesRoot = runCatching { filesDir.canonicalFile }.getOrNull() ?: return null
        val direct = runCatching { File(sourcePath).canonicalFile }.getOrNull()
        if (direct != null &&
            (direct == filesRoot || direct.path.startsWith(filesRoot.path + File.separator))
        ) {
            return direct
        }
        // The caller may carry a path captured under a different user/data alias.
        val marker = "/${packageName}/files/"
        val markerIndex = sourcePath.indexOf(marker)
        if (markerIndex < 0) {
            return null
        }
        val relative = sourcePath.substring(markerIndex + marker.length)
        if (relative.isEmpty()) {
            return null
        }
        val candidate = runCatching { File(filesRoot, relative).canonicalFile }.getOrNull()
            ?: return null
        if (!candidate.path.startsWith(filesRoot.path + File.separator)) {
            return null
        }
        return candidate
    }

    private data class ExportEntry(
        val source: File,
        val name: String,
        val temporary: Uri,
        var backup: Uri? = null,
        var finalDocument: Uri? = null,
        var committed: Boolean = false,
    )

    /// Exports the task-local product into the granted SAF folder with the same
    /// temporary → verify → backup → commit transaction Windows publication uses.
    ///
    /// A failure never truncates the previously exported ISO: the visible file is
    /// only replaced after the whole set was copied and verified, and the
    /// rollback restores every backup. The task workspace is not touched here, so
    /// the shared finish still decides its retention on failure.
    private fun exportDirectory(
        sourcePath: String,
        treeUriText: String,
        cachePaths: List<String>,
    ): Map<String, Any?> {
        exportCancelled.set(false)
        val source = privateSource(sourcePath) ?: return exportFailure("SOURCE_MISSING", "")
        if (!source.isDirectory) return exportFailure("SOURCE_MISSING", "")
        val treeUri = contentUri(treeUriText) ?: return exportFailure("TREE_URI_INVALID", "")
        persistTreePermission(treeUri, write = true)
        // Only the published ISO set is the product (same set Windows publication
        // moves); unrelated or stale files in the transport folder are ignored.
        val children = source.listFiles()
            ?.filter { it.isFile && it.name.endsWith(".iso", ignoreCase = true) }
            ?.sortedBy { it.name }
            ?: emptyList()
        if (children.isEmpty()) return exportFailure("NO_PRODUCT_FILES", "")
        // Product names must reach the user folder unchanged apart from unsafe
        // characters; SAF documents may exceed the local mirror name bound.
        val names = children.map { safeSafDocumentName(it.name, ENTRY_FALLBACK_NAME) }
        if (names.map { it.lowercase() }.toSet().size != names.size) {
            return exportFailure("DUPLICATE_NAMES", "")
        }
        val parent = runCatching {
            DocumentsContract.buildDocumentUriUsingTree(
                treeUri,
                DocumentsContract.getTreeDocumentId(treeUri),
            )
        }.getOrNull() ?: return exportFailure("TREE_URI_INVALID", "")
        val token = exportToken()
        val prepared = mutableListOf<ExportEntry>()
        return try {
            // Recover documents left by a crash inside a previous transaction
            // before starting a new one; the visible product must never stay
            // only under a .backup name.
            recoverInterruptedExport(treeUri, parent)
            val existing = childEntries(treeUri, parent)
                ?: throw ExportFailure("LIST_FOLDER", "")
            // Phase 1: copy every product into a temporary SAF document and
            // verify the stored length and SHA-256. A short write only deletes
            // the temporary document; the visible ISO is untouched.
            children.forEachIndexed { index, child ->
                if (exportCancelled.get()) throw SafOperationCancelled()
                val name = names[index]
                val temporaryName = exportTemporaryName(name, token, index)
                val temporary = createDocument(parent, mimeTypeFor(temporaryName), temporaryName)
                    ?: throw ExportFailure("CREATE_TEMPORARY", name)
                val entry = ExportEntry(child, name, temporary)
                prepared.add(entry)
                copyExportTemporary(entry)
                postIoProgress(
                    "exportProgress",
                    mapOf("done" to index + 1, "total" to children.size, "name" to name),
                )
            }
            if (exportCancelled.get()) throw SafOperationCancelled()
            // Phase 2: move the old ISO to a backup, then move the verified
            // temporary document into place. Any failure rolls the completed
            // steps back so an interrupted export keeps the old product.
            for ((index, entry) in prepared.withIndex()) {
                if (exportCancelled.get()) throw SafOperationCancelled()
                val previous = existing[entry.name.lowercase()]
                if (previous != null && previous.isDirectory) {
                    throw ExportFailure("TARGET_IS_FOLDER", entry.name)
                }
                if (previous != null) {
                    val backupName = exportBackupName(entry.name, token, index)
                    val backup = renameDocument(previous.uri, backupName)
                        ?: throw ExportFailure("RENAME_BACKUP", entry.name)
                    entry.backup = backup
                }
                val target = renameDocument(entry.temporary, entry.name)
                    ?: throw ExportFailure("RENAME_FINAL", entry.name)
                entry.finalDocument = target
                entry.committed = true
            }
            for (entry in prepared) {
                entry.backup?.let(::deleteDocument)
            }
            releaseTaskCaches(cachePaths)
            exportSuccess(prepared.map { it.name })
        } catch (failure: Throwable) {
            rollbackExport(prepared)
            when (failure) {
                is SafOperationCancelled -> exportCancelledResult()
                is ExportFailure -> exportFailure(failure.code, failure.file)
                else -> exportFailure("EXPORT_FAILED", "")
            }
        }
    }

    private inline fun readExportChunks(input: InputStream, consume: (ByteArray, Int) -> Unit) {
        val buffer = ByteArray(COPY_BUFFER_BYTES)
        while (true) {
            if (exportCancelled.get()) throw SafOperationCancelled()
            val size = input.read(buffer)
            if (size < 0) return
            consume(buffer, size)
        }
    }

    private fun copyExportTemporary(entry: ExportEntry) {
        val expectedSize = entry.source.length()
        val digest = MessageDigest.getInstance("SHA-256")
        var written = 0L
        val stream = contentResolver.openOutputStream(entry.temporary, "w")
            ?: contentResolver.openOutputStream(entry.temporary, "wt")
            ?: throw ExportFailure("OPEN_TEMPORARY", entry.name)
        try {
            BufferedInputStream(entry.source.inputStream()).use { input ->
                DigestOutputStream(BufferedOutputStream(stream), digest).use { output ->
                    readExportChunks(input) { buffer, size ->
                        output.write(buffer, 0, size)
                        written += size
                    }
                }
            }
        } catch (failure: Throwable) {
            deleteDocument(entry.temporary)
            throw failure
        }
        if (written != expectedSize) {
            deleteDocument(entry.temporary)
            throw ExportFailure("SHORT_WRITE", entry.name)
        }
        // The provider may not report COLUMN_SIZE for a fresh document; the
        // stored SHA-256 check below still proves the exact length and content.
        val reportedSize = documentSize(entry.temporary)
        if (reportedSize != null && reportedSize != expectedSize) {
            deleteDocument(entry.temporary)
            throw ExportFailure("SIZE_MISMATCH", entry.name)
        }
        val stored = documentDigest(entry.temporary)
        if (stored == null || !stored.contentEquals(digest.digest())) {
            deleteDocument(entry.temporary)
            throw ExportFailure("HASH_MISMATCH", entry.name)
        }
    }

    private fun rollbackExport(prepared: List<ExportEntry>) {
        for (entry in prepared.asReversed()) {
            if (entry.committed) {
                entry.finalDocument?.let(::deleteDocument)
                entry.backup?.let { backup ->
                    runCatching { renameDocument(backup, entry.name) }
                }
            } else {
                entry.backup?.let { backup ->
                    runCatching { renameDocument(backup, entry.name) }
                }
                deleteDocument(entry.temporary)
            }
        }
    }

    /// Recovers SAF documents left behind by a crash inside an earlier export
    /// transaction.
    ///
    /// Written transaction documents carry our exact token pattern:
    /// "<product>.backup-<index>-<pid>-<nanos>-<seq>" and the equivalent
    /// "publishing" temporary. If the visible product name is missing the
    /// backup is restored; if the visible product exists the backup is a stale
    /// duplicate of a committed replacement and is removed. Interrupted
    /// temporaries are deleted after backups were restored. This is export I/O
    /// crash recovery only and never touches unrelated user documents.
    private fun recoverInterruptedExport(treeUri: Uri, parent: Uri) {
        val snapshot = childEntries(treeUri, parent) ?: throw ExportFailure("LIST_FOLDER", "")
        val staleBackups = mutableListOf<Uri>()
        for (child in snapshot.values) {
            if (child.isDirectory) continue
            val match = TRANSACTION_BACKUP_PATTERN.matchEntire(child.name) ?: continue
            val base = match.groupValues[1]
            val visible = snapshot[base.lowercase()]
            if (visible != null && visible.isDirectory) {
                throw ExportFailure("RECOVER_CONFLICT", base)
            }
            if (visible != null) {
                staleBackups.add(child.uri)
            } else if (renameDocument(child.uri, base) == null) {
                throw ExportFailure("RECOVER_BACKUP", base)
            }
        }
        staleBackups.forEach(::deleteDocument)
        val restored = childEntries(treeUri, parent) ?: snapshot
        for (child in restored.values) {
            if (!child.isDirectory && TRANSACTION_PUBLISHING_PATTERN.matches(child.name)) {
                deleteDocument(child.uri)
            }
        }
    }

    /// Releases the app-private SAF mirrors consumed by the delivered task.
    ///
    /// Only directories owned by this adapter are removed; user-granted SAF
    /// folders, the unfinished workspace and the exported product are untouched.
    private fun releaseTaskCaches(cachePaths: List<String>) {
        for (path in cachePaths) {
            val directory = privateSource(path) ?: continue
            val parent = directory.parentFile?.name ?: continue
            if (parent != SAF_MIRROR_DIRECTORY && parent != LEGACY_LINK_DIRECTORY) {
                continue
            }
            deleteTree(directory)
            // The source mirror marks a complete copy with a sibling
            // "<key>.complete"; remove it with the cache so a stale marker
            // cannot claim a removed mirror.
            File(directory.parentFile, directory.name + ".complete").delete()
        }
    }

    private fun exportToken(): String = "%x-%x-%x".format(
        android.os.Process.myPid(),
        System.nanoTime(),
        EXPORT_SEQUENCE.incrementAndGet(),
    )

    private fun exportTemporaryName(name: String, token: String, index: Int): String =
        exportDerivedName(name, ".publishing-$index-$token")

    private fun exportBackupName(name: String, token: String, index: Int): String =
        exportDerivedName(name, ".backup-$index-$token")

    /// Keeps the transaction suffix inside the SAF display-name bound even when
    /// the product name is long; the committed document is renamed back to the
    /// exact product name.
    private fun exportDerivedName(name: String, suffix: String): String =
        safeSafDocumentName(name + suffix, ENTRY_FALLBACK_NAME)

    /// SAF display-name sanitizer for the user-visible product name.
    ///
    /// Unlike local mirror names it never truncates: the recovery pattern must
    /// be able to restore the exact product name after a crash.
    private fun safeSafDocumentName(raw: String?, fallback: String): String {
        val cleaned = raw.orEmpty()
            .map { character ->
                if (character == '/' || character == '\\' ||
                    character.code < 0x20 || character.code == 0x7f
                ) {
                    '_'
                } else {
                    character
                }
            }
            .joinToString("")
            .trim()
            .trim('.')
        return when {
            cleaned.isEmpty() -> fallback
            cleaned == "." || cleaned == ".." -> fallback
            else -> cleaned
        }
    }

    private fun exportFailure(code: String, file: String): Map<String, Any?> = mapOf(
        "ok" to false,
        "cancelled" to false,
        "failureCode" to code,
        "failureFile" to file,
        "exported" to emptyList<String>(),
        "freedBytes" to 0L,
    )

    private fun exportCancelledResult(): Map<String, Any?> = mapOf(
        "ok" to false,
        "cancelled" to true,
        "failureCode" to "CANCELLED",
        "failureFile" to "",
        "exported" to emptyList<String>(),
        "freedBytes" to 0L,
    )

    private fun exportSuccess(names: List<String>): Map<String, Any?> = mapOf(
        "ok" to true,
        "cancelled" to false,
        "failureCode" to "",
        "failureFile" to "",
        "exported" to names,
        "freedBytes" to 0L,
    )

    private fun renameDocument(uri: Uri, name: String): Uri? =
        runCatching { DocumentsContract.renameDocument(contentResolver, uri, name) }.getOrNull()

    private fun deleteDocument(uri: Uri) {
        runCatching { DocumentsContract.deleteDocument(contentResolver, uri) }
    }

    private fun documentSize(uri: Uri): Long? = runCatching {
        contentResolver.query(
            uri,
            arrayOf(DocumentsContract.Document.COLUMN_SIZE),
            null,
            null,
            null,
        )?.use { cursor ->
            if (!cursor.moveToFirst() || cursor.isNull(0)) null else cursor.getLong(0)
        }
    }.getOrNull()

    private fun documentDigest(uri: Uri): ByteArray? {
        val digest = MessageDigest.getInstance("SHA-256")
        val input = runCatching { contentResolver.openInputStream(uri) }.getOrNull()
            ?: return null
        try {
            BufferedInputStream(input).use { stream ->
                readExportChunks(stream) { buffer, size -> digest.update(buffer, 0, size) }
            }
        } catch (failure: SafOperationCancelled) {
            throw failure
        } catch (_: Throwable) {
            return null
        }
        return digest.digest()
    }

    /// Installs the bundled menu font collection into app-private storage.
    ///
    /// The APK asset is the only font source: this is a thin I/O copy, the shared
    /// Rust font resolution and family/fallback rules stay unchanged. The call is
    /// idempotent and runs at startup and before a task starts.
    private fun ensureRuntimeFonts(): Map<String, Any?> = runCatching {
        val directory = File(File(filesDir, RUNTIME_DIRECTORY), FONT_DIRECTORY)
        if (!directory.isDirectory && !directory.mkdirs()) {
            error("Cannot create the runtime font directory: $directory")
        }
        val target = File(directory, RUNTIME_FONT_NAME)
        val marker = File(directory, "." + RUNTIME_FONT_NAME + ".sha256")
        // The APK asset is immutable per installed package version; storing the
        // package update stamp means an app update replaces the extracted font
        // while ordinary starts do not re-copy 14 MB.
        val packageStamp = packageUpdateStamp()
        val markerText = if (marker.isFile) {
            runCatching { marker.readText() }.getOrDefault("")
        } else {
            ""
        }
        if (target.isFile && target.length() > 0L &&
            markerText.substringBefore(':') == packageStamp
        ) {
            return@runCatching mapOf(
                "path" to target.canonicalPath,
                "installed" to false,
            )
        }
        val digest = MessageDigest.getInstance("SHA-256")
        val temporary = File(directory, "." + RUNTIME_FONT_NAME + ".part")
        temporary.delete()
        assets.open(FONT_ASSET).use { raw ->
            BufferedInputStream(raw).use { input ->
                DigestOutputStream(BufferedOutputStream(temporary.outputStream()), digest)
                    .use { output -> input.copyTo(output, COPY_BUFFER_BYTES) }
            }
        }
        if (!temporary.renameTo(target)) {
            temporary.delete()
            error("Cannot install the bundled menu font")
        }
        marker.delete()
        val hash = digest.digest().joinToString("") { "%02x".format(it.toInt() and 0xff) }
        marker.writeText("$packageStamp:$hash")
        mapOf("path" to target.canonicalPath, "installed" to true)
    }.getOrElse { failure ->
        mapOf(
            "path" to null,
            "installed" to false,
            "error" to (failure.message ?: "Cannot install the bundled menu font"),
        )
    }

    /// Explicit user-confirmed recovery for caches left by older versions.
    ///
    /// Only known rebuildable app-private copies (SAF mirrors, legacy link trees
    /// and the old output folder) are removed. User-granted SAF folders, the
    /// unfinished task workspace and exported ISOs are never touched, and this
    /// entry never replaces the shared finish of the current task.
    private fun packageUpdateStamp(): String = runCatching {
        packageManager.getPackageInfo(packageName, 0).lastUpdateTime.toString()
    }.getOrDefault("unknown")

    private fun cleanupInternalCaches(): Map<String, Any?> = runCatching {
        var freed = 0L
        val deleted = mutableListOf<String>()
        for (name in INTERNAL_CACHE_DIRECTORIES) {
            val directory = File(filesDir, name)
            if (!directory.exists()) continue
            freed += treeBytes(directory)
            deleteTree(directory)
            deleted.add(name)
        }
        mapOf("ok" to true, "freedBytes" to freed, "deleted" to deleted)
    }.getOrElse { failure ->
        mapOf(
            "ok" to false,
            "freedBytes" to 0L,
            "deleted" to emptyList<String>(),
            "error" to (failure.message ?: "Cannot clean the internal caches"),
        )
    }

    private fun treeBytes(file: File): Long {
        if (!file.isDirectory) {
            return if (file.isFile) file.length() else 0L
        }
        return file.listFiles()?.sumOf(::treeBytes) ?: 0L
    }

    private fun postIoProgress(method: String, payload: Map<String, Any?>) {
        val channel = methodChannel ?: return
        runOnUiThread {
            runCatching { channel.invokeMethod(method, payload) }
        }
    }

    private fun childEntries(treeUri: Uri, parent: Uri): MutableMap<String, SafChild>? = runCatching {
        val parentId = DocumentsContract.getDocumentId(parent)
        val childrenUri = DocumentsContract.buildChildDocumentsUriUsingTree(treeUri, parentId)
        val entries = mutableMapOf<String, SafChild>()
        val cursor = contentResolver.query(
            childrenUri,
            arrayOf(
                DocumentsContract.Document.COLUMN_DOCUMENT_ID,
                DocumentsContract.Document.COLUMN_DISPLAY_NAME,
                DocumentsContract.Document.COLUMN_MIME_TYPE,
            ),
            null,
            null,
            null,
        ) ?: error("Cannot query the granted output directory")
        cursor.use {
            while (cursor.moveToNext()) {
                val documentId = cursor.getString(0)
                val displayName = cursor.getString(1)
                if (documentId != null && displayName != null) {
                    val uri = DocumentsContract.buildDocumentUriUsingTree(treeUri, documentId)
                    entries[displayName.lowercase()] = SafChild(
                        uri,
                        displayName,
                        cursor.getString(2) == DocumentsContract.Document.MIME_TYPE_DIR,
                    )
                }
            }
        }
        entries
    }.getOrNull()

    private fun createDocument(parent: Uri, mimeType: String, name: String): Uri? =
        runCatching {
            DocumentsContract.createDocument(contentResolver, parent, mimeType, name)
        }.getOrNull()

    private fun mimeTypeFor(name: String): String {
        URLConnection.guessContentTypeFromName(name)?.let { return it }
        return when (name.substringAfterLast('.', "").lowercase()) {
            "flac" -> "audio/flac"
            "xml" -> "application/xml"
            "png" -> "image/png"
            "jpg", "jpeg" -> "image/jpeg"
            else -> "application/octet-stream"
        }
    }

    private fun displayName(uri: Uri): String? = runCatching {
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)
            ?.use { cursor ->
                if (cursor.moveToFirst()) cursor.getString(0) else null
            }
    }.getOrNull()

    private fun safeFontName(raw: String?): String? {
        val name = raw?.substringAfterLast('/')?.substringAfterLast('\\')?.trim().orEmpty()
        if (name.isEmpty()) {
            return null
        }
        val extension = name.substringAfterLast('.', "").lowercase()
        if (extension !in FONT_EXTENSIONS) {
            return null
        }
        val stem = name.substringBeforeLast('.')
            .replace(Regex("[^A-Za-z0-9._-]"), "_")
            .trim('_', '.')
            .take(MAX_FONT_STEM)
        return "${stem.ifEmpty { "font" }}.$extension"
    }

    private fun uniqueFontFile(directory: File, name: String): File {
        val stem = name.substringBeforeLast('.')
        val extension = name.substringAfterLast('.')
        var candidate = File(directory, name)
        var index = 1
        while (candidate.exists()) {
            candidate = File(directory, "$stem-$index.$extension")
            index++
        }
        return candidate
    }

    private fun importFont(uri: Uri): String? {
        val fallbackName = runCatching {
            uri.lastPathSegment?.let { Uri.decode(it) }
        }.getOrNull()
        val safeName = safeFontName(displayName(uri) ?: fallbackName) ?: return null
        val directory = File(filesDir, FONT_DIRECTORY)
        if (!directory.isDirectory && !directory.mkdirs()) {
            return null
        }
        val target = uniqueFontFile(directory, safeName)
        val temporary = File(directory, "${target.name}.part")
        val copied = runCatching {
            contentResolver.openInputStream(uri)?.use { input ->
                temporary.outputStream().use { output -> input.copyTo(output) }
            } != null
        }.getOrDefault(false)
        if (!copied || temporary.length() == 0L) {
            temporary.delete()
            return null
        }
        if (!temporary.renameTo(target)) {
            temporary.delete()
            return null
        }
        return target.absolutePath
    }

    @Suppress("DEPRECATION")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        super.onActivityResult(requestCode, resultCode, data)
        when (requestCode) {
            REQUEST_DIRECTORY -> {
                val result = directoryResult ?: return
                directoryResult = null
                val linkForNative = directoryLinkForNative
                directoryLinkForNative = false
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    result.success(null)
                    return
                }
                persistTreePermission(uri, write = !linkForNative)
                // Return the selection immediately. Copying the tree into the
                // app-private mirror happens when a task starts, so picking a
                // large folder never looks like an ignored selection.
                result.success(selectionOf(uri))
            }
            REQUEST_OPEN_PROFILE -> {
                val result = openProfileResult ?: return
                openProfileResult = null
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    result.success(null)
                    return
                }
                val text = runCatching {
                    contentResolver.openInputStream(uri)?.bufferedReader()?.use { it.readText() }
                }.getOrNull()
                result.success(text)
            }
            REQUEST_SAVE_PROFILE, REQUEST_SAVE_TEXT -> {
                val pending = pendingTextSaves.remove(requestCode) ?: return
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    pending.result.success(false)
                    return
                }
                val saved = runCatching {
                    contentResolver.openOutputStream(uri)?.use { stream ->
                        stream.write(pending.content.toByteArray(Charsets.UTF_8))
                    } != null
                }.getOrDefault(false)
                pending.result.success(saved)
            }
            REQUEST_PICK_FONT -> {
                val result = pickFontResult ?: return
                pickFontResult = null
                val uri = data?.data
                if (resultCode != RESULT_OK || uri == null) {
                    result.success(null)
                    return
                }
                result.success(importFont(uri))
            }
        }
    }

    @Suppress("DEPRECATION")
    private fun treeUriForPath(path: String?): Uri? {
        val value = path?.trim().orEmpty()
        if (value.isEmpty()) {
            return null
        }
        val root = Environment.getExternalStorageDirectory().absolutePath.trimEnd('/')
        val document = when {
            value == root -> "primary:"
            value.startsWith("$root/") -> "primary:" + value.removePrefix("$root/")
            value.startsWith("/storage/") -> {
                val parts = value.removePrefix("/storage/").split("/", limit = 2)
                val volume = parts[0]
                if (volume.isEmpty() || volume == "emulated") {
                    return null
                }
                if (parts.size < 2 || parts[1].isEmpty()) "$volume:" else "$volume:" + parts[1]
            }
            else -> return null
        }
        return runCatching {
            DocumentsContract.buildTreeDocumentUri(EXTERNAL_STORAGE_AUTHORITY, document)
        }.getOrNull()
    }

    @Suppress("DEPRECATION")
    private fun treeDisplayPath(uri: Uri): String? = runCatching {
        val document = DocumentsContract.getTreeDocumentId(uri)
        val parts = document.split(":", limit = 2)
        val base = when (parts[0]) {
            "primary", "home" -> Environment.getExternalStorageDirectory().absolutePath
            else -> "/storage/" + parts[0]
        }
        if (parts.size < 2 || parts[1].isEmpty()) base else "$base/" + parts[1]
    }.getOrNull()

    private fun appVersion(result: MethodChannel.Result) {
        runCatching {
            val info = packageManager.getPackageInfo(packageName, 0)
            @Suppress("DEPRECATION")
            val code = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
                info.longVersionCode
            } else {
                info.versionCode.toLong()
            }
            result.success(
                mapOf(
                    "versionName" to (info.versionName ?: ""),
                    "versionCode" to code,
                ),
            )
        }.getOrElse {
            result.error("APP_VERSION", it.message ?: "Cannot read the package version", null)
        }
    }

    private fun showUpdateNotification(call: MethodCall, result: MethodChannel.Result) {
        val title = call.argument<String>("title").orEmpty()
        val text = call.argument<String>("text").orEmpty()
        val downloadUrl = call.argument<String>("downloadUrl").orEmpty()
        val target = updateDownloadUri(downloadUrl)
        if (target == null) {
            result.success(false)
            return
        }
        // Update notices never request the notification permission; when it is
        // unavailable Dart shows a clickable in-app notice instead.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            result.success(false)
            return
        }
        val manager = getSystemService(NotificationManager::class.java)
        if (!manager.areNotificationsEnabled()) {
            result.success(false)
            return
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            createUpdateChannel(manager)
            // A channel the user disabled must fall back to the in-app notice.
            if (manager.getNotificationChannel(UPDATE_CHANNEL_ID)?.importance ==
                NotificationManager.IMPORTANCE_NONE
            ) {
                result.success(false)
                return
            }
        }
        val contentIntent = PendingIntent.getActivity(
            this,
            REQUEST_UPDATE_LINK,
            Intent(Intent.ACTION_VIEW, target)
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, UPDATE_CHANNEL_ID)
        } else {
            Notification.Builder(this)
        }
        builder
            .setSmallIcon(R.drawable.ic_stat_dvda)
            .setContentTitle(title.ifEmpty { "DVD-Audio Maker" })
            .setContentText(text)
            .setContentIntent(contentIntent)
            .setAutoCancel(true)
            .setOnlyAlertOnce(true)
            .setCategory(Notification.CATEGORY_EVENT)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
        val posted = runCatching {
            manager.notify(UPDATE_NOTIFICATION_ID, builder.build())
        }.isSuccess
        result.success(posted)
    }

    private fun clearUpdateNotification(result: MethodChannel.Result) {
        // Only the update id is cleared; the task progress notification
        // (NOTIFICATION_ID) stays untouched.
        getSystemService(NotificationManager::class.java).cancel(UPDATE_NOTIFICATION_ID)
        result.success(null)
    }

    private fun openUpdateUrl(call: MethodCall, result: MethodChannel.Result) {
        val url = call.argument<String>("url").orEmpty()
        val target = updateDownloadUri(url)
        if (target == null) {
            result.success(false)
            return
        }
        result.success(
            runCatching {
                startActivity(
                    Intent(Intent.ACTION_VIEW, target)
                        .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK),
                )
            }.isSuccess,
        )
    }

    /**
     * Only the pinned Android ARM64 release asset:
     * https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/download/<tag>/DVD-Audio-Maker-<tag>-android-arm64.apk
     * with a numeric tag, no query/fragment/userinfo and no encoded path
     * differences. Everything else is rejected so update actions can never
     * open an arbitrary URL.
     */
    private fun updateDownloadUri(raw: String): Uri? {
        val uri = runCatching { Uri.parse(raw) }.getOrNull() ?: return null
        if (uri.scheme != "https" || uri.authority != "github.com") {
            return null
        }
        if (uri.userInfo != null || uri.port != -1 || uri.query != null || uri.fragment != null) {
            return null
        }
        val path = uri.path ?: return null
        if (uri.encodedPath != path) {
            return null
        }
        val prefix = "/Yuzuriha03/DVD-Audio-Maker/releases/download/"
        if (!path.startsWith(prefix)) {
            return null
        }
        val parts = path.removePrefix(prefix).split('/')
        if (parts.size != 2) {
            return null
        }
        val tag = parts[0]
        if (!isUpdateVersionTag(tag)) {
            return null
        }
        if (parts[1] != "DVD-Audio-Maker-$tag-android-arm64.apk") {
            return null
        }
        return uri
    }

    /** Mirrors the shared release convention: v?X.Y or v?X.Y.Z, numeric only. */
    private fun isUpdateVersionTag(tag: String): Boolean {
        val parts = tag.removePrefix("v").split('.')
        if (parts.size !in 2..3) {
            return false
        }
        return parts.all { part ->
            part.isNotEmpty() &&
                part.all { it in '0'..'9' } &&
                (part.length == 1 || part[0] != '0')
        }
    }

    private fun createUpdateChannel(manager: NotificationManager) {
        val channel = NotificationChannel(
            UPDATE_CHANNEL_ID,
            "App updates",
            NotificationManager.IMPORTANCE_DEFAULT,
        )
        channel.description = "Available application updates"
        manager.createNotificationChannel(channel)
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = getSystemService(NotificationManager::class.java)
        val channel = NotificationChannel(
            NOTIFICATION_CHANNEL_ID,
            "Build progress",
            NotificationManager.IMPORTANCE_LOW,
        ).apply {
            description = "DVD-Audio Maker build progress"
            setShowBadge(false)
        }
        manager.createNotificationChannel(channel)
    }

    private fun updateStatusNotification(payload: NotificationPayload) {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            pendingNotification = payload
            if (!notificationPermissionRequested) {
                notificationPermissionRequested = true
                requestPermissions(
                    arrayOf(Manifest.permission.POST_NOTIFICATIONS),
                    REQUEST_NOTIFICATIONS,
                )
            }
            return
        }
        postStatusNotification(payload)
    }

    @Suppress("DEPRECATION")
    private fun postStatusNotification(payload: NotificationPayload) {
        val manager = getSystemService(NotificationManager::class.java)
        val percent = payload.progress?.coerceIn(0.0, 1.0)?.times(100)?.toInt()
        val indeterminate = percent == null
        val contentIntent = PendingIntent.getActivity(
            this,
            0,
            Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        // Notification channels exist from API 26; older releases use the
        // channel-less builder so minSdk 24 devices keep working.
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(this, NOTIFICATION_CHANNEL_ID)
        } else {
            Notification.Builder(this)
        }
        builder
            .setSmallIcon(R.drawable.ic_stat_dvda)
            .setContentTitle(payload.title.ifEmpty { "DVD-Audio Maker" })
            .setContentText(payload.text)
            .setContentIntent(contentIntent)
            .setOngoing(payload.ongoing)
            .setAutoCancel(!payload.ongoing)
            .setOnlyAlertOnce(true)
            .setCategory(Notification.CATEGORY_PROGRESS)
            .setVisibility(Notification.VISIBILITY_PUBLIC)
            .setProgress(100, percent ?: 0, indeterminate)
        if (Build.VERSION.SDK_INT >= 36) {
            runCatching {
                val style = Notification.ProgressStyle()
                if (indeterminate) style.setProgressIndeterminate(true) else style.setProgress(percent ?: 0)
                builder.setStyle(style)
                builder.setShortCriticalText(payload.text.take(7))
                if (payload.ongoing) builder.setRequestPromotedOngoing(true)
            }
        }
        runCatching { manager.notify(NOTIFICATION_ID, builder.build()) }
    }

    companion object {
        private const val CHANNEL = "dvda/platform"
        private const val NOTIFICATION_CHANNEL_ID = "dvda-build-status"
        private const val NOTIFICATION_ID = 1202
        private const val UPDATE_CHANNEL_ID = "dvda-app-update"
        private const val UPDATE_NOTIFICATION_ID = 1203
        private const val REQUEST_UPDATE_LINK = 43
        private const val REQUEST_NOTIFICATIONS = 16
        private const val DEFAULT_PROFILE_NAME = "dvda-profile.json"
        private const val DEFAULT_TEXT_NAME = "dvda-log.txt"
        private const val DEFAULT_TEXT_MIME = "text/plain"
        private const val EXTERNAL_STORAGE_AUTHORITY = "com.android.externalstorage.documents"
        private const val REQUEST_DIRECTORY = 11
        private const val REQUEST_SAVE_PROFILE = 12
        private const val REQUEST_OPEN_PROFILE = 13
        private const val REQUEST_SAVE_TEXT = 14
        private const val REQUEST_PICK_FONT = 15
        private const val FONT_DIRECTORY = "fonts"
        private const val MAX_FONT_STEM = 64
        private const val SAF_MIRROR_DIRECTORY = "saf-mirror"
        private const val LEGACY_LINK_DIRECTORY = "saf-links"
        private const val TREE_KEY_BYTES = 12
        private const val MAX_SAF_DEPTH = 32
        private const val MAX_SAF_ENTRIES = 4096
        private const val ENTRY_FALLBACK_NAME = "item"
        private const val RUNTIME_DIRECTORY = "runtime"
        private const val FONT_ASSET = "fonts/DvdaNotoCJK-Regular.ttc"
        private const val RUNTIME_FONT_NAME = "DvdaNotoCJK-Regular.ttc"
        private const val COPY_BUFFER_BYTES = 256 * 1024
        private const val MIRROR_SPACE_MARGIN_BYTES = 64L * 1024 * 1024
        private val TRANSACTION_BACKUP_PATTERN = Regex(
            "^(.+)\\.backup-\\d+-[0-9a-f]+-[0-9a-f]+-[0-9a-f]+$",
            RegexOption.IGNORE_CASE,
        )
        private val TRANSACTION_PUBLISHING_PATTERN = Regex(
            "^(.+)\\.publishing-\\d+-[0-9a-f]+-[0-9a-f]+-[0-9a-f]+$",
            RegexOption.IGNORE_CASE,
        )
        private val INTERNAL_CACHE_DIRECTORIES = listOf(
            SAF_MIRROR_DIRECTORY,
            LEGACY_LINK_DIRECTORY,
            "output",
        )
        private val FONT_EXTENSIONS = setOf("ttf", "otf", "ttc")
        private val FONT_MIME_TYPES = arrayOf(
            "font/ttf",
            "font/otf",
            "font/ttc",
            "font/collection",
            "application/x-font-ttf",
            "application/x-font-opentype",
            "application/x-font-truetype",
            "application/vnd.ms-opentype",
            "application/font-sfnt",
            "application/octet-stream",
        )
        private val safExecutor = Executors.newSingleThreadExecutor()
        private val EXPORT_SEQUENCE = AtomicLong(0)
    }
}
