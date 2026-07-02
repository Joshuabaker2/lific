import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  spring,
  interpolate,
  Easing,
} from "remotion";
import { C } from "../theme";
import { BODY } from "../fonts";
import { Background } from "../components/Background";
import { BrowserFrame } from "../components/BrowserFrame";
import {
  LificApp,
  IssueCard,
  IssueData,
  Label,
  colX,
  cardsTop,
  CARD_W,
  CARD_PAD,
  CARD_GAP,
  COL_W,
  PILLS_H,
  COL_HEADER_H,
} from "../components/lific-ui";
import { Cursor, Waypoint } from "../components/Cursor";

/*
 * Pixel-faithful board demo: the real app shell, one drag-and-drop
 * (LIF-214: Todo -> Active) with svelte-dnd-action's dashed accent
 * drop-target outline, live count updates, and column reflow.
 */

// Project labels (label chips resolve color from these, like the app).
const L: Record<string, Label> = {
  webui: { name: "web-ui", color: "#4dd9c7" },
  core: { name: "core", color: "#9287d7" },
  mcp: { name: "mcp", color: "#b48af0" },
  auth: { name: "auth", color: "#fb923c" },
  bug: { name: "bug", color: "#f87171" },
};

type BoardCard = {
  issue: IssueData;
  col: number; // 0 backlog, 1 todo, 2 active, 3 done
  slot: number;
  lines: 1 | 2; // title line count (drives real card height)
};

const CARDS: BoardCard[] = [
  { col: 0, slot: 0, lines: 1, issue: { identifier: "LIF-241", title: "Swimlane picker for the board", updated: "6d ago" } },
  { col: 0, slot: 1, lines: 1, issue: { identifier: "LIF-249", title: "Import issues from CSV", priority: "low", updated: "3d ago" } },
  { col: 1, slot: 0, lines: 1, issue: { identifier: "LIF-231", title: "Board column virtualization", priority: "medium", labels: [L.webui], updated: "2d ago" } },
  { col: 1, slot: 1, lines: 2, issue: { identifier: "LIF-214", title: "Bulk-edit issues from the list view", priority: "high", labels: [L.webui], updated: "4h ago" } },
  { col: 1, slot: 2, lines: 1, issue: { identifier: "LIF-207", title: "Saved filters per project", priority: "low", updated: "1d ago" } },
  { col: 2, slot: 0, lines: 2, issue: { identifier: "LIF-198", title: "Fix WAL checkpoint race on shutdown", priority: "high", labels: [L.core, L.bug], updated: "26m ago" } },
  { col: 2, slot: 1, lines: 1, issue: { identifier: "LIF-226", title: "MCP: recurring plan templates", priority: "medium", labels: [L.mcp], updated: "2h ago" } },
  { col: 3, slot: 0, lines: 1, issue: { identifier: "LIF-183", title: "OAuth device flow for CLI login", labels: [L.auth], updated: "5h ago", status: "done" } },
  { col: 3, slot: 1, lines: 1, issue: { identifier: "LIF-171", title: "Backup retention config", labels: [L.core], updated: "1d ago", status: "done" } },
];

// Real card heights: 87px for 1-line titles, 105px for 2-line titles
// (p-2.5 + top row + title lines + mt-2 + chip row + borders).

// Drag: LIF-214 (todo slot 1) -> active slot 2, frames 55..105.
const DRAG_START = 55;
const DRAG_END = 105;
const MOVED = "LIF-214";

const ease = Easing.bezier(0.4, 0, 0.2, 1);

/** y of a slot in a column given the (post-move aware) stack. */
const slotY = (heights: number[], slot: number) => {
  let y = cardsTop();
  for (let i = 0; i < slot; i++) y += heights[i] + CARD_GAP;
  return y;
};

