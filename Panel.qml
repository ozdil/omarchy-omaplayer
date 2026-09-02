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

  property string trackTitle: "Müzik Çalmıyor"
  property string trackArtist: "OmaPlayer"
  property string sourceName: "Evrensel Müzik"
  property string playbackStatus: "STOPPED"
  property bool isPlaying: (playbackStatus === "PLAYING")

  Process {
    id: statusProc
    command: [Qt.resolvedUrl("omaplayer-engine").toString().replace(/^file:\/\//, ""), "--json"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try {
          var parsed = JSON.parse(text)
          root.trackTitle = parsed.title || "Müzik Çalmıyor"
          root.trackArtist = parsed.artist || "OmaPlayer"
          root.sourceName = parsed.source_name || "Müzik"
          root.playbackStatus = parsed.status || "STOPPED"
        } catch(e) {
          root.playbackStatus = "STOPPED"
        }
      }
    }
  }

  Process {
    id: actionProc
  }

  function sendCmd(arg) {
    var eng = Qt.resolvedUrl("omaplayer-engine").toString().replace(/^file:\/\//, "")
    actionProc.command = [eng, arg]
    actionProc.running = true
    refreshTimer.restart()
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

  BarIconButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: "󰎆 " + (root.isPlaying ? root.trackTitle.slice(0, 16) : "Müzik")
    color: root.isPlaying ? "#a855f7" : "#94a3b8"
    slotSize: Style.bar.statusSlot
    tooltipText: "OmaPlayer: " + root.trackTitle + " (" + root.sourceName + ")"
    onPressed: root.toggle()
  }

  KeyboardPanel {
    id: panel
    anchorItem: button
    owner: root
    width: 460
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
          text: "🎵 OmaPlayer • Müzik Merkezi"
          font.pixelSize: Style.font.title
          font.bold: true
          color: root.bar ? root.bar.foreground : "#ffffff"
          Layout.fillWidth: true
        }

        Rectangle {
          width: 90
          height: 22
          radius: 11
          color: root.isPlaying ? "#9333ea" : "#334155"
          Text {
            anchors.centerIn: parent
            text: root.isPlaying ? "ÇALIYOR" : "DURDU"
            font.pixelSize: 9
            font.bold: true
            color: "#ffffff"
          }
        }
      }

      // Track Card
      Rectangle {
        width: parent.width
        height: 64
        radius: 8
        color: "#0f172a"
        border.color: "#1e293b"
        border.width: 1

        RowLayout {
          anchors.fill: parent
          anchors.margins: 10
          spacing: 10

          Text { text: "󰎆"; font.pixelSize: 28; color: "#c084fc" }

          Column {
            Layout.fillWidth: true
            spacing: 2
            Text {
              text: root.trackTitle
              font.bold: true
              font.pixelSize: Style.font.body
              color: "#f8fafc"
              elide: Text.ElideRight
              width: 320
            }
            Text {
              text: root.trackArtist + " • " + root.sourceName
              font.pixelSize: Style.font.caption
              color: "#94a3b8"
              elide: Text.ElideRight
              width: 320
            }
          }
        }
      }

      // Playback Controls
      RowLayout {
        width: parent.width
        spacing: 8

        Button {
          text: "⏮️ Önceki"
          Layout.fillWidth: true
          onClicked: root.sendCmd("--prev")
        }

        Button {
          text: root.isPlaying ? "⏸️ Duraklat" : "▶️ Oynat"
          Layout.fillWidth: true
          color: "#9333ea"
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
        onClicked: {
          root.close()
          var dashPath = Qt.resolvedUrl("omaplayer-dashboard").toString().replace(/^file:\/\//, "")
          if (root.bar) root.bar.run("omarchy-launch-floating-terminal-with-presentation " + dashPath)
        }
      }
    }
  }
}
