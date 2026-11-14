import 'package:material_ui/material_ui.dart';

import 'common.dart';

/// Scrollable monospace log that follows the newest line.
class LogPanel extends StatefulWidget {
  const LogPanel({
    super.key,
    required this.text,
    required this.emptyLabel,
    this.maxHeight = 260,
  });

  final String text;
  final String emptyLabel;
  final double maxHeight;

  @override
  State<LogPanel> createState() => _LogPanelState();
}

class _LogPanelState extends State<LogPanel> {
  final ScrollController _scroll = ScrollController();
  bool _followTail = true;

  @override
  void initState() {
    super.initState();
    _scroll.addListener(_onScroll);
  }

  void _onScroll() {
    if (!_scroll.hasClients) {
      return;
    }
    final position = _scroll.position;
    _followTail = position.maxScrollExtent - position.pixels <= 120;
  }

  @override
  void didUpdateWidget(covariant LogPanel oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (widget.text != oldWidget.text && widget.text.isNotEmpty) {
      if (_scroll.hasClients) {
        final position = _scroll.position;
        _followTail = position.maxScrollExtent - position.pixels <= 120;
      }
      if (!_followTail) {
        return;
      }
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (!mounted || !_scroll.hasClients) {
          return;
        }
        _scroll.jumpTo(_scroll.position.maxScrollExtent);
      });
    }
  }

  @override
  void dispose() {
    _scroll.removeListener(_onScroll);
    _scroll.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final text = widget.text.trim();
    return Container(
      width: double.infinity,
      constraints: BoxConstraints(maxHeight: widget.maxHeight, minHeight: 92),
      decoration: BoxDecoration(
        color: Color.alphaBlend(scheme.primary.withAlpha(6), scheme.surface),
        borderRadius: BorderRadius.circular(14),
        border: Border.all(color: scheme.outline.withAlpha(46)),
      ),
      child: text.isEmpty
          ? Center(
              child: Padding(
                padding: const EdgeInsets.all(16),
                child: Text(
                  widget.emptyLabel,
                  style: theme.textTheme.bodySmall?.copyWith(
                    color: scheme.onSurfaceVariant,
                  ),
                ),
              ),
            )
          : Scrollbar(
              controller: _scroll,
              child: SingleChildScrollView(
                controller: _scroll,
                padding: const EdgeInsets.all(12),
                child: SelectableText(text, style: logTextStyle(context)),
              ),
            ),
    );
  }
}
