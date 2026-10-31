import 'package:flutter/material.dart';

import 'account.dart';

class PlannerNamedId {
  const PlannerNamedId({required this.id, required this.name});

  final String id;
  final String name;

  factory PlannerNamedId.fromJson(Object? value) {
    if (value is! Map || value['id'] is! String || value['name'] is! String) {
      throw const AccountException(
        'The server sent an unexpected property record',
      );
    }
    return PlannerNamedId(
      id: value['id'] as String,
      name: value['name'] as String,
    );
  }
}

class PlannerPlanSummary {
  const PlannerPlanSummary({required this.id, required this.title});

  final String id;
  final String title;

  factory PlannerPlanSummary.fromJson(Object? value) {
    if (value is! Map || value['id'] is! String || value['title'] is! String) {
      throw const AccountException(
        'The server sent an unexpected associated plan',
      );
    }
    return PlannerPlanSummary(
      id: value['id'] as String,
      title: value['title'] as String,
    );
  }
}

class PlannerRoomLink {
  const PlannerRoomLink({required this.roomId, required this.locationId});

  final String roomId;
  final String locationId;

  factory PlannerRoomLink.fromJson(Object? value) {
    if (value is! Map ||
        value['roomId'] is! String ||
        value['locationId'] is! String) {
      throw const AccountException('The server sent an unexpected room link');
    }
    return PlannerRoomLink(
      roomId: value['roomId'] as String,
      locationId: value['locationId'] as String,
    );
  }
}

class PlannerPropertyOverview {
  const PlannerPropertyOverview({
    required this.property,
    required this.locations,
    required this.plans,
  });

  final PlannerNamedId property;
  final List<PlannerNamedId> locations;
  final List<PlannerPlanSummary> plans;

  factory PlannerPropertyOverview.fromJson(Object? value) {
    if (value is! Map ||
        value['locations'] is! List ||
        value['plans'] is! List) {
      throw const AccountException(
        'The server sent an unexpected property overview',
      );
    }
    return PlannerPropertyOverview(
      property: PlannerNamedId.fromJson(value['property']),
      locations: [
        for (final row in value['locations'] as List)
          PlannerNamedId.fromJson(row),
      ],
      plans: [
        for (final row in value['plans'] as List)
          PlannerPlanSummary.fromJson(row),
      ],
    );
  }
}

class PlannerPlanContext {
  const PlannerPlanContext({
    required this.plan,
    required this.property,
    required this.locations,
    required this.roomLinks,
  });

  final PlannerPlanSummary plan;
  final PlannerNamedId property;
  final List<PlannerNamedId> locations;
  final List<PlannerRoomLink> roomLinks;

  factory PlannerPlanContext.fromJson(Object? value) {
    if (value is! Map ||
        value['locations'] is! List ||
        value['roomLinks'] is! List) {
      throw const AccountException(
        'The server sent an unexpected plan property context',
      );
    }
    return PlannerPlanContext(
      plan: PlannerPlanSummary.fromJson(value['plan']),
      property: PlannerNamedId.fromJson(value['property']),
      locations: [
        for (final row in value['locations'] as List)
          PlannerNamedId.fromJson(row),
      ],
      roomLinks: [
        for (final row in value['roomLinks'] as List)
          PlannerRoomLink.fromJson(row),
      ],
    );
  }
}

class PlannerInventoryItem {
  const PlannerInventoryItem({
    required this.id,
    required this.name,
    required this.detailsUrl,
  });

  final String id;
  final String name;
  final String detailsUrl;

  factory PlannerInventoryItem.fromJson(Object? value) {
    if (value is! Map ||
        value['id'] is! String ||
        value['name'] is! String ||
        value['detailsUrl'] is! String) {
      throw const AccountException(
        'The server sent an unexpected inventory item',
      );
    }
    return PlannerInventoryItem(
      id: value['id'] as String,
      name: value['name'] as String,
      detailsUrl: value['detailsUrl'] as String,
    );
  }
}

class PlannerRoomInventory {
  const PlannerRoomInventory({
    required this.property,
    required this.location,
    required this.roomId,
    required this.items,
  });

  final PlannerNamedId property;
  final PlannerNamedId location;
  final String roomId;
  final List<PlannerInventoryItem> items;

