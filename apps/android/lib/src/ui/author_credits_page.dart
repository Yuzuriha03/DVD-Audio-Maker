import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../localization/app_strings.dart';
import '../services/platform_service.dart';
import '../services/update_service.dart';

/// Author, project and offline license information bundled with the app.
///
/// The manual check shows its result in a dialog, with a download action only
/// for a newer release.
class AuthorCreditsPage extends StatefulWidget {
  const AuthorCreditsPage({
    super.key,
    required this.strings,
    this.updateService,
  });

  final AppStrings strings;
  final UpdateService? updateService;

  static const String projectUrl =
      'https://github.com/Yuzuriha03/DVD-Audio-Maker';

  static const String ffmpegAsset = 'assets/licenses/FFMPEG-LICENSE.txt';

  static const List<String> licenseAssets = <String>[
    'assets/licenses/LICENSE.txt',
    'assets/licenses/THIRD-PARTY.txt',
    'assets/licenses/THIRD-PARTY.en.txt',
    'assets/licenses/NOTICE-Image.txt',
    'assets/licenses/NOTICE-Menu.txt',
  ];

  @override
  State<AuthorCreditsPage> createState() => _AuthorCreditsPageState();
}

class _AuthorCreditsPageState extends State<AuthorCreditsPage> {
  AppVersion? _appVersion;
  bool _checking = false;

  @override
  void initState() {
    super.initState();
    final service = widget.updateService;
    if (service == null) {
      return;
    }
    service.loadCurrentVersion().then((_) {
      if (!mounted) {
        return;
      }
      setState(() => _appVersion = service.appVersion);
    });
  }

