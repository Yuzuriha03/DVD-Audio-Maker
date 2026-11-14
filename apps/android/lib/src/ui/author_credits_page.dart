import 'package:flutter/services.dart';
import 'package:material_ui/material_ui.dart';

import '../localization/app_strings.dart';

/// Author, project and offline license information bundled with the app.
class AuthorCreditsPage extends StatelessWidget {
  const AuthorCreditsPage({super.key, required this.strings});

  final AppStrings strings;

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

  void _openLicense(BuildContext context, String asset) {
    Navigator.of(context).push(
      MaterialPageRoute<void>(
        builder: (context) => LicenseTextPage(asset: asset),
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
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
              subtitle: const SelectableText(projectUrl),
            ),
          ),
          const SizedBox(height: 12),
          Card(
            child: Column(
              children: <Widget>[
                ListTile(
                  leading: const Icon(Icons.info_outline),
                  title: Text(strings.appInfo),
                  subtitle: const Text('DVD-Audio Maker · v1.2.0 (2)'),
                ),
                const Divider(height: 1),
                ListTile(
                  leading: const Icon(Icons.description_outlined),
                  title: Text(strings.flutterLicenses),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => showLicensePage(
                    context: context,
                    applicationName: strings.appTitle,
                    applicationVersion: '1.2.0 (2)',
                  ),
                ),
                const Divider(height: 1),
                ListTile(
                  leading: const Icon(Icons.movie_outlined),
                  title: Text(strings.ffmpegLicenses),
                  trailing: const Icon(Icons.chevron_right),
                  onTap: () => _openLicense(context, ffmpegAsset),
                ),
              ],
            ),
          ),
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
                for (final asset in licenseAssets)
                  ListTile(
                    leading: const Icon(Icons.article_outlined),
                    title: Text(asset.split('/').last),
                    trailing: const Icon(Icons.chevron_right),
                    onTap: () => _openLicense(context, asset),
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