  factory PlannerRoomInventory.fromJson(Object? value) {
    if (value is! Map ||
        value['items'] is! List ||
        value['roomId'] is! String) {
      throw const AccountException(
        'The server sent an unexpected room inventory',
      );
    }
    return PlannerRoomInventory(
      property: PlannerNamedId.fromJson(value['property']),
      location: PlannerNamedId.fromJson(value['location']),
      roomId: value['roomId'] as String,
      items: [
        for (final row in value['items'] as List)
          PlannerInventoryItem.fromJson(row),
      ],
    );
  }
}

abstract interface class PropertyPlannerClient {
  Future<PlannerPropertyOverview> propertyOverview(
    AccountSession session, {
    String? propertyHint,
  });
  Future<PlannerPlanContext> planPropertyContext(
    AccountSession session,
    String planId,
  );
  Future<void> linkPlanProperty(
    AccountSession session,
    String planId,
    String propertyId,
  );
  Future<void> unlinkPlanProperty(AccountSession session, String planId);
  Future<void> linkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
    String locationId,
  );
  Future<void> unlinkRoomLocation(
    AccountSession session,
    String planId,
    String roomId,
  );
  Future<PlannerRoomInventory> roomInventory(
    AccountSession session,
    String planId,
    String roomId,
  );
  Future<bool> openItemUri(Uri uri);
}

String? validatedPropertyHint(String? value) {
  if (value == null || !RegExp(r'^[A-Za-z0-9_-]{1,128}$').hasMatch(value))
    return null;
  return value;
}

Uri? validatedHomeHeroItemUri(String value) {
  final uri = Uri.tryParse(value);
  if (uri == null ||
      uri.scheme != 'https' ||
      uri.host != 'console.homehero.pro' ||
      uri.port != 443 ||
      uri.userInfo.isNotEmpty ||
      uri.hasQuery ||
      uri.hasFragment ||
      uri.pathSegments.length != 2 ||
      uri.pathSegments.first != 'view' ||
      uri.pathSegments.last.isEmpty) {
    return null;
  }
  return uri;
}

class PropertyPlannerManagerButton extends StatelessWidget {
  const PropertyPlannerManagerButton({
    required this.client,
    required this.planClient,
    required this.session,
    required this.propertyHint,
    required this.isSessionCurrent,
    required this.sessionChanges,
    required this.onOpen,
    super.key,
  });

  final PropertyPlannerClient client;
  final HostedPlanClient planClient;
  final AccountSession? session;
  final String? propertyHint;
  final bool Function(AccountSession session) isSessionCurrent;
  final Listenable sessionChanges;
  final Future<bool> Function(HostedPlan plan) onOpen;

  @override
  Widget build(BuildContext context) => TextButton.icon(
    key: const ValueKey('property-plans-button'),
    onPressed: () => showDialog<void>(
      context: context,
      builder: (_) => _PropertyPlannerDialog(
        client: client,
        planClient: planClient,
        session: session,
        propertyHint: validatedPropertyHint(propertyHint),
        isSessionCurrent: isSessionCurrent,
        sessionChanges: sessionChanges,
        onOpen: onOpen,
      ),
    ),
    icon: const Icon(Icons.home_work_outlined),
    label: const Text('Property plans'),
  );
}

class _PropertyPlannerDialog extends StatefulWidget {
  const _PropertyPlannerDialog({
    required this.client,
    required this.planClient,
    required this.session,
    required this.propertyHint,
    required this.isSessionCurrent,
    required this.sessionChanges,
    required this.onOpen,
  });

  final PropertyPlannerClient client;
  final HostedPlanClient planClient;
  final AccountSession? session;
  final String? propertyHint;
  final bool Function(AccountSession session) isSessionCurrent;
  final Listenable sessionChanges;
  final Future<bool> Function(HostedPlan plan) onOpen;

  @override
  State<_PropertyPlannerDialog> createState() => _PropertyPlannerDialogState();
}

class _PropertyPlannerDialogState extends State<_PropertyPlannerDialog> {
  PlannerPropertyOverview? _overview;
  List<HostedPlan> _plans = const [];
  String? _error;
  bool _loading = true;
  bool _working = false;
  int _generation = 0;

  AccountSession? get _session => widget.session;
  bool get _sessionIsCurrent =>
      _session != null && widget.isSessionCurrent(_session!);

