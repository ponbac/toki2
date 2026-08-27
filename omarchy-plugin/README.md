# Toki timer widget for Omarchy

This repository is a monorepo, so Omarchy cannot install it directly with
`omarchy plugin add` (that command requires `manifest.json` at the Git
repository root). Install the plugin directory manually from a trusted
checkout:

```bash
plugin_dir="$HOME/.config/omarchy/plugins/ponbac.toki"
install -d -m 700 "$plugin_dir"
install -m 644 \
  omarchy-plugin/manifest.json \
  omarchy-plugin/BarWidget.qml \
  omarchy-plugin/Panel.qml \
  omarchy-plugin/Model.js \
  omarchy-plugin/TimerMark.qml \
  "$plugin_dir/"
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

Left-click the timer mark to open the session popup (week meter, start/save,
project and activity). Right-click opens Toki in the browser. The helper
talks to Toki with your personal API token and never follows HTTP redirects.
Credentials must be owner-only (`600`); remote plaintext HTTP APIs are
rejected.

```bash
python3 -m unittest discover -s omarchy-plugin/tests -v
```
