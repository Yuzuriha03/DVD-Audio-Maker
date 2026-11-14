import 'package:material_ui/material_ui.dart';
import 'package:flutter/services.dart';

import '../../services/app_controller.dart';
import '../widgets/common.dart';
import '../widgets/log_panel.dart';

/// Progress, diagnostics, log and per-task actions.
class TaskSection extends StatefulWidget {
  const TaskSection({super.key, required this.controller});

  final AppController controller;

  @override
  State<TaskSection> createState() => _TaskSectionState();
}

class _TaskSectionState extends State<TaskSection> {
  AppController get controller => widget.controller;
  bool _issuesOnly = false;

  Color _phaseColor(TaskPhase phase, ColorScheme scheme) {
    if (phase == TaskPhase.success) {
      return scheme.primary;
    }
    if (phase == TaskPhase.failure) {
      return scheme.error;
    }
    if (phase == TaskPhase.running) {
      return scheme.tertiary;
    }
    return scheme.secondary;
  }

  IconData _phaseIcon(TaskPhase phase) {
    if (phase == TaskPhase.success) {
      return Icons.check_circle;
    }
    if (phase == TaskPhase.failure) {
      return Icons.error_outline;
    }
    if (phase == TaskPhase.running) {
      return Icons.sync;
    }
    if (phase == TaskPhase.cancelled) {
      return Icons.stop_circle;
    }
    return Icons.play_circle;
  }

