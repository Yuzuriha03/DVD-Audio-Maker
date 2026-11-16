import 'package:flutter/services.dart';

/// Failure raised by a platform picker or file action.
class PlatformBridgeException implements Exception {
  PlatformBridgeException(this.message, {this.code = ''});

  final String message;

  /// Native PlatformException code, e.g. "SAF_CANCELLED" for a user cancel.
  final String code;

  @override
  String toString() => message;
}

/// One progress event emitted by the SAF I/O adapters while exporting or while
/// mirroring the ISOs to verify. [kind] is "export" or "verify".
class TaskIoProgress {
  const TaskIoProgress({
    required this.kind,
    required this.done,
    required this.total,
    this.name = '',
  });

  final String kind;
  final int done;
  final int total;
  final String name;
}

/// Outcome of the transactional SAF export.
class ExportResult {
  const ExportResult({
    required this.ok,
    required this.cancelled,
    this.failureCode = '',
    this.failureFile = '',
    this.exported = const <String>[],
    this.freedBytes = 0,
  });

  final bool ok;
  final bool cancelled;
  final String failureCode;
  final String failureFile;
  final List<String> exported;
  final int freedBytes;

  static const ExportResult failed = ExportResult(
    ok: false,
    cancelled: false,
    failureCode: 'EXPORT_FAILED',
  );

  static ExportResult failure(String code, [String file = '']) {
    return ExportResult(
      ok: false,
      cancelled: false,
      failureCode: code,
      failureFile: file,
    );
  }

  static ExportResult fromMap(Object? value) {
    if (value is bool) {
      return ExportResult(ok: value, cancelled: false);
    }
    if (value is! Map) {
      return failed;
    }
    final exported = value['exported'];
    return ExportResult(
      ok: value['ok'] == true,
      cancelled: value['cancelled'] == true,
      failureCode: value['failureCode'] is String
          ? value['failureCode']! as String
          : '',
      failureFile: value['failureFile'] is String
          ? value['failureFile']! as String
          : '',
      exported: exported is List
          ? exported.whereType<String>().toList(growable: false)
          : const <String>[],
      freedBytes: value['freedBytes'] is int ? value['freedBytes']! as int : 0,
    );
  }
}

/// Outcome of the explicit internal-cache cleanup entry.
class CleanupResult {
  const CleanupResult({
    required this.ok,
    this.freedBytes = 0,
    this.deleted = const <String>[],
    this.error = '',
    this.progressDiscarded = false,
  });

  final bool ok;
  final int freedBytes;
  final List<String> deleted;
  final String error;

  /// True when the merged action also removed a real unfinished task. False is
  /// also valid (no progress existed) and must not be reported as a failure.
  final bool progressDiscarded;

  static CleanupResult fromMap(Object? value) {
    if (value is! Map) {
      return const CleanupResult(ok: false, error: 'cleanup unavailable');
    }
    final deleted = value['deleted'];
    return CleanupResult(
      ok: value['ok'] == true,
      freedBytes: value['freedBytes'] is int ? value['freedBytes']! as int : 0,
      deleted: deleted is List
          ? deleted.whereType<String>().toList(growable: false)
          : const <String>[],
      error: value['error'] is String ? value['error']! as String : '',
    );
  }
}

/// A directory granted through the Storage Access Framework.
///
/// [treeUri] is the persistable SAF tree URI, [displayPath] is a human-readable
/// path for the UI only, and [nativePath] is an absolute app-private tree that
/// mirrors the granted directory for Rust readers. [nativePath] is empty unless
/// the picker was asked to build the link tree.
class DirectorySelection {
  const DirectorySelection({
    required this.displayPath,
    required this.nativePath,
    required this.treeUri,
  });

  /// Human-readable path for display only.
  final String displayPath;

  /// Absolute app-private path linked to the SAF tree, or empty when the native
  /// link tree was not requested.
  final String nativePath;

  /// Persistable `content://` tree URI.
  final String treeUri;

  /// Whether [nativePath] can be handed to Rust readers.
  bool get hasNativePath => nativePath.isNotEmpty;

  /// Whether the selection carries no display path.
  bool get isEmpty => displayPath.isEmpty;

