import 'package:material_ui/material_ui.dart';

import 'src/app.dart';
import 'src/models/app_settings.dart';
import 'src/services/app_controller.dart';
import 'src/services/platform_service.dart';
import 'src/services/settings_store.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  final store = await SettingsStore.open();
  AppSettings settings;
  try {
    settings = store.load();
  } catch (_) {
    settings = AppSettings();
  }
  final controller = AppController(
    settings: settings,
    store: store,
    platform: PlatformService(),
  );
  await controller.initialize();
  runApp(DvdaMakerApp(controller: controller));
}
