import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

/// Rounded card with a leading icon, title and optional subtitle.
class SectionCard extends StatelessWidget {
  const SectionCard({
    super.key,
    required this.title,
    required this.child,
    this.subtitle,
    this.icon,
    this.trailing,
    this.footer,
  });

  final String title;
  final String? subtitle;
  final IconData? icon;
  final Widget child;
  final Widget? trailing;
  final Widget? footer;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    return Card(
      elevation: 0,
      margin: EdgeInsets.zero,
      color: scheme.surface,
      shape: RoundedRectangleBorder(
        borderRadius: BorderRadius.circular(20),
        side: BorderSide(color: scheme.outline.withAlpha(46)),
      ),
      child: Padding(
        padding: const EdgeInsets.fromLTRB(18, 16, 18, 18),
        child: Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: <Widget>[
            Row(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: <Widget>[
                if (icon != null)
                  Container(
                    width: 38,
                    height: 38,
                    alignment: Alignment.center,
                    decoration: BoxDecoration(
                      color: scheme.primary.withAlpha(20),
                      borderRadius: BorderRadius.circular(12),
                    ),
                    child: Icon(icon, size: 20, color: scheme.primary),
                  ),
                if (icon != null) const SizedBox(width: 12),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: <Widget>[
                      Text(
                        title,
                        style: theme.textTheme.titleMedium?.copyWith(
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                      if (subtitle != null && subtitle!.isNotEmpty)
                        Padding(
                          padding: const EdgeInsets.only(top: 2),
                          child: Text(
                            subtitle!,
                            style: theme.textTheme.bodySmall?.copyWith(
                              color: scheme.onSurfaceVariant,
                            ),
                          ),
                        ),
                    ],
                  ),
                ),
                ?trailing,
              ],
            ),
            const SizedBox(height: 16),
            child,
            if (footer != null) ...<Widget>[
              const SizedBox(height: 14),
              footer!,
            ],
          ],
        ),
      ),
    );
  }
}

/// Text field that keeps a local controller so typing never loses the caret
/// when the parent rebuilds, while still accepting external profile loads.
class SettingTextField extends StatefulWidget {
  const SettingTextField({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    this.hint,
    this.helper,
    this.keyboardType,
    this.inputFormatters,
    this.suffixIcon,
    this.enabled = true,
    this.readOnly = false,
    this.onTap,
  });

  final String label;
  final String value;
  final ValueChanged<String> onChanged;
  final String? hint;
  final String? helper;
  final TextInputType? keyboardType;
  final List<TextInputFormatter>? inputFormatters;
  final Widget? suffixIcon;
  final bool enabled;
  final bool readOnly;
  final VoidCallback? onTap;

  @override
  State<SettingTextField> createState() => _SettingTextFieldState();
}

class _SettingTextFieldState extends State<SettingTextField> {
  late final TextEditingController _controller = TextEditingController(
    text: widget.value,
  );

  @override
  void didUpdateWidget(covariant SettingTextField oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.value != oldWidget.value && widget.value != _controller.text) {
      _controller.value = TextEditingValue(
        text: widget.value,
        selection: TextSelection.collapsed(offset: widget.value.length),
      );
    }
  }

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return TextField(
      controller: _controller,
      enabled: widget.enabled,
      readOnly: widget.readOnly,
      onTap: widget.onTap,
      showCursor: widget.readOnly ? false : null,
      enableInteractiveSelection: !widget.readOnly,
      keyboardType: widget.keyboardType,
      inputFormatters: widget.inputFormatters,
      onChanged: widget.onChanged,
      decoration: InputDecoration(
        labelText: widget.label,
        hintText: widget.hint,
        helperText: widget.helper,
        helperMaxLines: 3,
        suffixIcon: widget.suffixIcon,
      ),
    );
  }
}

/// Text field with a folder picker button.
class BrowseField extends StatelessWidget {
  const BrowseField({
    super.key,
    required this.label,
    required this.value,
    required this.onChanged,
    required this.onBrowse,
    required this.browseTooltip,
    this.helper,
    this.enabled = true,
  });

  final String label;
  final String value;
  final ValueChanged<String> onChanged;
  final VoidCallback onBrowse;
  final String browseTooltip;
  final String? helper;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    return SettingTextField(
      label: label,
      value: value,
      onChanged: onChanged,
      helper: helper,
      enabled: enabled,
      readOnly: true,
      onTap: enabled ? onBrowse : null,
      suffixIcon: IconButton(
        tooltip: browseTooltip,
        icon: const Icon(Icons.folder_open),
        onPressed: enabled ? onBrowse : null,
      ),
    );
  }
}

/// Labeled dropdown whose items mirror two parallel string lists.
class SettingStringDropdown extends StatelessWidget {
  const SettingStringDropdown({
    super.key,
    required this.label,
    required this.value,
    required this.options,
    required this.labels,
    required this.onChanged,
    this.helper,
    this.enabled = true,
  });

