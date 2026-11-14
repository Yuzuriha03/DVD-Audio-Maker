import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

import '../../models/app_settings.dart';
import '../../services/app_controller.dart';
import '../widgets/common.dart';

/// Disc menu, cover and video standard settings.
class MenuSection extends StatelessWidget {
  const MenuSection({super.key, required this.controller});

  final AppController controller;

  Future<void> _pickFont(
    BuildContext context,
    ValueChanged<String> apply,
  ) async {
    try {
      final picked = await controller.pickFont();
      if (picked != null) {
        apply(picked);
      }
    } catch (error) {
      if (context.mounted) {
        ScaffoldMessenger.of(context)
            .showSnackBar(SnackBar(content: Text(error.toString())));
      }
    }
  }

  Widget _fontField(
    BuildContext context,
    String label,
    String value,
    ValueChanged<String> apply,
  ) {
    return BrowseField(
      label: label,
      value: value,
      browseTooltip: controller.strings.browse,
      onChanged: apply,
      onBrowse: () => _pickFont(context, apply),
    );
  }

  @override
  Widget build(BuildContext context) {
    final strings = controller.strings;
    final settings = controller.settings;
    final theme = Theme.of(context);
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        SectionCard(
          title: strings.sectionDiscMenu,
          subtitle: strings.menuSubtitle,
          icon: Icons.grid_view,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              ToggleTile(
                title: strings.makeMenu,
                value: settings.menu,
                icon: Icons.grid_view,
                onChanged: (value) =>
                    controller.update(() => settings.menu = value),
              ),
              if (settings.menu) ...<Widget>[
                ToggleTile(
                  title: strings.menuStills,
                  value: settings.stills,
                  icon: Icons.image,
                  onChanged: (value) =>
                      controller.update(() => settings.stills = value),
                ),
                const SizedBox(height: 10),
                FieldPair(
                  first: SettingTextField(
                    label: strings.tracksPerPage,
                    value: settings.tracksPerPage,
                    keyboardType: TextInputType.number,
                    inputFormatters: <TextInputFormatter>[
                      FilteringTextInputFormatter.digitsOnly,
                    ],
                    onChanged: (value) =>
                        controller.update(() => settings.tracksPerPage = value),
                  ),
                  second: SettingStringDropdown(
                    label: strings.norm,
                    value: settings.norm,
                    options: kVideoStandards,
                    labels: <String>[
                      strings.normPal,
                      strings.normSecam,
                      strings.normNtsc,
                    ],
                    onChanged: (value) =>
                        controller.update(() => settings.norm = value),
                  ),
                ),
                const SizedBox(height: 18),
                Row(
                  children: <Widget>[
                    Expanded(child: Text(strings.coverDim)),
                    Text(
                      '${settings.coverDimValue}%',
                      style: theme.textTheme.labelLarge?.copyWith(
                        color: theme.colorScheme.primary,
                        fontWeight: FontWeight.w700,
                      ),
                    ),
                  ],
                ),
                Slider(
                  value: settings.coverDimValue.toDouble(),
                  min: 0,
                  max: 100,
                  divisions: 100,
                  label: '${settings.coverDimValue}%',
                  onChanged: (value) => controller.update(
                    () => settings.coverDim = value.round().toString(),
                  ),
                ),
                if (settings.advanced) ...<Widget>[
                  const SizedBox(height: 8),
                  _fontField(
                    context,
                    strings.fontSc,
                    settings.fontSc,
                    (value) => controller.update(() => settings.fontSc = value),
                  ),
                  const SizedBox(height: 14),
                  FieldPair(
                    first: _fontField(
                      context,
                      strings.fontJp,
                      settings.fontJp,
                      (value) =>
                          controller.update(() => settings.fontJp = value),
                    ),
                    second: _fontField(
                      context,
                      strings.fontKr,
                      settings.fontKr,
                      (value) =>
                          controller.update(() => settings.fontKr = value),
                    ),
                  ),
                  const SizedBox(height: 12),
                  InfoBanner(text: strings.fontHelp, icon: Icons.text_fields),
                ],
              ],
            ],
          ),
        ),
      ],
    );
  }
}
