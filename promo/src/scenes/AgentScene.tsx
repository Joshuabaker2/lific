import React from "react";
import {
  AbsoluteFill,
  useCurrentFrame,
  useVideoConfig,
  spring,
  interpolate,
} from "remotion";
import { C } from "../theme";
import { BODY, DISPLAY, MONO } from "../fonts";
import { Background } from "../components/Background";
import {
  ColumnHeader,
  IssueCard,
  Label,
  CARD_W,
  CARD_PAD,
  COL_W,
} from "../components/lific-ui";
import { FadeUp } from "../components/text";

/*
 * The differentiator: an AI coding agent drives the tracker over MCP,
 * and a live crop of the real board reacts. Board chrome is the same
 * pixel-faithful kit as UIScene.
 */

const L: Record<string, Label> = {
  core: { name: "core", color: "#9287d7" },
  mcp: { name: "mcp", color: "#b48af0" },
  auth: { name: "auth", color: "#fb923c" },
  bug: { name: "bug", color: "#f87171" },
};

const TOOL_1 = 42; // update_issue chip
const BOARD_1 = 56; // LIF-198 appears in Done
const TOOL_2 = 88; // create_issue chip
const BOARD_2 = 104; // LIF-232 pops into Todo
const REPLY = 126;
const CAPTION = 146;

const ToolChip: React.FC<{ label: string; at: number }> = ({ label, at }) => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();
  const s = spring({ frame: frame - at, fps, config: { damping: 200, stiffness: 160 } });
  const okIn = interpolate(frame, [at + 12, at + 18], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });
  if (frame < at) return null;
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 10,
        fontFamily: MONO,
        fontSize: 18,
        color: C.textMuted,
        backgroundColor: C.bgSubtle,
        border: `1px solid ${C.border}`,
        borderRadius: 9,
        padding: "10px 16px",
        opacity: s,
        transform: `translateY(${(1 - s) * 12}px)`,
      }}
    >
      <span style={{ color: C.accent }}>⚙</span>
      {label}
      <span style={{ color: C.success, opacity: okIn, fontWeight: 600 }}>✓</span>
    </div>
  );
};