  final String label;
  final String value;
  final List<String> options;
  final List<String> labels;
  final ValueChanged<String> onChanged;
  final String? helper;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    final items = <DropdownMenuItem<String>>[];
    for (var index = 0; index < options.length; index++) {
      items.add(
        DropdownMenuItem<String>(
          value: options[index],
          child: Text(
            labels[index],
            maxLines: 1,
            overflow: TextOverflow.ellipsis,
          ),
        ),
      );
    }
    final selected = options.contains(value) ? value : options.first;
    return InputDecorator(
      decoration: InputDecoration(labelText: label, helperText: helper),
      child: DropdownButtonHideUnderline(
        child: DropdownButton<String>(
          value: selected,
          isExpanded: true,
          isDense: true,
          items: items,
          onChanged: enabled
              ? (next) {
                  if (next != null) {
                    onChanged(next);
                  }
                }
              : null,
        ),
      ),
    );
  }
}

/// Compact switch row used for the boolean profile keys.
class ToggleTile extends StatelessWidget {
  const ToggleTile({
    super.key,
    required this.title,
    required this.value,
    required this.onChanged,
    this.subtitle,
    this.icon,
    this.enabled = true,
  });

  final String title;
  final String? subtitle;
  final bool value;
  final ValueChanged<bool> onChanged;
  final IconData? icon;
  final bool enabled;

  @override
  Widget build(BuildContext context) {
    return SwitchListTile(
      contentPadding: EdgeInsets.zero,
      value: value,
      onChanged: enabled ? onChanged : null,
      title: Text(title),
      subtitle: subtitle == null ? null : Text(subtitle!),
      secondary: icon == null ? null : Icon(icon),
    );
  }
}

/// Two fields side by side on tablets, stacked on phones.
class FieldPair extends StatelessWidget {
  const FieldPair({
    super.key,
    required this.first,
    required this.second,
    this.breakpoint = 520,
  });

  final Widget first;
  final Widget second;
  final double breakpoint;

  @override
  Widget build(BuildContext context) {
    return LayoutBuilder(
      builder: (context, constraints) {
        if (constraints.maxWidth >= breakpoint) {
          return Row(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: <Widget>[
              Expanded(child: first),
              const SizedBox(width: 14),
              Expanded(child: second),
            ],
          );
        }
        return Column(
          crossAxisAlignment: CrossAxisAlignment.stretch,
          children: <Widget>[first, const SizedBox(height: 14), second],
        );
      },
    );
  }
}

enum BannerTone { neutral, success, warning, error }

/// Tinted callout used for hints and errors.
class InfoBanner extends StatelessWidget {
  const InfoBanner({
    super.key,
    required this.text,
    this.icon = Icons.info_outline,
    this.tone = BannerTone.neutral,
  });

  final String text;
  final IconData icon;
  final BannerTone tone;

  Color _toneColor(ColorScheme scheme) {
    if (tone == BannerTone.error) {
      return scheme.error;
    }
    if (tone == BannerTone.warning) {
      return scheme.tertiary;
    }
    if (tone == BannerTone.success) {
      return scheme.primary;
    }
    return scheme.secondary;
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final color = _toneColor(scheme);
    return Container(
      padding: const EdgeInsets.all(12),
      decoration: BoxDecoration(
        color: Color.alphaBlend(color.withAlpha(18), scheme.surface),
        borderRadius: BorderRadius.circular(12),
        border: Border.all(color: color.withAlpha(64)),
      ),
      child: Row(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: <Widget>[
          Icon(icon, size: 18, color: color),
          const SizedBox(width: 10),
          Expanded(
            child: Text(
              text,
              style: theme.textTheme.bodySmall?.copyWith(
                color: scheme.onSurfaceVariant,
              ),
            ),
          ),
        ],
      ),
    );
  }
}

/// Small rounded label used in the task summary.
class StatusPill extends StatelessWidget {
  const StatusPill({
    super.key,
    required this.label,
    required this.icon,
    this.color,
  });

  final String label;
  final IconData icon;
  final Color? color;

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final tint = color ?? scheme.primary;
    return Container(
      padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 6),
      decoration: BoxDecoration(
        color: tint.withAlpha(20),
        borderRadius: BorderRadius.circular(999),
        border: Border.all(color: tint.withAlpha(60)),
      ),
      child: Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          Icon(icon, size: 15, color: tint),
          const SizedBox(width: 6),
          Text(
            label,
            style: theme.textTheme.labelMedium?.copyWith(
              color: tint,
              fontWeight: FontWeight.w600,
            ),
          ),
        ],
      ),
    );
  }
}

/// Shared text style for raw log output.
TextStyle logTextStyle(BuildContext context) {
  final theme = Theme.of(context);
  return theme.textTheme.bodySmall?.copyWith(
        fontFamily: 'monospace',
        height: 1.45,
      ) ??
      const TextStyle(fontFamily: 'monospace');
}