  @override
  void initState() {
    super.initState();
    widget.sessionChanges.addListener(_contextChanged);
    if (_session == null) {
      _loading = false;
      _error =
          'Sign in to the hosted Planner account to manage property plans.';
    } else {
      _refresh();
    }
  }

  void _contextChanged() {
    _generation++;
    if (!mounted) return;
    setState(() {
      _overview = null;
      _plans = const [];
      _loading = false;
      _working = false;
      _error = 'The Planner account changed. Close and reopen Property plans.';
    });
  }

  @override
  void didUpdateWidget(covariant _PropertyPlannerDialog oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (!identical(oldWidget.session, widget.session) ||
        oldWidget.propertyHint != widget.propertyHint) {
      _generation++;
      _overview = null;
      _plans = const [];
      _error = null;
      _loading = widget.session != null;
      if (widget.session == null) {
        _loading = false;
        _error =
            'Sign in to the hosted Planner account to manage property plans.';
      } else {
        WidgetsBinding.instance.addPostFrameCallback((_) {
          if (mounted) _refresh();
        });
      }
    }
  }

  bool _requestIsCurrent(int generation) {
    if (!mounted || generation != _generation) return false;
    if (!_sessionIsCurrent) {
      _contextChanged();
      return false;
    }
    return true;
  }

  Future<void> _refresh() async {
    final session = _session;
    if (session == null) {
      _contextChanged();
      return;
    }
    if (!_sessionIsCurrent) {
      _contextChanged();
      return;
    }
    final generation = ++_generation;
    setState(() {
      _loading = true;
      _error = null;
      _overview = null;
      _plans = const [];
    });
    try {
      final results = await Future.wait<Object>([
        widget.client.propertyOverview(
          session,
          propertyHint: widget.propertyHint,
        ),
        widget.planClient.listPlans(session),
      ]);
      if (!_requestIsCurrent(generation)) return;
      setState(() {
        _overview = results[0] as PlannerPropertyOverview;
        _plans = results[1] as List<HostedPlan>;
        _loading = false;
      });
    } on Object catch (error) {
      if (!_requestIsCurrent(generation)) return;
      setState(() {
        _error = _plannerError(error);
        _loading = false;
      });
    }
  }

  Future<String?> _titleDialog() => showDialog<String>(
    context: context,
    builder: (_) => const _PlannerTitleDialog(),
  );

  Future<String?> _choosePlan(List<HostedPlan> candidates) =>
      showDialog<String>(
        context: context,
        builder: (context) => AlertDialog(
          title: const Text('Link an existing hosted plan'),
          content: SizedBox(
            width: 400,
            height: 320,
            child: ListView(
              children: [
                for (final plan in candidates)
                  ListTile(
                    key: ValueKey('property-link-plan-${plan.id}'),
                    title: Text(plan.title),
                    subtitle: Text('Updated ${plan.updatedAt.toLocal()}'),
                    onTap: () => Navigator.pop(context, plan.id),
                  ),
              ],
            ),
          ),
          actions: [
            TextButton(
              onPressed: () => Navigator.pop(context),
              child: const Text('Cancel'),
            ),
          ],
        ),
      );

  Future<bool?> _confirmUnlink(HostedPlan plan) => showDialog<bool>(
    context: context,
    builder: (context) => AlertDialog(
      title: const Text('Unlink this plan?'),
      content: Text('“${plan.title}” will remain in your hosted plans.'),
      actions: [
        TextButton(
          onPressed: () => Navigator.pop(context, false),
          child: const Text('Cancel'),
        ),
        FilledButton(
          onPressed: () => Navigator.pop(context, true),
          child: const Text('Unlink'),
        ),
      ],
    ),
  );

  Future<void> _create() async {
    final title = (await _titleDialog())?.trim();
    final session = _session;
    final overview = _overview;
    if (title == null ||
        title.isEmpty ||
        session == null ||
        overview == null ||
        !_sessionIsCurrent)
      return;
    final generation = ++_generation;
    setState(() {
      _working = true;
      _error = null;
    });
    try {
      final plan = await widget.planClient.createPlan(session, title);
      if (!_requestIsCurrent(generation)) return;
      await widget.client.linkPlanProperty(
        session,
        plan.id,
        overview.property.id,
      );
      if (!_requestIsCurrent(generation)) return;
      setState(() => _working = false);
      await _refresh();
    } on Object catch (error) {
      if (!_requestIsCurrent(generation)) return;
      setState(() {
        _working = false;
        _error = _plannerError(error);
      });
    }
  }

