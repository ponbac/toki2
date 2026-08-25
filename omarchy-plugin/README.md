# Toki timer widget for Omarchy

This repository is a monorepo, so Omarchy cannot install it directly with
`omarchy plugin add` (that command requires `manifest.json` at the Git
repository root). Install the plugin directory manually from a trusted
checkout:

```bash
plugin_dir="$HOME/.config/omarchy/plugins/ponbac.toki"
install -d -m 700 "$plugin_dir"
install -m 644 omarchy-plugin/manifest.json omarchy-plugin/BarWidget.qml omarchy-plugin/ember.png "$plugin_dir/"
install -m 644 omarchy-plugin/toki_timer_status.py "$plugin_dir/"
omarchy-shell shell rescanPlugins
omarchy plugin enable ponbac.toki
```

Create a timer access token in Toki's account settings, save the displayed
credentials as `~/.config/toki/credentials`, and restrict the file to its
owner:

```bash
chmod 600 ~/.config/toki/credentials
```

API tokens authenticate as the issuing user and can call protected Toki routes;
store them like passwords. This widget only calls `GET /time-tracking/timer`.
The helper refuses group/world-readable credentials, remote plaintext HTTP APIs,
oversized or malformed responses, and all HTTP redirects.
