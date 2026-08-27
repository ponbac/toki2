import QtQuick
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Session popup for the Toki bar widget: week meter, per-day hours, and
// start/save/discard. Nested dropdowns are avoided — project and activity
// lists replace the session body in-place, like Wi-Fi's network list.
Panel {
  id: root
  moduleName: "ponbac.toki"
  ipcTarget: "ponbac.toki"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  readonly property var barIdentity: hostWidget || root

  property string picking: ""
  property string pickFilter: ""
  property var projects: []
  property var activities: []
  property bool confirmDiscard: false
  property string draftNote: ""
  property string draftProjectId: ""
  property string draftProjectName: ""
  property string draftActivityId: ""
  property string draftActivityName: ""

  readonly property var timer: hostWidget ? hostWidget.timer : null
  readonly property var week: hostWidget ? hostWidget.week : null
  readonly property var recents: hostWidget && hostWidget.recents ? hostWidget.recents : []
  readonly property string status: hostWidget ? String(hostWidget.status) : "loading"
  readonly property bool running: hostWidget ? hostWidget.running === true : false
  readonly property bool busy: hostWidget ? hostWidget.busy === true : false
  readonly property date now: hostWidget && hostWidget.now ? hostWidget.now : new Date()
  readonly property color tokiOrange: hostWidget ? hostWidget.tokiOrange : "#f9a91f"
  readonly property color contentForeground: bar ? bar.foreground : Color.foreground
  readonly property string contentFontFamily: bar ? bar.fontFamily : Style.font.family
  readonly property real liveHours: running && timer ? Model.liveHours(timer.startTime, now) : 0
  readonly property var weekDays: week && week.days ? week.days : []
  readonly property int dayCount: Model.visibleCount(weekDays, liveHours, running)
  readonly property real workedHours: Model.weekWorked(week, liveHours, running)
  readonly property real remainingHours: Model.weekRemaining(week, liveHours, running)
  readonly property real scheduledHours: week ? Number(week.scheduledHours || 40) : 40
  readonly property real meterRatio: scheduledHours > 0
    ? Math.min(1, Math.max(0, workedHours / scheduledHours))
    : 0
  readonly property bool canSave: draftProjectId !== "" && draftActivityId !== ""
    && draftProjectName !== "" && draftActivityName !== ""
  readonly property var pickItems: {
    var query = pickFilter.toLowerCase()
    var source = picking === "activity" ? activities : projects
    var out = []
    for (var i = 0; i < source.length; i++) {
      var item = source[i]
      var label = picking === "activity" ? item.activityName : item.projectName
      if (!query || String(label).toLowerCase().indexOf(query) !== -1) out.push(item)
    }
    return out
  }
  readonly property bool editing: noteArea.activeFocus || pickSearch.activeFocus || picking !== "" || confirmDiscard

  function open() {
    syncDraftFromTimer()
    if (hostWidget && hostWidget.refresh) hostWidget.refresh()
    root.controller.show()
    Qt.callLater(function() {
      if (root.opened) setCenterHoverRevealSuppressed(true)
    })
  }

  function close() {
    setCenterHoverRevealSuppressed(false)
    picking = ""
    pickFilter = ""
    confirmDiscard = false
    root.controller.hide()
  }

  function toggle() {
    if (root.opened) root.close()
    else root.open()
  }

  function switchPanel(direction) {
    if (root.bar && typeof root.bar.switchPanelFrom === "function")
      return root.bar.switchPanelFrom(root.barIdentity, direction)
    return false
  }

  function setCenterHoverRevealSuppressed(value) {
    if (root.bar && "centerHoverRevealSuppressed" in root.bar)
      root.bar.centerHoverRevealSuppressed = value
  }

  function syncDraftFromTimer() {
    if (timer) {
      draftProjectId = timer.projectId ? String(timer.projectId) : ""
      draftProjectName = timer.projectName ? String(timer.projectName) : ""
      draftActivityId = timer.activityId ? String(timer.activityId) : ""
      draftActivityName = timer.activityName ? String(timer.activityName) : ""
      if (!noteArea.activeFocus)
        draftNote = timer.note ? String(timer.note) : ""
      return
    }
    if (draftProjectId !== "" || recents.length === 0) return
    var recent = recents[0]
    draftProjectId = String(recent.projectId || "")
    draftProjectName = String(recent.projectName || "")
    draftActivityId = String(recent.activityId || "")
    draftActivityName = String(recent.activityName || "")
  }

  function applyListPayload(payload) {
    if (!payload || payload.status !== "ok") return
    if (payload.projects) projects = payload.projects
    if (payload.activities) activities = payload.activities
  }

  function startPicking(kind) {
    picking = kind
    pickFilter = ""
    if (!hostWidget) return
    if (kind === "project") hostWidget.runList("projects")
    else if (draftProjectId) hostWidget.runList("activities", draftProjectId)
  }

  function chooseItem(item) {
    if (picking === "project") {
      draftProjectId = String(item.projectId || "")
      draftProjectName = String(item.projectName || "")
      draftActivityId = ""
      draftActivityName = ""
      activities = []
      picking = "activity"
      pickFilter = ""
      if (hostWidget && draftProjectId) hostWidget.runList("activities", draftProjectId)
      return
    }
    draftActivityId = String(item.activityId || "")
    draftActivityName = String(item.activityName || "")
    picking = ""
    pickFilter = ""
    if (running && hostWidget) hostWidget.runAction("update", {
      projectId: draftProjectId,
      projectName: draftProjectName,
      activityId: draftActivityId,
      activityName: draftActivityName,
      userNote: draftNote
    })
  }

  function startTimer(fromRecent) {
    if (!hostWidget || busy) return
    var source = fromRecent || {
      projectId: draftProjectId,
      projectName: draftProjectName,
      activityId: draftActivityId,
      activityName: draftActivityName,
      note: ""
    }
    hostWidget.runAction("start", {
      projectId: source.projectId || "",
      projectName: source.projectName || "",
      activityId: source.activityId || "",
      activityName: source.activityName || "",
      userNote: source.note || ""
    })
  }

  function saveTimer() {
    if (!hostWidget || busy || !canSave) return
    hostWidget.runAction("save", { userNote: draftNote })
  }

  function discardTimer() {
    if (!hostWidget || busy) return
    confirmDiscard = false
    hostWidget.runAction("stop")
  }

  function commitNote() {
    if (!running || !hostWidget || busy) return
    var current = timer && timer.note ? String(timer.note) : ""
    if (draftNote === current) return
    hostWidget.runAction("update", { userNote: draftNote })
  }

  onTimerChanged: if (!picking) syncDraftFromTimer()
  onRecentsChanged: if (!running && !picking) syncDraftFromTimer()
  onOpenedChanged: if (opened) syncDraftFromTimer()

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root.barIdentity
    bar: root.bar
    open: root.opened
    centerOnBar: true
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(400))
    contentHeight: panel.fittedContentHeight(sessionColumn.implicitHeight)

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      blocked: root.editing
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onActivateRequested: {
        if (root.running) root.saveTimer()
        else root.startTimer()
      }
      onDeleteRequested: if (root.running) root.confirmDiscard = true

      Flickable {
        id: sessionScroll
        anchors.fill: parent
        contentWidth: width
        contentHeight: sessionColumn.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        interactive: contentHeight > height

        Column {
          id: sessionColumn
          width: sessionScroll.width
          spacing: Style.space(12)

          // ---- Week meter
          Column {
            width: parent.width
            spacing: Style.space(6)
            visible: root.status === "ok" && root.picking === ""

            Item {
              width: parent.width
              height: workedLabel.height

              Text {
                id: workedLabel
                text: Model.formatHm(root.workedHours) + " of " + Model.formatHm(root.scheduledHours)
                color: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.subtitle
                font.features: { "tnum": 1 }
              }

              Text {
                anchors.right: parent.right
                text: root.remainingHours > 0.01
                  ? Model.formatHm(root.remainingHours) + " left"
                  : "+" + Model.formatHm(Math.abs(Model.weekFlex(root.week, root.liveHours, root.running))) + " flex"
                color: root.remainingHours > 0.01 ? root.tokiOrange : "#8fbf6a"
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.subtitle
                font.weight: Font.DemiBold
                font.features: { "tnum": 1 }
              }
            }

            Rectangle {
              width: parent.width
              height: Style.space(7)
              radius: height / 2
              color: Util.alpha(root.contentForeground, 0.08)

              Rectangle {
                width: parent.width * root.meterRatio
                height: parent.height
                radius: parent.radius
                color: root.tokiOrange
              }
            }
          }

          PanelSeparator {
            width: parent.width
            foreground: root.contentForeground
            visible: root.status === "ok" && root.picking === ""
          }

          // ---- Per-day hours. Weekends collapse unless they have time.
          Row {
            id: weekRow
            width: parent.width
            spacing: Style.space(8)
            visible: root.status === "ok" && root.picking === "" && root.weekDays.length > 0

            Repeater {
              model: root.weekDays

              Item {
                required property var modelData
                readonly property bool show: Model.dayVisible(modelData, root.liveHours, root.running)
                readonly property real hours: Model.dayHours(modelData, root.liveHours, root.running)
                readonly property real savedHours: Number(modelData.hours || 0)
                readonly property real liveSlice: Math.max(0, hours - savedHours)
                readonly property real cellWidth: weekRow.width > 0 && root.dayCount > 0
                  ? (weekRow.width - weekRow.spacing * (root.dayCount - 1)) / root.dayCount
                  : 0

                width: show ? cellWidth : 0
                height: show ? well.height + hourLabel.height + dayLabel.height + Style.space(6) : 0
                visible: show
                clip: true

                Rectangle {
                  id: well
                  width: parent.width
                  height: Style.space(52)
                  radius: Style.space(5)
                  color: Util.alpha(root.contentForeground, 0.06)
                  border.width: modelData.today ? 1 : 0
                  border.color: Util.alpha(root.tokiOrange, 0.55)

                  Item {
                    anchors.fill: parent
                    anchors.margins: 2
                    clip: true

                    Column {
                      anchors.left: parent.left
                      anchors.right: parent.right
                      anchors.bottom: parent.bottom
                      height: parent.height * Math.min(1.12, hours / 8)
                      spacing: 0

                      Rectangle {
                        visible: liveSlice > 0.008
                        width: parent.width
                        height: hours > 0 ? parent.height * (liveSlice / hours) : 0
                        color: root.tokiOrange
                        opacity: 0.95
                      }

                      Rectangle {
                        visible: savedHours > 0.008
                        width: parent.width
                        height: hours > 0 ? parent.height * (savedHours / hours) : 0
                        color: Util.alpha(root.tokiOrange, 0.58)
                      }
                    }
                  }
                }

                Text {
                  id: hourLabel
                  anchors.top: well.bottom
                  anchors.topMargin: Style.space(4)
                  anchors.horizontalCenter: parent.horizontalCenter
                  text: hours > 0.01 ? Model.formatHm(hours) : "—"
                  color: modelData.today ? root.tokiOrange : (hours > 0.01 ? root.contentForeground : Util.alpha(root.contentForeground, 0.35))
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.caption
                  font.weight: modelData.today ? Font.DemiBold : Font.Normal
                  font.features: { "tnum": 1 }
                }

                Text {
                  id: dayLabel
                  anchors.top: hourLabel.bottom
                  anchors.topMargin: Style.space(2)
                  anchors.horizontalCenter: parent.horizontalCenter
                  text: String(modelData.label || "")
                  color: modelData.today ? root.tokiOrange : Util.alpha(root.contentForeground, 0.45)
                  font.family: root.contentFontFamily
                  font.pixelSize: Style.font.caption
                  font.letterSpacing: 0.6
                }
              }
            }
          }

          // ---- Hero
          Row {
            width: parent.width
            spacing: Style.space(12)
            visible: root.picking === ""

            TimerMark {
              width: Style.space(28)
              height: Style.space(28)
              anchors.verticalCenter: parent.verticalCenter
              fontFamily: root.contentFontFamily
              color: root.running
                ? root.tokiOrange
                : Util.alpha(root.contentForeground, 0.45)
            }

            Column {
              anchors.verticalCenter: parent.verticalCenter
              spacing: Style.space(2)

              Text {
                text: root.status !== "ok"
                  ? (hostWidget ? hostWidget.statusMessage(root.status) : "Toki is unavailable")
                  : (root.running ? (hostWidget ? hostWidget.elapsedText : "") : "Idle")
                color: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: root.running ? Style.font.display : Style.font.title
                font.bold: true
                font.features: { "tnum": 1 }
              }

              Text {
                visible: root.status === "ok"
                text: root.running && root.timer
                  ? Model.startedLabel(root.timer.startTime)
                  : "Start from last, or pick a recent"
                color: Util.alpha(root.contentForeground, 0.55)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.caption
              }
            }
          }

          // ---- Picker
          Column {
            width: parent.width
            spacing: Style.space(8)
            visible: root.picking !== ""

            Row {
              spacing: Style.space(8)

              Button {
                text: "Back"
                iconText: "󰅁"
                fontFamily: root.contentFontFamily
                foreground: root.contentForeground
                onClicked: {
                  root.picking = ""
                  root.pickFilter = ""
                }
              }

              Text {
                anchors.verticalCenter: parent.verticalCenter
                text: root.picking === "activity" ? "Activity" : "Project"
                color: root.contentForeground
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.title
                font.bold: true
              }
            }

            TextField {
              id: pickSearch
              width: parent.width
              placeholderText: "Filter…"
              text: root.pickFilter
              font.family: root.contentFontFamily
              foreground: root.contentForeground
              onTextChanged: root.pickFilter = text
            }

            Repeater {
              model: root.pickItems

              Button {
                required property var modelData
                width: sessionColumn.width
                leftAlign: true
                text: root.picking === "activity" ? modelData.activityName : modelData.projectName
                fontFamily: root.contentFontFamily
                foreground: root.contentForeground
                bordered: false
                onClicked: root.chooseItem(modelData)
              }
            }

            Text {
              visible: root.pickItems.length === 0
              text: root.busy ? "Loading…" : "No matches"
              color: Util.alpha(root.contentForeground, 0.5)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.bodySmall
            }
          }

          // ---- Session fields
          Column {
            width: parent.width
            spacing: Style.space(8)
            visible: root.status === "ok" && root.picking === ""

            Button {
              width: parent.width
              leftAlign: true
              iconText: "󰉋"
              text: root.draftProjectName || "Choose project"
              fontFamily: root.contentFontFamily
              foreground: root.draftProjectName ? root.contentForeground : Util.alpha(root.contentForeground, 0.55)
              bordered: true
              onClicked: root.startPicking("project")
            }

            Button {
              width: parent.width
              leftAlign: true
              iconText: "󰃖"
              enabled: root.draftProjectId !== ""
              text: root.draftActivityName || "Choose activity"
              fontFamily: root.contentFontFamily
              foreground: root.draftActivityName ? root.contentForeground : Util.alpha(root.contentForeground, 0.55)
              bordered: true
              onClicked: root.startPicking("activity")
            }

            TextArea {
              id: noteArea
              width: parent.width
              visible: root.running
              wrapMode: TextArea.Wrap
              placeholderText: "Add a note…"
              text: root.draftNote
              color: root.contentForeground
              placeholderTextColor: Util.alpha(root.contentForeground, 0.4)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.body
              selectedTextColor: root.contentForeground
              selectionColor: Util.alpha(root.tokiOrange, 0.35)
              onTextChanged: root.draftNote = text
              onEditingFinished: root.commitNote()
              background: Rectangle {
                color: Util.alpha(root.contentForeground, 0.04)
                radius: Style.cornerRadius
                border.width: 1
                border.color: noteArea.activeFocus
                  ? Util.alpha(root.tokiOrange, 0.5)
                  : Util.alpha(root.contentForeground, 0.12)
              }
            }

            Row {
              width: parent.width
              spacing: Style.space(8)

              Button {
                width: root.running ? (parent.width - parent.spacing) / 2 : parent.width
                enabled: !root.busy && root.canSave
                opacity: enabled ? 1 : 0.4
                text: root.running ? "Save" : "Start"
                iconText: root.running ? "󰆓" : "󰐊"
                fontFamily: root.contentFontFamily
                foreground: "#1a1204"
                background: root.tokiOrange
                onClicked: root.running ? root.saveTimer() : root.startTimer()
              }

              Button {
                visible: root.running
                width: (parent.width - parent.spacing) / 2
                enabled: !root.busy
                text: "Discard"
                iconText: "󰆴"
                fontFamily: root.contentFontFamily
                foreground: bar ? bar.urgent : Color.urgent
                bordered: true
                onClicked: root.confirmDiscard = true
              }
            }

            Column {
              width: parent.width
              spacing: Style.space(2)
              visible: !root.running && root.recents.length > 0

              Text {
                text: "RECENT"
                color: Util.alpha(root.contentForeground, 0.45)
                font.family: root.contentFontFamily
                font.pixelSize: Style.font.caption
                font.letterSpacing: 1
              }

              Repeater {
                model: root.recents

                Button {
                  required property var modelData
                  width: sessionColumn.width
                  leftAlign: true
                  iconText: "󰐊"
                  text: Model.displayText((modelData.projectName || "") + " · " + (modelData.activityName || ""), 42)
                  fontFamily: root.contentFontFamily
                  foreground: root.contentForeground
                  onClicked: root.startTimer(modelData)
                }
              }
            }
          }

          Item {
            width: parent.width
            height: openLink.height
            visible: root.picking === ""

            Text {
              text: root.busy ? "Working…" : ""
              color: Util.alpha(root.contentForeground, 0.45)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.caption
            }

            Text {
              id: openLink
              anchors.right: parent.right
              text: "open Toki ↗"
              color: Util.alpha(root.contentForeground, 0.55)
              font.family: root.contentFontFamily
              font.pixelSize: Style.font.caption

              MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: if (root.hostWidget) root.hostWidget.openToki()
              }
            }
          }
        }
      }

      ConfirmDialog {
        id: discardConfirm
        anchors.fill: parent
        opened: root.confirmDiscard
        z: 10
        message: "Discard the running timer? No time entry will be created."
        confirmText: "Discard"
        background: Color.popups.background
        foreground: root.contentForeground
        fontFamily: root.contentFontFamily
        cornerRadius: Style.cornerRadius
        onCanceled: root.confirmDiscard = false
        onConfirmed: root.discardTimer()
      }
    }
  }
}