  Future<void> _linkExisting() async {
    final overview = _overview;
    final session = _session;
    if (overview == null || session == null || !_sessionIsCurrent) return;
    final associated = overview.plans.map((plan) => plan.id).toSet();
    final candidates = _plans
        .where((plan) => !associated.contains(plan.id))
        .toList();
    if (candidates.isEmpty) {
      setState(() => _error = 'There are no other hosted plans to link.');
      return;
    }
    final selectedId = await _choosePlan(candidates);
    if (selectedId == null || !_sessionIsCurrent) return;
    final generation = ++_generation;
    setState(() {
      _working = true;
      _error = null;
    });
    try {
      await widget.client.linkPlanProperty(
        session,
        selectedId,
        overview.property.id,
      );
      if (!_requestIsCurrent(generation)) return;
      setState(() => _working = false);
      await _refresh();
    } on Object catch (error) {
      if (!_requestIsCurrent(generation)) return;
      setState(() {
        _working = false;
        _error = _plannerError(error);
      });
    }
  }

  Future<void> _open(HostedPlan plan) async {
    if (!_sessionIsCurrent) return;
    final generation = ++_generation;
    setState(() {
      _working = true;
      _error = null;
    });
    try {
      final opened = await widget.onOpen(plan);
      if (!_requestIsCurrent(generation)) return;
      if (opened) {
        Navigator.pop(context);
      } else {
        setState(() {
          _working = false;
          _error = 'Could not open this plan. Your current plan remains open.';
        });
      }
    } on Object catch (error) {
      if (!_requestIsCurrent(generation)) return;
      setState(() {
        _working = false;
        _error = _plannerError(error);
      });
    }
  }

  Future<void> _unlink(HostedPlan plan) async {
    if (await _confirmUnlink(plan) != true) return;
    final session = _session;
    if (session == null || !_sessionIsCurrent) return;
    final generation = ++_generation;
    setState(() {
      _working = true;
      _error = null;
    });
    try {
      await widget.client.unlinkPlanProperty(session, plan.id);
      if (!_requestIsCurrent(generation)) return;
      setState(() => _working = false);
      await _refresh();
    } on Object catch (error) {
      if (!_requestIsCurrent(generation)) return;
      setState(() {
        _working = false;
        _error = _plannerError(error);
      });
    }
  }

  @override
  void dispose() {
    _generation++;
    widget.sessionChanges.removeListener(_contextChanged);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    key: const ValueKey('property-planner-dialog'),
    title: const Text('Property plans'),
    content: SizedBox(
      width: 480,
      height: 440,
      child: _loading
          ? const Center(child: CircularProgressIndicator())
          : _overview == null
          ? Text(_error ?? 'Property plans are unavailable.')
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text('HomeHero property: ${_overview!.property.name}'),
                const SizedBox(height: 8),
                const Text(
                  'Hosted sign-in and property access are checked separately.',
                ),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Text(
                      _error!,
                      key: const ValueKey('property-planner-error'),
                    ),
                  ),
                Expanded(
                  child: _overview!.plans.isEmpty
                      ? const Center(
                          child: Text(
                            'No plans are linked to this property yet.',
                          ),
                        )
                      : ListView(
                          children: [
                            for (final summary in _overview!.plans)
                              _planRow(summary),
                          ],
                        ),
                ),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    OutlinedButton.icon(
                      key: const ValueKey('property-planner-refresh'),
                      onPressed: _working ? null : _refresh,
                      icon: const Icon(Icons.refresh),
                      label: const Text('Refresh'),
                    ),
                    OutlinedButton.icon(
                      key: const ValueKey('property-planner-link-existing'),
                      onPressed: _working ? null : _linkExisting,
                      icon: const Icon(Icons.link),
                      label: const Text('Link existing'),
                    ),
                    FilledButton.tonalIcon(
                      key: const ValueKey('property-planner-create'),
                      onPressed: _working ? null : _create,
                      icon: const Icon(Icons.add),
                      label: const Text('Create alternative'),
                    ),
                  ],
                ),
              ],
            ),
    ),
    actions: [
      TextButton(
        onPressed: _working ? null : () => Navigator.pop(context),
        child: const Text('Close'),
      ),
    ],
  );

  Widget _planRow(PlannerPlanSummary summary) {
    final plan = _plans
        .where((candidate) => candidate.id == summary.id)
        .firstOrNull;
    return ListTile(
      key: ValueKey('property-plan-${summary.id}'),
      title: Text(summary.title),
      subtitle: Text(
        plan == null ? 'Hosted plan unavailable' : 'Revision ${plan.revision}',
      ),
      trailing: Wrap(
        children: [
          IconButton(
            key: ValueKey('property-plan-open-${summary.id}'),
            tooltip: 'Open plan',
            onPressed: _working || plan == null ? null : () => _open(plan),
            icon: const Icon(Icons.open_in_new),
          ),
          IconButton(
            key: ValueKey('property-plan-unlink-${summary.id}'),
            tooltip: 'Unlink plan',
            onPressed: _working || plan == null ? null : () => _unlink(plan),
            icon: const Icon(Icons.link_off),
          ),
        ],
      ),
    );
  }
}

