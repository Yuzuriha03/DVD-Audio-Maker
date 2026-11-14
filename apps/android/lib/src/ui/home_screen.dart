import 'package:material_ui/material_ui.dart';

import '../localization/app_strings.dart';
import '../services/app_controller.dart';
import 'sections/audio_section.dart';
import 'sections/basics_section.dart';
import 'sections/menu_section.dart';
import 'sections/task_section.dart';

enum RecoveryChoice { continueBuild, restartBuild }

/// Root shell: responsive navigation, profile actions and the build lifecycle.
class HomeScreen extends StatefulWidget {
  const HomeScreen({super.key, required this.controller});

  final AppController controller;

  @override
  State<HomeScreen> createState() => _HomeScreenState();
}

class _HomeScreenState extends State<HomeScreen> {
  int _index = 0;
  bool _buildFlow = false;
  bool _wasRunning = false;

  AppController get _controller => widget.controller;

  @override
  void initState() {
    super.initState();
    _controller.addListener(_showTaskWhenRunning);
  }

  void _showTaskWhenRunning() {
    final running = _controller.isRunning;
    if (running && !_wasRunning && mounted) {
      setState(() => _index = 3);
    }
    _wasRunning = running;
  }

  @override
  void dispose() {
    _controller.removeListener(_showTaskWhenRunning);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) {
    return AnimatedBuilder(
      animation: _controller,
      builder: (context, _) {
        final strings = _controller.strings;
        return LayoutBuilder(
          builder: (context, constraints) {
            final wide = constraints.maxWidth >= 900;
            final extendedRail = constraints.maxWidth >= 1240;
            final content = SingleChildScrollView(
              padding: EdgeInsets.fromLTRB(
                wide ? 28.0 : 16.0,
                wide ? 24.0 : 16.0,
                wide ? 28.0 : 16.0,
                28,
              ),
              child: Center(
                child: ConstrainedBox(
                  constraints: const BoxConstraints(maxWidth: 980),
                  child: _buildSection(),
                ),
              ),
            );
            return PopScope(
              canPop: !_controller.isRunning,
              onPopInvokedWithResult: (didPop, result) {
                if (!didPop) {
                  _confirmExit();
                }
              },
              child: Scaffold(
                appBar: _buildAppBar(strings, wide),
                body: wide
                    ? Row(
                        children: <Widget>[
                          NavigationRail(
                            selectedIndex: _index,
                            extended: extendedRail,
                            labelType: extendedRail
                                ? NavigationRailLabelType.none
                                : NavigationRailLabelType.selected,
                            onDestinationSelected: _selectTab,
                            destinations: _railDestinations(strings),
                            leading: const SizedBox(height: 12),
                          ),
                          VerticalDivider(width: 1, thickness: 1),
                          Expanded(child: content),
                        ],
                      )
                    : content,
                bottomNavigationBar: wide
                    ? _buildBottomBar(strings)
                    : Column(
                        mainAxisSize: MainAxisSize.min,
                        children: <Widget>[
                          _buildBottomBar(strings),
                          NavigationBar(
                            selectedIndex: _index,
                            onDestinationSelected: _selectTab,
                            destinations: _barDestinations(strings),
                          ),
                        ],
                      ),
              ),
            );
          },
        );
      },
    );
  }

  void _selectTab(int index) {
    if (!_controller.isRunning) {
      setState(() => _index = index);
    }
  }

  Widget _buildSection() {
    if (_index == 3) {
      return TaskSection(controller: _controller);
    }
    final running = _controller.isRunning;
    final section = switch (_index) {
      1 => AudioSection(controller: _controller),
      2 => MenuSection(controller: _controller),
      _ => BasicsSection(controller: _controller),
    };
    return IgnorePointer(
      ignoring: running,
      child: Opacity(opacity: running ? 0.6 : 1, child: section),
    );
  }

