import 'dart:async';
import 'dart:io';

import 'package:dvda_maker/src/localization/app_strings.dart';
import 'package:dvda_maker/src/services/platform_service.dart';
import 'package:dvda_maker/src/services/update_service.dart';
import 'package:flutter_test/flutter_test.dart';

const _url =
    'https://github.com/Yuzuriha03/DVD-Audio-Maker/releases/download/'
    'v1.3/DVD-Audio-Maker-v1.3-android-arm64.apk';

class _Platform extends PlatformService {
  _Platform({this.allowNotifications = true});

  final bool allowNotifications;
  final posted = <String>[];

  @override
  Future<AppVersion?> appVersion() async =>
      const AppVersion(version: '1.2.0', buildNumber: 2);

  @override
  Future<bool> showUpdateNotification({
    required String title,
    required String text,
    required String downloadUrl,
  }) async {
    posted.add(downloadUrl);
    return allowNotifications;
  }

  @override
  Future<void> clearUpdateNotification() async {}
}

class _Headers implements HttpHeaders {
  @override
  void set(String name, Object value, {bool preserveHeaderCase = false}) {}

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _Response extends Stream<List<int>> implements HttpClientResponse {
  _Response(this.statusCode, this.data);

  @override
  final int statusCode;
  final List<int> data;

  @override
  int get contentLength => -1;

  @override
  StreamSubscription<List<int>> listen(
    void Function(List<int>)? onData, {
    Function? onError,
    void Function()? onDone,
    bool? cancelOnError,
  }) => Stream.value(data).listen(
    onData,
    onError: onError,
    onDone: onDone,
    cancelOnError: cancelOnError,
  );

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _Request implements HttpClientRequest {
  _Request(this.response);
  final Future<HttpClientResponse> Function() response;

  @override
  final headers = _Headers();

  @override
  bool followRedirects = true;

  @override
  Future<HttpClientResponse> close() => response();

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

class _Client implements HttpClient {
  _Client(this.request);
  final _Request request;
  bool closed = false;
  int calls = 0;

  @override
  Duration? connectionTimeout;

  @override
  Future<HttpClientRequest> getUrl(Uri url) async {
    expect(url.scheme, 'https');
    calls++;
    return request;
  }

  @override
  void close({bool force = false}) => closed = true;

  @override
  dynamic noSuchMethod(Invocation invocation) => super.noSuchMethod(invocation);
}

UpdateService _service(
  _Platform platform,
  _Client client, {
  bool newer = false,
}) {
  return UpdateService(
    platform: platform,
    updateApiUrl: () => 'https://api.github.com/test',
    selectUpdate: (version, body) {
      expect(version, '1.2.0');
      if (body != 'fixture') throw const FormatException('invalid fixture');
      return newer ? (version: 'v1.3', downloadUrl: _url) : null;
    },
    clientFactory: () => client,
    maxResponseBytes: 24,
  );
}

void main() {
  const strings = AppStrings.english;

  test('automatic checks stay silent for all non-update results', () async {
    final fixtures =
        <(Future<HttpClientResponse> Function(), UpdateCheckOutcome)>[
          (
            () async => _Response(200, 'fixture'.codeUnits),
            UpdateCheckOutcome.upToDate,
          ),
          (
            () async => throw const SocketException('offline'),
            UpdateCheckOutcome.networkFailed,
          ),
          (() async => _Response(302, []), UpdateCheckOutcome.failed),
          (
            () async => _Response(200, 'invalid'.codeUnits),
            UpdateCheckOutcome.failed,
          ),
          (
            () async => _Response(200, List.filled(25, 0)),
            UpdateCheckOutcome.failed,
          ),
        ];
    for (final (response, expected) in fixtures) {
      final platform = _Platform();
      final client = _Client(_Request(response));
      final service = _service(platform, client);
      await service.autoCheck(strings);
      expect(service.lastResult?.outcome, expected);
      expect(platform.posted, isEmpty);
      expect(service.inAppNotice, isFalse);
      expect(client.request.followRedirects, isFalse);
      expect(client.closed, isTrue);
      service.dispose();
    }
  });

  test(
    'automatic update uses the platform notification or an in-app fallback',
    () async {
      for (final allowed in [true, false]) {
        final platform = _Platform(allowNotifications: allowed);
        final client = _Client(
          _Request(() async => _Response(200, 'fixture'.codeUnits)),
        );
        final service = _service(platform, client, newer: true);
        await service.autoCheck(strings);
        expect(platform.posted, [_url]);
        expect(service.inAppNotice, !allowed);
        expect(service.noticeDownloadUrl, allowed ? null : _url);
        service.dispose();
      }
    },
  );

  test(
    'manual check joins an in-flight startup check without an automatic notice',
    () async {
      final platform = _Platform();
      final response = Completer<HttpClientResponse>();
      final client = _Client(_Request(() => response.future));
      final service = _service(platform, client, newer: true);
      final automatic = service.autoCheck(strings);
      final manual = service.check(manual: true);
      response.complete(_Response(200, 'fixture'.codeUnits));
      expect((await manual).downloadUrl, _url);
      await automatic;
      expect(client.calls, 1);
      expect(platform.posted, isEmpty);
      expect(service.inAppNotice, isFalse);
      service.dispose();
    },
  );

  test(
    'completion after dispose does not post a notice or notify listeners',
    () async {
      final platform = _Platform();
      final response = Completer<HttpClientResponse>();
      final client = _Client(_Request(() => response.future));
      final service = _service(platform, client, newer: true);
      var changes = 0;
      service.addListener(() => changes++);
      final automatic = service.autoCheck(strings);
      service.dispose();
      final before = changes;
      response.complete(_Response(200, 'fixture'.codeUnits));
      await automatic;
      expect(changes, before);
      expect(platform.posted, isEmpty);
    },
  );
}
