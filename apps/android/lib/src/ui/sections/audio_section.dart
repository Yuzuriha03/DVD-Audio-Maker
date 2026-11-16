import 'package:material_ui/material_ui.dart';

import '../../services/app_controller.dart';
import '../../services/platform_service.dart';
import '../formatting.dart';
import '../widgets/common.dart';

/// MLP / LPCM encoding options.
class AudioSection extends StatelessWidget {
  const AudioSection({super.key, required this.controller});

  final AppController controller;

  Future<void> _pickDirectory(BuildContext context, String current) async {
    try {
      final DirectorySelection? picked = await controller.pickDirectory(
        current,
      );
      if (picked == null) {
        return;
      }
      controller.update(() {
        controller.settings.externalDir = picked.displayPath;
        controller.settings.externalTreeUri = picked.treeUri;
        // Mirror on demand: see BasicsSection's source folder handling.
        controller.settings.externalStagingPath = '';
        controller.settings.externalMirrorDirty = true;
      });
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
    final imported = settings.audioMode == 'external';
    final lpcm = settings.audioMode == 'lpcm';
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        SectionCard(
          title: strings.sectionEncoding,
          subtitle: strings.audioSubtitle,
          icon: Icons.graphic_eq,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              SettingStringDropdown(
                label: strings.audioMode,
                value: settings.audioMode,
                options: withCurrent(settings.audioModes, settings.audioMode),
                labels: <String>[
                  strings.modeMlp,
                  strings.modeLpcm,
                  strings.modeExternal,
                  if (!settings.audioModes.contains(settings.audioMode))
                    settings.audioMode,
                ],
                onChanged: (value) =>
                    controller.update(() => settings.audioMode = value),
              ),
              if (imported) ...<Widget>[
                const SizedBox(height: 14),
                InfoBanner(
                  text: strings.externalDirHelp,
                  icon: Icons.music_note,
                ),
                const SizedBox(height: 12),
                BrowseField(
                  label: strings.externalDir,
                  value: settings.externalDir,
                  browseTooltip: strings.browse,
                  onChanged: (value) =>
                      controller.update(() => settings.externalDir = value),
                  onBrowse: () => _pickDirectory(context, settings.externalDir),
                ),
              ],
              const SizedBox(height: 14),
              FieldPair(
                first: SettingStringDropdown(
                  label: strings.sampleRate,
                  value: settings.sampleRate,
                  options: withCurrent(
                    settings.sampleRates,
                    settings.sampleRate,
                  ),
                  labels: withCurrent(
                    settings.sampleRateLabels,
                    settings.sampleRate,
                  ),
                  enabled: !imported,
                  onChanged: (value) =>
                      controller.update(() => settings.sampleRate = value),
                ),
                second: SettingStringDropdown(
                  label: strings.bits,
                  value: settings.bits,
                  // Only the shared allowed set is offered. An out-of-set
                  // value (e.g. 20-bit LPCM from an imported profile) stays in
                  // settings for the Rust invalid_bits rejection instead of
                  // being silently rewritten.
                  options: lpcm ? settings.lpcmBits : settings.mlpBits,
                  labels: lpcm
                      ? settings.lpcmBitDepthLabels
                      : settings.bitDepthLabels,
                  enabled: !imported,
                  onChanged: (value) =>
                      controller.update(() => settings.bits = value),
                ),
              ),
              if (!imported && settings.bits == '16') ...<Widget>[
                const SizedBox(height: 6),
                ToggleTile(
                  title: strings.dither16,
                  subtitle: strings.ditherHint,
                  value: settings.dither16,
                  icon: Icons.speed,
                  onChanged: (value) =>
                      controller.update(() => settings.dither16 = value),
                ),
              ],
              if (!imported) ...<Widget>[
                const SizedBox(height: 12),
                InfoBanner(
                  text: settings.audioMode == 'lpcm'
                      ? strings.lpcmNote
                      : strings.mlpNote,
                  icon: Icons.music_note,
                ),
              ],
            ],
          ),
        ),
      ],
    );
  }
}