  static DirectorySelection? fromMap(Object? value) {
    if (value is! Map<Object?, Object?>) {
      return null;
    }
    final displayPath = value['displayPath'];
    final nativePath = value['nativePath'];
    final treeUri = value['treeUri'];
    if (displayPath is! String || treeUri is! String || treeUri.isEmpty) {
      return null;
    }
    return DirectorySelection(
      displayPath: displayPath,
      nativePath: nativePath is String ? nativePath : '',
      treeUri: treeUri,
    );
  }
}

/// Thin wrapper over `MethodChannel('dvda/platform')`.
///
/// All user files are reached through the Storage Access Framework; no broad
/// storage permission is requested.
///
/// Native contract (owned by the Android shell task):
/// * `pickDirectory` — args `{initial: String?, linkForNative: bool}`,
///   returns a `DirectorySelection` map, or `null` when cancelled or failed.
/// * `linkDirectory` — args `{treeUri: String}`, rebuilds the native link
///   tree after a restart and returns a `DirectorySelection` map, or `null`.
/// * `exportDirectory` — args `{sourcePath: String, treeUri: String,
///   cachePaths: a list of app-private mirrors}`, returns the transactional
///   export result map
///   (`ok`, `cancelled`, `failureCode`, `failureFile`, `exported`); the adapter
///   verifies temporary SAF documents before replacing the visible ISO and
///   releases the task's app-private mirrors after success.
/// * `releaseTaskCaches` — args `{cachePaths: List<String>}`, releases the
///   app-private source mirrors of one operation and returns the released paths.
/// * `cancelExport` / `cancelIsoMirror` — no args, abort the running adapter.
/// * `ensureRuntimeFonts` — no args, installs the APK font asset under
///   app-private runtime storage and returns `{path, installed}`.
/// * `cleanupInternalCaches` — no args, removes known rebuildable caches from
///   older versions and returns `{ok, freedBytes, deleted}`.
/// * `openOutput` — args `{treeUri: String}`, returns `true` when the SAF
///   directory was handed to a viewer.
/// * `openProfile` — no args, returns the picked profile JSON text.
/// * `saveProfile` — args `{content: String, suggestedName: String}`,
///   returns `true`/null when saved.
/// * `saveTextFile` — args `{content: String, suggestedName: String,
///   mimeType: String}`, returns `true` when the text file was written.
/// * `pickFont` — no args, returns the absolute path of a copied
///   `.ttf`/`.otf`/`.ttc` font, or `null` when cancelled or unsupported.
/// * `ensureStorageAccess` — no args, legacy no-op.
///
/// A missing plugin is treated as a no-op so the UI can still be rendered and
/// exercised without the Android shell; result-returning helpers fall back to
/// `null` or `false` instead of throwing.
class PlatformService {
  PlatformService({MethodChannel? channel})
    : _channel = channel ?? const MethodChannel('dvda/platform') {
    try {
      _channel.setMethodCallHandler(_handlePlatformCall);
    } catch (_) {
      // A pure unit test may construct the service before a Flutter binding
      // exists; progress events are optional and the method calls still fail
      // gracefully as MissingPluginException in that environment.
    }
  }

  final MethodChannel _channel;

  /// Receives export/verify progress events pushed by the SAF adapters.
  void Function(TaskIoProgress progress)? onProgress;

  Future<Object?> _handlePlatformCall(MethodCall call) async {
    final handler = onProgress;
    if (handler == null) {
      return null;
    }
    final arguments = call.arguments;
    if (arguments is! Map) {
      return null;
    }
    final done = arguments['done'];
    final total = arguments['total'];
    if (done is! int || total is! int) {
      return null;
    }
    final name = arguments['name'] is String
        ? arguments['name']! as String
        : '';
    if (call.method == 'exportProgress') {
      handler(
        TaskIoProgress(kind: 'export', done: done, total: total, name: name),
      );
    } else if (call.method == 'isoMirrorProgress') {
      handler(
        TaskIoProgress(kind: 'verify', done: done, total: total, name: name),
      );
    }
    return null;
  }

  /// Opens the system directory picker and persists the returned SAF grant.
  ///
  /// Set [linkForNative] when the directory will be read by Rust. The native
  /// side then mirrors the tree under app-private storage and returns that
  /// absolute path in [DirectorySelection.nativePath].
  Future<DirectorySelection?> pickDirectory({
    String initial = '',
    bool linkForNative = false,
  }) async {
    final result = await _invoke<Object?>('pickDirectory', <String, Object?>{
      'initial': initial,
      'linkForNative': linkForNative,
    });
    return DirectorySelection.fromMap(result);
  }