  void _openLicense(String asset) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (context) => LicenseTextPage(asset: asset),
      ),
    );
  }

  Future<void> _check() async {
    final service = widget.updateService;
    if (service == null || _checking) {
      return;
    }
    setState(() => _checking = true);
    final result = await service.check(manual: true);
    if (!mounted) {
      return;
    }
    // Manual checks always report through a dialog and never post a system
    // notification, Toast or SnackBar. The automatic path stays silent unless
    // a downloadable new version exists.
    setState(() => _checking = false);
    await _showResultDialog(result);
  }

  Future<void> _showResultDialog(UpdateCheckResult result) async {
    final strings = widget.strings;
    switch (result.outcome) {
      case UpdateCheckOutcome.updateAvailable:
        await showDialog<void>(
          context: context,
          builder: (dialogContext) => AlertDialog(
            title: Text(strings.updateNotificationTitle),
            content: Text(strings.updateAvailable(result.version)),
            actions: <Widget>[
              TextButton(
                onPressed: () => Navigator.of(dialogContext).pop(),
                child: Text(strings.updateClose),
              ),
              FilledButton(
                onPressed: () async {
                  Navigator.of(dialogContext).pop();
                  final service = widget.updateService;
                  if (service != null) {
                    await service.openDownload(result.downloadUrl);
                  }
                },
                child: Text(strings.updateDownload),
              ),
            ],
          ),
        );
        return;
      case UpdateCheckOutcome.upToDate:
        final version =
            widget.updateService?.currentVersion ?? _versionLabel ?? '';
        await _infoDialog(strings.updateCheck, strings.updateUpToDate(version));
        return;
      case UpdateCheckOutcome.networkFailed:
        await _infoDialog(strings.updateCheck, strings.updateNetworkFailed);
        return;
      case UpdateCheckOutcome.failed:
        await _infoDialog(strings.updateCheck, strings.updateCheckFailed);
        return;
    }
  }

  Future<void> _infoDialog(String title, String message) {
    return showDialog<void>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(title),
        content: Text(message),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(),
            child: Text(widget.strings.updateClose),
          ),
        ],
      ),
    );
  }

  String? get _versionLabel {
    final version = _appVersion;
    if (version == null) {
      return null;
    }
    final build = version.buildNumber;
    return build > 0 ? '${version.version} ($build)' : version.version;
  }

  @override
  Widget build(BuildContext context) {
    final strings = widget.strings;
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final service = widget.updateService;
    final versionLabel = _versionLabel;
    return Scaffold(
      appBar: AppBar(title: Text(strings.authorCredits)),
      body: ListView(
        padding: const EdgeInsets.fromLTRB(16, 16, 16, 28),
        children: <Widget>[
          Card(
            child: Padding(
              padding: const EdgeInsets.all(20),
              child: Column(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: <Widget>[
                  Row(
                    children: <Widget>[
                      CircleAvatar(
                        radius: 28,
                        backgroundColor: scheme.primaryContainer,
                        foregroundColor: scheme.onPrimaryContainer,
                        child: const Icon(Icons.person_outline, size: 30),
                      ),
                      const SizedBox(width: 16),
                      Expanded(
                        child: Column(
                          crossAxisAlignment: CrossAxisAlignment.start,
                          children: <Widget>[
                            Text(
                              'Yuzuriha03',
                              style: theme.textTheme.headlineSmall?.copyWith(
                                fontWeight: FontWeight.w800,
                              ),
                            ),
                            const SizedBox(height: 4),
                            Text(
                              strings.authorRole,
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: scheme.onSurfaceVariant,
                              ),
                            ),
                          ],
                        ),
                      ),
                    ],
                  ),
                  const SizedBox(height: 16),
                  Text(strings.authorBio, style: theme.textTheme.bodyLarge),
                ],
              ),
            ),
          ),
          const SizedBox(height: 12),
          Card(
            child: ListTile(
              leading: const Icon(Icons.code),
              title: Text(strings.projectHome),
              subtitle: const SelectableText(AuthorCreditsPage.projectUrl),
            ),
          ),
          const SizedBox(height: 12),
          Card(
            child: Column(
              children: <Widget>[
                ListTile(
                  leading: const Icon(Icons.info_outline),
                  title: Text(strings.appInfo),
                  subtitle: Text(
                    versionLabel == null
                        ? 'DVD-Audio Maker'
                        : 'DVD-Audio Maker · v$versionLabel',
                  ),
                ),
                const Divider(height: 1),
                ListTile(
                  leading: const Icon(Icons.description_outlined),
                  title: Text(strings.flutterLicenses),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => showLicensePage(
                    context: context,
                    applicationName: strings.appTitle,
                    applicationVersion: versionLabel,
                  ),
                ),
                const Divider(height: 1),
                ListTile(
                  leading: const Icon(Icons.movie_outlined),
                  title: Text(strings.ffmpegLicenses),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _openLicense(AuthorCreditsPage.ffmpegAsset),
                ),
              ],
            ),
          ),
          if (service != null) ...<Widget>[
            const SizedBox(height: 12),
            Card(
              child: Column(
                children: <Widget>[
                  ListTile(
                    leading: const Icon(Icons.system_update),
                    title: Text(strings.updateCheck),
                    subtitle: versionLabel == null
                        ? null
                        : Text(strings.updateCurrentVersion(versionLabel)),
                    trailing: _checking
                        ? const SizedBox(
                            width: 20,
                            height: 20,
                            child: CircularProgressIndicator(strokeWidth: 2),
                          )
                        : IconButton(
                            tooltip: strings.updateCheck,
                            onPressed: _check,
                            icon: const Icon(Icons.refresh),
                          ),
                  ),
                ],
              ),
            ),
          ],
          const SizedBox(height: 12),
          Card(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: <Widget>[
                Padding(
                  padding: const EdgeInsets.fromLTRB(16, 16, 16, 4),
                  child: Text(
                    strings.bundledLicenses,
                    style: theme.textTheme.titleMedium?.copyWith(
                      fontWeight: FontWeight.w700,
                    ),
                  ),
                ),
                for (final asset in AuthorCreditsPage.licenseAssets)
                  ListTile(
                    leading: const Icon(Icons.article_outlined),
                    title: Text(asset.split('/').last),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () => _openLicense(asset),
                  ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}

class LicenseTextPage extends StatelessWidget {
  const LicenseTextPage({super.key, required this.asset});

  final String asset;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    return Scaffold(
      appBar: AppBar(title: Text(asset.split('/').last)),
      body: FutureBuilder<String>(
        future: rootBundle.loadString(asset),
        builder: (context, snapshot) {
          if (snapshot.hasError) {
            return Center(child: Text(snapshot.error.toString()));
          }
          final text = snapshot.data;
          if (text == null) {
            return const Center(child: CircularProgressIndicator());
          }
          return SelectionArea(
            child: SingleChildScrollView(
              padding: const EdgeInsets.all(18),
              child: Text(
                text,
                style: theme.textTheme.bodySmall?.copyWith(
                  height: 1.5,
                  fontFamily: 'monospace',
                ),
              ),
            ),
          );
        },
      ),
    );
  }
}