export const AgentScene: React.FC = () => {
  const frame = useCurrentFrame();
  const { fps } = useVideoConfig();

  const userIn = spring({ frame: frame - 6, fps, config: { damping: 200, stiffness: 140 } });

  const doneIn = spring({ frame: frame - BOARD_1, fps, config: { damping: 16, stiffness: 140 } });
  const doneFlash = frame >= BOARD_1 ? Math.max(0, 1 - (frame - BOARD_1) / 40) : 0;
  const newIn = spring({ frame: frame - BOARD_2, fps, config: { damping: 15, stiffness: 130 } });
  const newFlash = frame >= BOARD_2 ? Math.max(0, 1 - (frame - BOARD_2) / 40) : 0;

  // LIF-226 shifts down when LIF-232 lands on top of Todo.
  const shift = spring({ frame: frame - BOARD_2, fps, config: { damping: 200, stiffness: 140 } });

  const captionIn = interpolate(frame, [CAPTION, CAPTION + 16], [0, 1], {
    extrapolateLeft: "clamp",
    extrapolateRight: "clamp",
  });

  const doneCount = frame >= BOARD_1 ? 3 : 2;
  const todoCount = frame >= BOARD_2 ? 2 : 1;

  return (
    <Background>
      <AbsoluteFill
        style={{
          flexDirection: "row",
          justifyContent: "center",
          alignItems: "center",
          gap: 42,
          paddingBottom: 70,
        }}
      >
        {/* Agent chat panel */}
        <div
          style={{
            width: 690,
            height: 600,
            borderRadius: 16,
            border: `1px solid ${C.border}`,
            backgroundColor: C.chrome,
            boxShadow: "0 30px 80px rgba(0,0,0,0.55)",
            padding: "24px 28px",
            display: "flex",
            flexDirection: "column",
            gap: 16,
            boxSizing: "border-box",
          }}
        >
          <div
            style={{
              fontFamily: DISPLAY,
              fontSize: 19,
              fontWeight: 600,
              color: C.textMuted,
              paddingBottom: 13,
              borderBottom: `1px solid ${C.border}`,
            }}
          >
            coding agent
          </div>

          <div
            style={{
              alignSelf: "flex-end",
              maxWidth: 520,
              fontFamily: BODY,
              fontSize: 22,
              color: C.stone950,
              backgroundColor: C.accent,
              borderRadius: "16px 16px 4px 16px",
              padding: "13px 19px",
              opacity: userIn,
              transform: `translateY(${(1 - userIn) * 14}px)`,
            }}
          >
            Close out the WAL race fix and file a follow-up for login
            rate-limiting.
          </div>

          <div style={{ display: "flex", flexDirection: "column", gap: 12, marginTop: 6 }}>
            <ToolChip at={TOOL_1} label="lific · update_issue LIF-198 → done" />
            <ToolChip at={TOOL_2} label='lific · create_issue "Rate-limit login endpoint"' />
          </div>

          <FadeUp delay={REPLY} duration={12} style={{ marginTop: 2 }}>
            <div
              style={{
                fontFamily: BODY,
                fontSize: 21,
                color: C.text,
                lineHeight: 1.5,
              }}
            >
              Done — LIF-198 closed, follow-up filed as{" "}
              <span style={{ fontFamily: MONO, color: C.accent }}>LIF-232</span>.
            </div>
          </FadeUp>
        </div>

        {/* Live crop of the real board: Todo + Done columns */}
        <div
          style={{
            width: COL_W * 2 + 2,
            height: 600,
            borderRadius: 16,
            border: `1px solid ${C.border}`,
            backgroundColor: C.bg,
            boxShadow: "0 30px 80px rgba(0,0,0,0.55)",
            overflow: "hidden",
            display: "flex",
            position: "relative",
          }}
        >
          {/* Todo column */}
          <div
            style={{
              width: COL_W,
              flexShrink: 0,
              borderRight: `1px solid ${C.border}`,
              boxSizing: "border-box",
              position: "relative",
            }}
          >
            <ColumnHeader status="todo" count={todoCount} />
            {/* LIF-232 pops in on create_issue */}
            {frame >= BOARD_2 ? (
              <div
                style={{
                  position: "absolute",
                  left: CARD_PAD,
                  top: 40 + CARD_PAD,
                  opacity: newIn,
                  transform: `scale(${0.92 + newIn * 0.08}) translateY(${(1 - newIn) * -14}px)`,
                }}
              >
                <div
                  style={{
                    borderRadius: 6,
                    boxShadow: newFlash > 0 ? `0 0 ${18 * newFlash}px ${C.success}66` : undefined,
                  }}
                >
                  <IssueCard
                    issue={{
                      identifier: "LIF-232",
                      title: "Rate-limit login endpoint",
                      priority: "high",
                      labels: [L.auth],
                      updated: "just now",
                    }}
                    width={CARD_W}
                  />
                </div>
              </div>
            ) : null}
            {/* LIF-226 shifts down to make room */}
            <div
              style={{
                position: "absolute",
                left: CARD_PAD,
                top: 40 + CARD_PAD + (frame >= BOARD_2 ? shift * 95 : 0),
              }}
            >
              <IssueCard
                issue={{
                  identifier: "LIF-226",
                  title: "MCP: recurring plan templates",
                  priority: "medium",
                  labels: [L.mcp],
                  updated: "2h ago",
                }}
                width={CARD_W}
              />
            </div>
          </div>

          {/* Done column */}
          <div
            style={{
              width: COL_W,
              flexShrink: 0,
              boxSizing: "border-box",
              position: "relative",
            }}
          >
            <ColumnHeader status="done" count={doneCount} />
            {/* LIF-198 lands in Done on update_issue */}
            {frame >= BOARD_1 ? (
              <div
                style={{
                  position: "absolute",
                  left: CARD_PAD,
                  top: 40 + CARD_PAD,
                  opacity: doneIn,
                  transform: `scale(${0.92 + doneIn * 0.08})`,
                }}
              >
                <div
                  style={{
                    borderRadius: 6,
                    boxShadow: doneFlash > 0 ? `0 0 ${18 * doneFlash}px ${C.success}66` : undefined,
                  }}
                >
                  <IssueCard
                    issue={{
                      identifier: "LIF-198",
                      title: "Fix WAL checkpoint race on shutdown",
                      priority: "high",
                      labels: [L.core, L.bug],
                      updated: "just now",
                      status: "done",
                    }}
                    width={CARD_W}
                  />
                </div>
              </div>
            ) : null}
            {/* Existing done cards shift down */}
            <div
              style={{
                position: "absolute",
                left: CARD_PAD,
                top: 40 + CARD_PAD + (frame >= BOARD_1 ? spring({ frame: frame - BOARD_1, fps, config: { damping: 200, stiffness: 140 } }) * 113 : 0),
                display: "flex",
                flexDirection: "column",
                gap: 8,
              }}
            >
              <IssueCard
                issue={{
                  identifier: "LIF-183",
                  title: "OAuth device flow for CLI login",
                  labels: [L.auth],
                  updated: "5h ago",
                  status: "done",
                }}
                width={CARD_W}
              />
              <IssueCard
                issue={{
                  identifier: "LIF-171",
                  title: "Backup retention config",
                  labels: [L.core],
                  updated: "1d ago",
                  status: "done",
                }}
                width={CARD_W}
              />
            </div>
          </div>
        </div>
      </AbsoluteFill>

      <div
        style={{
          position: "absolute",
          bottom: 54,
          width: "100%",
          textAlign: "center",
          fontFamily: BODY,
          fontSize: 40,
          fontWeight: 500,
          color: C.text,
          opacity: captionIn,
        }}
      >
        Your coding agents are first-class citizens.{" "}
        <span style={{ color: C.accent, fontWeight: 600 }}>MCP built in.</span>
      </div>
    </Background>
  );
};
