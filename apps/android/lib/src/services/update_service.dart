import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import 'package:flutter/foundation.dart';

import '../../bridge.dart';
import '../localization/app_strings.dart';
import 'platform_service.dart';

/// Returns the fixed release API URL supplied by the shared core.
typedef UpdateApiUrl = String Function();

/// Parses a release payload with the shared Rust rule and compares versions.
///
/// The callback is `AppBridge.selectUpdate` in production; an invalid payload
/// throws and is reported as a failed check instead of being guessed here.
typedef SelectUpdate =
    ({String version, String downloadUrl})? Function(
      String currentVersion,
      String releaseJson,
    );

/// Outcome of one update check.
enum UpdateCheckOutcome { updateAvailable, upToDate, networkFailed, failed }

/// Result of one update check.
@immutable
class UpdateCheckResult {
  const UpdateCheckResult._({
    required this.outcome,
    this.version = '',
    this.downloadUrl = '',
    this.error = '',
  });

  factory UpdateCheckResult.updateAvailable({
    required String version,
    required String downloadUrl,
  }) => UpdateCheckResult._(
    outcome: UpdateCheckOutcome.updateAvailable,
    version: version,
    downloadUrl: downloadUrl,
  );

  factory UpdateCheckResult.upToDate() =>
      const UpdateCheckResult._(outcome: UpdateCheckOutcome.upToDate);

  factory UpdateCheckResult.networkFailed(String error) => UpdateCheckResult._(
    outcome: UpdateCheckOutcome.networkFailed,
    error: error,
  );

  factory UpdateCheckResult.failed(String error) =>
      UpdateCheckResult._(outcome: UpdateCheckOutcome.failed, error: error);

  final UpdateCheckOutcome outcome;
  final String version;
  final String downloadUrl;
  final String error;

  bool get hasUpdate => outcome == UpdateCheckOutcome.updateAvailable;
}

/// Startup/manual Android update check.
///
/// This service is intentionally independent of the production task flow: it
/// only reads the installed version, fetches the fixed HTTPS release API and
/// lets the shared Rust rule decide whether a newer version exists. It never
/// installs anything; the download action opens the release URL in the browser.
class UpdateService extends ChangeNotifier {
  UpdateService({
    required PlatformService platform,
    required UpdateApiUrl updateApiUrl,
    required SelectUpdate selectUpdate,
    HttpClient Function()? clientFactory,
    this.connectTimeout = const Duration(seconds: 10),
    this.totalTimeout = const Duration(seconds: 20),
    this.maxResponseBytes = 1024 * 1024,
  }) : _platform = platform,
       _updateApiUrl = updateApiUrl,
       _selectUpdate = selectUpdate,
       _clientFactory = clientFactory ?? (() => HttpClient());

  /// Production wiring: both callbacks are implemented by the shared bridge.
  factory UpdateService.forCurrentApp(PlatformService platform) {
    return UpdateService(
      platform: platform,
      updateApiUrl: AppBridge.updateReleaseApiUrl,
      selectUpdate: AppBridge.selectUpdate,
    );
  }

  final PlatformService _platform;
  final UpdateApiUrl _updateApiUrl;
  final SelectUpdate _selectUpdate;
  final HttpClient Function() _clientFactory;
  final Duration connectTimeout;
  final Duration totalTimeout;
  final int maxResponseBytes;

  Future<UpdateCheckResult>? _inFlight;
  UpdateCheckResult? _lastResult;
  AppVersion? _appVersion;
  String? _currentVersion;
  bool _isChecking = false;
  String? _postedVersion;
  int _manualEpoch = 0;
  bool _disposed = false;
  String? _noticeVersion;
  String? _noticeDownloadUrl;

  bool get isChecking => _isChecking;
  UpdateCheckResult? get lastResult => _lastResult;
  AppVersion? get appVersion => _appVersion;
  String? get currentVersion => _currentVersion;

