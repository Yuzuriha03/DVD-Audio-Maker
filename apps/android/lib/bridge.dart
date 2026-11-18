import 'package:flutter/foundation.dart';
import 'src/rust/api.dart' as rust_api;
import 'src/rust/frb_generated.dart';

/// One profile validation problem returned by the shared Rust rules.
class ProfileIssueView {
  const ProfileIssueView({required this.key, required this.code});

  final String key;
  final String code;
}

class BridgeProgress {
  const BridgeProgress({
    required this.done,
    required this.total,
    required this.text,
    required this.log,
  });

  final int done;
  final int total;
  final String text;
  final String log;
}

abstract final class AppBridge {
  static Future<void>? _initialization;

  /// Initializes flutter_rust_bridge exactly once, sharing the in-flight Future.
  ///
  /// The profile catalog needs the Rust library before the settings form
  /// exists, while the controller also initializes the bridge; both await the
  /// same Future so concurrent callers cannot initialize twice. A failed
  /// attempt clears the cache so a later call can retry.
  static Future<void> init() {
    final existing = _initialization;
    if (existing != null) {
      return existing;
    }
    final future = RustLib.init();
    _initialization = future;
    // Clear the cache on failure without swallowing the error for callers.
    future.then<void>(
      (_) {},
      onError: (Object _, StackTrace _) {
        if (identical(_initialization, future)) {
          _initialization = null;
        }
      },
    );
    return future;
  }

  /// Test hook: forget the cached initialization between fake-async zones.
  @visibleForTesting
  static void resetInitializationForTests() {
    _initialization = null;
  }

  static Future<String> runProfile(String profileJson, String action) {
    return rust_api.runProfile(profileJson: profileJson, action: action);
  }

  static BridgeProgress progress() {
    final value = rust_api.progressSnapshot();
    return BridgeProgress(
      done: value.done.toInt(),
      total: value.total.toInt(),
      text: value.text,
      log: value.log,
    );
  }

  static void cancel() => rust_api.cancel();

  /// Schema defaults and allowed values from `dvda_core::profile`.
  static String profileOptions() => rust_api.profileOptions();

  /// Shared disc-capacity rule.
  static int profileDiscBytes({
    required int capacity,
    required String customBytes,
  }) => rust_api.profileDiscBytes(capacity: capacity, customBytes: customBytes);

  /// Shared profile validation; LPCM 20-bit is reported, never rewritten.
  static List<ProfileIssueView> profileValidate({
    required String valuesJson,
    required bool forTask,
  }) => rust_api
      .profileValidate(valuesJson: valuesJson, forTask: forTask)
      .map((issue) => ProfileIssueView(key: issue.key, code: issue.code))
      .toList();

  /// Localized text for a profile validation issue code.
  static String profileIssueMessage(String language, String code) =>
      rust_api.profileIssueMessage(language: language, code: code);

  /// Endpoint and release selection are shared with the Windows host.
  static String updateReleaseApiUrl() => rust_api.updateReleaseApiUrl();

  static ({String version, String downloadUrl})? selectUpdate(
    String currentVersion,
    String releaseJson,
  ) {
    final updates = rust_api.selectUpdate(
      currentVersion: currentVersion,
      releaseJson: releaseJson,
    );
    if (updates.isEmpty) return null;
    final update = updates.single;
    return (version: update.version, downloadUrl: update.downloadUrl);
  }

  /// Formats a whole task log with the shared Rust log rules.
  static rust_api.FormattedLog formatLog(String language, String log) =>
      rust_api.formatLog(language: language, log: log);
}