  /// Returns canonical app-private roots supplied by the running Android shell.
  Future<Map<String, String>?> appPaths() async {
    final result = await _invoke<Object?>('appPaths');
    if (result is! Map) {
      return null;
    }
    return <String, String>{
      for (final entry in result.entries)
        if (entry.key is String && entry.value is String)
          entry.key as String: entry.value as String,
    };
  }

  Future<DirectorySelection?> ensureSourceLink(String treeUri) async {
    final result = await _invoke<Object?>('ensureSourceLink', <String, Object?>{
      'treeUri': treeUri,
    });
    return DirectorySelection.fromMap(result);
  }

  /// Checks that [path] is a readable directory inside the current filesDir.
  Future<bool> validatePrivateDirectory(String path) async {
    final result = await _invoke<Object?>(
      'validatePrivateDirectory',
      <String, Object?>{'path': path},
    );
    return result == true;
  }

  /// Rebuilds the native mirror for a persisted SAF directory.
  Future<DirectorySelection?> linkDirectory(String treeUri) async {
    final result = await _invoke<Object?>('linkDirectory', <String, Object?>{
      'treeUri': treeUri,
    });
    return DirectorySelection.fromMap(result);
  }

  /// Copies only the ISOs the shared verification expects into an app-private
  /// mirror so the shared Rust verification can read the exported product.
  ///
  /// [isoPrefix] is the shared ISO prefix derivation (explicit prefix or
  /// derived from the disc title); the native adapter applies the same
  /// find_isos name rule and only copies matching ISOs. Never mirrors the whole
  /// output directory. This is the read-side twin of [exportDirectory].
  Future<DirectorySelection?> linkIsoDirectory(
    String treeUri, {
    String isoPrefix = '',
  }) async {
    final result = await _invoke<Object?>('linkIsoDirectory', <String, Object?>{
      'treeUri': treeUri,
      'isoPrefix': isoPrefix,
    });
    return DirectorySelection.fromMap(result);
  }

  /// Removes the temporary verification mirror for [treeUri].
  Future<bool> releaseIsoDirectory(String treeUri) async {
    final result = await _invoke<Object?>(
      'releaseIsoDirectory',
      <String, Object?>{'treeUri': treeUri},
    );
    return result == true;
  }

  /// Checks a system font or an imported font under the current filesDir.
  Future<bool> validateReadableFont(String path) async {
    final result = await _invoke<Object?>(
      'validateReadableFont',
      <String, Object?>{'path': path},
    );
    return result == true;
  }

  /// Exports the task-local product into a granted SAF tree as a transaction.
  ///
  /// The native adapter writes temporary SAF documents, verifies their stored
  /// length and SHA-256, backs the visible ISO up and only then renames the
  /// verified temporaries into place. A failure or cancellation leaves the
  /// previously exported ISO untouched. [cachePaths] names the app-private SAF
  /// mirrors the adapter releases after a successful export.
  Future<ExportResult> exportDirectory(
    String sourcePath,
    String treeUri, {
    List<String> cachePaths = const <String>[],
  }) async {
    final result = await _invoke<Object?>('exportDirectory', <String, Object?>{
      'sourcePath': sourcePath,
      'treeUri': treeUri,
      'cachePaths': cachePaths,
    });
    return ExportResult.fromMap(result);
  }

  /// Releases app-private SAF source mirrors consumed by one operation.
  ///
  /// Thin wrapper over the same native cache adapter the export path uses; the
  /// caller decides whether the mirror is still needed by a resumable task.
  Future<List<String>> releaseTaskCaches({
    required List<String> cachePaths,
  }) async {
    if (cachePaths.isEmpty) {
      return const <String>[];
    }
    final result = await _invoke<Object?>(
      'releaseTaskCaches',
      <String, Object?>{'cachePaths': cachePaths},
    );
    return result is List
        ? result.whereType<String>().toList(growable: false)
        : const <String>[];
  }

  /// Aborts a running transactional export between copy chunks.
  Future<void> cancelExport() async {
    await _invoke<Object?>('cancelExport');
  }