export const UIScene: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const dragT = interpolate(frame, [DRAG_START, DRAG_END], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
    easing: ease,
  });
  const dragging = frame >= DRAG_START && frame < DRAG_END;
  const landed = frame >= DRAG_END;

  // Counts update at drop, exactly like the live app.
  const counts = landed
    ? { backlog: 2, todo: 2, active: 3, done: 2 }
    : { backlog: 2, todo: 3, active: 2, done: 2 };

  // Source + destination geometry for the moved card.
  const srcX = colX(1) + CARD_PAD;
  const srcY = slotY([87, 105, 87], 1);
  const dstX = colX(2) + CARD_PAD;
  const dstY = slotY([105, 87, 105], 2);

  const settle = spring({
    frame: frame - DRAG_END,
    fps,
    config: { damping: 15, stiffness: 170, mass: 0.6 },
  });

  const movedPos = {
    x: srcX + (dstX - srcX) * dragT,
    y: srcY + (dstY - srcY) * dragT + Math.sin(dragT * Math.PI) * -18,
  };

  // Cursor tracks the card's grab point; leaves after the drop.
  const CURSOR: Waypoint[] = [
    { at: 14, x: colX(3) + 220, y: 560 },
    { at: 48, x: srcX + 150, y: srcY + 40 },
    { at: DRAG_START, x: srcX + 150, y: srcY + 40, click: true },
    { at: DRAG_END, x: dstX + 150, y: dstY + 40 },
    { at: DRAG_END + 8, x: dstX + 150, y: dstY + 40, click: true },
    { at: DRAG_END + 45, x: dstX + 260, y: dstY + 260 },
  ];

  const frameIn = spring({ frame, fps, config: { damping: 200, stiffness: 90 } });
  const captionIn = interpolate(frame, [DRAG_END + 25, DRAG_END + 42], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  // Native app size; scaled up for legibility, proportions untouched.
  const APP_W = 1500;
  const APP_H = 764;
  const SCALE = 1.14;

  return (
    <Background>
      <AbsoluteFill style={{ justifyContent: "center", alignItems: "center" }}>
        <div
          style={{
            transform: `scale(${SCALE * (0.985 + frameIn * 0.015)})`,
            opacity: frameIn,
            marginTop: -26,
          }}
        >
          <BrowserFrame url="localhost:8080/#/LIF/board" width={APP_W} height={APP_H + 52}>
            <LificApp
              width={APP_W}
              height={APP_H}
              counts={counts}
              totalLabel={"9"}
            >
              {/* svelte-dnd-action drop-target outline on the hovered zone */}
              {dragging && dragT > 0.45 ? (
                <div
                  style={{
                    position: "absolute",
                    left: colX(2) + 4,
                    top: PILLS_H + COL_HEADER_H + 4,
                    width: COL_W - 9,
                    bottom: 8,
                    outline: `2px dashed ${C.accent}`,
                    outlineOffset: -4,
                    borderRadius: 8,
                  }}
                />
              ) : null}

              {/* Static cards */}
              {CARDS.filter((c) => c.issue.identifier !== MOVED).map((card) => {
                // Only LIF-207 reflows: slot 2 of [87,105,87] -> slot 1 of
                // [87,87] once the dragged card lifts out of Todo.
                let y: number;
                if (card.issue.identifier === "LIF-207") {
                  const s = spring({
                    frame: frame - (DRAG_START + 6),
                    fps,
                    config: { damping: 200, stiffness: 140 },
                  });
                  const from = slotY([87, 105, 87], 2);
                  const to = slotY([87, 87], 1);
                  y = frame < DRAG_START + 6 ? from : from + (to - from) * s;
                } else if (card.col === 2) {
                  y = slotY([105, 87], card.slot);
                } else {
                  y = slotY([87, 87], card.slot);
                }
                const enter = spring({
                  frame: frame - 4 - (card.col * 2 + card.slot) * 2,
                  fps,
                  config: { damping: 200, stiffness: 120 },
                });
                return (
                  <div
                    key={card.issue.identifier}
                    style={{
                      position: "absolute",
                      left: colX(card.col) + CARD_PAD,
                      top: y,
                      opacity: enter,
                      transform: `translateY(${(1 - enter) * 14}px)`,
                    }}
                  >
                    <IssueCard issue={card.issue} width={CARD_W} />
                  </div>
                );
              })}

              {/* The dragged card */}
              <div
                style={{
                  position: "absolute",
                  left: movedPos.x,
                  top: movedPos.y,
                  zIndex: 30,
                  transform: dragging
                    ? "rotate(2deg) scale(1.02)"
                    : landed
                      ? `scale(${1 + (1 - settle) * 0.03})`
                      : undefined,
                  filter: dragging
                    ? "drop-shadow(0 14px 22px rgba(0,0,0,0.5))"
                    : undefined,
                  opacity: spring({
                    frame: frame - 8,
                    fps,
                    config: { damping: 200, stiffness: 120 },
                  }),
                }}
              >
                <IssueCard
                  issue={CARDS.find((c) => c.issue.identifier === MOVED)!.issue}
                  width={CARD_W}
                />
              </div>

              <Cursor points={CURSOR} />
            </LificApp>
          </BrowserFrame>
        </div>

        <div
          style={{
            position: "absolute",
            bottom: 36,
            fontFamily: BODY,
            fontSize: 36,
            fontWeight: 500,
            color: C.text,
            opacity: captionIn,
          }}
        >
          Issues, kanban, pages, modules —{" "}
          <span style={{ color: C.textMuted }}>the whole tracker, no seat math.</span>
        </div>
      </AbsoluteFill>
    </Background>
  );
};
