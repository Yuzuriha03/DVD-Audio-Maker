import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

import '../../services/app_controller.dart';
import '../../services/platform_service.dart';
import '../formatting.dart';
import '../widgets/common.dart';

/// Disc, folder and advanced profile settings.
class BasicsSection extends StatelessWidget {
  const BasicsSection({super.key, required this.controller});

  final AppController controller;

  Future<void> _pickDirectory(
    BuildContext context,
    String current,
    ValueChanged<DirectorySelection> apply, {
    bool linkForNative = true,
  }) async {
    try {
      final picked = await controller.pickDirectory(
        current,
        linkForNative: linkForNative,
      );
      if (picked == null || picked.isEmpty) {
        return;
      }
      apply(picked);
    } catch (error) {
      if (!context.mounted) {
        return;
      }
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(error.toString())));
    }
  }

  @override
  Widget build(BuildContext context) {
    final strings = controller.strings;
    final settings = controller.settings;
    final theme = Theme.of(context);
    final hintStyle = theme.textTheme.bodySmall?.copyWith(
      color: theme.colorScheme.onSurfaceVariant,
    );
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        SectionCard(
          title: strings.sectionStorage,
          subtitle: strings.setupSubtitle,
          icon: Icons.album,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              BrowseField(
                label: strings.sourceDir,
                value: settings.source,
                browseTooltip: strings.browse,
                onChanged: (value) =>
                    controller.update(() => settings.source = value),
                onBrowse: () => _pickDirectory(
                  context,
                  settings.source,
                  (selection) => controller.update(() {
                    settings.source = selection.displayPath;
                    settings.sourceStagingPath = selection.nativePath;
                    settings.sourceTreeUri = selection.treeUri;
                  }),
                ),
              ),
              const SizedBox(height: 14),
              BrowseField(
                label: strings.finalDir,
                value: settings.finalDir,
                browseTooltip: strings.browse,
                onChanged: (value) =>
                    controller.update(() => settings.finalDir = value),
                onBrowse: () => _pickDirectory(
                  context,
                  settings.finalDir,
                  (selection) => controller.update(() {
                    settings.finalDir = selection.displayPath;
                    settings.finalTreeUri = selection.treeUri;
                  }),
                  linkForNative: false,
                ),
              ),
              const SizedBox(height: 14),
              SettingTextField(
                label: strings.discTitle,
                value: settings.title,
                onChanged: (value) =>
                    controller.update(() => settings.title = value),
              ),
              const SizedBox(height: 14),
              SettingStringDropdown(
                label: strings.discCapacity,
                value: settings.capacityOption,
                options: const <String>['dvd5', 'dvd9', 'custom'],
                labels: <String>[
                  strings.capacityDvd5,
                  strings.capacityDvd9,
                  strings.capacityCustom,
                ],
                onChanged: (value) => controller.update(() {
                  if (value == 'dvd9') {
                    settings.capacity = 1;
                  } else if (value == 'custom') {
                    settings.capacity = 2;
                  } else {
                    settings.capacity = 0;
                  }
                }),
              ),
              if (settings.capacity == 2) ...<Widget>[
                const SizedBox(height: 14),
                SettingTextField(
                  label: strings.customBytes,
                  value: settings.customBytes,
                  keyboardType: TextInputType.number,
                  inputFormatters: <TextInputFormatter>[
                    FilteringTextInputFormatter.digitsOnly,
                  ],
                  onChanged: (value) =>
                      controller.update(() => settings.customBytes = value),
                ),
              ],
              const SizedBox(height: 12),
              Align(
                alignment: Alignment.centerLeft,
                child: Text(
                  strings.capacitySummary(formatGigabytes(settings.discBytes)),
                  style: hintStyle,
                ),
              ),
            ],
          ),
        ),
        const SizedBox(height: 16),
        if (settings.advanced)
          SectionCard(
            title: strings.sectionAdvanced,
            icon: Icons.tune,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: <Widget>[
                SettingTextField(
                  label: strings.isoPrefix,
                  value: settings.isoPrefix,
                  onChanged: (value) =>
                      controller.update(() => settings.isoPrefix = value),
                ),
                const SizedBox(height: 14),
                SettingTextField(
                  label: strings.groupTrackLimit,
                  value: settings.groupTrackLimit,
                  keyboardType: TextInputType.number,
                  inputFormatters: <TextInputFormatter>[
                    FilteringTextInputFormatter.digitsOnly,
                  ],
                  onChanged: (value) =>
                      controller.update(() => settings.groupTrackLimit = value),
                ),
              ],
            ),
          )
        else
          InfoBanner(text: strings.advancedHint, icon: Icons.tune),
      ],
    );
  }
}