class _PlannerTitleDialog extends StatefulWidget {
  const _PlannerTitleDialog();

  @override
  State<_PlannerTitleDialog> createState() => _PlannerTitleDialogState();
}

class _PlannerTitleDialogState extends State<_PlannerTitleDialog> {
  final _controller = TextEditingController(text: 'Alternative layout');

  @override
  void dispose() {
    _controller.dispose();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    title: const Text('Create alternative'),
    content: TextField(
      key: const ValueKey('property-planner-title'),
      controller: _controller,
      autofocus: true,
      decoration: const InputDecoration(labelText: 'Plan title'),
      onSubmitted: (value) => Navigator.pop(context, value.trim()),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Cancel'),
      ),
      FilledButton(
        key: const ValueKey('property-planner-title-confirm'),
        onPressed: () => Navigator.pop(context, _controller.text.trim()),
        child: const Text('Create'),
      ),
    ],
  );
}

class PropertyRoomInventoryDialog extends StatefulWidget {
  const PropertyRoomInventoryDialog({
    required this.client,
    required this.session,
    required this.planId,
    required this.roomId,
    required this.roomName,
    required this.isContextCurrent,
    required this.sessionChanges,
    required this.contextChanges,
    super.key,
  });

  final PropertyPlannerClient client;
  final AccountSession session;
  final String planId;
  final String roomId;
  final String roomName;
  final bool Function() isContextCurrent;
  final Listenable sessionChanges;
  final Listenable contextChanges;

  @override
  State<PropertyRoomInventoryDialog> createState() =>
      _PropertyRoomInventoryDialogState();
}

