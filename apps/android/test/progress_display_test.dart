import 'dart:async';
import 'dart:convert';

import 'package:dvda_maker/bridge.dart';
import 'package:dvda_maker/src/models/app_settings.dart';
import 'package:dvda_maker/src/models/profile_catalog.dart';
import 'package:dvda_maker/src/services/app_controller.dart';
import 'package:dvda_maker/src/services/platform_service.dart';
import 'package:dvda_maker/src/services/settings_store.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'profile_catalog_fixture.dart';

class _TrackingPlatform extends PlatformService {
  int statusNotifications = 0;

  @override
  bool get supportsSafDescriptors => false;

  @override
  Future<bool> validatePrivateDirectory(String path) async => true;

  @override
  Future<Map<String, Object?>?> ensureRuntimeFonts() async => <String, Object?>{
    'path':
        '/data/user/0/com.dvda.maker/files/runtime/fonts/DvdaNotoCJK-Regular.ttc',
    'installed': false,
  };

  @override
  Future<void> updateStatusNotification({
    required String title,
    required String text,
    double? progress,
    required bool ongoing,
  }) async {
    statusNotifications++;
  }

  @override
  Future<void> clearStatusNotification() async {}
}

Future<AppController> _controller() async {
  SharedPreferences.setMockInitialValues(<String, Object>{});
  return AppController(
    settings: AppSettings(),
    store: await SettingsStore.open(),
    platform: PlatformService(),
  );
}

void main() {
  setUpAll(() => ProfileCatalog.install(testCatalog()));

  group('progress label never exposes raw wire JSON', () {
    test('raw and truncated structured events are dropped', () {
      expect(
        AppController.sanitizeProgressText(
          '{"Action":"disc_plan","Current":2,"Total":149}',
        ),
        '',
      );
      expect(
        AppController.sanitizeProgressText(
          '[TASK] {"Action":"disc_plan","Current":2,"Total":149,"War',
        ),
        '',
      );
      expect(AppController.sanitizeProgressText('  '), '');
    });

    test('localized readable text is preserved', () {
      expect(
        AppController.sanitizeProgressText('打包 ISO，光盘： 2/149'),
        '打包 ISO，光盘： 2/149',
      );
    });
  });

  group('progress is shown as a percentage on the shared 0-100 scale', () {
    test('maps counters to integer percent', () async {
      final controller = await _controller();
      addTearDown(controller.dispose);
      controller.applyProgressForTest(done: 65, total: 100);
      expect(controller.progressPercent, 65);
      controller.applyProgressForTest(done: 0, total: 100);
      expect(controller.progressPercent, 0);
      controller.applyProgressForTest(done: 100, total: 100);
      expect(controller.progressPercent, 100);
      controller.applyProgressForTest(done: 200, total: 100);
      expect(controller.progressPercent, 100);
    });

    test('unknown progress has no percentage', () async {
      final controller = await _controller();
      addTearDown(controller.dispose);
      controller.applyProgressForTest(done: 0, total: 0);
      expect(controller.progressPercent, isNull);
      controller.applyProgressForTest(done: 1, total: 1);
      expect(controller.progressPercent, isNull);
    });
  });

  group('150ms incremental polling avoids duplicate work and log corruption', () {
    test(
      'passes lastRevision, replaces snapshot cleanly, caches line formatting, and deduplicates notifications',
      () async {
        SharedPreferences.setMockInitialValues(<String, Object>{});
        final platform = _TrackingPlatform();
        final seenRevisions = <int>[];
        var lineFormatCalls = 0;
        var logFormatCalls = 0;
        var step = 0;
        final runBlocker = Completer<String>();

        final settings = AppSettings()
          ..appFilesDir = '/data/user/0/com.dvda.maker/files'
          ..appWorkDir = '/data/user/0/com.dvda.maker/files/build'
          ..sourceStagingPath =
              '/data/user/0/com.dvda.maker/files/saf-mirror/src'
          ..finalTreeUri = 'content://output/tree';

        final controller = AppController(
          settings: settings,
          store: await SettingsStore.open(),
          platform: platform,
          bridgeValidateProfile: (_, _) => const <ProfileIssueView>[],
          bridgeRunProfile: (_, _) => runBlocker.future,
          bridgeProgress: ({required int lastRevision}) {
            seenRevisions.add(lastRevision);
            step++;
            if (step == 1) {
              return const BridgeProgress(
                done: 20,
                total: 100,
                text: 'raw-step-1',
                log: 'line 1\nline 2',
                logRevision: 5,
              );
            }
            if (step == 2) {
              // Unchanged log revision and unchanged text: empty log delta, no re-format!
              return const BridgeProgress(
                done: 20,
                total: 100,
                text: 'raw-step-1',
                log: '',
                logRevision: 5,
              );
            }
            // Updated log revision and changed progress text, same 20% overall progress:
            return const BridgeProgress(
              done: 20,
              total: 100,
              text: 'raw-step-2',
              log: 'line 1\nline 2\nline 3',
              logRevision: 6,
            );
          },
          lineFormatter: (_, raw) {
            lineFormatCalls++;
            return 'formatted:$raw';
          },
          logFormatter: (_, log) {
            logFormatCalls++;
            return FormattedLogView(text: log, problemText: '', problems: 0);
          },
        );
        addTearDown(controller.dispose);

        final running = controller.runAction('build');
        // Wait across three 150ms poll ticks (~520ms)
        await Future<void>.delayed(const Duration(milliseconds: 520));

        expect(seenRevisions.take(3).toList(), <int>[0, 5, 5]);
        // Line formatter must only run for distinct raw lines ('raw-step-1' and 'raw-step-2'), not on step 2!
        expect(lineFormatCalls, 2);
        // Snapshot replacement must never duplicate earlier lines!
        expect(controller.logText, 'line 1\nline 2\nline 3');
        expect(logFormatCalls, 1);
        // Notification text ('Start build 20%') did not change between step 1 and step 3:
        // only initial 0% -> 20% should invoke updateStatusNotification!
        expect(platform.statusNotifications, 2);

        runBlocker.complete(
          jsonEncode(<String, Object?>{
            'Ok': true,
            'Cancelled': false,
            'Message': 'Done',
            'Diagnostics': const <Object?>[],
            'Log': 'line 1\nline 2\nline 3',
            'ProgressDone': 100,
            'ProgressTotal': 100,
          }),
        );
        await running;
      },
    );
  });
}
