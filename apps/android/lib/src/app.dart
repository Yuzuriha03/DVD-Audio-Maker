import 'package:material_ui/material_ui.dart';

import 'services/app_controller.dart';
import 'theme/app_theme.dart';
import 'ui/home_screen.dart';

/// Application root: Material 3 light/dark theme and the responsive shell.
class DvdaMakerApp extends StatelessWidget {
  const DvdaMakerApp({super.key, required this.controller});

  final AppController controller;

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
        home: HomeScreen(controller: controller),
      ),
    );
  }
}
