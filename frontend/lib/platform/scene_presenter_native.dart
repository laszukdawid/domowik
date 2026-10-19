import 'package:flutter/services.dart';

import '../core/domowik_core.dart';
import '../scene_presenter.dart';

/// The platform runner's or plugin's presenter on [channel].
ScenePresenter hostScenePresenter(MethodChannel channel, DomowikCore core) =>
    ChannelScenePresenter(channel);