  /// Aborts a running verification mirror between copied ISOs.
  Future<void> cancelIsoMirror() async {
    await _invoke<Object?>('cancelIsoMirror');
  }

  /// Installs the bundled menu font collection into app-private storage.
  ///
  /// The APK asset is the only font source. Returns the installed absolute path
  /// in "path" when available; a missing asset or failed copy reports "error".
  Future<Map<String, Object?>?> ensureRuntimeFonts() async {
    final result = await _invoke<Object?>('ensureRuntimeFonts');
    if (result is! Map) {
      return null;
    }
    return <String, Object?>{
      for (final entry in result.entries)
        if (entry.key is String) entry.key as String: entry.value,
    };
  }

  /// Removes known rebuildable app-private caches from older versions.
  ///
  /// This is the explicit user-confirmed recovery entry; it never deletes
  /// user-granted SAF folders, the unfinished workspace or exported ISOs, and it
  /// does not replace the shared finish of the current task.
  Future<CleanupResult> cleanupInternalCaches() async {
    final result = await _invoke<Object?>('cleanupInternalCaches');
    return CleanupResult.fromMap(result);
  }

  Future<String?> openProfile() async {
    final result = await _invoke<Object?>('openProfile');
    if (result is String && result.trim().isNotEmpty) {
      return result;
    }
    return null;
  }

  /// Opens the system document picker for a `.ttf`, `.otf` or `.ttc` font.
  ///
  /// The native side copies the selection into the app-private `files/fonts`
  /// directory under a sanitized file name and returns that absolute path, so
  /// the Rust side can read it without SAF permissions. A cancelled picker, an
  /// unsupported file or a missing platform shell returns `null`.
  Future<String?> pickFont() async {
    final result = await _invoke<Object?>('pickFont');
    if (result is String && result.trim().isNotEmpty) {
      return result;
    }
    return null;
  }

  Future<bool> saveProfile(
    String content, {
    String suggestedName = 'dvda-profile.json',
  }) async {
    final result = await _invoke<Object?>('saveProfile', <String, Object?>{
      'content': content,
      'suggestedName': suggestedName,
    });
    return result != false;
  }

  /// Saves [content] as a UTF-8 text file through the system create-document UI.
  ///
  /// Returns `true` only when the file was actually written; a cancelled picker
  /// or a missing platform shell returns `false`.
  Future<bool> saveTextFile(
    String content, {
    String suggestedName = 'dvda-log.txt',
    String mimeType = 'text/plain',
  }) async {
    final result = await _invoke<Object?>('saveTextFile', <String, Object?>{
      'content': content,
      'suggestedName': suggestedName,
      'mimeType': mimeType,
    });
    return result == true;
  }

  /// Opens the granted SAF [treeUri] with the system directory viewer.
  ///
  /// Returns `true` only when the platform handed the SAF directory to a
  /// viewer; an invalid URI, an unavailable viewer or a missing platform shell
  /// returns `false`.
  Future<bool> openOutput(String treeUri) async {
    final result = await _invoke<Object?>('openOutput', <String, Object?>{
      'treeUri': treeUri,
    });
    return result == true;
  }

  /// Legacy entry point kept for existing callers.
  ///
  /// SAF needs no broad storage permission, so the Android shell treats this as
  /// a no-op.
  Future<void> ensureStorageAccess() async {
    await _invoke<Object?>('ensureStorageAccess');
  }

  /// Publishes the ongoing task progress in the status bar and, on
  /// Android 16+, as a promoted ongoing Live Update.
  Future<void> updateStatusNotification({
    required String title,
    required String text,
    double? progress,
    required bool ongoing,
  }) async {
    await _invoke<Object?>('updateStatusNotification', <String, Object?>{
      'title': title,
      'text': text,
      'progress': progress,
      'ongoing': ongoing,
    });
  }

  /// Removes the task notification.
  Future<void> clearStatusNotification() async {
    await _invoke<Object?>('clearStatusNotification');
  }

  Future<T?> _invoke<T>(
    String method, [
    Map<String, Object?>? arguments,
  ]) async {
    try {
      return await _channel.invokeMethod<T>(method, arguments);
    } on MissingPluginException {
      return null;
    } on PlatformException catch (error) {
      throw PlatformBridgeException(
        error.message ?? error.code,
        code: error.code,
      );
    }
  }
}
