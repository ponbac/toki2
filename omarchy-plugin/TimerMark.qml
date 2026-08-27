import QtQuick
import QtQuick.Shapes

// Vector stopwatch, same geometry as the chosen timer mark. Drawn with
// Shape instead of a Nerd Font glyph so we don't pick up lookalikes
// (U+F051C in this font is the "10s" timer badge).
Item {
  id: root
  property color color: "#f9a91f"
  property string fontFamily: ""

  readonly property real u: Math.min(width, height) / 24
  readonly property real stroke: Math.max(1.15, 2 * u)

  Shape {
    anchors.fill: parent
    antialiasing: true
    preferredRendererType: Shape.CurveRenderer

    ShapePath {
      strokeColor: root.color
      strokeWidth: root.stroke
      fillColor: "transparent"
      capStyle: ShapePath.RoundCap
      joinStyle: ShapePath.RoundJoin
      startX: (12 + 7.1) * root.u
      startY: 13.2 * root.u
      PathArc {
        x: (12 - 7.1) * root.u
        y: 13.2 * root.u
        radiusX: 7.1 * root.u
        radiusY: 7.1 * root.u
        useLargeArc: true
      }
      PathArc {
        x: (12 + 7.1) * root.u
        y: 13.2 * root.u
        radiusX: 7.1 * root.u
        radiusY: 7.1 * root.u
        useLargeArc: true
      }
    }

    ShapePath {
      strokeColor: root.color
      strokeWidth: root.stroke
      fillColor: "transparent"
      capStyle: ShapePath.RoundCap
      startX: 12 * root.u
      startY: 13.2 * root.u
      PathLine { x: 12 * root.u; y: 9.6 * root.u }
    }

    ShapePath {
      strokeColor: root.color
      strokeWidth: root.stroke
      fillColor: "transparent"
      capStyle: ShapePath.RoundCap
      startX: 9.2 * root.u
      startY: 3.6 * root.u
      PathLine { x: 14.8 * root.u; y: 3.6 * root.u }
    }

    ShapePath {
      strokeColor: root.color
      strokeWidth: root.stroke
      fillColor: "transparent"
      capStyle: ShapePath.RoundCap
      startX: 12 * root.u
      startY: 3.6 * root.u
      PathLine { x: 12 * root.u; y: 5.7 * root.u }
    }
  }
}
