import 'package:dvda_maker/src/models/app_settings.dart';
import 'package:dvda_maker/src/models/profile_catalog.dart';
import 'package:dvda_maker/src/services/app_controller.dart';
import 'package:dvda_maker/src/services/platform_service.dart';
import 'package:dvda_maker/src/services/settings_store.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:shared_preferences/shared_preferences.dart';

import 'profile_catalog_fixture.dart';

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
}
