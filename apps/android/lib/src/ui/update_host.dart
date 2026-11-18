import 'package:material_ui/material_ui.dart';

import '../localization/app_strings.dart';
import '../services/update_service.dart';

/// Shows the clickable in-app update notice when a system notification could
/// not be posted (notification permission/channel unavailable). It never
/// covers the normal app flow: with no update the child is returned untouched.
class UpdateHost extends StatelessWidget {
  const UpdateHost({
    super.key,
    required this.service,
    required this.strings,
    required this.child,
  });

  final UpdateService service;
  final AppStrings strings;
  final Widget child;

  Future<void> _download() async {
    final url = service.noticeDownloadUrl;
    if (url == null) {
      return;
    }
    final opened = await service.openDownload(url);
    if (opened) {
      service.dismissInAppNotice();
    }
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: service,
      builder: (context, _) {
        final version = service.noticeVersion;
        if (!service.inAppNotice || version == null) {
          return child;
        }
        final scheme = Theme.of(context).colorScheme;
        return Column(
          children: <Widget>[
            SafeArea(
              bottom: false,
              child: Material(
                color: scheme.secondaryContainer,
                child: Padding(
                  padding: const EdgeInsets.fromLTRB(12, 6, 6, 6),
                  child: Row(
                    children: <Widget>[
                      Icon(
                        Icons.system_update,
                        color: scheme.onSecondaryContainer,
                      ),
                      const SizedBox(width: 10),
                      Expanded(
                        child: Text(
                          strings.updateAvailable(version),
                          style: TextStyle(color: scheme.onSecondaryContainer),
                        ),
                      ),
                      TextButton(
                        onPressed: _download,
                        child: Text(strings.updateDownload),
                      ),
                      IconButton(
                        tooltip: strings.cancel,
                        onPressed: service.dismissInAppNotice,
                        icon: const Icon(Icons.close),
                      ),
                    ],
                  ),
                ),
              ),
            ),
            Expanded(child: child),
          ],
        );
      },
    );
  }
}