  AppBar _buildAppBar(AppStrings strings, bool wide) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    return AppBar(
      titleSpacing: wide ? 28.0 : 16.0,
      title: Text(
        strings.appTitle,
        maxLines: 1,
        overflow: TextOverflow.ellipsis,
        style: const TextStyle(
          fontWeight: FontWeight.w800,
          letterSpacing: -0.4,
        ),
      ),
      actions: <Widget>[
        PopupMenuButton<String>(
          enabled: !_controller.isRunning,
          tooltip: strings.language,
          icon: const Icon(Icons.translate),
          onSelected: _controller.setLanguage,
          itemBuilder: (context) {
            final current = _controller.settings.language;
            return AppLanguage.values.map((language) {
              final selected = language.code == current;
              return PopupMenuItem<String>(
                value: language.code,
                child: Row(
                  children: <Widget>[
                    Icon(
                      selected ? Icons.check_circle : Icons.language,
                      size: 18,
                      color: selected ? scheme.primary : null,
                    ),
                    const SizedBox(width: 10),
                    Text(language.label),
                  ],
                ),
              );
            }).toList();
          },
        ),
        PopupMenuButton<String>(
          tooltip: _themeTooltip(strings),
          icon: Icon(switch (_controller.settings.theme) {
            'light' => Icons.light_mode,
            'dark' => Icons.dark_mode,
            _ => Icons.brightness_auto,
          }),
          onSelected: _controller.setTheme,
          itemBuilder: (context) {
            final current = _controller.settings.theme;
            return <String>['system', 'light', 'dark'].map((theme) {
              final label = switch (theme) {
                'light' => strings.themeLight,
                'dark' => strings.themeDark,
                _ => strings.themeSystem,
              };
              return PopupMenuItem<String>(
                value: theme,
                child: Row(
                  children: <Widget>[
                    Icon(
                      theme == current
                          ? Icons.check_circle
                          : Icons.circle_outlined,
                      size: 18,
                      color: theme == current ? scheme.primary : null,
                    ),
                    const SizedBox(width: 10),
                    Text(label),
                  ],
                ),
              );
            }).toList();
          },
        ),
        if (wide) ...<Widget>[
          IconButton(
            tooltip: strings.openProfile,
            icon: const Icon(Icons.folder_open),
            onPressed: _controller.isRunning ? null : _openProfile,
          ),
          IconButton(
            tooltip: strings.saveProfile,
            icon: const Icon(Icons.save_alt),
            onPressed: _controller.isRunning ? null : _saveProfile,
          ),
        ],
        IconButton(
          tooltip: strings.moreSettings,
          icon: const Icon(Icons.settings_outlined),
          onPressed: _controller.isRunning ? null : _openSettingsSheet,
        ),
        SizedBox(width: wide ? 12.0 : 4.0),
      ],
    );
  }

  Widget _buildBottomBar(AppStrings strings) {
    final theme = Theme.of(context);
    final scheme = theme.colorScheme;
    final running = _controller.isRunning;
    final busy = running || _buildFlow;
    final progressLabel = _controller.progressText.isNotEmpty
        ? _controller.progressText
        : _controller.statusText;
    final Widget trailingActions;
    if (running) {
      trailingActions = FilledButton.tonalIcon(
        onPressed: _controller.cancelRequested ? null : _controller.cancel,
        icon: const Icon(Icons.stop_circle, size: 18),
        label: Text(strings.actionCancel),
      );
    } else {
      trailingActions = Row(
        mainAxisSize: MainAxisSize.min,
        children: <Widget>[
          OutlinedButton(
            onPressed: busy ? null : () => _runAction('prepare'),
            child: Text(strings.actionPrepare),
          ),
          const SizedBox(width: 10),
          FilledButton.icon(
            onPressed: busy ? null : _startBuild,
            icon: const Icon(Icons.play_arrow),
            label: Text(strings.actionBuild),
          ),
        ],
      );
    }
    return Material(
      elevation: 8,
      color: scheme.surface,
      child: SafeArea(
        top: false,
        child: Padding(
          padding: const EdgeInsets.symmetric(horizontal: 14, vertical: 9),
          child: Row(
            children: <Widget>[
              Expanded(
                child: running
                    ? Column(
                        mainAxisSize: MainAxisSize.min,
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: <Widget>[
                          Row(
                            children: <Widget>[
                              Expanded(
                                child: Text(
                                  progressLabel,
                                  maxLines: 1,
                                  overflow: TextOverflow.ellipsis,
                                  style: theme.textTheme.bodySmall,
                                ),
                              ),
                              if (_controller.total > 1)
                                Text(
                                  strings.progressSummary(
                                    _controller.done.toString(),
                                    _controller.total.toString(),
                                  ),
                                  style: theme.textTheme.labelSmall?.copyWith(
                                    color: scheme.primary,
                                    fontWeight: FontWeight.w700,
                                  ),
                                ),
                            ],
                          ),
                          const SizedBox(height: 6),
                          ClipRRect(
                            borderRadius: BorderRadius.circular(999),
                            child: LinearProgressIndicator(
                              value: _controller.progressFraction,
                              minHeight: 4,
                            ),
                          ),
                        ],
                      )
                    : Text(
                        _controller.statusText,
                        maxLines: 1,
                        overflow: TextOverflow.ellipsis,
                        style: theme.textTheme.titleSmall?.copyWith(
                          color: scheme.onSurfaceVariant,
                        ),
                      ),
              ),
              const SizedBox(width: 12),
              trailingActions,
            ],
          ),
        ),
      ),
    );
  }

  List<NavigationRailDestination> _railDestinations(AppStrings strings) {
    return <NavigationRailDestination>[
      NavigationRailDestination(
        icon: const Icon(Icons.album),
        label: Text(strings.tabSetup),
      ),
      NavigationRailDestination(
        icon: const Icon(Icons.graphic_eq),
        label: Text(strings.tabAudio),
      ),
      NavigationRailDestination(
        icon: const Icon(Icons.grid_view),
        label: Text(strings.tabMenu),
      ),
      NavigationRailDestination(
        icon: const Icon(Icons.play_circle),
        label: Text(strings.tabTask),
      ),
    ];
  }

  List<NavigationDestination> _barDestinations(AppStrings strings) {
    return <NavigationDestination>[
      NavigationDestination(
        icon: const Icon(Icons.album),
        label: strings.tabSetup,
      ),
      NavigationDestination(
        icon: const Icon(Icons.graphic_eq),
        label: strings.tabAudio,
      ),
      NavigationDestination(
        icon: const Icon(Icons.grid_view),
        label: strings.tabMenu,
      ),
      NavigationDestination(
        icon: const Icon(Icons.play_circle),
        label: strings.tabTask,
      ),
    ];
  }

  void _toggleAdvanced() {
    _controller.update(
      () => _controller.settings.advanced = !_controller.settings.advanced,
    );
  }

  Future<void> _openSettingsSheet() async {
    final strings = _controller.strings;
    await showModalBottomSheet<void>(
      context: context,
      showDragHandle: true,
      builder: (sheetContext) => AnimatedBuilder(
        animation: _controller,
        builder: (context, _) => SafeArea(
          child: ListView(
            shrinkWrap: true,
            padding: const EdgeInsets.only(bottom: 12),
            children: <Widget>[
              Padding(
                padding: const EdgeInsets.fromLTRB(20, 0, 20, 8),
                child: Text(
                  strings.moreSettings,
                  style: Theme.of(context).textTheme.titleMedium?.copyWith(
                    fontWeight: FontWeight.w700,
                  ),
                ),
              ),
              SwitchListTile(
                key: const ValueKey('advanced-settings-switch'),
                secondary: const Icon(Icons.tune),
                title: Text(strings.showAdvanced),
                subtitle: Text(strings.advancedSettingsHint),
                value: _controller.settings.advanced,
                onChanged: _controller.isRunning
                    ? null
                    : (_) => _toggleAdvanced(),
              ),
              const Divider(height: 1),
              ListTile(
                leading: const Icon(Icons.folder_open),
                title: Text(strings.openProfile),
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  _openProfile();
                },
              ),
              ListTile(
                leading: const Icon(Icons.save_alt),
                title: Text(strings.saveProfile),
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  _saveProfile();
                },
              ),
              ListTile(
                leading: const Icon(Icons.delete_outline),
                title: Text(strings.discardProgress),
                onTap: () {
                  Navigator.of(sheetContext).pop();
                  _discardProgress();
                },
              ),
            ],
          ),
        ),
      ),
    );
  }

  String _themeTooltip(AppStrings strings) =>
      switch (_controller.settings.theme) {
        'light' => strings.themeLight,
        'dark' => strings.themeDark,
        _ => strings.themeSystem,
      };

  Future<void> _confirmExit() async {
    final strings = _controller.strings;
    final confirmed = await showDialog<bool>(
      context: context,
      builder: (dialogContext) => AlertDialog(
        title: Text(strings.appTitle),
        content: Text(strings.runningConfirm),
        actions: <Widget>[
          TextButton(
            onPressed: () => Navigator.of(dialogContext).pop(false),
            child: Text(strings.cancel),
          ),
          FilledButton(
            onPressed: () => Navigator.of(dialogContext).pop(true),
            child: Text(strings.stopAndExit),
          ),
        ],
      ),
    );
    if (confirmed != true) {
      return;
    }
    await _controller.cancelAndWait();
    if (mounted) {
      Navigator.of(context).pop();
    }
  }

  Future<void> _startBuild() async {
    if (_buildFlow || _controller.isRunning) {
      return;
    }
    setState(() => _buildFlow = true);
    try {
      final unfinished = await _controller.checkUnfinished();
      if (!mounted) {
        return;
      }
      var action = 'build';
      if (unfinished) {
        final choice = await showDialog<RecoveryChoice>(
          context: context,
          barrierDismissible: true,
          builder: (dialogContext) =>
              _RecoveryDialog(strings: _controller.strings),
        );
        if (choice == null) {
          return;
        }
        action = choice == RecoveryChoice.continueBuild
            ? 'build_continue'
            : 'build_restart';
      }
      final outcome = await _controller.runAction(action);
      if (!mounted) {
        return;
      }
      _reportOutcome(outcome);
      if (outcome == null || outcome.ok) {
        return;
      }
      final stillUnfinished = await _controller.checkUnfinished();
      if (!mounted || !stillUnfinished) {
        return;
      }
      final keep = await showDialog<bool>(
        context: context,
        barrierDismissible: false,
        builder: (dialogContext) =>
            _RetentionDialog(strings: _controller.strings),
      );
      if (keep == false) {
        await _controller.discardProgress();
      }
    } finally {
      if (mounted) {
        setState(() => _buildFlow = false);
      }
    }
  }

  Future<void> _runAction(String action) async {
    if (_controller.isRunning || _buildFlow) {
      return;
    }
    final outcome = await _controller.runAction(action);
    if (!mounted) {
      return;
    }
    _reportOutcome(outcome);
  }

  void _reportOutcome(TaskOutcome? outcome) {
    if (outcome == null) {
      return;
    }
    final strings = _controller.strings;
    if (_controller.isCancelled) {
      _showMessage(strings.statusCancelled);
      return;
    }
    final prefix = outcome.ok ? strings.resultSuccess : strings.resultFailure;
    final detail = outcome.message.isEmpty
        ? _controller.statusText
        : outcome.message;
    _showMessage('$prefix: $detail', error: !outcome.ok);
  }

  Future<void> _openProfile() async {
    try {
      final applied = await _controller.openProfileFromDevice();
      if (!mounted || !applied) {
        return;
      }
      _showMessage(_controller.strings.profileLoaded);
    } on FormatException {
      if (!mounted) {
        return;
      }
      _showMessage(_controller.strings.profileInvalid, error: true);
    } catch (_) {
      if (!mounted) {
        return;
      }
      _showMessage(_controller.strings.profileOpenFailed, error: true);
    }
  }

  Future<void> _saveProfile() async {
    try {
      final saved = await _controller.saveProfileToDevice();
      if (!mounted) {
        return;
      }
      _showMessage(
        saved
            ? _controller.strings.profileSaved
            : _controller.strings.profileSaveFailed,
        error: !saved,
      );
    } catch (_) {
      if (!mounted) {
        return;
      }
      _showMessage(_controller.strings.profileSaveFailed, error: true);
    }
  }

  Future<void> _discardProgress() async {
    final strings = _controller.strings;
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
    final discarded = await _controller.discardProgress();
    if (!mounted) {
      return;
    }
    _showMessage(
      discarded ? strings.statusDone : strings.statusFailed,
      error: !discarded,
    );
  }

  void _showMessage(String text, {bool error = false}) {
    final messenger = ScaffoldMessenger.maybeOf(context);
    if (messenger == null) {
      return;
    }
    final scheme = Theme.of(context).colorScheme;
    messenger.hideCurrentSnackBar();
    messenger.showSnackBar(
      SnackBar(
        content: Text(
          text,
          style: TextStyle(color: error ? scheme.onErrorContainer : null),
        ),
        backgroundColor: error ? scheme.errorContainer : null,
      ),
    );
  }
}

