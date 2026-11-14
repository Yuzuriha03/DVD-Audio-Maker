import 'package:flutter/services.dart';

/// Failure raised by a platform picker or file action.
class PlatformBridgeException implements Exception {
  PlatformBridgeException(this.message);

  final String message;

  @override
  String toString() => message;
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
/// * `exportDirectory` — args `{sourcePath: String, treeUri: String}`,
///   returns `true` when every entry was exported.
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
    : _channel = channel ?? const MethodChannel('dvda/platform');

  final MethodChannel _channel;

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

  /// Rebuilds the native link tree for a persisted SAF directory.
  Future<DirectorySelection?> linkDirectory(String treeUri) async {
    final result = await _invoke<Object?>('linkDirectory', <String, Object?>{
      'treeUri': treeUri,
    });
    return DirectorySelection.fromMap(result);
  }

  /// Recursively exports an app-private directory into a granted SAF tree.
  Future<bool> exportDirectory(String sourcePath, String treeUri) async {
    final result = await _invoke<Object?>('exportDirectory', <String, Object?>{
      'sourcePath': sourcePath,
      'treeUri': treeUri,
    });
    return result == true;
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

  Future<T?> _invoke<T>(
    String method, [
    Map<String, Object?>? arguments,
  ]) async {
    try {
      return await _channel.invokeMethod<T>(method, arguments);
    } on MissingPluginException {
      return null;
    } on PlatformException catch (error) {
      throw PlatformBridgeException(error.message ?? error.code);
    }
  }
}
