import 'package:material_ui/material_ui.dart';

import 'services/app_controller.dart';
import 'services/update_service.dart';
import 'theme/app_theme.dart';
import 'ui/home_screen.dart';
import 'ui/update_host.dart';

/// Application root: Material 3 light/dark theme and the responsive shell.
class DvdaMakerApp extends StatelessWidget {
  const DvdaMakerApp({super.key, required this.controller, this.updateService});

  final AppController controller;
  final UpdateService? updateService;

  Widget _home() {
    final controller = this.controller;
    final updateService = this.updateService;
    final screen = HomeScreen(
      controller: controller,
      updateService: updateService,
    );
    if (updateService == null) {
      return screen;
    }
    return UpdateHost(
      service: updateService,
      strings: controller.strings,
      child: screen,
    );
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: controller,
      builder: (context, _) => MaterialApp(
        title: 'DVD-Audio Maker',
        debugShowCheckedModeBanner: false,
        theme: AppTheme.light(),
        darkTheme: AppTheme.dark(),
        themeMode: switch (controller.settings.theme) {
          'light' => ThemeMode.light,
          'dark' => ThemeMode.dark,
          _ => ThemeMode.system,
        },
        home: _home(),
      ),
    );
  }
}