class _RecoveryDialog extends StatelessWidget {
  const _RecoveryDialog({required this.strings});

  final AppStrings strings;

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      icon: const Icon(Icons.history),
      title: Text(strings.unfinishedTitle),
      content: Text(strings.unfinishedBody),
      actions: <Widget>[
        TextButton(
          onPressed: () => Navigator.of(context).pop(),
          child: Text(strings.cancel),
        ),
        TextButton(
          onPressed: () =>
              Navigator.of(context).pop(RecoveryChoice.restartBuild),
          child: Text(strings.buildRestart),
        ),
        FilledButton(
          onPressed: () =>
              Navigator.of(context).pop(RecoveryChoice.continueBuild),
          child: Text(strings.buildContinue),
        ),
      ],
    );
  }
}

class _RetentionDialog extends StatelessWidget {
  const _RetentionDialog({required this.strings});

  final AppStrings strings;

  @override
  Widget build(BuildContext context) {
    return AlertDialog(
      icon: const Icon(Icons.save),
      title: Text(strings.retentionTitle),
      content: Text(strings.retentionBody),
      actions: <Widget>[
        TextButton(
          onPressed: () => Navigator.of(context).pop(false),
          child: Text(strings.deleteProgress),
        ),
        FilledButton(
          onPressed: () => Navigator.of(context).pop(true),
          child: Text(strings.keepProgress),
        ),
      ],
    );
  }
}
