import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { HTMLAttributes, ReactNode } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { PillWindow } from "@/components/Pill/PillWindow";
import i18n from "@/i18n";
import { audioCommands } from "@/lib/tauri";

const eventCallbacks = new Map<string, (event: { payload: unknown }) => void>();

vi.mock("framer-motion", () => {
  const MotionDiv = ({ children, ...props }: HTMLAttributes<HTMLDivElement>) => (
    <div {...props}>{children}</div>
  );
  return {
    AnimatePresence: ({ children }: { children: ReactNode }) => <>{children}</>,
    motion: {
      div: MotionDiv,
    },
  };
});

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    startDragging: vi.fn(),
  }),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (eventName: string, callback: (event: { payload: unknown }) => void) => {
      eventCallbacks.set(eventName, callback);
      return vi.fn();
    },
  ),
}));

vi.mock("@/components/Pill/AudioDots", () => ({
  AudioDots: () => <div data-testid="audio-dots" />,
}));

vi.mock("@/components/Pill/SettingsButton", () => ({
  SettingsButton: () => <button type="button">Settings</button>,
}));

vi.mock("@/lib/logger", () => ({
  logger: {
    error: vi.fn(),
  },
}));

vi.mock("@/lib/tauri", async () => {
  const actual = await vi.importActual<typeof import("@/lib/tauri")>("@/lib/tauri");
  return {
    ...actual,
    audioCommands: {
      ...actual.audioCommands,
      cancelRecording: vi.fn(),
    },
    settingsCommands: {
      ...actual.settingsCommands,
      getSettings: vi.fn().mockResolvedValue({ pill_indicator_mode: "always" }),
    },
    events: {
      ...actual.events,
      onSettingsChanged: vi.fn().mockResolvedValue(vi.fn()),
    },
  };
});

describe("PillWindow", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    eventCallbacks.clear();
    void i18n.changeLanguage("en");
  });

  it("cancels recording when Escape is pressed during recording", async () => {
    render(<PillWindow />);

    await waitFor(() => {
      expect(eventCallbacks.has("recording-state-changed")).toBe(true);
    });

    await act(async () => {
      eventCallbacks.get("recording-state-changed")?.({
        payload: {
          status: "recording",
          task_id: 1,
        },
      });
    });

    await waitFor(() => {
      expect(screen.getByRole("button", { name: "Cancel" })).toBeInTheDocument();
    });

    fireEvent.keyDown(window, { key: "Escape" });

    await waitFor(() => {
      expect(audioCommands.cancelRecording).toHaveBeenCalledTimes(1);
    });
  });
});
