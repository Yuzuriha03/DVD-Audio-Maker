import 'package:material_ui/material_ui.dart';

import 'bridge.dart';

import 'src/app.dart';
import 'src/models/app_settings.dart';
import 'src/models/profile_catalog.dart';
import 'src/services/app_controller.dart';
import 'src/services/platform_service.dart';
import 'src/services/settings_store.dart';

Future<void> main() async {
  WidgetsFlutterBinding.ensureInitialized();
  String? catalogError;
  try {
    // The shared profile schema comes from the Rust library, so load the
    // bridge before the settings form is created.
    await AppBridge.init();
    ProfileCatalog.install(ProfileCatalog.load());
  } catch (error) {
    catalogError = error.toString();
  }
  final store = await SettingsStore.open();
  if (catalogError != null) {
    runApp(_BootstrapFailureApp(error: catalogError));
    return;
  }
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

/// Shown when the shared Rust profile catalog cannot be loaded.
class _BootstrapFailureApp extends StatelessWidget {
  const _BootstrapFailureApp({required this.error});

  final String error;

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      home: Scaffold(
        body: Center(
          child: Padding(
            padding: const EdgeInsets.all(24),
            child: Text('Cannot load the shared profile schema:\n$error'),
          ),
        ),
      ),
    );
  }
}
