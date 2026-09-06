import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "ozdil.omaplayer"
  ipcTarget: "ozdil.omaplayer"

  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  property string trackTitle: "Müzik Çalmıyor"
  property string trackArtist: "OmaPlayer"
  property string sourceName: "Evrensel Müzik"
  property string playbackStatus: "STOPPED"
  property bool isPlaying: (playbackStatus === "PLAYING")

  function cleanSanitized(str, maxLen) {
    if (!str) return ""
    var s = String(str).replace(/[\x00-\x1f\x7f-\x9f<>&`'"\\]/g, "").trim()
    return s.slice(0, maxLen || 64)
  }

  function resolveEnginePath() {
    return Qt.resolvedUrl("omaplayer-engine").toString().replace(/^file:\/\//, "")
  }

  function resolveDashPath() {
    return Qt.resolvedUrl("omaplayer-dashboard").toString().replace(/^file:\/\//, "")
  }

  Process {
    id: statusProc
    command: [root.resolveEnginePath(), "--json"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try {
          var cleanText = String(text || "").slice(0, 65536)
          var parsed = JSON.parse(cleanText)
          root.trackTitle = root.cleanSanitized(parsed.title || "Müzik Çalmıyor", 40)
          root.trackArtist = root.cleanSanitized(parsed.artist || "OmaPlayer", 35)
          root.sourceName = root.cleanSanitized(parsed.source_name || "Müzik", 30)
          root.playbackStatus = root.cleanSanitized(parsed.status || "STOPPED", 20)
        } catch(e) {
          root.playbackStatus = "STOPPED"
        }
      }
    }
  }

  Process {
    id: actionProc
  }

  Process {
    id: launchProc
    onExited: function(exitCode) {
      launchDeadlineTimer.stop()
    }
  }

  Timer {
    id: launchDeadlineTimer
    interval: 5000
    repeat: false
    onTriggered: {
      if (launchProc.running) launchProc.kill()
    }
  }

  Component.onDestruction: {
    if (statusProc.running) statusProc.kill()
    if (actionProc.running) actionProc.kill()
    if (launchProc.running) launchProc.kill()
  }

  Component.onCompleted: {
    if (!statusProc.running) statusProc.running = true
  }

  function sendCmd(arg) {
    var eng = root.resolveEnginePath()
    actionProc.command = [eng, arg]
    actionProc.running = true
    refreshTimer.restart()
  }

  function launchDashboard() {
    root.close()
    var dashPath = root.resolveDashPath()
    launchProc.command = ["omarchy-launch-floating-terminal-with-presentation", dashPath]
    launchDeadlineTimer.restart()
    launchProc.running = true
  }

  Timer {
    id: refreshTimer
    interval: 500
    repeat: false
    onTriggered: {
      if (!statusProc.running) statusProc.running = true
    }
  }

  Timer {
    interval: 4000
    running: true
    repeat: true
    triggeredOnStart: true
    onTriggered: {
      if (!statusProc.running) statusProc.running = true
    }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.isPlaying ? ("󰎆 " + root.trackTitle.slice(0, 18)) : "󰎆 OmaPlayer"
    foreground: root.isPlaying ? (Color.accent || "#00ff66") : (root.bar ? root.bar.foreground : Color.foreground)
    tooltipText: "OmaPlayer: " + root.trackTitle + " (" + root.sourceName + ")"
    onPressed: function(b) { root.toggle() }
  }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    bar: root.bar
    open: root.opened
    contentWidth: panel.fittedContentWidth(Style.space(460))
    contentHeight: panel.fittedContentHeight(mainCol.implicitHeight)

    Column {
      id: mainCol
      anchors.left: parent.left
      anchors.right: parent.right
      anchors.top: parent.top
      spacing: Style.space(12)

      // Header
      RowLayout {
        width: parent.width
        Text {
          textFormat: Text.PlainText
          text: "🎵 OmaPlayer • Müzik & Radyo"
          font.pixelSize: Style.font.title
          font.bold: true
          color: root.bar ? root.bar.foreground : Color.foreground
          font.family: root.bar ? root.bar.fontFamily : Style.font.family
          Layout.fillWidth: true
        }

        Rectangle {
          height: Style.space(20)
          width: statusBadgeText.implicitWidth + Style.space(14)
          radius: Style.space(10)
          color: root.isPlaying ? Qt.rgba(0.0, 1.0, 0.4, 0.16) : Qt.rgba(0.5, 0.5, 0.5, 0.12)
          border.color: root.isPlaying ? (Color.accent || "#00ff66") : Qt.darker(root.bar ? root.bar.foreground : Color.foreground, 1.8)
          border.width: 1

          Text {
            id: statusBadgeText
            textFormat: Text.PlainText
            anchors.centerIn: parent
            text: root.isPlaying ? "ÇALIYOR" : "DURDU"
            font.pixelSize: 9
            font.bold: true
            color: root.isPlaying ? (Color.accent || "#00ff66") : Qt.darker(root.bar ? root.bar.foreground : Color.foreground, 1.4)
          }
        }
      }

      // Track Card
      Rectangle {
        width: parent.width
        height: Style.space(68)
        radius: Style.cornerRadius
        color: Qt.rgba(0, 0, 0, 0.25)
        border.color: Qt.rgba(1, 1, 1, 0.08)
        border.width: 1

        RowLayout {
          anchors.fill: parent
          anchors.margins: Style.space(10)
          spacing: Style.space(12)

          Text {
            textFormat: Text.PlainText
            text: "󰎆"
            font.pixelSize: Style.font.display
            color: root.isPlaying ? (Color.accent || "#00ff66") : Qt.darker(root.bar ? root.bar.foreground : Color.foreground, 1.5)
          }

          Column {
            Layout.fillWidth: true
            spacing: Style.space(2)
            Text {
              textFormat: Text.PlainText
              text: root.trackTitle
              font.bold: true
              font.pixelSize: Style.font.body
              color: root.bar ? root.bar.foreground : Color.foreground
              elide: Text.ElideRight
              width: parent.width
            }
            Text {
              textFormat: Text.PlainText
              text: root.trackArtist + " • " + root.sourceName
              font.pixelSize: Style.font.caption
              color: Qt.darker(root.bar ? root.bar.foreground : Color.foreground, 1.4)
              elide: Text.ElideRight
              width: parent.width
            }
          }
        }
      }

      // Playback Controls
      RowLayout {
        width: parent.width
        spacing: Style.space(8)

        Button {
          text: "⏮️ Önceki"
          Layout.fillWidth: true
          onClicked: root.sendCmd("--prev")
        }

        Button {
          text: root.isPlaying ? "⏸️ Duraklat" : "▶️ Oynat"
          Layout.fillWidth: true
          selected: root.isPlaying
          onClicked: root.sendCmd("--play-pause")
        }

        Button {
          text: "⏭️ Sonraki"
          Layout.fillWidth: true
          onClicked: root.sendCmd("--next")
        }
      }

      // Open Studio Button
      Button {
        width: parent.width
        text: "⚡ Terminal Müzik & Radyo Stüdyosunu Aç"
        onClicked: root.launchDashboard()
      }
    }
  }
}
