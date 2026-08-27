import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

BarWidget {
  id: root
  moduleName: "ponbac.toki"

  readonly property color tokiOrange: "#f9a91f"

  property var timer: null
  property var week: null
  property var recents: []
  property string appUrl: ""
  property string status: "loading"
  property date now: new Date()
  property int stateGeneration: 0
  property string pendingListKind: ""
  property string pendingListProjectId: ""
  property bool busy: actionProc.running || listProc.running || pendingListKind !== ""

  readonly property bool running: root.status === "ok" && root.timer && root.elapsedText !== ""
  readonly property bool ready: root.status !== "loading"
  readonly property string helperPath: {
    var url = Qt.resolvedUrl("toki_timer_status.py").toString()
    return url.replace(/^file:\/\//, "")
  }
  readonly property string elapsedText: formatElapsed(root.timer, root.now)
  readonly property string projectName: root.timer && root.timer.projectName ? String(root.timer.projectName) : ""
  readonly property string activityName: root.timer && root.timer.activityName ? String(root.timer.activityName) : ""
  readonly property string note: root.timer && root.timer.note ? String(root.timer.note) : ""
  readonly property string label: {
    if (root.status === "loading") return ""
    if (root.status !== "ok") return "! Toki"
    if (!root.running) return ""
    return root.elapsedText
  }
  readonly property color markColor: root.running
    ? root.tokiOrange
    : (root.bar ? root.bar.barForeground : Color.foreground)
  readonly property bool peekIdle: root.status === "ok" && !root.running && (
    root.opened
    || (root.bar && root.bar.centerSectionRevealHeld === true && root.bar.centerHoverRevealSuppressed !== true)
  )
  readonly property bool shown: root.status !== "ok" || root.running || root.peekIdle
  readonly property bool opened: panelLoader.item ? panelLoader.item.opened === true : false
  readonly property real openPanelIndicatorWidth: mark.visible ? Math.max(Style.space(10), mark.width) : 0
  readonly property real openPanelIndicatorHeight: Math.max(Style.space(10), Math.round(Style.bar.iconSlot * 0.55))
  readonly property bool popoutSwitchClosing: panelLoader.item ? panelLoader.item.popoutSwitchClosing === true : false

  function formatElapsed(timer, now) {
    if (!timer || !timer.startTime) return ""
    var started = Date.parse(timer.startTime)
    if (isNaN(started)) return ""
    var total = Math.max(0, Math.floor((now.getTime() - started) / 1000))
    var hours = Math.floor(total / 3600)
    var minutes = Math.floor((total % 3600) / 60)
    var seconds = total % 60
    function pad(value) { return value < 10 ? "0" + value : String(value) }
    if (hours > 0) return hours + ":" + pad(minutes) + ":" + pad(seconds)
    return minutes + ":" + pad(seconds)
  }

  function statusMessage(value) {
    if (value === "unconfigured") return "Toki credentials are not configured"
    if (value === "insecure_credentials") return "Toki credentials must have mode 600"
    if (value === "invalid_api_url") return "Toki API URL must use HTTPS (or loopback HTTP)"
    if (value === "unauthorized") return "Toki credentials were rejected"
    return "Could not refresh Toki"
  }

  function refresh() {
    if (!statusProc.running && !actionProc.running) {
      statusProc.generation = root.stateGeneration
      statusProc.running = true
    }
  }

  function openToki() {
    if (!root.bar || !root.appUrl) return
    root.bar.run("xdg-open " + Util.shellQuote(root.appUrl))
  }

  function open() {
    if (panelLoader.item) panelLoader.item.open()
  }

  function close() {
    if (panelLoader.item) panelLoader.item.close()
  }

  function togglePanel() {
    if (panelLoader.item) panelLoader.item.toggle()
  }

  function closeForPopoutSwitch() {
    if (panelLoader.item) panelLoader.item.closeForPopoutSwitch()
  }

  function injectPanel() {
    var target = panelLoader.item
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("settings" in target) target.settings = root.settings
    if ("anchorItem" in target) target.anchorItem = button
    if ("hostWidget" in target) target.hostWidget = root
  }

  function applyPayload(payload) {
    var nextStatus = payload && typeof payload.status === "string"
      ? payload.status
      : "error"
    if (["ok", "unconfigured", "insecure_credentials", "invalid_api_url", "unauthorized", "error"].indexOf(nextStatus) === -1)
      nextStatus = "error"

    var statusChanged = root.status !== nextStatus
    root.status = nextStatus
    if (nextStatus !== "ok") {
      root.timer = null
      root.week = null
      root.recents = []
      root.appUrl = ""
      if (statusChanged) console.warn(root.statusMessage(nextStatus))
      return
    }

    root.timer = payload.timer || null
    root.week = payload.week || null
    root.recents = payload.recents || []
    root.appUrl = payload.appUrl ? String(payload.appUrl) : ""
    root.now = new Date()
  }

  function startProcess(proc, args) {
    if (proc.running) return false
    proc.command = ["python3", root.helperPath].concat(args)
    proc.running = true
    return true
  }

  function runAction(action, fields) {
    if (actionProc.running) return false
    var args = ["--action", action]
    if (fields) args.push("--payload", JSON.stringify(fields))
    root.stateGeneration += 1
    actionProc.generation = root.stateGeneration
    return startProcess(actionProc, args)
  }

  function startList(kind, projectId) {
    var args = ["--action", kind]
    if (kind === "activities" && projectId) args.push("--project-id", projectId)
    return startProcess(listProc, args)
  }

  function runList(kind, projectId) {
    var requestedProjectId = projectId ? String(projectId) : ""
    if (listProc.running) {
      root.pendingListKind = kind
      root.pendingListProjectId = requestedProjectId
      return true
    }
    return root.startList(kind, requestedProjectId)
  }

  function runPendingList() {
    if (listProc.running || root.pendingListKind === "") return
    var kind = root.pendingListKind
    var projectId = root.pendingListProjectId
    root.pendingListKind = ""
    root.pendingListProjectId = ""
    root.startList(kind, projectId)
  }

  visible: root.ready && root.shown
  implicitWidth: {
    if (!visible) return 0
    if (root.vertical) return barSize
    if (root.status === "ok") {
      if (root.running) return mark.implicitWidth + Style.space(8)
      return Style.bar.statusSlot
    }
    return button.implicitWidth
  }
  implicitHeight: {
    if (!visible) return 0
    if (root.vertical && root.status === "ok") {
      if (root.running) return mark.implicitHeight + Style.space(8)
      return Style.bar.statusSlot
    }
    return barSize
  }

  Component.onCompleted: refresh()
  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()

  Loader {
    id: panelLoader
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    visible: false
    onLoaded: {
      root.injectPanel()
      Qt.callLater(root.injectPanel)
    }
  }

  IpcHandler {
    target: "ponbac.toki"

    function refresh(): void { root.broadcast("refresh") }
    function open(): void { root.open() }
    function close(): void { root.close() }
    function show(): void { root.open() }
    function hide(): void { root.close() }
    function toggle(): void { root.togglePanel() }
  }

  Process {
    id: statusProc
    property int generation: 0
    command: ["python3", root.helperPath]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        if (statusProc.generation !== root.stateGeneration) return
        try {
          root.applyPayload(JSON.parse(text || "{}"))
        } catch (e) {
          root.applyPayload({"status": "error"})
        }
      }
    }
    onRunningChanged: {
      if (running) stallTimer.restart()
      else stallTimer.stop()
    }
  }

  Process {
    id: actionProc
    property int generation: 0
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        if (actionProc.generation !== root.stateGeneration) return
        try {
          root.applyPayload(JSON.parse(text || "{}"))
        } catch (e) {
          root.applyPayload({"status": "error"})
        }
      }
    }
  }

  Process {
    id: listProc
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var payload
        try {
          payload = JSON.parse(text || "{}")
        } catch (e) {
          return
        }
        if (panelLoader.item && panelLoader.item.applyListPayload)
          panelLoader.item.applyListPayload(payload)
      }
    }
    onRunningChanged: {
      if (!running && root.pendingListKind !== "")
        Qt.callLater(root.runPendingList)
    }
  }

  Timer {
    id: stallTimer
    interval: 30000
    onTriggered: {
      var current = statusProc.generation === root.stateGeneration
      statusProc.running = false
      if (current) root.applyPayload({"status": "error"})
    }
  }

  Timer {
    interval: 20000
    running: true
    repeat: true
    onTriggered: root.refresh()
  }

  Timer {
    interval: 1000
    running: root.timer !== null
    repeat: true
    onTriggered: root.now = new Date()
  }

  Item {
    id: mark
    visible: root.status === "ok"
    anchors.centerIn: parent
    implicitWidth: content.implicitWidth
    implicitHeight: content.implicitHeight
    width: implicitWidth
    height: implicitHeight

    Grid {
      id: content
      anchors.centerIn: parent
      rows: root.vertical && root.running ? 2 : 1
      columns: (!root.vertical && root.running) ? 2 : 1
      flow: root.vertical ? Grid.TopToBottom : Grid.LeftToRight
      horizontalItemAlignment: Grid.AlignHCenter
      verticalItemAlignment: Grid.AlignVCenter
      columnSpacing: Style.space(6)
      rowSpacing: Style.space(4)

      TimerMark {
        width: Style.bar.iconCanvas
        height: Style.bar.iconCanvas
        color: root.markColor
        fontFamily: root.bar ? root.bar.fontFamily : Style.font.family
        opacity: root.running ? 1 : 0.45
      }

      Text {
        visible: root.running
        text: root.elapsedText
        color: root.tokiOrange
        font.family: root.bar ? root.bar.fontFamily : Style.font.family
        font.pixelSize: Style.font.caption
        font.weight: Font.Bold
        font.bold: true
        font.features: { "tnum": 1 }
        renderType: Text.NativeRendering
      }
    }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.label
    labelVisible: root.status !== "ok"
    keepSpace: root.status === "ok"
    fontSize: Style.font.caption
    horizontalMargin: 6
    tooltipText: root.opened ? "" : (root.status === "ok" ? (root.running ? "" : "Toki") : root.statusMessage(root.status))
    onPressed: function(b) {
      if (b === Qt.RightButton) root.openToki()
      else root.togglePanel()
    }
  }
}