  Future<void> _run(BuildContext context, String action) async {
    final outcome = await controller.runAction(action);
    if (!context.mounted) {
      return;
    }
    final strings = controller.strings;
    if (controller.isCancelled) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(strings.statusCancelled)));
      return;
    }
    final prefix = outcome != null && outcome.ok
        ? strings.resultSuccess
        : strings.resultFailure;
    final detail = outcome == null || outcome.message.isEmpty
        ? controller.statusText
        : outcome.message;
    ScaffoldMessenger.of(
      context,
    ).showSnackBar(SnackBar(content: Text('$prefix: $detail')));
  }

  Future<void> _openOutput(BuildContext context) async {
    try {
      final opened = await controller.openOutputFolder();
      if (context.mounted && !opened) {
        ScaffoldMessenger.of(
          context,
        ).showSnackBar(SnackBar(content: Text(controller.strings.noOutput)));
      }
    } catch (error) {
      if (!context.mounted) {
        return;
      }
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(error.toString())));
    }
  }

  Future<void> _showStatus(BuildContext context) async {
    final unfinished = await controller.checkUnfinished();
    if (!context.mounted) {
      return;
    }
    final strings = controller.strings;
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(
          unfinished ? strings.statusUnfinished : strings.statusNoUnfinished,
        ),
      ),
    );
  }

  Future<void> _discard(BuildContext context) async {
    final strings = controller.strings;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(strings.discardProgress),
        content: Text(strings.retentionBody),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: Text(strings.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: Text(strings.deleteProgress),
          ),
        ],
      ),
    );
    if (confirmed != true) {
      return;
    }
    final discarded = await controller.discardProgress();
    if (!context.mounted) {
      return;
    }
    ScaffoldMessenger.of(context).showSnackBar(
      SnackBar(
        content: Text(discarded ? strings.statusDone : strings.statusFailed),
      ),
    );
  }

  String get _visibleLog {
    if (!_issuesOnly) {
      return controller.logText;
    }
    return controller.logText.split('\n').where(_isProblem).join('\n');
  }

  bool _isProblem(String line) {
    final lower = line.toLowerCase();
    return lower.contains('[error]') ||
        lower.contains('[warn]') ||
        lower.contains('failed') ||
        line.contains('[错误]') ||
        line.contains('[警告]') ||
        line.contains('失败') ||
        line.contains('失敗');
  }

  Future<void> _copyLog(BuildContext context) async {
    await Clipboard.setData(ClipboardData(text: _visibleLog));
    if (context.mounted) {
      ScaffoldMessenger.of(
        context,
      ).showSnackBar(SnackBar(content: Text(controller.strings.logCopied)));
    }
  }

  Future<void> _exportLog(BuildContext context) async {
    final saved = await controller.exportLog();
    if (context.mounted) {
      ScaffoldMessenger.of(context).showSnackBar(
        SnackBar(
          content: Text(
            saved
                ? controller.strings.logExported
                : controller.strings.logExportFailed,
          ),
        ),
      );
    }
  }

  @override
  Widget build(BuildContext context) {
    final strings = controller.strings;
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final phase = controller.phase;
    final running = controller.isRunning;
    final statusColor = _phaseColor(phase, scheme);
    final detail = controller.progressText.isNotEmpty
        ? controller.progressText
        : controller.message;
    return Column(
      crossAxisAlignment: CrossAxisAlignment.stretch,
      children: <Widget>[
        SectionCard(
          title: strings.sectionStatus,
          subtitle: strings.taskSubtitle,
          icon: Icons.sync,
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            children: <Widget>[
              Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: <Widget>[
                  Container(
                    width: 54,
                    height: 54,
                    decoration: BoxDecoration(
                      color: statusColor.withAlpha(18),
                      shape: BoxShape.circle,
                    ),
                    child: Icon(
                      _phaseIcon(phase),
                      color: statusColor,
                      size: 28,
                    ),
                  ),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: <Widget>[
                        Text(
                          controller.statusText,
                          style: theme.textTheme.titleLarge?.copyWith(
                            fontWeight: FontWeight.w700,
                          ),
                        ),
                        if (detail.isNotEmpty)
                          Padding(
                            padding: const EdgeInsets.only(top: 4),
                            child: Text(
                              detail,
                              maxLines: 3,
                              overflow: TextOverflow.ellipsis,
                              style: theme.textTheme.bodyMedium?.copyWith(
                                color: scheme.onSurfaceVariant,
                              ),
                            ),
                          ),
                      ],
                    ),
                  ),
                ],
              ),
              const SizedBox(height: 8),
              Text(
                strings.statistics(
                  controller.elapsedSeconds,
                  controller.problemCount,
                ),
                style: theme.textTheme.bodySmall?.copyWith(
                  color: scheme.onSurfaceVariant,
                ),
              ),
              if (running) ...<Widget>[
                const SizedBox(height: 18),
                ClipRRect(
                  borderRadius: BorderRadius.circular(999),
                  child: LinearProgressIndicator(
                    value: controller.progressFraction,
                    minHeight: 6,
                  ),
                ),
                const SizedBox(height: 10),
                Row(
                  children: <Widget>[
                    Text(strings.progressLabel),
                    if (controller.total > 1) ...<Widget>[
                      const SizedBox(width: 8),
                      Text(
                        strings.progressSummary(
                          controller.done.toString(),
                          controller.total.toString(),
                        ),
                        style: theme.textTheme.labelLarge?.copyWith(
                          color: scheme.primary,
                          fontWeight: FontWeight.w700,
                        ),
                      ),
                    ],
                    const Spacer(),
                    FilledButton.tonalIcon(
                      onPressed: controller.cancelRequested
                          ? null
                          : controller.cancel,
                      icon: const Icon(Icons.stop_circle, size: 18),
                      label: Text(strings.actionCancel),
                    ),
                  ],
                ),
                const SizedBox(height: 10),
                InfoBanner(
                  text: controller.cancelRequested
                      ? strings.statusStopping
                      : strings.runningHint,
                  icon: Icons.sync,
                  tone: BannerTone.warning,
                ),
              ] else if (phase == TaskPhase.idle) ...<Widget>[
                const SizedBox(height: 14),
                InfoBanner(text: strings.tasksIdleHint, icon: Icons.lightbulb),
              ],
              if (controller.diagnostics.isNotEmpty) ...<Widget>[
                const SizedBox(height: 16),
                Divider(color: scheme.outline.withAlpha(50)),
                Text(
                  strings.diagnosticsLabel,
                  style: theme.textTheme.titleSmall?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
                const SizedBox(height: 6),
                ...controller.diagnostics.map(
                  (item) => Padding(
                    padding: const EdgeInsets.only(top: 6),
                    child: Row(
                      crossAxisAlignment: CrossAxisAlignment.start,
                      children: <Widget>[
                        Icon(
                          Icons.error_outline,
                          size: 16,
                          color: scheme.error,
                        ),
                        const SizedBox(width: 8),
                        Expanded(
                          child: Text(
                            item.code.isEmpty
                                ? item.message
                                : '${item.code}: ${item.message}',
                            style: theme.textTheme.bodySmall,
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              ],
              if (controller.initError != null) ...<Widget>[
                const SizedBox(height: 14),
                InfoBanner(
                  text: controller.initError!,
                  icon: Icons.error_outline,
                  tone: BannerTone.error,
                ),
              ],
            ],
          ),
        ),
        const SizedBox(height: 16),
        SectionCard(
          title: strings.sectionLog,
          icon: Icons.subject,
          trailing: Row(
            mainAxisSize: MainAxisSize.min,
            children: <Widget>[
              IconButton(
                tooltip: strings.copyLog,
                onPressed: _visibleLog.isEmpty ? null : () => _copyLog(context),
                icon: const Icon(Icons.copy),
              ),
              IconButton(
                tooltip: strings.exportLog,
                onPressed: controller.logText.isEmpty
                    ? null
                    : () => _exportLog(context),
                icon: const Icon(Icons.download),
              ),
            ],
          ),
          footer: Align(
            alignment: Alignment.centerLeft,
            child: FilterChip(
              label: Text(strings.onlyIssues),
              selected: _issuesOnly,
              onSelected: (value) => setState(() => _issuesOnly = value),
            ),
          ),
          child: LogPanel(text: _visibleLog, emptyLabel: strings.logEmpty),
        ),
        const SizedBox(height: 16),
        SectionCard(
          title: strings.sectionActions,
          icon: Icons.bolt,
          child: Wrap(
            spacing: 10,
            runSpacing: 10,
            children: <Widget>[
              OutlinedButton.icon(
                onPressed: running ? null : () => _run(context, 'prepare'),
                icon: const Icon(Icons.search),
                label: Text(strings.actionPrepare),
              ),
              OutlinedButton.icon(
                onPressed: running ? null : () => _run(context, 'verify'),
                icon: const Icon(Icons.verified),
                label: Text(strings.actionVerify),
              ),
              OutlinedButton.icon(
                onPressed: () => _openOutput(context),
                icon: const Icon(Icons.open_in_new),
                label: Text(strings.actionOpenOutput),
              ),
              OutlinedButton.icon(
                onPressed: running ? null : () => _showStatus(context),
                icon: const Icon(Icons.refresh),
                label: Text(strings.actionStatus),
              ),
              TextButton.icon(
                onPressed: running ? null : () => _discard(context),
                icon: const Icon(Icons.delete_outline),
                label: Text(strings.actionDiscard),
              ),
            ],
          ),
        ),
      ],
    );
  }
}