  /// True when an available update could not be posted as a system
  /// notification (permission disabled, channel blocked or no plugin) and the
  /// app should show its own clickable notice instead.
  bool get inAppNotice => _noticeVersion != null;
  String? get noticeVersion => _noticeVersion;
  String? get noticeDownloadUrl => _noticeDownloadUrl;

  /// Reads the installed versionName (comparison is done on versionName).
  Future<String?> loadCurrentVersion() async {
    if (_currentVersion != null) {
      return _currentVersion;
    }
    try {
      final version = await _platform.appVersion().timeout(connectTimeout);
      _appVersion = version;
      _currentVersion = version?.version;
    } catch (_) {
      _currentVersion = null;
    }
    return _currentVersion;
  }

  /// Startup check: silent when there is no update or the check fails.
  Future<void> autoCheck(AppStrings strings) async {
    final epoch = _manualEpoch;
    // A manual check already owns the result if it began before this callback.
    if (_disposed || epoch != 0) return;
    final result = await check(manual: false);
    if (_disposed || !result.hasUpdate) {
      return;
    }
    // A manual check reused this request and shows its own dialog; never post
    // the automatic system notification in that case.
    if (_manualEpoch != epoch) {
      return;
    }
    await _notifyAvailableUpdate(result, strings, epoch);
  }

  /// Runs one update check. Concurrent callers share the same in-flight check.
  Future<UpdateCheckResult> check({required bool manual}) {
    if (manual) {
      _manualEpoch++;
      unawaited(_clearUpdateNotification());
    }
    final running = _inFlight;
    if (running != null) {
      return running;
    }
    _isChecking = true;
    final future = _run();
    _inFlight = future;
    _notify();
    future.then<void>(
      (_) {
        if (identical(_inFlight, future)) {
          _inFlight = null;
          _isChecking = false;
          _notify();
        }
      },
      onError: (Object _, StackTrace _) {
        if (identical(_inFlight, future)) {
          _inFlight = null;
          _isChecking = false;
          _notify();
        }
      },
    );
    return future;
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }

  /// Never notifies listeners after the service was disposed (page closed or
  /// app teardown); in-flight checks may finish after that.
  void _notify() {
    if (!_disposed) {
      notifyListeners();
    }
  }

  /// Opens the download URL through the Android ACTION_VIEW adapter.
  Future<bool> openDownload(String url) async {
    try {
      return await _platform.openUpdateUrl(url);
    } catch (_) {
      return false;
    }
  }

  /// Clears the in-app notice after the user dismisses or opens it.
  void dismissInAppNotice() {
    if (_noticeVersion == null) {
      return;
    }
    _noticeVersion = null;
    _noticeDownloadUrl = null;
    _notify();
  }

  Future<UpdateCheckResult> _run() async {
    // The whole check (including the fixed-API/provider callbacks) is guarded:
    // an unexpected exception is still just a silent failed check for callers.
    UpdateCheckResult result;
    try {
      result = await _performCheck();
    } catch (error) {
      result = UpdateCheckResult.failed(error.toString());
    }
    _lastResult = result;
    if (_disposed) {
      return result;
    }
    if (result.outcome == UpdateCheckOutcome.upToDate) {
      // Only a definitive "no update" clears a previously posted notice; a
      // transient network failure stays silent and keeps the existing state.
      await _clearUpdateNotification();
    }
    _notify();
    return result;
  }