class _PropertyRoomInventoryDialogState
    extends State<PropertyRoomInventoryDialog> {
  PlannerPlanContext? _context;
  PlannerRoomInventory? _inventory;
  String? _error;
  String? _selectedLocation;
  bool _loading = true;
  bool _working = false;
  int _generation = 0;

  bool _generationIsCurrent(int generation) =>
      mounted && generation == _generation;

  bool _requestIsCurrent(int generation) {
    if (!_generationIsCurrent(generation)) return false;
    if (!widget.isContextCurrent()) {
      _invalidateStaleContext();
      return false;
    }
    return true;
  }

  void _invalidateStaleContext() {
    _generation++;
    if (!mounted) return;
    setState(() {
      _context = null;
      _inventory = null;
      _selectedLocation = null;
      _loading = false;
      _working = false;
      _error =
          'The account, plan or room changed. Close and reopen this panel.';
    });
  }

  @override
  void initState() {
    super.initState();
    widget.sessionChanges.addListener(_invalidateStaleContext);
    widget.contextChanges.addListener(_invalidateStaleContext);
    _refresh();
  }

  @override
  void didUpdateWidget(covariant PropertyRoomInventoryDialog oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.planId != widget.planId ||
        oldWidget.roomId != widget.roomId ||
        !identical(oldWidget.session, widget.session)) {
      _generation++;
      _context = null;
      _inventory = null;
      _selectedLocation = null;
      WidgetsBinding.instance.addPostFrameCallback((_) {
        if (mounted) _refresh();
      });
    }
    if (!identical(oldWidget.contextChanges, widget.contextChanges)) {
      oldWidget.contextChanges.removeListener(_invalidateStaleContext);
      widget.contextChanges.addListener(_invalidateStaleContext);
    }
    if (!identical(oldWidget.sessionChanges, widget.sessionChanges)) {
      oldWidget.sessionChanges.removeListener(_invalidateStaleContext);
      widget.sessionChanges.addListener(_invalidateStaleContext);
    }
  }

  Future<void> _refresh() async {
    if (!widget.isContextCurrent()) {
      _invalidateStaleContext();
      return;
    }
    final generation = ++_generation;
    setState(() {
      _loading = true;
      _error = null;
      _context = null;
      _inventory = null;
      _selectedLocation = null;
    });
    try {
      final planContext = await widget.client.planPropertyContext(
        widget.session,
        widget.planId,
      );
      if (!_requestIsCurrent(generation)) return;
      final link = planContext.roomLinks
          .where((candidate) => candidate.roomId == widget.roomId)
          .firstOrNull;
      final currentLink =
          link != null &&
              planContext.locations.any(
                (location) => location.id == link.locationId,
              )
          ? link
          : null;
      setState(() {
        _context = planContext;
        _selectedLocation = currentLink?.locationId;
      });
      if (link != null) {
        final inventory = await widget.client.roomInventory(
          widget.session,
          widget.planId,
          widget.roomId,
        );
        if (!_requestIsCurrent(generation)) return;
        setState(() => _inventory = inventory);
      }
      if (!_requestIsCurrent(generation)) return;
      setState(() => _loading = false);
    } on Object catch (error) {
      if (!_requestIsCurrent(generation)) return;
      setState(() {
        _error = _roomError(error);
        _loading = false;
        _inventory = null;
      });
    }
  }

  Future<void> _changeLocation({required bool unlink}) async {
    final locationId = _selectedLocation;
    if (!unlink && (locationId == null || locationId.isEmpty)) return;
    if (!widget.isContextCurrent()) {
      _invalidateStaleContext();
      return;
    }
    final generation = ++_generation;
    setState(() {
      _working = true;
      _inventory = null;
      _error = null;
    });
    try {
      if (unlink) {
        await widget.client.unlinkRoomLocation(
          widget.session,
          widget.planId,
          widget.roomId,
        );
      } else {
        await widget.client.linkRoomLocation(
          widget.session,
          widget.planId,
          widget.roomId,
          locationId!,
        );
      }
      if (!_requestIsCurrent(generation)) return;
      setState(() => _working = false);
      await _refresh();
    } on Object catch (error) {
      if (!_requestIsCurrent(generation)) return;
      setState(() {
        _working = false;
        _error = _roomError(error);
      });
    }
  }

  Future<void> _openItem(PlannerInventoryItem item) async {
    if (!widget.isContextCurrent() ||
        _inventory == null ||
        !_inventory!.items.any(
          (candidate) =>
              candidate.id == item.id &&
              candidate.detailsUrl == item.detailsUrl,
        )) {
      _invalidateStaleContext();
      return;
    }
    final uri = validatedHomeHeroItemUri(item.detailsUrl);
    if (uri == null) {
      setState(() => _error = 'HomeHero returned an unsafe item link.');
      return;
    }
    await widget.client.openItemUri(uri);
  }

  @override
  void dispose() {
    _generation++;
    _inventory = null;
    widget.sessionChanges.removeListener(_invalidateStaleContext);
    widget.contextChanges.removeListener(_invalidateStaleContext);
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => AlertDialog(
    key: const ValueKey('property-room-inventory-dialog'),
    title: Text('HomeHero · ${widget.roomName}'),
    content: SizedBox(
      width: 480,
      height: 430,
      child: _loading
          ? const Center(child: CircularProgressIndicator())
          : _context == null
          ? Text(_error ?? 'Room inventory is unavailable.')
          : Column(
              crossAxisAlignment: CrossAxisAlignment.stretch,
              children: [
                Text('Property: ${_context!.property.name}'),
                if (_error != null)
                  Padding(
                    padding: const EdgeInsets.symmetric(vertical: 8),
                    child: Text(
                      _error!,
                      key: const ValueKey('room-inventory-error'),
                    ),
                  ),
                const SizedBox(height: 12),
                DropdownButtonFormField<String>(
                  key: const ValueKey('room-location-picker'),
                  initialValue:
                      _context!.locations.any(
                        (location) => location.id == _selectedLocation,
                      )
                      ? _selectedLocation
                      : null,
                  decoration: const InputDecoration(
                    labelText: 'HomeHero location',
                  ),
                  items: [
                    for (final location in _context!.locations)
                      DropdownMenuItem(
                        value: location.id,
                        child: Text(location.name),
                      ),
                  ],
                  onChanged: _working
                      ? null
                      : (value) => setState(() => _selectedLocation = value),
                ),
                const SizedBox(height: 8),
                if (_context!.roomLinks.any(
                  (link) => link.roomId == widget.roomId,
                ))
                  Text(
                    'Linked to ${_inventory?.location.name ?? _linkedLocationName()}',
                  ),
                Expanded(
                  child: _inventory == null
                      ? const Center(
                          child: Text(
                            'Choose a location to link this room and view inventory.',
                          ),
                        )
                      : _inventory!.items.isEmpty
                      ? const Center(child: Text('No items at this location.'))
                      : ListView(
                          children: [
                            for (final item in _inventory!.items)
                              ListTile(
                                key: ValueKey('homehero-item-${item.id}'),
                                title: Text(item.name),
                                trailing: IconButton(
                                  tooltip: 'Open item in HomeHero',
                                  onPressed: _working
                                      ? null
                                      : () => _openItem(item),
                                  icon: const Icon(Icons.open_in_new),
                                ),
                              ),
                          ],
                        ),
                ),
                Wrap(
                  spacing: 8,
                  runSpacing: 8,
                  children: [
                    OutlinedButton.icon(
                      key: const ValueKey('room-inventory-refresh'),
                      onPressed: _working ? null : _refresh,
                      icon: const Icon(Icons.refresh),
                      label: const Text('Refresh'),
                    ),
                    OutlinedButton(
                      key: const ValueKey('room-location-unlink'),
                      onPressed:
                          _working ||
                              !_context!.roomLinks.any(
                                (link) => link.roomId == widget.roomId,
                              )
                          ? null
                          : () => _changeLocation(unlink: true),
                      child: const Text('Unlink location'),
                    ),
                    FilledButton(
                      key: const ValueKey('room-location-link'),
                      onPressed: _working || _selectedLocation == null
                          ? null
                          : () => _changeLocation(unlink: false),
                      child: const Text('Link location'),
                    ),
                  ],
                ),
              ],
            ),
    ),
    actions: [
      TextButton(
        onPressed: () => Navigator.pop(context),
        child: const Text('Close'),
      ),
    ],
  );

  String _linkedLocationName() {
    final id = _context!.roomLinks
        .where((link) => link.roomId == widget.roomId)
        .firstOrNull
        ?.locationId;
    if (id == null) return 'location';
    return _context!.locations
            .where((location) => location.id == id)
            .firstOrNull
            ?.name ??
        'deleted location';
  }
}

