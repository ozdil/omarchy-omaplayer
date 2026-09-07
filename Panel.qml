import QtQuick
import QtQuick.Layouts
import QtQuick.Controls
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui

Panel {
  id: root
  moduleName: "ozdil.omaplayer"
  ipcTarget: "ozdil.omaplayer"

  property string trackTitle: "NO AUDIO PLAYING"
  property string trackArtist: "OMAPLAYER"
  property string playbackStatus: "STOPPED"
  property string sourceName: "OMAPLAYER"
  property int volumeLevel: 80
  property bool isPlaying: (playbackStatus === "PLAYING")

  Process {
    id: statusProc
    command: [Qt.resolvedUrl("omaplayer-engine").toString().replace(/^file:\/\//, ""), "--json"]
    stdout: StdioCollector {
      waitForEnd: true
      onStreamFinished: {
        try {
          var raw = String(text || "").slice(0, 65536)
          var data = JSON.parse(raw)
          root.playbackStatus = data.status || "STOPPED"
          root.trackTitle = (data.title || "NO AUDIO").toUpperCase()
          root.trackArtist = (data.artist || "OMAPLAYER").toUpperCase()
          root.sourceName = (data.source_name || "OMAPLAYER").toUpperCase()
          root.volumeLevel = data.volume_pct || 80
        } catch (e) {
          root.playbackStatus = "STOPPED"
        }
      }
    }
  }

  Process {
    id: actionProc
    onExited: function(exitCode) {
      statusProc.running = true
    }
  }

  Process {
    id: guiProc
  }

  Component.onDestruction: {
    if (statusProc.running) statusProc.kill()
    if (actionProc.running) actionProc.kill()
    if (guiProc.running) guiProc.kill()
  }

  function sendCmd(arg) {
    var eng = Qt.resolvedUrl("omaplayer-engine").toString().replace(/^file:\/\//, "")
    actionProc.command = [eng, arg]
    actionProc.running = true
  }

  function playRadio(stId) {
    var eng = Qt.resolvedUrl("omaplayer-engine").toString().replace(/^file:\/\//, "")
    actionProc.command = [eng, "--play-radio", stId]
    actionProc.running = true
  }

  function launchGui() {
    root.close()
    var guiBin = Qt.resolvedUrl("omaplayer-gui").toString().replace(/^file:\/\//, "")
    guiProc.command = [guiBin]
    guiProc.running = true
  }

  Timer {
    interval: 3000
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
    text: root.isPlaying ? ("MUSIC: " + root.trackTitle.slice(0, 16)) : "MUSIC: IDLE"
    tooltipText: "OmaPlayer Audio Hub\nStatus: " + root.playbackStatus + "\nTrack: " + root.trackTitle + "\nArtist: " + root.trackArtist + "\nSource: " + root.sourceName + "\nEngine: Native Rust"
    onPressed: function(b) { if (root.opened) root.close(); else root.open(); }
  }

  KeyboardPanel {
    bar: root.bar
    id: panel
    anchorItem: button
    owner: root
    width: Style.space(480)
    contentHeight: Math.min(Style.space(620), panel.fittedContentHeight(mainCol.implicitHeight + Style.space(24)))

    Flickable {
      anchors.fill: parent
      anchors.margins: Style.space(12)
      contentHeight: mainCol.implicitHeight
      clip: true

      ColumnLayout {
        id: mainCol
        width: parent.width
        spacing: Style.space(12)

        // Header
        RowLayout {
          Layout.fillWidth: true
          ColumnLayout {
            spacing: Style.space(2)
            Text {
              textFormat: Text.PlainText
              text: "OMAPLAYER"
              font.bold: true
              font.pixelSize: Style.font.title
              color: "#c084fc"
            }
            Text {
              textFormat: Text.PlainText
              text: "AUDIO & RADIO HUB • NATIVE ARCH"
              font.pixelSize: Style.font.caption
              color: "#94a3b8"
            }
          }
          Item { Layout.fillWidth: true }
          Rectangle {
            width: Style.space(90)
            height: Style.space(24)
            radius: Style.space(4)
            color: root.isPlaying ? "#1e1b4b" : "#1e293b"
            border.color: root.isPlaying ? "#818cf8" : "#334155"
            border.width: 1
            Text {
              anchors.centerIn: parent
              textFormat: Text.PlainText
              text: root.playbackStatus
              font.bold: true
              font.pixelSize: Style.font.caption
              color: root.isPlaying ? "#a5b4fc" : "#94a3b8"
            }
          }
        }

        // Now Playing Card
        Rectangle {
          Layout.fillWidth: true
          height: Style.space(72)
          radius: Style.space(8)
          color: "#0f172a"
          border.color: "#1e293b"
          border.width: 1

          RowLayout {
            anchors.fill: parent
            anchors.margins: Style.space(12)
            ColumnLayout {
              Layout.fillWidth: true
              spacing: Style.space(2)
              Text {
                textFormat: Text.PlainText
                text: root.trackTitle
                font.bold: true
                font.pixelSize: Style.font.body
                color: "#f8fafc"
                elide: Text.ElideRight
              }
              Text {
                textFormat: Text.PlainText
                text: root.trackArtist + " • " + root.sourceName
                font.pixelSize: Style.font.caption
                color: "#94a3b8"
                elide: Text.ElideRight
              }
            }
          }
        }

        // Transport Controls
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.space(8)

          Button {
            text: "PREV"
            onClicked: root.sendCmd("--prev")
          }

          Button {
            Layout.fillWidth: true
            text: root.isPlaying ? "PAUSE" : "PLAY"
            onClicked: root.sendCmd("--play-pause")
          }

          Button {
            text: "NEXT"
            onClicked: root.sendCmd("--next")
          }

          Button {
            text: "STOP"
            onClicked: root.sendCmd("--stop")
          }

          Button {
            text: "VOL -"
            onClicked: root.sendCmd("--vol-down")
          }

          Button {
            text: "VOL +"
            onClicked: root.sendCmd("--vol-up")
          }
        }

        // Curated Live Radio Stations
        Text {
          textFormat: Text.PlainText
          text: "CURATED INTERNET RADIO"
          font.bold: true
          font.pixelSize: Style.font.caption
          color: "#94a3b8"
        }

        GridLayout {
          Layout.fillWidth: true
          columns: 3
          columnSpacing: Style.space(6)
          rowSpacing: Style.space(6)

          Button {
            Layout.fillWidth: true
            text: "LOFI BEATS"
            onClicked: root.playRadio("lofi")
          }
          Button {
            Layout.fillWidth: true
            text: "JAZZ RADIO"
            onClicked: root.playRadio("jazz")
          }
          Button {
            Layout.fillWidth: true
            text: "SYNTHWAVE"
            onClicked: root.playRadio("synthwave")
          }
          Button {
            Layout.fillWidth: true
            text: "CLASSIC ROCK"
            onClicked: root.playRadio("rock")
          }
          Button {
            Layout.fillWidth: true
            text: "TRT RADYO 3"
            onClicked: root.playRadio("trt")
          }
          Button {
            Layout.fillWidth: true
            text: "FULL GUI"
            onClicked: root.launchGui()
          }
        }

        // Footer Actions
        RowLayout {
          Layout.fillWidth: true
          spacing: Style.space(8)

          Button {
            Layout.fillWidth: true
            text: "REFRESH"
            onClicked: {
              if (!statusProc.running) statusProc.running = true
            }
          }

          Button {
            Layout.fillWidth: true
            text: "CLOSE"
            onClicked: root.close()
          }
        }
      }
    }
  }
}