  Future<UpdateCheckResult> _performCheck() async {
    final current = await loadCurrentVersion();
    if (current == null || current.trim().isEmpty) {
      return UpdateCheckResult.failed('installed version unavailable');
    }
    final rawApi = _updateApiUrl().trim();
    final apiUri = Uri.tryParse(rawApi);
    if (apiUri == null || apiUri.scheme != 'https' || apiUri.host.isEmpty) {
      return UpdateCheckResult.failed('invalid release API URL');
    }
    HttpClient? client;
    // One overall deadline covers connect, headers and body. The per-step
    // timeouts below all use the remaining budget so a slow drip cannot extend
    // the total check time.
    final elapsed = Stopwatch()..start();
    Duration remaining() {
      final left = totalTimeout - elapsed.elapsed;
      if (left <= Duration.zero) {
        throw TimeoutException('release check timed out');
      }
      return left;
    }

    try {
      client = _clientFactory();
      client.connectionTimeout = connectTimeout;
      final request = await client.getUrl(apiUri).timeout(remaining());
      // A redirect could point at an unexpected host; the API URL is fixed, so
      // any redirect is treated as a failed check instead of followed.
      request.followRedirects = false;
      request.headers.set(
        HttpHeaders.userAgentHeader,
        'DVD-Audio-Maker-Android/${current.trim()}',
      );
      final response = await request.close().timeout(remaining());
      // Strictly 200: redirects are disabled and any other status is an
      // invalid release response, not a network-availability problem.
      if (response.statusCode != HttpStatus.ok) {
        return UpdateCheckResult.failed(
          'release API returned HTTP ${response.statusCode}',
        );
      }
      final declared = response.contentLength;
      if (declared > maxResponseBytes) {
        return UpdateCheckResult.failed('release payload too large');
      }
      final bytes = await _readCapped(response).timeout(remaining());
      final body = utf8.decode(bytes);
      final selected = _selectUpdate(current.trim(), body);
      if (selected == null) {
        return UpdateCheckResult.upToDate();
      }
      final downloadUrl = selected.downloadUrl.trim();
      final downloadUri = Uri.tryParse(downloadUrl);
      if (downloadUri == null ||
          downloadUri.scheme != 'https' ||
          downloadUri.host.isEmpty) {
        return UpdateCheckResult.failed('invalid download URL');
      }
      return UpdateCheckResult.updateAvailable(
        version: selected.version,
        downloadUrl: downloadUrl,
      );
    } on TimeoutException {
      return UpdateCheckResult.networkFailed('release API timed out');
    } on IOException {
      // Socket/TLS/connection failures: reachable-network category.
      return UpdateCheckResult.networkFailed('release API unreachable');
    } catch (error) {
      return UpdateCheckResult.failed(error.toString());
    } finally {
      client?.close(force: true);
    }
  }

  Future<List<int>> _readCapped(HttpClientResponse response) async {
    final builder = BytesBuilder(copy: false);
    await for (final chunk in response) {
      if (builder.length + chunk.length > maxResponseBytes) {
        throw const FormatException('release payload too large');
      }
      builder.add(chunk);
    }
    return builder.takeBytes();
  }

  Future<void> _notifyAvailableUpdate(
    UpdateCheckResult result,
    AppStrings strings,
    int epoch,
  ) async {
    if (_disposed ||
        _manualEpoch != epoch ||
        _postedVersion == result.version) {
      return;
    }
    var posted = false;
    try {
      posted = await _platform.showUpdateNotification(
        title: strings.updateNotificationTitle,
        text: strings.updateNotificationText(result.version),
        downloadUrl: result.downloadUrl,
      );
    } catch (_) {
      posted = false;
    }
    if (_disposed || _manualEpoch != epoch) {
      if (posted) await _clearUpdateNotification();
      return;
    }
    if (posted) {
      _postedVersion = result.version;
      _noticeVersion = null;
      _noticeDownloadUrl = null;
    } else {
      // Notification permission/channel is unavailable (we must not request it
      // here); fall back to a clickable in-app notice.
      _noticeVersion = result.version;
      _noticeDownloadUrl = result.downloadUrl;
    }
    _notify();
  }

  Future<void> _clearUpdateNotification() async {
    _postedVersion = null;
    _noticeVersion = null;
    _noticeDownloadUrl = null;
    try {
      // Always ask the shell to cancel the update id: a notification left by a
      // previous process must not survive a later definitive "up to date".
      await _platform.clearUpdateNotification().timeout(connectTimeout);
    } catch (_) {
      // Clearing is best effort; a later successful check retries it.
    }
  }
}