String _plannerError(Object error) {
  if (error is AccountException) {
    return switch (error.code) {
      'property-access-restricted' =>
        'This Planner account has no verified HomeHero identity linked.',
      'property-integration-disabled' =>
        'HomeHero property linking is not enabled on this Planner service.',
      'property-upstream-unavailable' || 'property-upstream-denied' =>
        'HomeHero could not confirm property access. Refresh and try again.',
      'property-not-found' =>
        'This property is no longer available to the linked HomeHero account.',
      _ => error.message,
    };
  }
  return '$error';
}

String _roomError(Object error) {
  if (error is AccountException) {
    return switch (error.code) {
      'property-unlinked' =>
        'Link this plan to a HomeHero property in Property plans first.',
      'location-unlinked' => 'Choose a HomeHero location to link this room.',
      'room-not-associated' =>
        'This saved room is no longer associated with the plan.',
      'location-not-found' => 'This HomeHero location was deleted. Unlink it or choose another location.',
      'property-access-restricted' =>
        'The signed-in Planner account cannot access this HomeHero property.',
      'property-integration-disabled' =>
        'HomeHero property linking is not enabled on this Planner service.',
      'property-upstream-unavailable' || 'property-upstream-denied' =>
        'HomeHero could not confirm current access. Refresh and try again.',
      _ => error.message,
    };
  }
  return '$error';
}
