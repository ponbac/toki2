import QtQuick
import Quickshell.Io
import qs.Commons
import qs.Ui

BarWidget {
  id: root
  moduleName: "ponbac.toki"

  readonly property color tokiOrange: "#f9a91f"

  property var timer: null
  property string appUrl: ""
  property string status: "loading"
  property date now: new Date()

  readonly property bool running: root.status === "ok" && root.timer && root.elapsedText !== ""
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
  readonly property string tooltip: {
    if (root.status !== "ok") return root.statusMessage(root.status)
    if (!root.timer) return ""
    var parts = []
    if (root.projectName) parts.push(root.displayText(root.projectName, 80))
    if (root.activityName) parts.push(root.displayText(root.activityName, 80))
    if (root.note) parts.push(root.displayText(root.note, 120))
    parts.push(root.elapsedText)
    return parts.join(" · ")
  }
  readonly property color wellColor: {
    var fg = root.bar ? root.bar.barForeground : Color.foreground
    return Util.alpha(fg, 0.10)
  }

  function truncate(value, limit) {
    if (value.length <= limit) return value
    return value.slice(0, limit - 1) + "…"
  }

  function displayText(value, limit) {
    var plain = String(value)
      .replace(/[\x00-\x1f\x7f-\x9f]+/g, " ")
      .replace(/\s+/g, " ")
      .trim()
      .replace(/&/g, "＆")
      .replace(/</g, "‹")
      .replace(/>/g, "›")
    return root.truncate(plain, limit)
  }

  function statusMessage(value) {
    if (value === "unconfigured") return "Toki credentials are not configured"
    if (value === "insecure_credentials") return "Toki credentials must have mode 600"
    if (value === "invalid_api_url") return "Toki API URL must use HTTPS (or loopback HTTP)"
    if (value === "unauthorized") return "Toki credentials were rejected"
    return "Could not refresh Toki timer status"
  }

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

  function refresh() {
    if (!statusProc.running) statusProc.running = true
  }

  function openToki() {
    if (!root.bar || !root.appUrl) return
    root.bar.run("xdg-open " + Util.shellQuote(root.appUrl))
  }

  visible: root.label !== ""
  implicitWidth: {
    if (!visible) return 0
    if (root.vertical) return barSize
    if (root.running) return pill.implicitWidth + Style.space(8)
    return button.implicitWidth
  }
  implicitHeight: {
    if (!visible) return 0
    if (root.vertical && root.running) return pill.implicitHeight + Style.space(8)
    return barSize
  }

  Component.onCompleted: refresh()

  Process {
    id: statusProc
    command: ["python3", root.helperPath]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        var payload
        try {
          payload = JSON.parse(text || "{}")
        } catch (e) {
          root.applyPayload({"status": "error"})
          return
        }
        root.applyPayload(payload)
      }
    }
    onRunningChanged: {
      if (running) {
        stallTimer.restart()
        return
      }
      stallTimer.stop()
    }
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
      root.appUrl = ""
      if (statusChanged) console.warn(root.statusMessage(nextStatus))
      return
    }

    root.timer = payload.timer || null
    root.appUrl = payload.appUrl ? String(payload.appUrl) : ""
    root.now = new Date()
  }

  Timer {
    id: stallTimer
    interval: 8000
    onTriggered: {
      statusProc.running = false
      root.applyPayload({"status": "error"})
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

  Rectangle {
    id: pill
    visible: root.running
    anchors.centerIn: parent
    implicitWidth: root.vertical
      ? Math.max(Style.space(18), content.implicitWidth + Style.space(12))
      : content.implicitWidth + Style.space(19)
    implicitHeight: root.vertical
      ? content.implicitHeight + Style.space(12)
      : Style.space(20)
    width: implicitWidth
    height: implicitHeight
    radius: height / 2
    color: root.wellColor

    Grid {
      id: content
      anchors.centerIn: parent
      rows: root.vertical ? 2 : 1
      columns: root.vertical ? 1 : 2
      flow: root.vertical ? Grid.TopToBottom : Grid.LeftToRight
      horizontalItemAlignment: Grid.AlignHCenter
      verticalItemAlignment: Grid.AlignVCenter
      columnSpacing: Style.space(7)
      rowSpacing: Style.space(4)

      Item {
        width: Style.space(6)
        height: Style.space(6)

        Image {
          anchors.centerIn: parent
          width: Style.space(22)
          height: Style.space(22)
          source: Qt.resolvedUrl("ember.png")
          sourceSize: Qt.size(88, 88)
          fillMode: Image.PreserveAspectFit
          smooth: true
          mipmap: true
          asynchronous: false
        }
      }

      Text {
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
    labelVisible: !root.running
    keepSpace: root.running
    fontSize: Style.font.caption
    horizontalMargin: 6
    tooltipText: root.tooltip
    onPressed: function() { root.openToki() }
  }
}
